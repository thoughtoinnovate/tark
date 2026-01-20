#!/bin/bash
# Test script to debug mouse events in TUI

# Build the project
echo "Building tark..."
cargo build --release || exit 1

# Create debug log directory
mkdir -p /tmp/tark-debug

# Run with debug logging enabled
echo "Starting TUI with debug logging..."
echo "Scroll events will be logged to: /tmp/tark-debug/tark-mouse-debug.log"
echo "Try scrolling with your touchpad and mouse wheel"
echo ""
echo "Press Ctrl+Q to quit"
echo ""

RUST_LOG=debug,tark=debug \
RUST_BACKTRACE=1 \
./target/release/tark tui 2>&1 | tee /tmp/tark-debug/tark-mouse-debug.log
