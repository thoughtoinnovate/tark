//! Criterion-style TUI benchmark harness (plan item P2.1, std only).
//!
//! Requirements: R4 (frame/input latency stays observable under load).
//! Scenarios: S9 (frame render cost with history), S10 (input/scroll cost
//! under interleaved load). NFRs: NFR2 (bench at bounded history sizes),
//! NFR3 (benchmarks PRINT p50/p99 latencies; they never assert on
//! wall-clock time, so they cannot be flaky).
//!
//! Timing uses `std::time::Instant` only (`criterion` is intentionally not
//! a dependency). Each harness prints one summary line:
//! `bench <name>: n=<samples> min=.. p50=.. p99=.. max=..` (microseconds).
//! View numbers with `cargo test --test tui_bench -- --nocapture`.
//!
//! Run: `cargo test --all-features --test tui_bench -- --nocapture`

use std::time::Instant;

use ratatui::backend::TestBackend;
use ratatui::Terminal;
use tark_cli::tui_new::{Message, MessageRole, Theme};
use tark_cli::ui_backend::SharedState;

/// Fixed viewport used by every render bench (deterministic layout).
const BENCH_WIDTH: u16 = 100;
const BENCH_HEIGHT: u16 = 30;

/// Frames sampled per render bench (S9).
const BENCH_FRAMES: usize = 60;

/// Input/scroll operations sampled per bench (S10).
const BENCH_OPS: usize = 500;

/// Bounded history size for benches (NFR2): large enough to be
/// representative, small enough for fast default runs.
const BENCH_HISTORY: usize = 300;

/// Nearest-rank percentile over sorted nanosecond samples.
fn percentile_sorted(sorted_nanos: &[u128], pct: f64) -> u128 {
    assert!(
        !sorted_nanos.is_empty(),
        "percentile needs at least one sample"
    );
    let last = sorted_nanos.len() - 1;
    let rank = (pct / 100.0 * last as f64).round() as usize;
    sorted_nanos[rank.min(last)]
}

fn nanos_to_micros(nanos: u128) -> f64 {
    nanos as f64 / 1000.0
}

/// Print (never assert) the latency distribution for a bench.
fn print_summary(name: &str, samples: &mut [u128]) {
    samples.sort_unstable();
    let n = samples.len();
    let min = samples[0];
    let max = samples[n - 1];
    let p50 = percentile_sorted(samples, 50.0);
    let p99 = percentile_sorted(samples, 99.0);
    println!(
        "bench {name}: n={n} min={:.3}us p50={:.3}us p99={:.3}us max={:.3}us",
        nanos_to_micros(min),
        nanos_to_micros(p50),
        nanos_to_micros(p99),
        nanos_to_micros(max)
    );
}

/// Deterministic widget message (cycles User/Agent/Tool roles).
fn bench_widget_message(index: usize) -> Message {
    let role = match index % 3 {
        0 => MessageRole::User,
        1 => MessageRole::Agent,
        _ => MessageRole::Tool,
    };
    Message::new(
        role,
        format!("bench message {index:04} with stable trailing prose for layout"),
    )
}

/// Frame render latency over a bounded history (S9/NFR2).
///
/// Renders [`BENCH_FRAMES`] frames of a [`BENCH_HISTORY`]-message
/// `MessageArea` into a `TestBackend` and prints the distribution.
#[test]
fn bench_frame_render_reports_p50_p99() {
    let theme = Theme::default();
    let messages: Vec<Message> = (0..BENCH_HISTORY).map(bench_widget_message).collect();
    let backend = TestBackend::new(BENCH_WIDTH, BENCH_HEIGHT);
    let mut terminal = Terminal::new(backend).expect("test backend should build");

    let mut samples = Vec::with_capacity(BENCH_FRAMES);
    for frame in 0..BENCH_FRAMES {
        let scroll = (frame * 3) % BENCH_HISTORY;
        let start = Instant::now();
        terminal
            .draw(|f| {
                let area = f.area();
                let widget = tark_cli::tui_new::MessageArea::new(&messages, &theme).scroll(scroll);
                f.render_widget(widget, area);
            })
            .expect("test backend draw should succeed");
        samples.push(start.elapsed().as_nanos());
    }
    assert_eq!(samples.len(), BENCH_FRAMES, "expected one sample per frame");
    print_summary("frame_render", &mut samples);
}

/// Input handling latency: `set_input_text` + cursor + read round-trip
/// on `SharedState` (S10). Prints the distribution.
#[test]
fn bench_input_handling_reports_p50_p99() {
    let state = SharedState::new();
    let mut samples = Vec::with_capacity(BENCH_OPS);
    for i in 0..BENCH_OPS {
        let text = format!("bench input {i:04} with some trailing text to edit");
        let start = Instant::now();
        state.set_input_text(text);
        state.set_input_cursor(i % 32);
        let _ = state.input_text();
        samples.push(start.elapsed().as_nanos());
    }
    assert_eq!(samples.len(), BENCH_OPS, "expected one sample per op");
    assert!(
        !state.input_text().is_empty(),
        "bench should leave deterministic input behind"
    );
    print_summary("input_handling", &mut samples);
}

/// Scroll-update latency against a bounded history (S9/S10/NFR2).
///
/// Alternates controller-style scroll up/down steps on a `SharedState`
/// holding [`BENCH_HISTORY`] messages and prints the distribution.
#[test]
fn bench_scroll_update_reports_p50_p99() {
    use tark_cli::ui_backend::{Message as BackendMessage, MessageRole as BackendRole};

    let state = SharedState::new();
    for i in 0..BENCH_HISTORY {
        state.add_message(BackendMessage {
            role: if i % 2 == 0 {
                BackendRole::User
            } else {
                BackendRole::Assistant
            },
            content: format!("bench history {i:04}"),
            thinking: None,
            context_transient: false,
            tool_calls: Vec::new(),
            segments: Vec::new(),
            collapsed: false,
            timestamp: "00:00:00".to_string(),
            remote: false,
            provider: None,
            model: None,
            tool_args: None,
        });
    }
    state.set_messages_metrics(BENCH_HISTORY * 3, 24);

    let mut samples = Vec::with_capacity(BENCH_OPS);
    for i in 0..BENCH_OPS {
        let start = Instant::now();
        if i % 2 == 0 {
            let max = state
                .messages_total_lines()
                .saturating_sub(state.messages_viewport_height());
            let current = state.messages_scroll_offset().min(max);
            state.set_messages_scroll_offset(current.saturating_sub(3));
            state.set_follow_tail(false);
        } else {
            let max = state
                .messages_total_lines()
                .saturating_sub(state.messages_viewport_height());
            let current = if state.messages_scroll_offset() == usize::MAX {
                max
            } else {
                state.messages_scroll_offset().min(max)
            };
            let next = current.saturating_add(3).min(max);
            state.set_messages_scroll_offset(next);
            state.set_follow_tail(next >= max);
        }
        samples.push(start.elapsed().as_nanos());
    }
    assert_eq!(samples.len(), BENCH_OPS, "expected one sample per op");
    let max = state
        .messages_total_lines()
        .saturating_sub(state.messages_viewport_height());
    let norm = if state.messages_scroll_offset() == usize::MAX {
        max
    } else {
        state.messages_scroll_offset().min(max)
    };
    assert!(
        norm <= max,
        "scroll offset {norm} out of bounds (max {max}) after bench"
    );
    print_summary("scroll_update", &mut samples);
}
