#!/bin/bash
# Script to run Tark TUI with proper terminal setup

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

echo "=== Tark TUI Runner ==="
echo ""

# Check if we have a TTY
if ! tty -s; then
    echo "❌ ERROR: No TTY detected"
    echo ""
    echo "The TUI requires a real terminal. You are running in a non-TTY environment."
    echo ""
    echo "Solutions:"
    echo ""
    echo "1. If SSH'd, connect with -t flag:"
    echo "   ssh -t user@host"
    echo ""
    echo "2. Use non-TUI chat mode instead:"
    echo "   ./target/release/tark chat"
    echo ""
    echo "3. Use script to create a pseudo-terminal:"
    echo "   script -c './target/release/tark tui' /dev/null"
    echo ""
    exit 1
fi

# Check API key
if [ -z "$OPENAI_API_KEY" ]; then
    echo "⚠️  WARNING: OPENAI_API_KEY not set"
    echo ""
    echo "Set your API key:"
    echo "   export OPENAI_API_KEY='sk-your-key-here'"
    echo ""
    echo "Or use a different provider:"
    echo "   ./target/release/tark tui --provider gemini  # Requires GEMINI_API_KEY"
    echo "   ./target/release/tark tui --provider ollama  # Requires Ollama running"
    echo ""
    read -p "Continue anyway? (y/N) " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        exit 1
    fi
fi

# Check terminal size
COLS=$(tput cols 2>/dev/null || echo "80")
LINES=$(tput lines 2>/dev/null || echo "24")

echo "Terminal size: ${COLS}x${LINES}"

if [ "$COLS" -lt 100 ]; then
    echo "⚠️  WARNING: Terminal width is $COLS columns"
    echo "   Recommended: At least 100 columns for sidebar visibility"
    echo "   Sidebar will be hidden at <= 80 columns"
    echo ""
fi

if [ "$LINES" -lt 30 ]; then
    echo "⚠️  WARNING: Terminal height is $LINES rows"
    echo "   Recommended: At least 30 rows for full UI"
    echo ""
fi

# Check if binary exists
if [ ! -f "target/release/tark" ]; then
    echo "Building tark..."
    cargo build --release
fi

echo ""
echo "Starting Tark TUI..."
echo "Provider filter: OpenAI and Gemini only (configure in ~/.config/tark/config.toml)"
echo ""
echo "Press Ctrl+C to exit"
echo "Press ? for help"
echo ""
sleep 1

# Run with debug logging
RUST_LOG=info,tark=debug ./target/release/tark tui --debug "$@"
