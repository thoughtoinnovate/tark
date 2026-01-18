# Theme System Documentation

The Tark TUI includes a powerful theme system with live preview, search, and support for custom themes.

## Features

### 1. Live Theme Preview
- Navigate through themes with **Up/Down arrows** to see instant preview
- Changes apply immediately as you navigate
- Press **Enter** to confirm, **Escape** to cancel and restore original theme

### 2. Theme Search & Filter
- Type in the theme picker to filter themes by name
- Example: Type "cat" to show only "Catppuccin Mocha"
- Filter is case-insensitive and matches partial names

### 3. Visual Indicators
- **✓** (green checkmark) - Your currently saved theme
- **(previewing)** (yellow) - Theme you're currently previewing
- **▸** (cyan arrow) - Selected item in list

## Using the Theme Picker

### Open Theme Picker
```
/theme          # Type in chat input
Ctrl+T Ctrl+H   # Keyboard shortcut (if configured)
```

### Navigate & Preview
```
Type            # Filter themes (e.g., "dark", "cat", "nord")
↑/↓ arrows      # Navigate and live preview themes
Enter           # Apply selected theme permanently
Escape          # Cancel and restore original theme
```

### Example Workflow
```
1. Type: /theme
2. See list of all themes with current theme marked (✓)
3. Type "dr" to filter → Shows "Dracula"
4. Press Down → Instantly see Dracula theme applied
5. Press Down again → See GitHub Dark
6. Press Enter → Confirm GitHub Dark as your theme
   OR Press Escape → Go back to your original theme
```

## Built-in Themes

| Theme | Description | Best For |
|-------|-------------|----------|
| **Catppuccin Mocha** | Pastel dark theme (default) | Modern, soft colors |
| **Nord** | Arctic-inspired blue/cyan | Cool, minimal aesthetic |
| **Dracula** | Purple and pink dark | High contrast, vibrant |
| **GitHub Dark** | GitHub's official dark | Professional, clean |
| **One Dark** | Atom editor classic | Popular, balanced |
| **Gruvbox Dark** | Retro groove colors | Warm, vintage feel |
| **Tokyo Night** | Storm-inspired dark | Cool blues, modern |

## Adding Custom Themes

### Method 1: Add a Preset (Rust)

1. Add variant to `ThemePreset` enum in `src/tui_new/theme.rs`:

```rust
pub enum ThemePreset {
    // ... existing themes
    MyCustomTheme,
}
```

2. Implement the theme:

```rust
impl Theme {
    pub fn my_custom_theme() -> Self {
        Self {
            bg_main: Color::Rgb(30, 30, 46),
            bg_dark: Color::Rgb(24, 24, 37),
            // ... define all colors
        }
    }
}
```

3. Update `from_preset()` match:

```rust
ThemePreset::MyCustomTheme => Self::my_custom_theme(),
```

4. Add to `display_name()` and `all()` methods

### Method 2: Load from Config File

Create `~/.config/tark/themes/mytheme.toml`:

```toml
[theme]
name = "My Custom Theme"

[theme.colors]
bg_main = "#1e1e2e"
bg_dark = "#181825"
text_primary = "#cdd6f4"
blue = "#89b4fa"
green = "#a6e3a1"
# ... etc (see examples/tark-config/themes/custom.toml)
```

Set in config:

```toml
[ui]
theme = "mytheme"  # Filename without .toml
```

### Method 3: Load from Neovim Colorscheme

The TUI can extract colors from any Neovim colorscheme:

**Option A: Via Lua Plugin**

```lua
-- In your Neovim config
require('tark').export_theme('tokyonight')  -- Export current colorscheme
```

This creates `~/.config/tark/themes/tokyonight.toml` from Neovim highlight groups.

**Option B: Manual Export**

```vim
" In Neovim
:lua vim.api.nvim_exec_lua('return vim.api.nvim_get_hl(0, {})', {})
```

Copy the highlight groups and convert to TOML format.

## Theme Mapping from Neovim

The following Neovim highlight groups map to TUI theme colors:

| Neovim Group | TUI Theme Field | Usage |
|--------------|-----------------|-------|
| `Normal` | `bg_main`, `text_primary` | Base background and text |
| `Comment` | `text_muted`, `border` | Dimmed text and borders |
| `Constant` | `green`, `agent_bubble` | Constants and agent messages |
| `Identifier` | `blue`, `user_bubble` | Identifiers and user messages |
| `Statement` | `purple` | Keywords |
| `Type` | `yellow`, `thinking_fg` | Types and thinking blocks |
| `Special` | `cyan`, `system_fg` | Special chars and system |
| `Error` | `red` | Errors and warnings |

## Programmatic Theme Access

### From Rust

```rust
use tark::tui_new::theme::{Theme, ThemePreset};

// Load a preset
let theme = Theme::from_preset(ThemePreset::Dracula);

// Parse from string (CLI/config)
if let Some(preset) = ThemePreset::from_str("nord") {
    let theme = Theme::from_preset(preset);
}

// Load from Neovim highlights
let highlights = get_nvim_highlights();  // Your implementation
let theme = Theme::from_nvim_highlights(&highlights);
```

### From Neovim Lua

```lua
-- Set theme
vim.fn['tark#set_theme']('dracula')

-- Get current theme
local theme = vim.fn['tark#get_theme']()

-- Export Neovim colorscheme to Tark
vim.fn['tark#export_colorscheme']('tokyonight')
```

## Theme Configuration

### In tark config.toml

```toml
[ui]
# Use a built-in theme
theme = "catppuccin-mocha"

# Or use a custom theme file
theme = "mytheme"  # Loads from ~/.config/tark/themes/mytheme.toml
```

### In Neovim

```lua
require('tark').setup({
    theme = 'nord',
    -- Or sync with Neovim colorscheme
    theme = 'nvim',  -- Auto-sync with vim.g.colors_name
})
```

## Color Format

Themes support multiple color formats:

```toml
[theme.colors]
# Hex format (preferred)
blue = "#89b4fa"

# RGB format
green = "rgb(166, 227, 161)"

# Named colors
red = "red"

# Neovim hl group (auto-resolved)
text_primary = "Normal.fg"
bg_main = "Normal.bg"
```

## Theme Development Tips

1. **Start from existing theme** - Copy a preset and modify colors
2. **Test with different content** - Ensure colors work for all message types
3. **Check contrast** - Text should be readable on all backgrounds
4. **Preview live** - Use the theme picker's live preview feature
5. **Match your editor** - Extract colors from your Neovim colorscheme for consistency

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `/theme` | Open theme picker |
| `Type` | Filter themes |
| `↑/↓` | Navigate & live preview |
| `Enter` | Apply theme |
| `Escape` | Cancel preview |
| `Ctrl+T T` | Cycle to next theme (global) |

## Examples

### Example 1: Quick Theme Switch

```
/theme      # Open picker
dr          # Type "dr" to filter to Dracula
Down        # Preview Dracula
Enter       # Apply Dracula
```

### Example 2: Browse All Themes

```
/theme      # Open picker
Down Down   # Preview Nord, then Dracula
Down Down   # Preview GitHub Dark, then One Dark
Escape      # Cancel, restore original theme
```

### Example 3: Find Specific Theme

```
/theme      # Open picker
tokyo       # Type "tokyo" to filter
Enter       # Only Tokyo Night shown, apply it
```

## Advanced: Custom Theme from Neovim

1. **Set your colorscheme in Neovim:**
```vim
:colorscheme tokyonight
```

2. **Export to Tark:**
```lua
require('tark').export_theme()
```

3. **Use in Tark:**
```
/theme
tokyonight  # Filter to your exported theme
Enter       # Apply it
```

The theme will persist in `~/.config/tark/themes/tokyonight.toml`.

## Troubleshooting

**Theme doesn't apply:**
- Check theme file syntax (valid TOML)
- Ensure all required color fields are defined
- Check file permissions

**Colors look wrong:**
- Verify hex codes (must be #RRGGBB format)
- Test in different terminal emulators
- Some terminals don't support RGB colors

**Live preview not working:**
- Ensure you're in the theme picker modal
- Try pressing Up/Down arrows explicitly
- Check that modal is focused

## See Also

- `examples/tark-config/themes/custom.toml` - Custom theme template
- `src/tui_new/theme.rs` - Theme implementation
- `README.md` - General configuration guide
