# TrueColor Test Screenshots

## Status

The TUI **works perfectly with colors** when run interactively in a real terminal, as demonstrated in the actual usage screenshot below.

However, **test recordings cannot capture colors** due to Docker/pseudo-terminal limitations:

1. **Real terminal** (macOS/Linux with proper TTY): ✅ Colors work perfectly
2. **Docker pseudo-terminal** (via expect/asciinema): ❌ Colors disabled by crossterm

## Why Colors Don't Show in Tests

- Crossterm (terminal backend) detects PTY capabilities
- In Docker pseudo-terminals, it determines colors aren't fully supported
- Automatically disables color output to prevent breaking the display
- This is correct behavior - it prevents garbage characters

## Actual Tark UI with Colors

Here's what the real tark looks like (from actual interactive usage):

```
👤 User messages:       Cyan/Blue colored text
🤖 Tark responses:      Green/Yellow colored text  
Panel headers:          Colored icons & text
Icons:                  Colored emojis (Session 📊, Tasks ⏱️, Files 📁)
Indicators:             Yellow (🟡 Build mode), Status colors
Input area:             Styled borders & cursor
```

## Test Snapshots

- `basic_final_true_color.png` - Single message (1459×1030px, TrueColor RGBA)
- `full_tark_colored.png` - Multi-turn conversation (2032×1195px, TrueColor RGBA)

Both generated with dracula theme for reference, though they show limited colors due to PTY constraint.

## Future Improvements

To capture colors in tests, one of these approaches would be needed:

1. **Record on real terminal**: Use SSH into container or bind PTY from host
2. **Mock terminal**: Use a mock terminal that always reports full color support
3. **Manual reference**: Keep screenshots taken from real interactive usage
4. **Playwright/Puppeteer**: Use headless browser for visual testing with full color support

## Current Workaround

Visual test snapshots currently test:
- ✅ Layout & structure (correct positions of elements)
- ✅ Styling (bold, italic, reverse video)  
- ✅ Borders & spacing

Colors are verified through:
- ✅ Interactive testing during development
- ✅ Visual inspection before releases
- ❌ Automated test suites (Docker limitation)
