#!/bin/bash
# tests/visual/compare_snapshots.sh
set -e

COMMAND="${1:-chat}"
BASELINE_DIR="tests/visual/${COMMAND}/snapshots"
CURRENT_DIR="tests/visual/${COMMAND}/current"
DIFF_DIR="tests/visual/${COMMAND}/diffs"

mkdir -p "$DIFF_DIR"

echo "=== Comparing Visual Snapshots ==="
echo "Command: $COMMAND"
echo "Baseline: $BASELINE_DIR"
echo "Current: $CURRENT_DIR"
echo ""

# Check if baseline directory exists
if [ ! -d "$BASELINE_DIR" ]; then
    echo "WARNING: No baseline directory found at $BASELINE_DIR"
    echo "Run tests first, then copy current/ to snapshots/ to create baseline."
    exit 0
fi

# Check if current directory exists
if [ ! -d "$CURRENT_DIR" ]; then
    echo "ERROR: No current snapshots found at $CURRENT_DIR"
    echo "Run tests first with: ./tests/visual/run_visual_tests.sh --command $COMMAND"
    exit 1
fi

passed=0
failed=0
missing=0

for baseline in "$BASELINE_DIR"/*.png; do
  [ -f "$baseline" ] || continue
  name=$(basename "$baseline")
  current="$CURRENT_DIR/$name"
  diff="$DIFF_DIR/$name"
  
  if [ -f "$current" ]; then
    # Compare images, output diff pixel count
    result=$(compare -metric AE "$baseline" "$current" "$diff" 2>&1 || true)
    if [ "$result" -gt 100 ]; then  # Allow small pixel differences
      echo "FAIL: $name differs by $result pixels"
      ((failed++))
    else
      echo "PASS: $name ($result pixels diff)"
      ((passed++))
    fi
  else
    echo "WARN: Missing current snapshot for $name"
    ((missing++))
  fi
done

echo ""
echo "Results: $passed passed, $failed failed, $missing missing"
echo ""

if [ "$failed" -gt 0 ]; then
    echo "Visual regression detected!"
    echo "Review diff images in: $DIFF_DIR"
    echo ""
    echo "If changes are intentional, update baseline with:"
    echo "  cp -r $CURRENT_DIR/* $BASELINE_DIR/"
    exit 1
fi

if [ "$missing" -gt 0 ]; then
    echo "Missing current snapshots!"
    exit 1
fi

echo "All visual tests passed!"
