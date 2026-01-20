#!/bin/bash
# Test script for debug logging feature

set -e

echo "Testing Debug Logging Feature..."
echo "================================"
echo

# Check if binary exists
if [ ! -f "./target/release/tark" ]; then
    echo "Error: Binary not found. Run 'cargo build --release' first."
    exit 1
fi

# Create a test directory
TEST_DIR=$(mktemp -d)
cd "$TEST_DIR"
echo "Test directory: $TEST_DIR"
echo

# Initialize git repo (required for tark)
git init -q
git config user.name "Test User"
git config user.email "test@example.com"

# Create a simple test file
echo "fn main() { println!(\"Hello\"); }" > test.rs

# Note: This test would require an actual LLM provider configured
# For now, we just verify the binary starts with --debug flag
echo "1. Testing that --debug flag is recognized..."
timeout 2s /home/dev/data/work/code/tark/target/release/tark tui --debug 2>&1 | grep -q "DEBUG MODE ENABLED" && echo "✓ Debug mode flag recognized" || echo "✗ Debug mode flag not recognized"

echo
echo "2. Checking if debug directory is created..."
if [ -d ".tark/debug" ]; then
    echo "✓ .tark/debug directory created"
    ls -la .tark/debug/
else
    echo "Note: .tark/debug directory not created (requires actual TUI run)"
fi

echo
echo "3. Verifying binary compiled successfully..."
/home/dev/data/work/code/tark/target/release/tark --version
echo "✓ Binary works"

echo
echo "Test completed. To fully test debug logging:"
echo "  1. Set up an LLM provider (e.g., export OPENAI_API_KEY=...)"
echo "  2. Run: tark tui --debug"
echo "  3. Send a message that triggers tool use"
echo "  4. Check: cat .tark/debug/tark-debug.log | jq ."
echo "  5. Verify correlation_id links all related events"
echo
echo "Cleanup test directory: rm -rf $TEST_DIR"
