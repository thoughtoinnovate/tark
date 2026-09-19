//! Deterministic TUI stress harness (plan item P2.1).
//!
//! Requirements: R4 (TUI state stays consistent under load).
//! Scenarios: S9 (long-history scrolling over 10k messages),
//! S10 (background event storm of interleaved input/scroll/cancel ops).
//! NFRs: NFR2 (bounded retained message state), NFR3 (no wall-clock
//! assertions; every check here is structural and seeded).
//!
//! All tests are deterministic: fixed seed, fixed message counts, fixed
//! viewport sizes. No test asserts on timing or wall-clock behavior.
//!
//! Run the fast subset: `cargo test --test tui_stress`
//! Run the nightly/CI slow cases:
//! `cargo test --test tui_stress -- --ignored --nocapture`
//!
//! Scroll helpers below mirror the normalization logic in
//! `src/tui_new/controller.rs` (`Command::ScrollUp`/`ScrollDown`) and
//! `src/tui_new/renderer.rs` (`usize::MAX` means "pinned to bottom").
//! They are intentionally duplicated here so the harness stays
//! self-contained and exercises `SharedState` through its public API.

use tark_cli::ui_backend::{Message, MessageRole, SharedState};

/// Fixed seed for the deterministic PRNG. Changing this value changes the
/// generated operation sequences, so it must stay constant.
const STRESS_SEED: u64 = 0x9E37_79B9_7F4A_7C15;

/// Full-scale history size for the nightly/CI slow cases (S9/NFR2).
const STRESS_10K: usize = 10_000;

/// Reduced history size for the fast cases that run on every `cargo test`.
const FAST_COUNT: usize = 1_000;

/// Fixed viewport height in lines (matches a typical message pane).
const VIEWPORT_HEIGHT: usize = 24;

/// Deterministic line estimate per message used to derive `total_lines`.
const LINES_PER_MESSAGE: usize = 3;

/// Documented cap on retained message-store bytes after [`STRESS_10K`]
/// synthetic messages (NFR2). The synthetic bodies average ~200 bytes, so
/// 10k messages retain roughly 2-3 MiB; the 8 MiB cap leaves comfortable
/// headroom while still catching unbounded growth.
const MAX_RETAINED_BYTES: usize = 8 * 1024 * 1024;

/// Per-message structural overhead estimate (timestamps, role tag, `Vec`
/// bookkeeping) added to the measured content bytes.
const PER_MESSAGE_OVERHEAD_BYTES: usize = 64;

/// Scroll step mirroring `Command::ScrollUp`/`ScrollDown` in the controller.
const SCROLL_STEP: usize = 3;

/// Deterministic xorshift64* PRNG (std only; fixed seed => reproducible).
struct SeededRng {
    state: u64,
}

impl SeededRng {
    fn new(seed: u64) -> Self {
        Self { state: seed.max(1) }
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n.max(1) as u64) as usize
    }
}

/// Build a deterministic synthetic message for `SharedState`.
///
/// Roles cycle deterministically; bodies grow deterministically with the
/// index (1..=8 chunks of fixed prose) so retained-byte math is stable.
fn synthetic_message(index: usize) -> Message {
    let role = match index % 4 {
        0 => MessageRole::User,
        1 => MessageRole::Assistant,
        2 => MessageRole::Tool,
        _ => MessageRole::System,
    };
    let chunks = 1 + (index % 8);
    let mut content = format!("stress-msg-{index:05} ");
    for _ in 0..chunks {
        content.push_str("the quick brown fox jumps over the lazy dog. ");
    }
    Message {
        role,
        content,
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
    }
}

/// Maximum valid scroll offset for a (total, viewport) pair.
fn max_offset(total_lines: usize, viewport_height: usize) -> usize {
    total_lines.saturating_sub(viewport_height)
}

/// Normalize a raw scroll offset the way the renderer does:
/// `usize::MAX` means "pinned to bottom".
fn normalized_offset(raw: usize, max: usize) -> usize {
    if raw == usize::MAX {
        max
    } else {
        raw.min(max)
    }
}

/// Mirror `Command::ScrollUp`: move up, disengage follow-tail.
fn scroll_up(state: &SharedState, step: usize) {
    let max = max_offset(
        state.messages_total_lines(),
        state.messages_viewport_height(),
    );
    let current = normalized_offset(state.messages_scroll_offset(), max);
    if current > 0 {
        state.set_messages_scroll_offset(current.saturating_sub(step));
    }
    state.set_follow_tail(false);
}

/// Mirror `Command::ScrollDown`: move down, re-engage follow-tail at bottom.
fn scroll_down(state: &SharedState, step: usize) {
    let max = max_offset(
        state.messages_total_lines(),
        state.messages_viewport_height(),
    );
    let current = normalized_offset(state.messages_scroll_offset(), max);
    let next = current.saturating_add(step).min(max);
    state.set_messages_scroll_offset(next);
    state.set_follow_tail(next >= max);
}

/// Fill `state` with `count` synthetic messages and publish deterministic
/// viewport metrics (`total_lines = count * LINES_PER_MESSAGE`).
fn fill_history(state: &SharedState, count: usize) {
    for i in 0..count {
        state.add_message(synthetic_message(i));
    }
    state.set_messages_metrics(count * LINES_PER_MESSAGE, VIEWPORT_HEIGHT);
}

/// Assert the structural scroll invariants (S9):
/// normalized offset stays within bounds and `usize::MAX` (pinned) implies
/// an engaged follow-tail flag.
fn assert_state_invariants(state: &SharedState) {
    let total = state.messages_total_lines();
    let viewport = state.messages_viewport_height();
    let max = max_offset(total, viewport);
    let raw = state.messages_scroll_offset();
    let norm = normalized_offset(raw, max);
    assert!(
        norm <= max,
        "scroll offset {norm} out of bounds (max {max}, total {total}, viewport {viewport})"
    );
    if total > viewport {
        assert!(
            norm + viewport <= total,
            "viewport [{norm}..{}] exceeds total lines {total}",
            norm + viewport
        );
    }
    if raw == usize::MAX {
        assert!(
            state.follow_tail(),
            "pinned offset (usize::MAX) requires engaged follow-tail"
        );
    }
}

/// Estimate retained message-store bytes without cloning (NFR2).
fn retained_bytes(state: &SharedState) -> usize {
    state.with_messages(|msgs| {
        msgs.iter()
            .map(|m| {
                m.content.len()
                    + m.timestamp.len()
                    + m.thinking.as_ref().map_or(0, |t| t.len())
                    + PER_MESSAGE_OVERHEAD_BYTES
            })
            .sum()
    })
}

/// Shared long-history scenario used by both the fast and the 10k case.
fn exercise_long_history(state: &SharedState, count: usize) {
    fill_history(state, count);
    assert_eq!(
        state.message_count(),
        count,
        "expected {count} messages in history"
    );

    // Pinned-to-bottom starting point, as after a fresh load.
    state.scroll_to_bottom();
    assert_state_invariants(state);
    let max = max_offset(
        state.messages_total_lines(),
        state.messages_viewport_height(),
    );
    assert_eq!(
        normalized_offset(state.messages_scroll_offset(), max),
        max,
        "scroll_to_bottom should pin to max offset {max}"
    );

    // Walk up in controller-sized steps; follow-tail must disengage.
    for _ in 0..10 {
        scroll_up(state, SCROLL_STEP);
        assert_state_invariants(state);
    }
    assert!(
        !state.follow_tail(),
        "follow-tail must disengage after scrolling up"
    );

    // Walk back down to the bottom; follow-tail must re-engage.
    for _ in 0..=(max / SCROLL_STEP + 2) {
        scroll_down(state, SCROLL_STEP);
        assert_state_invariants(state);
    }
    let max = max_offset(
        state.messages_total_lines(),
        state.messages_viewport_height(),
    );
    assert_eq!(
        normalized_offset(state.messages_scroll_offset(), max),
        max,
        "scrolling down past the end should clamp to max offset {max}"
    );
    assert!(
        state.follow_tail(),
        "follow-tail must re-engage at the bottom"
    );

    // Sweep the full range coarsely; every position must stay in bounds.
    let mut offset = 0;
    while offset <= max {
        state.set_messages_scroll_offset(offset);
        assert_state_invariants(state);
        offset += 7;
    }
}

#[test]
fn long_history_viewport_math_small() {
    let state = SharedState::new();
    exercise_long_history(&state, FAST_COUNT);
}

// Slow case: full 10k-message history sweep. Deterministic but heavy in
// debug builds, so it runs in nightly/CI (`-- --ignored`), not on every
// `cargo test`.
#[test]
#[ignore]
fn stress_long_history_10k_scrolling() {
    let state = SharedState::new();
    exercise_long_history(&state, STRESS_10K);
}

/// Interleave synthetic input/scroll/cancel operations deterministically
/// (S10). Every `CHECK_EVERY` ops the structural invariants are asserted;
/// completion without panic is itself the regression signal.
fn exercise_event_storm(state: &SharedState, seed: u64, ops: usize) {
    const CHECK_EVERY: usize = 256;
    let mut rng = SeededRng::new(seed);
    fill_history(state, FAST_COUNT);

    for i in 0..ops {
        match rng.below(8) {
            // Synthetic input edits.
            0 => {
                state.set_input_text(format!("storm-input-{i:05} lorem ipsum dolor"));
                state.set_input_cursor(rng.below(32));
            }
            1 => state.clear_input(),
            // Scroll operations.
            2 => scroll_up(state, 1 + rng.below(10)),
            3 => scroll_down(state, 1 + rng.below(10)),
            4 => state.scroll_to_bottom(),
            // Cancel operations: processing flag toggling + stream reset.
            5 => {
                state.set_llm_processing(true);
                state.append_streaming_content("storm-chunk ");
                state.set_llm_processing(false);
            }
            6 => state.clear_streaming(),
            // Background append while scrolled up (must not steal position).
            _ => {
                state.scroll_to_bottom_if_following();
                state.append_streaming_content("bg ");
            }
        }
        // Occasionally grow/shrink history so offsets are re-derived.
        if i % 511 == 0 {
            state.add_message(synthetic_message(i));
            state.set_messages_metrics(state.message_count() * LINES_PER_MESSAGE, VIEWPORT_HEIGHT);
        }
        if i % 1023 == 0 {
            state.remove_oldest_messages(4);
            state.set_messages_metrics(state.message_count() * LINES_PER_MESSAGE, VIEWPORT_HEIGHT);
        }
        if i % CHECK_EVERY == 0 {
            assert_state_invariants(state);
        }
    }
    // End in a clean, quiescent state and re-check invariants.
    state.clear_streaming();
    state.set_llm_processing(false);
    state.clear_input();
    assert_state_invariants(state);
}

#[test]
fn event_storm_invariants_small() {
    let state = SharedState::new();
    exercise_event_storm(&state, STRESS_SEED, 2_000);
}

// Slow case: 20k interleaved ops against a 10k-message history.
// Deterministic but heavy in debug builds, so it runs in nightly/CI
// (`-- --ignored`), not on every `cargo test`.
#[test]
#[ignore]
fn stress_event_storm_interleaved() {
    let state = SharedState::new();
    fill_history(&state, STRESS_10K);
    exercise_event_storm(&state, STRESS_SEED ^ 0x1234_5678, 20_000);
    assert!(
        state.message_count() >= STRESS_10K,
        "storm should retain at least the initial 10k-message history"
    );
}

#[test]
fn retained_state_cap_small() {
    let state = SharedState::new();
    fill_history(&state, FAST_COUNT);
    assert_eq!(state.message_count(), FAST_COUNT);
    let bytes = retained_bytes(&state);
    assert!(
        bytes <= MAX_RETAINED_BYTES,
        "retained {bytes} bytes exceeds cap {MAX_RETAINED_BYTES}"
    );
}

// Slow case: retained-state cap after the full 10k-message history (NFR2).
// Deterministic but heavy in debug builds, so it runs in nightly/CI
// (`-- --ignored`), not on every `cargo test`.
#[test]
#[ignore]
fn stress_retained_state_10k_bounded() {
    let state = SharedState::new();
    fill_history(&state, STRESS_10K);
    assert_eq!(
        state.message_count(),
        STRESS_10K,
        "expected full 10k-message history"
    );
    let bytes = retained_bytes(&state);
    assert!(
        bytes <= MAX_RETAINED_BYTES,
        "retained {bytes} bytes for 10k messages exceeds cap {MAX_RETAINED_BYTES}"
    );
}
