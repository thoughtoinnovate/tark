# TUI UX Improvements — Full Pass Plan

Status: approved. Scope: full UX pass (selection + readability/space + theme/contrast/modal/discoverability/feedback).
Saved before implementation per user request.

## Problem statement
- Cannot select text from messages window: `EnableMouseCapture` (`src/transport/cli.rs:1116`)
  hands all mouse to crossterm; terminal-native drag selection is dead. Drag today only drives
  the scrollbar (`src/tui_new/renderer.rs:2040-2044`).
- Look-and-feel verdict: solid prototype, a notch below modern agent CLIs (Amp/Codex/Aider,
  lazygit-class pickers). Right information architecture; dated chrome, weak readability,
  discoverability gaps.

## Batch 1 — Selection / copy (highest pain)
1. Polish existing `v`/`y` yank: verify `arboard`→OSC52 reliability (`src/ui_backend/service.rs:44-75`),
   add `yy` (whole focused message) + yank-last-response, add `/copy` alias.
2. Surface in `/help`, status-bar hint, empty-state hints (discoverability).
3. Runtime mouse-capture toggle: config `mouse=false` + keybind doing
   `execute!(Disable|EnableMouseCapture)` so terminal-native selection works on demand.
   Document Shift+drag bypass in `/help`.
4. Wire `/export` for real: connect `ExportSession(PathBuf)` (`commands.rs:326-327`) to
   `SessionService::export()`; add per-message export (markdown to file).
5. Tests: unit tests for yank/export paths; help-text assertions.

## Batch 2 — Readability + space
6. Word-boundary wrapping on display width (`unicode_width`) in
   `message_area.rs:94-125` (`wrap_text`) and `markdown.rs:145-179,190-342` (`wrap_lines`);
   stop mid-word breaks.
7. Code blocks: at least keyword/comment tint (or `syntect`); render tables/quotes/HR/ordered
   lists instead of dropping them.
8. Replace emoji role icons (`👤🤖🧠🔧`) with 1-cell glyphs or use `UnicodeWidthStr` everywhere
   (`message_area.rs:247-254,1255-1265,371-386`, `header.rs:65-72`).
9. Dynamic input height (3–7 by content), `Header:1`, minimum-size guard + "terminal too small"
   overlay (`renderer.rs:2740-2749`).
10. Single-border cleanup: drop outer `TerminalFrame` border, keep inner (`terminal_frame.rs:69-72`,
    `message_area.rs:1324-1331`, `input.rs:318-327`).
11. Overlay sidebar (`Clear` + float) under ~110 cols instead of hard on/off at 80
    (`renderer.rs:2726-2737,2156-2166`).
12. Snapshot updates (`cargo insta review`) + widget tests for wrap/sidebar/input-height.

## Batch 3 — Theme / contrast / modals / discoverability / feedback
13. Theme picker honesty: implement or hide alias themes (CatppuccinLatte/Macchiato/Frappe,
    GruvboxLight, Solarized* → Mocha/Nord, `theme.rs:322-336`); per-theme `selection_bg`
    (replace hardcoded `Rgb(45,60,83)`, `command_autocomplete.rs:319-326`); fix Gruvbox cyan==blue.
14. Contrast pass on presets (≥4.5:1 target); derive bubble BGs from bg+accent instead of
    hardcoded Mocha (`theme.rs:212-238,365-367,385-387`).
15. Modals per guide: size caps + max-height + inner scroll + dim layer
    (`modal.rs:62-70`); reuse real `last_layout.modal` rect for questionnaire outside-click
    (`renderer.rs:255-276`).
16. Unify `Esc` = close/cancel everywhere; soften approval selection (tint + bold key, color for
    icon only) (`renderer.rs:1380-1419`, `approval_modal.rs:61-126`).
17. Discoverability: wire `[💭]`/`[?]` status-bar clicks (`renderer.rs:2242-2262`); add `?`/`F1`
    help binding + on-screen hint; restore `Enter send · Shift-Enter newline` hint + `Alt+Enter`
    fallback; footer hints for task keys.
18. Input: persist history (incl. slash), fuzzy + `@file` autocomplete provider, centralize
    vim/picker key router.
19. Feedback: status text + spinner in strip; expandable/copyable error line; preserve errors over
    rate-limit branch; per-tool spinner phase.
20. Empty state: welcome card with 3 starter hints; ellipsis-truncate header/status on
    `unicode_width`.

## Explicitly deferred
- Mouse-drag-to-select with OSC52 yank (fragile cell→offset map; keyboard flow covers it).

## Validation (per AGENTS.md, after each batch)
`cargo fmt --all` → `cargo clippy --all-targets --all-features -- -D warnings` →
`cargo test --all-features` → snapshot review where visual → manual TUI smoke (`/help`, `/model`,
`/theme`, `/compact`, `/export`, `v`/`y`, Esc, Enter) → docs update (README keybindings,
docs/THEMES.md, docs/TUI_MODAL_DESIGN_GUIDE.md) where user-visible.
