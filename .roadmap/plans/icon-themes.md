# Icon Themes & Cross-Terminal Compatibility

**Status:** Planned  
**Priority:** Medium  
**Created:** 2026-01-10  
**Author:** AI Assistant  

## Summary

Implement a configurable icon system with automatic terminal capability detection and user-customizable themes. This ensures consistent TUI rendering across all terminals while allowing power users to customize icons.

---

## Problem Statement

### Current Issues

1. **Hardcoded emoji icons** (`🤖 🧠 📁 👤`) scattered across TUI widgets
2. **Inconsistent rendering** across terminals:
   - Emoji may show as tofu boxes on systems without emoji fonts
   - Emoji have variable display widths (1-2 cells depending on terminal)
   - Some terminals (linux console, dumb) don't support Unicode at all
3. **Layout bugs** from using `.len()` (byte count) instead of display width
4. **No user customization** for icon preferences

### Affected Files

| File | Icons Used |
|------|------------|
| `src/tui/widgets/message_list.rs` | `🤖 👤 ⚙️ 🔧 ▼ ▶ ✓ ✗ ⏳ •` |
| `src/tui/widgets/status_bar.rs` | `◆ ◇ ❓ ● 🧠` |
| `src/tui/widgets/panel.rs` | `○ ● ✓ ✗ ⊘ ℹ ⚠ ▼ ▶` |
| `src/tui/widgets/collapsible.rs` | `🧠 ⚙️ ▼ ▶` |
| `src/tui/widgets/tool_block.rs` | `⏳ ✓ ✗` |
| `src/tui/widgets/file_dropdown.rs` | `📁 🦀 🐍 📜 ⚙️ 📝 📄 🖼️` |
| `src/tui/widgets/picker.rs` | Various via `with_icon()` |
| `src/tui/widgets/approval_card.rs` | Risk level icons |
| `src/tui/attachments.rs` | `📷 📄 📕 📝 📊` |
| `src/tui/app.rs` | `✅ ❌ ⚠️ 📎 💰 🧠` + inline messages |
| `src/tools/file_ops.rs` | `📁 📄` |

---

## Solution Design

### Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    config.toml                          │
│  [tui.icons]                                            │
│  preset = "auto"                                        │
│  [tui.icons.theme]                                      │
│  role_user = "👤"  # optional overrides                 │
└─────────────────────────────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────┐
│                   IconSet::from_config()                │
│                                                         │
│  Fallback chain:                                        │
│  1. User theme override (if set)                        │
│  2. Base preset (unicode/ascii/auto-detected)           │
│  3. ASCII (guaranteed safe)                             │
└─────────────────────────────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────┐
│              static ICONS: OnceLock<IconSet>            │
│                                                         │
│  Accessed via icons() from any widget                   │
└─────────────────────────────────────────────────────────┘
```

### Icon Presets

#### Unicode Preset (default when detected)
Safe symbols present in virtually all monospace fonts:

| Icon Key | Symbol | Description |
|----------|--------|-------------|
| `role_user` | `●` | User messages |
| `role_assistant` | `◆` | Assistant messages |
| `role_system` | `⚙` | System messages (single char, no VS16) |
| `role_tool` | `⚡` | Tool messages |
| `status_ok` | `✓` | Success/completed |
| `status_fail` | `✗` | Failed/error |
| `status_warn` | `⚠` | Warning |
| `status_running` | `◌` | In progress |
| `status_pending` | `○` | Pending/waiting |
| `expand_open` | `▼` | Expanded section |
| `expand_closed` | `▶` | Collapsed section |
| `bullet` | `•` | List bullet |
| `thinking` | `≡` | Thinking/reasoning |
| `file` | `·` | File indicator |
| `folder` | `▸` | Directory indicator |
| `mode_build` | `◆` | Build mode |
| `mode_plan` | `◇` | Plan mode |
| `mode_ask` | `?` | Ask mode |
| `connection_on` | `●` | Connected |
| `connection_off` | `○` | Disconnected |

#### ASCII Preset (fallback)

| Icon Key | Symbol |
|----------|--------|
| `role_user` | `*` |
| `role_assistant` | `>` |
| `role_system` | `@` |
| `role_tool` | `#` |
| `status_ok` | `+` |
| `status_fail` | `x` |
| `status_warn` | `!` |
| `status_running` | `~` |
| `status_pending` | `o` |
| `expand_open` | `v` |
| `expand_closed` | `>` |
| `bullet` | `-` |
| `thinking` | `~` |
| `file` | `-` |
| `folder` | `/` |
| `mode_build` | `*` |
| `mode_plan` | `o` |
| `mode_ask` | `?` |

### Terminal Detection Logic

```rust
fn detect_unicode_support() -> bool {
    // 1. Check TARK_ICONS env override
    if let Ok(val) = std::env::var("TARK_ICONS") {
        return !val.eq_ignore_ascii_case("ascii");
    }

    // 2. Check locale for UTF-8
    let locale_ok = std::env::var("LANG")
        .or_else(|_| std::env::var("LC_ALL"))
        .or_else(|_| std::env::var("LC_CTYPE"))
        .map(|v| v.to_uppercase().contains("UTF"))
        .unwrap_or(false);

    // 3. Check TERM for known-bad terminals
    let term = std::env::var("TERM").unwrap_or_default();
    let term_ok = !matches!(term.as_str(), 
        "dumb" | "linux" | "cons25" | "emacs" | "vt100" | "vt220"
    );

    // 4. Windows-specific checks
    #[cfg(windows)]
    let platform_ok = std::env::var("WT_SESSION").is_ok()      // Windows Terminal
        || std::env::var("TERM_PROGRAM").is_ok()                // VS Code etc.
        || std::env::var("ConEmuANSI").is_ok();                 // ConEmu
    #[cfg(not(windows))]
    let platform_ok = true;

    locale_ok && term_ok && platform_ok
}
```

### Config Schema

```toml
[tui.icons]
# Base preset: "auto" (detect), "unicode", or "ascii"
# Default: "auto"
preset = "auto"

# Optional theme overrides
# Any key not specified falls back to the preset
[tui.icons.theme]
role_user = "👤"
role_assistant = "🤖"
role_system = "⚙️"
role_tool = "🔧"
thinking = "🧠"
folder = "📁"
file = "📄"
# ... etc
```

---

## Implementation Plan

### Phase 1: Core Infrastructure
**Estimated: 2-3 hours**

- [ ] Create `src/tui/icons.rs` module
  - [ ] `IconPreset` enum (Auto, Unicode, Ascii)
  - [ ] `IconThemeConfig` struct (serde, all fields Optional)
  - [ ] `IconsConfig` struct (preset + theme)
  - [ ] `IconSet` struct (resolved runtime icons)
  - [ ] `IconSet::from_config()` with fallback chain
  - [ ] `detect_unicode_support()` function
  - [ ] Global `OnceLock<IconSet>` with `icons()` accessor
  - [ ] Unit tests for detection and fallback

### Phase 2: Config Integration
**Estimated: 1 hour**

- [ ] Add `icons: IconsConfig` to `TuiConfig` in `src/tui/config.rs`
- [ ] Update `TuiConfig::merge()` to handle icons
- [ ] Initialize icons in `TuiApp::new()` / app startup
- [ ] Add example config to `examples/tark-config/config.toml`

### Phase 3: Widget Migration
**Estimated: 3-4 hours**

- [ ] `src/tui/widgets/message_list.rs`
  - [ ] Replace `Role::icon()` to use `icons()`
  - [ ] Replace inline `⏳ ✓ ✗ ▼ ▶ •` symbols
- [ ] `src/tui/widgets/status_bar.rs`
  - [ ] Replace `AgentMode::icon()` to use `icons()`
  - [ ] Replace `TrustLevel::icon()` calls
  - [ ] Fix connection status icons
- [ ] `src/tui/widgets/panel.rs`
  - [ ] Replace `TaskStatus::icon()`
  - [ ] Replace `NotificationLevel::icon()`
  - [ ] Replace expand/collapse icons
- [ ] `src/tui/widgets/collapsible.rs`
  - [ ] Replace `BlockType::icon()`
- [ ] `src/tui/widgets/tool_block.rs`
  - [ ] Replace `ToolStatus::icon()`
- [ ] `src/tui/widgets/file_dropdown.rs`
  - [ ] Replace file type icons
- [ ] `src/tui/widgets/picker.rs`
  - [ ] Update icon handling
- [ ] `src/tui/widgets/approval_card.rs`
  - [ ] Replace risk level icons
- [ ] `src/tui/widgets/plan_picker.rs`
  - [ ] Replace plan status icons
- [ ] `src/tui/attachments.rs`
  - [ ] Replace `AttachmentType::icon()`
- [ ] `src/tui/app.rs`
  - [ ] Replace inline emoji in status messages
- [ ] `src/tools/file_ops.rs`
  - [ ] Replace file/folder icons in output

### Phase 4: Width Calculation Fixes
**Estimated: 1-2 hours**

- [ ] Audit all `.len()` usage in TUI code
- [ ] Replace with `unicode_width::UnicodeWidthStr::width()`
- [ ] Key files to check:
  - [ ] `src/tui/widgets/status_bar.rs` (right-alignment)
  - [ ] `src/tui/widgets/message_list.rs` (wrap calculations)
  - [ ] `src/tui/widgets/panel.rs` (padding)
  - [ ] Any `format!()` + padding logic

### Phase 5: Testing & Documentation
**Estimated: 1-2 hours**

- [ ] Add integration tests for icon rendering
- [ ] Test on multiple terminals:
  - [ ] macOS Terminal.app
  - [ ] iTerm2
  - [ ] Linux console (`TERM=linux`)
  - [ ] Windows Terminal
  - [ ] VS Code integrated terminal
  - [ ] tmux/screen
- [ ] Update `README.md` with icon configuration docs
- [ ] Update `AGENTS.md` if needed
- [ ] Add `[tui.icons]` section to example config

---

## Test Cases

### Unit Tests

```rust
#[test]
fn test_unicode_preset_all_single_width() {
    let icons = IconSet::unicode();
    assert_eq!(UnicodeWidthStr::width(icons.role_user.as_str()), 1);
    assert_eq!(UnicodeWidthStr::width(icons.role_assistant.as_str()), 1);
    // ... all icons should be 1 cell wide
}

#[test]
fn test_ascii_preset_all_ascii() {
    let icons = IconSet::ascii();
    assert!(icons.role_user.is_ascii());
    assert!(icons.role_assistant.is_ascii());
    // ... all should be pure ASCII
}

#[test]
fn test_theme_override_partial() {
    let config = IconsConfig {
        preset: "unicode".into(),
        theme: IconThemeConfig {
            role_user: Some("👤".into()),
            ..Default::default()
        },
    };
    let icons = IconSet::from_config(&config);
    assert_eq!(icons.role_user, "👤");
    assert_eq!(icons.role_assistant, "◆"); // fallback to preset
}

#[test]
fn test_detection_respects_env_override() {
    std::env::set_var("TARK_ICONS", "ascii");
    assert!(!detect_unicode_support());
    std::env::remove_var("TARK_ICONS");
}
```

### Manual Testing Matrix

| Terminal | Locale | Expected Preset | Test Status |
|----------|--------|-----------------|-------------|
| iTerm2 (macOS) | en_US.UTF-8 | unicode | ⬜ |
| Terminal.app (macOS) | en_US.UTF-8 | unicode | ⬜ |
| Windows Terminal | - | unicode | ⬜ |
| VS Code terminal | - | unicode | ⬜ |
| Linux console | C | ascii | ⬜ |
| tmux | en_US.UTF-8 | unicode | ⬜ |
| ssh (TERM=xterm-256color) | UTF-8 | unicode | ⬜ |

---

## Rollback Plan

If issues arise:
1. Set `TARK_ICONS=ascii` env var as immediate workaround
2. Or add `preset = "ascii"` to config.toml
3. Revert to hardcoded icons if fundamental issues found

---

## Future Enhancements

- [ ] Built-in theme presets ("nerd-font", "emoji", "minimal")
- [ ] Theme sharing/import from file
- [ ] Per-widget icon overrides
- [ ] Dynamic theme switching at runtime
- [ ] Icon preview in `/settings` command

---

## Dependencies

- `unicode-width` crate (already in Cargo.toml)
- No new dependencies required

---

## Success Criteria

1. ✅ Same visual experience on UTF-8 terminals
2. ✅ Graceful ASCII fallback on limited terminals
3. ✅ No layout/alignment issues with any preset
4. ✅ User can fully customize icons via config
5. ✅ Zero breaking changes for users (default behavior preserved)
6. ✅ All existing tests pass
