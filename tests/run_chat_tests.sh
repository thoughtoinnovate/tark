#!/bin/bash
# Quick script to run chat tests with provider display verification

set -e

echo "🧪 Running tark chat tests..."
echo ""

# Run tests
nvim --headless -u tests/minimal_init.lua \
  -c "PlenaryBustedFile tests/specs/chat_spec.lua" 2>&1

EXIT_CODE=$?

if [ $EXIT_CODE -eq 0 ]; then
    echo ""
    echo "✅ All chat tests passed!"
    echo ""
    echo "Manual verification steps:"
    echo "1. nvim"
    echo "2. :TarkChat"
    echo "3. /model"
    echo "4. Select Google → Pick gemini-1.5-flash"
    echo "5. Verify title shows 'Google' not 'Openai'"
else
    echo ""
    echo "❌ Tests failed with exit code $EXIT_CODE"
    exit $EXIT_CODE
fi

