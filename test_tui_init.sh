#!/bin/bash
# Diagnostic script for TUI initialization issues

echo "=== TUI Initialization Diagnostic ==="
echo ""

echo "1. Checking OPENAI_API_KEY..."
if [ -z "$OPENAI_API_KEY" ]; then
    echo "   ❌ OPENAI_API_KEY is NOT set"
    exit 1
else
    echo "   ✅ OPENAI_API_KEY is set (${#OPENAI_API_KEY} characters)"
fi
echo ""

echo "2. Checking working directory..."
pwd
ls -la .tark/ 2>&1 | head -5
echo ""

echo "3. Checking tark binary..."
if [ -f "target/release/tark" ]; then
    echo "   ✅ Binary exists at target/release/tark"
else
    echo "   ❌ Binary not found - run: cargo build --release"
    exit 1
fi
echo ""

echo "4. Testing OpenAI provider directly..."
cat > /tmp/test_openai.rs << 'EOF'
use std::env;

fn main() {
    match env::var("OPENAI_API_KEY") {
        Ok(key) => {
            println!("✅ API key found: {}... ({} chars)", 
                     &key[..key.len().min(15)], 
                     key.len());
        }
        Err(_) => {
            println!("❌ OPENAI_API_KEY not found in environment");
            std::process::exit(1);
        }
    }
}
EOF

rustc /tmp/test_openai.rs -o /tmp/test_openai 2>/dev/null
/tmp/test_openai
rm -f /tmp/test_openai /tmp/test_openai.rs
echo ""

echo "5. Running TUI with verbose logging (will exit after 5 seconds)..."
echo "   Command: RUST_LOG=info,tark=debug ./target/release/tark tui --debug"
echo ""

timeout 5 env RUST_LOG=info,tark=debug ./target/release/tark tui --debug 2>&1 | grep -E "provider|LLM|AgentBridge|Failed|Error" | head -20

echo ""
echo "6. Checking for debug log..."
if [ -f "tark-debug.log" ]; then
    echo "   ✅ Debug log created"
    echo "   Last 10 lines:"
    tail -10 tark-debug.log
else
    echo "   ❌ No debug log found at $(pwd)/tark-debug.log"
fi

echo ""
echo "=== Diagnostic Complete ==="
