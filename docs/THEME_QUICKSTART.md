# Theme System - Quick Start Guide

## What You Can Do Now

### ✅ 1. Live Theme Preview (IMPLEMENTED)
When you navigate through themes with Up/Down arrows, you see **instant preview** - the entire TUI changes colors immediately!

```
/theme      # Open theme picker
↓           # Preview "Nord" - entire UI changes to Nord colors
↓           # Preview "Dracula" - UI changes to Dracula colors
Enter       # Keep Dracula
```

or

```
/theme      # Open theme picker
↓↓↓         # Preview multiple themes
Escape      # Don't like any? Press Escape, returns to original
```

### ✅ 2. Search & Filter Themes (IMPLEMENTED)
Type to filter themes by name:

```
/theme      # Open theme picker
cat         # Type "cat" - only shows "Catppuccin Mocha"
↓           # If multiple matches, navigate through them
Enter       # Apply
```

or

```
/theme      # Open theme picker
dark        # Shows: GitHub Dark, Gruvbox Dark, One Dark
↓↓          # Navigate filtered list
Enter       # Apply
```

### ✅ 3. Visual Indicators
- **✓** (green) = Your saved theme
- **(previewing)** (yellow) = Currently previewing this theme
- **▸** (cyan) = Selected item

## How It Works

### Live Preview
```rust
// When you press Up/Down in theme picker:
1. Theme changes immediately (preview_theme())
2. UI re-renders with new colors
3. You see the full theme in action

// When you press Enter:
- Theme is saved permanently (apply_theme())

// When you press Escape:
- Original theme is restored
- Preview is discarded
```

### Search Filter
```rust
// As you type:
1. Filter updates in real-time
2. Theme list is filtered
3. Selection resets to first match
4. You can still navigate filtered results

// Example:
"tokyo" → Shows only Tokyo Night
"o" → Shows One Dark, Tokyo Night (both have "o")
```

## All 7 Built-in Themes

| Theme | Preview | Use Case |
|-------|---------|----------|
| **Catppuccin Mocha** | Soft pastels on dark navy | Default, easy on eyes |
| **Nord** | Cool blues and cyans | Arctic, minimal |
| **Dracula** | Purple, pink, vibrant | High contrast |
| **GitHub Dark** | Professional grays | Clean, corporate |
| **One Dark** | Balanced colors | Popular, familiar |
| **Gruvbox Dark** | Warm retro colors | Vintage feel |
| **Tokyo Night** | Storm blues | Modern, sleek |

## Quick Examples

### Example 1: Try Dracula
```
/theme          # Open
dracula         # Type "dracula"
Enter           # Instantly applied!
```

### Example 2: Browse All Themes
```
/theme          # Open
↓↓↓↓↓↓         # Press Down 6 times, see all 7 themes
Escape          # Don't like any? Back to original
```

### Example 3: Find "Dark" Themes
```
/theme          # Open
dark            # Type "dark"
                # Shows: GitHub Dark, Gruvbox Dark, One Dark
↓               # Preview GitHub Dark
↓               # Preview Gruvbox Dark
↓               # Preview One Dark
Enter           # Apply One Dark
```

## Adding Custom Themes

### Method 1: TOML Config File (Easiest)

Create `~/.config/tark/themes/mytheme.toml`:

```toml
[theme]
name = "My Theme"

[theme.colors]
bg_main = "#1a1b26"
bg_dark = "#16161e"
text_primary = "#c0caf5"
blue = "#7aa2f7"
green = "#9ece6a"
yellow = "#e0af68"
red = "#f7768e"
cyan = "#7dcfff"
purple = "#bb9af7"
# ... (see examples/tark-config/themes/custom.toml for full template)
```

Then use it:
```
/theme
mytheme         # Type your theme name
Enter           # Apply
```

### Method 2: From Neovim Colorscheme

**In Neovim:**
```lua
require('tark').export_theme()  -- Exports current colorscheme
```

This creates `~/.config/tark/themes/<colorscheme-name>.toml` automatically.

**Or manually:**
```vim
:colorscheme tokyonight
:lua require('tark').export_theme('tokyonight')
```

**Then in Tark:**
```
/theme
tokyonight      # Your exported theme appears
Enter           # Apply
```

### Method 3: Add Rust Preset (Advanced)

Edit `src/tui_new/theme.rs`:

```rust
// 1. Add to enum
pub enum ThemePreset {
    // ... existing
    MyTheme,
}

// 2. Implement colors
impl Theme {
    pub fn my_theme() -> Self {
        Self {
            bg_main: Color::Rgb(26, 27, 38),
            // ... all color fields
        }
    }
}

// 3. Update from_preset()
ThemePreset::MyTheme => Self::my_theme(),

// 4. Update display_name() and all()
```

## Live Preview Demo

Try this to see live preview in action:

```bash
# Start TUI
cargo run --features test-sim -- tui

# In TUI:
/theme          # Open theme picker
                # Current theme shown with ✓

# Now press Down repeatedly and watch:
↓               # → Nord appears (blues and cyans)
↓               # → Dracula appears (purples and pinks)
↓               # → GitHub Dark appears (professional grays)
↓               # → One Dark appears (balanced colors)
↓               # → Gruvbox Dark appears (warm retros)
↓               # → Tokyo Night appears (storm blues)

# Found one you like?
Enter           # Apply it!

# Changed your mind?
Escape          # Back to original theme
```

## Tips

1. **Use search** - Don't scroll through all themes, just type part of the name
2. **Preview freely** - Escape always returns to your original theme
3. **Match your editor** - Export your Neovim colorscheme for consistency
4. **Test with content** - Navigate while modal is open to see how themes look with your actual messages

## Keyboard Reference

| Key | Action |
|-----|--------|
| `/theme` | Open theme picker |
| `a-z` | Type to filter themes |
| `↑` | Previous theme + live preview |
| `↓` | Next theme + live preview |
| `Enter` | Apply theme permanently |
| `Escape` | Cancel and restore original |
| `Backspace` | Delete filter character |

## Architecture

```
AppState
├── theme_preset (saved theme)
├── theme (current visual theme)
├── theme_before_preview (for cancel)
├── theme_picker_filter (search text)
└── theme_picker_selected (current index)

ThemePicker Modal
├── Filters themes by search
├── Shows current theme (✓)
├── Shows preview state
└── Applies on Enter, cancels on Escape

Theme Preview Flow:
1. Open modal → Save current theme
2. Navigate → preview_theme() (temporary)
3. Enter → apply_theme() (permanent)
4. Escape → Restore saved theme
```

## See Also
- `docs/THEMES.md` - Full theme system documentation
- `examples/tark-config/themes/custom.toml` - Custom theme template
- `src/tui_new/theme.rs` - Theme implementation
