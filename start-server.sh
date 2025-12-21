#!/bin/bash
# Start tark HTTP server for Neovim ghost text completions
# Tark (तर्क) = Logic/Reasoning in Sanskrit

# Use Ollama with ibm/granite4 model by default
export OLLAMA_MODEL="${OLLAMA_MODEL:-ibm/granite4}"

# Make sure Ollama is running
if ! curl -s http://localhost:11434/api/tags > /dev/null 2>&1; then
    echo "Starting Ollama server..."
    ollama serve > /dev/null 2>&1 &
    sleep 2
fi

echo "Starting tark HTTP server on port 8765..."
echo "Using model: $OLLAMA_MODEL"
echo ""
echo "Press Ctrl+C to stop"
echo ""

# Start the server
exec "$(dirname "$0")/target/release/tark" serve --port 8765

