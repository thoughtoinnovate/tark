//! Resource-adaptive caps for subagent fan-out (plan §B1).
//!
//! `ResourceMonitor` tunes the effective concurrency cap in `[min, max]`
//! from system resources. It emits through a [`watch`] channel
//! (latest-value semantics — slow readers never build a queue).
//! `manual` mode spawns no task; the caller uses the validated `max`.
//!
//! The shift decision itself is the pure [`decide`] function so it can be
//! unit-tested without touching `/proc`.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, OnceLock,
};
use std::time::{Duration, Instant};

use tokio::sync::watch;

use crate::config::SubagentConfig;

/// Point-in-time resource sample.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    /// Logical CPU count.
    pub cpu: usize,
    /// Free memory in MiB (`u64::MAX` when unknown, e.g. non-Linux).
    pub free_mb: u64,
    /// 1-minute load average (`0.0` when unknown).
    pub load1: f64,
}

/// Input bundle for [`decide`] (kept as a struct for clippy's arg limit).
#[derive(Debug, Clone, Copy)]
pub struct DecideInput {
    pub current: usize,
    pub min: usize,
    pub max: usize,
    pub sample: Sample,
    pub floor_free_mb: u64,
    pub saturated: bool,
    pub rate_limited: bool,
    pub clean_polls: u32,
}

/// Pure shift decision: returns the new effective cap when a shift is
/// warranted, `None` to hold.
///
/// - Down when memory is below `floor_free_mb`, load/cpu exceeds 0.85,
///   or a recent rate-limit signal arrived.
/// - Up only when memory has 512 MiB headroom above the floor, load is
///   cool, the pool is saturated (work is queuing), and at least two
///   consecutive clean polls have passed (hysteresis).
/// - Always clamped to `[min, max]`; never returns the current value.
pub fn decide(input: DecideInput) -> Option<usize> {
    let DecideInput {
        current,
        min,
        max,
        sample,
        floor_free_mb,
        saturated,
        rate_limited,
        clean_polls,
    } = input;
    let cpu = sample.cpu.max(1) as f64;
    let hot = sample.load1 / cpu > 0.85;
    let cool = sample.load1 / cpu < 0.55;

    if sample.free_mb < floor_free_mb || hot || rate_limited {
        if current > min {
            return Some(current - 1);
        }
        return None;
    }

    if saturated
        && clean_polls >= 2
        && sample.free_mb > floor_free_mb.saturating_add(512)
        && cool
        && current < max
    {
        return Some(current + 1);
    }

    None
}

/// Cached logical CPU count (a syscall — read once per process).
fn cached_cpu() -> usize {
    static CPU: OnceLock<usize> = OnceLock::new();
    *CPU.get_or_init(|| {
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(2)
            .max(1)
    })
}

/// Blocking system sample. `/proc` reads are Linux-only; other platforms
/// report unknown memory/load so only the CPU bound applies.
fn sample_blocking() -> Sample {
    let cpu = cached_cpu();
    #[cfg(target_os = "linux")]
    {
        Sample {
            cpu,
            free_mb: read_mem_available_mb().unwrap_or(u64::MAX),
            load1: read_load1().unwrap_or(0.0),
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        Sample {
            cpu,
            free_mb: u64::MAX,
            load1: 0.0,
        }
    }
}

#[cfg(target_os = "linux")]
fn read_mem_available_mb() -> Option<u64> {
    let content = std::fs::read_to_string("/proc/meminfo").ok()?;
    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("MemAvailable:") {
            let kb: u64 = rest.split_whitespace().next()?.parse().ok()?;
            return Some(kb / 1024);
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn read_load1() -> Option<f64> {
    let content = std::fs::read_to_string("/proc/loadavg").ok()?;
    content.split_whitespace().next()?.parse().ok()
}

/// Adaptive cap monitor. Owns the background task (if any); drop to stop it.
pub struct ResourceMonitor {
    handle: Option<tokio::task::JoinHandle<()>>,
    pressure: Arc<AtomicBool>,
    saturated: Arc<AtomicBool>,
}

impl ResourceMonitor {
    /// Spawn the monitor for `auto` mode.
    ///
    /// `local_provider` (e.g. Ollama) caps the effective value at 2 —
    /// local VRAM, not CPU, is the binding constraint.
    /// Returns the monitor plus a receiver seeded with the initial value
    /// `min(max, cpu/2)` clamped to `[min, max]`.
    pub fn spawn(cfg: &SubagentConfig, local_provider: bool) -> (Self, watch::Receiver<usize>) {
        let (min, max) = cfg.validated_bounds();
        let mut initial = (cached_cpu() / 2).clamp(min, max);
        if local_provider {
            initial = initial.min(2).clamp(min, max);
        }
        let (tx, rx) = watch::channel(initial);

        let poll = Duration::from_millis(cfg.poll_ms.max(250));
        let cooldown = Duration::from_millis(cfg.cooldown_ms);
        let floor = cfg.min_free_mem_mb;
        let pressure = Arc::new(AtomicBool::new(false));
        let saturated = Arc::new(AtomicBool::new(false));
        let pressure_task = pressure.clone();
        let saturated_task = saturated.clone();

        let handle = tokio::spawn(async move {
            let mut current = initial;
            let mut last_shift = Instant::now();
            let mut clean: u32 = 0;
            // Small jitter so multiple pools don't sample in lockstep.
            let mut jitter = 0u64;
            loop {
                tokio::time::sleep(poll + Duration::from_millis(jitter)).await;
                jitter = (jitter + 97) % 200;

                let sample = tokio::task::spawn_blocking(sample_blocking)
                    .await
                    .unwrap_or(Sample {
                        cpu: cached_cpu(),
                        free_mb: u64::MAX,
                        load1: 0.0,
                    });
                let rate_limited = pressure_task.swap(false, Ordering::SeqCst);
                clean = if rate_limited {
                    0
                } else {
                    clean.saturating_add(1)
                };
                let is_saturated = saturated_task.load(Ordering::Relaxed);

                if let Some(next) = decide(DecideInput {
                    current,
                    min,
                    max,
                    sample,
                    floor_free_mb: floor,
                    saturated: is_saturated,
                    rate_limited,
                    clean_polls: clean,
                }) {
                    if last_shift.elapsed() >= cooldown {
                        current = next;
                        last_shift = Instant::now();
                        clean = 0;
                        let _ = tx.send(current);
                    }
                }
            }
        });

        (
            Self {
                handle: Some(handle),
                pressure,
                saturated,
            },
            rx,
        )
    }

    /// Record a provider rate-limit (429/529) — forces a downshift check.
    pub fn note_pressure(&self) {
        self.pressure.store(true, Ordering::SeqCst);
    }

    /// Tell the monitor whether spawn requests are currently queuing.
    pub fn set_saturated(&self, saturated: bool) {
        self.saturated.store(saturated, Ordering::Relaxed);
    }
}

impl Drop for ResourceMonitor {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            handle.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(cpu: usize, free_mb: u64, load1: f64) -> Sample {
        Sample {
            cpu,
            free_mb,
            load1,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn d(
        current: usize,
        min: usize,
        max: usize,
        sample: Sample,
        floor_free_mb: u64,
        saturated: bool,
        rate_limited: bool,
        clean_polls: u32,
    ) -> Option<usize> {
        decide(DecideInput {
            current,
            min,
            max,
            sample,
            floor_free_mb,
            saturated,
            rate_limited,
            clean_polls,
        })
    }

    #[test]
    fn shifts_down_on_low_memory() {
        assert_eq!(
            d(3, 1, 5, sample(8, 100, 0.2), 512, false, false, 9),
            Some(2)
        );
    }

    #[test]
    fn shifts_down_on_hot_load() {
        // 7.6 / 8 = 0.95 > 0.85
        assert_eq!(
            d(3, 1, 5, sample(8, 4096, 7.6), 512, false, false, 9),
            Some(2)
        );
    }

    #[test]
    fn shifts_down_on_rate_limit() {
        assert_eq!(
            d(3, 1, 5, sample(8, 4096, 0.2), 512, false, true, 9),
            Some(2)
        );
    }

    #[test]
    fn holds_at_floor() {
        assert_eq!(d(1, 1, 5, sample(8, 100, 7.9), 512, false, false, 0), None);
    }

    #[test]
    fn shifts_up_when_saturated_and_cool_and_clean() {
        // 4.0 / 8 = 0.5 < 0.55
        assert_eq!(
            d(2, 1, 5, sample(8, 2048, 4.0), 512, true, false, 2),
            Some(3)
        );
    }

    #[test]
    fn no_upshift_without_saturation_or_clean_history() {
        assert_eq!(d(2, 1, 5, sample(8, 2048, 4.0), 512, false, false, 9), None);
        assert_eq!(d(2, 1, 5, sample(8, 2048, 4.0), 512, true, false, 1), None);
    }

    #[test]
    fn holds_at_ceiling_and_mid() {
        assert_eq!(d(5, 1, 5, sample(8, 8192, 0.1), 512, true, false, 9), None);
        assert_eq!(d(3, 1, 5, sample(8, 2048, 4.5), 512, false, false, 9), None);
    }

    #[test]
    fn downshift_beats_upshift_conditions() {
        // Cool and saturated, but memory below floor → still down.
        assert_eq!(
            d(3, 1, 5, sample(8, 100, 0.1), 512, true, false, 9),
            Some(2)
        );
    }
}
