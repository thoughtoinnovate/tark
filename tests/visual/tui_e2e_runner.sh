#!/bin/bash
#
# E2E Visual Test Runner for tark TUI
# Uses asciinema + agg + expect for visual regression testing
#
# Usage:
#   ./tui_e2e_runner.sh                    # Run all tests
#   ./tui_e2e_runner.sh --tier p0          # Run P0 smoke tests
#   ./tui_e2e_runner.sh --tier p1          # Run P0+P1 core tests
#   ./tui_e2e_runner.sh --tier all         # Run all tiers
#   ./tui_e2e_runner.sh --scenario NAME    # Run specific scenario
#   ./tui_e2e_runner.sh --verify           # Compare against baseline
#   ./tui_e2e_runner.sh --update-baseline  # Update baseline snapshots
#   ./tui_e2e_runner.sh --install-deps     # Install dependencies
#   ./tui_e2e_runner.sh --list             # List available scenarios
#

set -euo pipefail

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m'

# Directories
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
FEATURES_DIR="$SCRIPT_DIR/tui/features"
RECORDINGS_DIR="$SCRIPT_DIR/tui/recordings"
CURRENT_DIR="$SCRIPT_DIR/tui/current"
SNAPSHOTS_DIR="$SCRIPT_DIR/tui/snapshots"
DIFFS_DIR="$SCRIPT_DIR/tui/diffs"
LOGS_DIR="$SCRIPT_DIR/tui/logs"
REFERENCE_DIR="$PROJECT_ROOT/web/ui/mocks/screenshots"

# Binary
TARK_BINARY="${TARK_BINARY:-$PROJECT_ROOT/target/release/tark}"

# Logging
log_info() { echo -e "${BLUE}[INFO]${NC} $1"; }
log_success() { echo -e "${GREEN}[OK]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1" >&2; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_header() { echo -e "\n${CYAN}=== $1 ===${NC}\n"; }

# Setup directories
setup_dirs() {
    mkdir -p "$RECORDINGS_DIR" "$CURRENT_DIR" "$SNAPSHOTS_DIR" "$DIFFS_DIR" "$LOGS_DIR"
}

# Check dependencies
check_deps() {
    local missing=()
    command -v asciinema >/dev/null 2>&1 || missing+=("asciinema")
    command -v expect >/dev/null 2>&1 || missing+=("expect")
    command -v convert >/dev/null 2>&1 || missing+=("imagemagick")
    
    # Check for agg in cargo bin
    if ! command -v agg >/dev/null 2>&1; then
        if [ -f "$HOME/.cargo/bin/agg" ]; then
            export PATH="$HOME/.cargo/bin:$PATH"
        else
            missing+=("agg")
        fi
    fi
    
    if [ ${#missing[@]} -gt 0 ]; then
        log_error "Missing dependencies: ${missing[*]}"
        log_info "Run: $0 --install-deps"
        return 1
    fi
    log_success "Dependencies OK"
}

# Install dependencies
install_deps() {
    log_header "Installing E2E Dependencies"
    
    # Detect OS
    if [[ "$OSTYPE" == "darwin"* ]]; then
        log_info "Detected macOS"
        brew install asciinema expect imagemagick || true
    elif [[ "$OSTYPE" == "linux-gnu"* ]]; then
        log_info "Detected Linux"
        if command -v apt-get >/dev/null 2>&1; then
            sudo apt-get update
            sudo apt-get install -y asciinema expect imagemagick bc \
                fonts-noto-color-emoji fonts-symbola fontconfig
            fc-cache -f 2>/dev/null || true
        elif command -v dnf >/dev/null 2>&1; then
            sudo dnf install -y asciinema expect ImageMagick bc \
                google-noto-emoji-fonts gdouros-symbola-fonts fontconfig
            fc-cache -f 2>/dev/null || true
        elif command -v pacman >/dev/null 2>&1; then
            sudo pacman -S --noconfirm asciinema expect imagemagick bc \
                noto-fonts-emoji ttf-symbola fontconfig
            fc-cache -f 2>/dev/null || true
        fi
    fi
    
    # Install agg via cargo
    if ! command -v agg >/dev/null 2>&1; then
        log_info "Installing agg (asciinema gif generator)..."
        cargo install --git https://github.com/asciinema/agg || {
            log_warn "Failed to install agg from git, trying crates.io..."
            cargo install agg || true
        }
    fi
    
    log_success "Dependencies installed"
}

# Build tark with test-sim feature
build_tark() {
    if [ "${SKIP_BUILD:-}" = "1" ]; then
        log_info "Skipping build (SKIP_BUILD=1)"
        return 0
    fi
    
    log_info "Building tark with test-sim feature..."
    cd "$PROJECT_ROOT"
    cargo build --release --features test-sim 2>&1 | tail -5
    log_success "Build complete: $TARK_BINARY"
}

# Generate expect script for a TUI scenario
generate_expect_script() {
    local scenario="$1"
    local script_file="/tmp/tui_e2e_${scenario}.exp"
    
    case "$scenario" in
        terminal_layout|layout)
            cat > "$script_file" << 'EXPEOF'
#!/usr/bin/expect -f
set timeout 30
log_user 1
set env(TERM) xterm-256color
set env(COLORTERM) truecolor
set env(TARK_FORCE_COLOR) 1
spawn ./target/release/tark chat --provider tark_sim --cwd /tmp
sleep 3
# Just show the initial layout
sleep 2
send "/exit\r"
sleep 2
expect eof
EXPEOF
            ;;
        status_bar)
            cat > "$script_file" << 'EXPEOF'
#!/usr/bin/expect -f
set timeout 30
log_user 1
set env(TERM) xterm-256color
set env(COLORTERM) truecolor
set env(TARK_FORCE_COLOR) 1
spawn ./target/release/tark chat --provider tark_sim --cwd /tmp
sleep 3
# Show status bar elements
sleep 2
send "/exit\r"
sleep 2
expect eof
EXPEOF
            ;;
        messages|message_display)
            cat > "$script_file" << 'EXPEOF'
#!/usr/bin/expect -f
set timeout 45
log_user 1
set env(TERM) xterm-256color
set env(COLORTERM) truecolor
set env(TARK_FORCE_COLOR) 1
spawn ./target/release/tark chat --provider tark_sim --cwd /tmp
sleep 3
send "Hello, can you help me?\r"
sleep 5
send "/exit\r"
sleep 2
expect eof
EXPEOF
            ;;
        input_area|input)
            cat > "$script_file" << 'EXPEOF'
#!/usr/bin/expect -f
set timeout 30
log_user 1
set env(TERM) xterm-256color
set env(COLORTERM) truecolor
set env(TARK_FORCE_COLOR) 1
spawn ./target/release/tark chat --provider tark_sim --cwd /tmp
sleep 3
send "This is a test message"
sleep 2
# Show input with text
sleep 2
send "\r"
sleep 3
send "/exit\r"
sleep 2
expect eof
EXPEOF
            ;;
        provider_picker|provider)
            cat > "$script_file" << 'EXPEOF'
#!/usr/bin/expect -f
set timeout 30
log_user 1
set env(TERM) xterm-256color
set env(COLORTERM) truecolor
set env(TARK_FORCE_COLOR) 1
spawn ./target/release/tark chat --provider tark_sim --cwd /tmp
sleep 3
send "/model\r"
sleep 3
send "\x1b"
sleep 2
send "/exit\r"
sleep 2
expect eof
EXPEOF
            ;;
        model_picker|model)
            cat > "$script_file" << 'EXPEOF'
#!/usr/bin/expect -f
set timeout 30
log_user 1
set env(TERM) xterm-256color
set env(COLORTERM) truecolor
set env(TARK_FORCE_COLOR) 1
spawn ./target/release/tark chat --provider tark_sim --cwd /tmp
sleep 3
send "/model\r"
sleep 2
send "\r"
sleep 3
send "\x1b"
sleep 2
send "/exit\r"
sleep 2
expect eof
EXPEOF
            ;;
        file_picker|file)
            cat > "$script_file" << 'EXPEOF'
#!/usr/bin/expect -f
set timeout 30
log_user 1
set env(TERM) xterm-256color
set env(COLORTERM) truecolor
set env(TARK_FORCE_COLOR) 1
spawn ./target/release/tark chat --provider tark_sim --cwd /tmp
sleep 3
send "@"
sleep 3
send "\x1b"
sleep 2
send "/exit\r"
sleep 2
expect eof
EXPEOF
            ;;
        theme_picker|theme)
            cat > "$script_file" << 'EXPEOF'
#!/usr/bin/expect -f
set timeout 30
log_user 1
set env(TERM) xterm-256color
set env(COLORTERM) truecolor
set env(TARK_FORCE_COLOR) 1
spawn ./target/release/tark chat --provider tark_sim --cwd /tmp
sleep 3
send "/theme\r"
sleep 3
send "\x1b"
sleep 2
send "/exit\r"
sleep 2
expect eof
EXPEOF
            ;;
        help_modal|help)
            cat > "$script_file" << 'EXPEOF'
#!/usr/bin/expect -f
set timeout 30
log_user 1
set env(TERM) xterm-256color
set env(COLORTERM) truecolor
set env(TARK_FORCE_COLOR) 1
spawn ./target/release/tark chat --provider tark_sim --cwd /tmp
sleep 3
send "/help\r"
sleep 3
send "\x1b"
sleep 2
send "/exit\r"
sleep 2
expect eof
EXPEOF
            ;;
        sidebar)
            cat > "$script_file" << 'EXPEOF'
#!/usr/bin/expect -f
set timeout 30
log_user 1
set env(TERM) xterm-256color
set env(COLORTERM) truecolor
set env(TARK_FORCE_COLOR) 1
spawn ./target/release/tark chat --provider tark_sim --cwd /tmp
sleep 3
# Toggle sidebar
send "\t"
sleep 2
send "/exit\r"
sleep 2
expect eof
EXPEOF
            ;;
        questions|question)
            cat > "$script_file" << 'EXPEOF'
#!/usr/bin/expect -f
set timeout 45
log_user 1
set env(TERM) xterm-256color
set env(COLORTERM) truecolor
set env(TARK_FORCE_COLOR) 1
spawn env TARK_SIM_SCENARIO=question ./target/release/tark chat --provider tark_sim --cwd /tmp
sleep 3
send "Ask me a question\r"
sleep 5
send "/exit\r"
sleep 2
expect eof
EXPEOF
            ;;
        thinking)
            cat > "$script_file" << 'EXPEOF'
#!/usr/bin/expect -f
set timeout 60
log_user 1
set env(TERM) xterm-256color
set env(COLORTERM) truecolor
set env(TARK_FORCE_COLOR) 1
spawn env TARK_SIM_SCENARIO=thinking ./target/release/tark chat --provider tark_sim --cwd /tmp
sleep 3
send "Think about this problem\r"
sleep 10
send "/exit\r"
sleep 2
expect eof
EXPEOF
            ;;
        keyboard|shortcuts)
            cat > "$script_file" << 'EXPEOF'
#!/usr/bin/expect -f
set timeout 30
log_user 1
set env(TERM) xterm-256color
set env(COLORTERM) truecolor
set env(TARK_FORCE_COLOR) 1
spawn ./target/release/tark chat --provider tark_sim --cwd /tmp
sleep 3
# Test various keyboard shortcuts
send "Hello\r"
sleep 3
send "\x1b"
sleep 1
send "j"
sleep 1
send "k"
sleep 1
send "/exit\r"
sleep 2
expect eof
EXPEOF
            ;;
        *)
            # Default: basic layout test
            cat > "$script_file" << EXPEOF
#!/usr/bin/expect -f
set timeout 30
log_user 1
set env(TERM) xterm-256color
set env(COLORTERM) truecolor
set env(TARK_FORCE_COLOR) 1
spawn ./target/release/tark chat --provider tark_sim --cwd /tmp
sleep 3
send "Test message for ${scenario}\r"
sleep 5
send "/exit\r"
sleep 2
expect eof
EXPEOF
            ;;
    esac
    
    chmod +x "$script_file"
    echo "$script_file"
}

# Run a single scenario
run_scenario() {
    local scenario="$1"
    local cast_file="$RECORDINGS_DIR/${scenario}.cast"
    local animated_cast="$RECORDINGS_DIR/${scenario}_animated.cast"
    local gif_file="$RECORDINGS_DIR/${scenario}.gif"
    local log_file="$LOGS_DIR/${scenario}.log"
    
    log_info "Running TUI scenario: $scenario"
    
    # Generate expect script
    local exp_script
    exp_script=$(generate_expect_script "$scenario")
    
    # Record with asciinema
    cd "$PROJECT_ROOT"
    
    # Clean up old files
    rm -f "$cast_file" "$gif_file" "$animated_cast"
    
    # Record with proper terminal type for colors
    if ! TERM=xterm-256color asciinema rec --overwrite --cols 120 --rows 40 --idle-time-limit 2 \
        --command "$exp_script" "$cast_file" 2>&1 | tee "$log_file"; then
        log_error "Recording failed for $scenario"
        return 1
    fi
    
    # Make animated (spread out timestamps for better GIF)
    if [ -f "$SCRIPT_DIR/make_animated.py" ]; then
        python3 "$SCRIPT_DIR/make_animated.py" "$cast_file" "$animated_cast" 2>/dev/null || {
            log_warn "Animation script failed, using raw cast"
            cp "$cast_file" "$animated_cast"
        }
    else
        cp "$cast_file" "$animated_cast"
    fi
    
    # Convert to GIF
    log_info "Converting to GIF..."
    AGG_FONT_FAMILY="DejaVu Sans Mono,Symbola,Noto Color Emoji"
    if command -v agg >/dev/null 2>&1; then
        agg --font-size 14 --cols 120 --rows 40 --theme monokai \
            --font-family "$AGG_FONT_FAMILY" \
            "$animated_cast" "$gif_file" 2>&1 || {
            log_warn "agg failed, GIF not generated"
        }
    elif [ -f "$HOME/.cargo/bin/agg" ]; then
        "$HOME/.cargo/bin/agg" --font-size 14 --cols 120 --rows 40 --theme monokai \
            --font-family "$AGG_FONT_FAMILY" \
            "$animated_cast" "$gif_file" 2>&1 || {
            log_warn "agg failed, GIF not generated"
        }
    else
        log_warn "agg not found, skipping GIF generation"
    fi
    
    # Extract snapshots
    if [ -f "$gif_file" ] && command -v convert >/dev/null 2>&1; then
        log_info "Extracting snapshots..."
        
        TOTAL_FRAMES=$(identify "$gif_file" 2>/dev/null | wc -l || echo 0)
        
        if [ "$TOTAL_FRAMES" -le 0 ]; then
            log_warn "GIF has no frames, skipping snapshot extraction"
        elif [ "$TOTAL_FRAMES" -eq 1 ]; then
            convert "$gif_file" -coalesce \
                -define png:color-type=6 "$CURRENT_DIR/${scenario}_initial.png" 2>/dev/null || true
            convert "$gif_file" -coalesce \
                -define png:color-type=6 "$CURRENT_DIR/${scenario}_final.png" 2>/dev/null || true
        elif [ "$TOTAL_FRAMES" -eq 2 ]; then
            convert "$gif_file" -coalesce -delete 1--1 \
                -define png:color-type=6 "$CURRENT_DIR/${scenario}_initial.png" 2>/dev/null || true
            convert "$gif_file" -coalesce -delete 0-0 \
                -define png:color-type=6 "$CURRENT_DIR/${scenario}_final.png" 2>/dev/null || true
        else
            convert "$gif_file" -coalesce -delete 0-2 -delete 1--1 \
                -define png:color-type=6 "$CURRENT_DIR/${scenario}_initial.png" 2>/dev/null || true
            
            FINAL_IDX=$((TOTAL_FRAMES - 2))
            if [ "$FINAL_IDX" -ge 0 ]; then
                convert "$gif_file" -coalesce -delete 0-$((FINAL_IDX-1)) -delete 1--1 \
                    -define png:color-type=6 "$CURRENT_DIR/${scenario}_final.png" 2>/dev/null || true
            fi
        fi
    fi
    
    # Clean up intermediate files
    rm -f "$animated_cast"
    
    log_success "Scenario complete: $scenario"
    [ -f "$gif_file" ] && echo "  Recording: $gif_file"
    [ -f "$CURRENT_DIR/${scenario}_initial.png" ] && echo "  Snapshots: $CURRENT_DIR/${scenario}_*.png"
    
    return 0
}

# Get scenarios for a tier
get_tier_scenarios() {
    local tier="$1"
    case "$tier" in
        p0|smoke)
            # Features 01, 04 - Core layout
            echo "terminal_layout input_area"
            ;;
        p1|core)
            # Features 01-04 + 05-12 - Core + modals + questions
            echo "terminal_layout status_bar messages input_area provider_picker model_picker file_picker theme_picker help_modal questions"
            ;;
        p2|extended)
            # All features including sidebar, theming, keyboard
            echo "terminal_layout status_bar messages input_area provider_picker model_picker file_picker theme_picker help_modal questions sidebar thinking keyboard"
            ;;
        all)
            echo "terminal_layout status_bar messages input_area provider_picker model_picker file_picker theme_picker help_modal questions sidebar thinking keyboard"
            ;;
        *)
            echo "terminal_layout"
            ;;
    esac
}

# Run scenarios by tier
run_tier() {
    local tier="$1"
    local scenarios
    scenarios=$(get_tier_scenarios "$tier")
    
    log_header "Running TUI Tier: $tier"
    log_info "Scenarios: $scenarios"
    
    local failed=0
    local passed=0
    
    for scenario in $scenarios; do
        if run_scenario "$scenario"; then
            passed=$((passed + 1))
        else
            failed=$((failed + 1))
        fi
    done
    
    echo ""
    log_info "Results: $passed passed, $failed failed"
    
    if [ "$failed" -eq 0 ]; then
        log_success "All scenarios passed!"
        return 0
    else
        log_error "$failed scenario(s) failed"
        return 1
    fi
}

# Verify snapshots against baseline
verify_snapshots() {
    log_header "Verifying TUI Snapshots"
    
    local failed=0
    local checked=0
    
    for current in "$CURRENT_DIR"/*.png; do
        [ -f "$current" ] || continue
        
        local name=$(basename "$current")
        local baseline="$SNAPSHOTS_DIR/$name"
        local diff_out="$DIFFS_DIR/diff_$name"
        
        if [ ! -f "$baseline" ]; then
            log_warn "No baseline for: $name (new file)"
            continue
        fi
        
        ((checked++))
        
        # Compare using ImageMagick
        local diff_metric
        diff_metric=$(compare -metric RMSE "$baseline" "$current" "$diff_out" 2>&1 | awk -F'[()]' '{print $2}' | cut -d' ' -f1) || true
        
        local diff_value
        diff_value=$(echo "$diff_metric" | grep -oE '[0-9]+\.?[0-9]*' | head -1) || diff_value="0"
        
        if [ -n "$diff_value" ] && (( $(echo "$diff_value > 0.1" | bc -l 2>/dev/null || echo 0) )); then
            log_error "Visual diff: $name (diff: $diff_value)"
            ((failed++))
        else
            log_success "Match: $name"
            rm -f "$diff_out"
        fi
    done
    
    echo ""
    log_info "Checked $checked files, $failed failures"
    
    if [ $failed -gt 0 ]; then
        log_error "Visual regression detected! Check diffs in: $DIFFS_DIR"
        return 1
    fi
    
    log_success "All snapshots match baseline"
}

# Update baseline from current
update_baseline() {
    log_header "Updating TUI Baseline Snapshots"
    
    local updated=0
    
    for current in "$CURRENT_DIR"/*.png; do
        [ -f "$current" ] || continue
        cp "$current" "$SNAPSHOTS_DIR/"
        log_success "Updated: $(basename "$current")"
        ((updated++))
    done
    
    for gif in "$RECORDINGS_DIR"/*.gif; do
        [ -f "$gif" ] || continue
        cp "$gif" "$SNAPSHOTS_DIR/"
        log_success "Updated: $(basename "$gif")"
        ((updated++))
    done
    
    log_success "Updated $updated baseline files"
    log_info "Review changes: git diff $SNAPSHOTS_DIR/"
}

# List available scenarios
list_scenarios() {
    log_header "Available TUI Scenarios"
    
    echo "TUI scenarios (mapped from BDD feature files):"
    echo ""
    echo "  P0 (Smoke) - Features 01, 04:"
    echo "    terminal_layout  - Main layout structure"
    echo "    input_area       - Input field and submission"
    echo ""
    echo "  P1 (Core) - Features 02-03, 05-12:"
    echo "    status_bar       - Status bar elements"
    echo "    messages         - Message display types"
    echo "    provider_picker  - Provider selection modal"
    echo "    model_picker     - Model selection modal"
    echo "    file_picker      - File picker modal"
    echo "    theme_picker     - Theme selection modal"
    echo "    help_modal       - Help and shortcuts modal"
    echo "    questions        - Interactive questions"
    echo ""
    echo "  P2 (Extended) - Features 13-15:"
    echo "    sidebar          - Sidebar panels"
    echo "    thinking         - Thinking blocks"
    echo "    keyboard         - Keyboard shortcuts"
    echo ""
    echo "Run with: $0 --scenario <name>"
    echo "Run tier: $0 --tier <p0|p1|p2|all>"
}

# Clean generated files
clean() {
    log_header "Cleaning Generated Files"
    rm -rf "$CURRENT_DIR"/* "$RECORDINGS_DIR"/*.gif "$RECORDINGS_DIR"/*.cast "$DIFFS_DIR"/* "$LOGS_DIR"/*
    log_success "Clean complete"
}

# Print usage
print_usage() {
    cat << EOF
E2E Visual Test Runner for tark TUI

Usage: $0 [OPTIONS]

Options:
  --tier TIER           Run scenarios by tier: p0/smoke, p1/core, p2/extended, all
  --scenario NAME       Run specific scenario
  --verify              Compare current snapshots against baseline
  --update-baseline     Copy current snapshots to baseline
  --install-deps        Install required dependencies
  --list                List available scenarios
  --clean               Remove generated files
  --help                Show this help

Tiers (mapped to feature files):
  p0/smoke      Features 01, 04 (terminal_layout, input_area)
  p1/core       Features 01-12 (core + modals + questions)
  p2/extended   Features 01-15 (all features)
  all           Run everything

Examples:
  $0 --install-deps             # First-time setup
  $0 --tier p0                  # Quick smoke test
  $0 --tier p1                  # Run core tests for PR
  $0 --scenario terminal_layout # Run specific scenario
  $0 --verify                   # Check for regressions
  $0 --update-baseline          # Accept current as new baseline

Environment:
  TARK_BINARY     Path to tark binary (default: ./target/release/tark)
  SKIP_BUILD      Skip cargo build if set to 1
EOF
}

# Main
main() {
    local tier=""
    local scenario=""
    local do_verify=false
    local do_update=false
    local do_install=false
    local do_list=false
    local do_clean=false
    
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --tier|-t)
                tier="$2"
                shift 2
                ;;
            --scenario|-s)
                scenario="$2"
                shift 2
                ;;
            --verify|-v)
                do_verify=true
                shift
                ;;
            --update-baseline|-u)
                do_update=true
                shift
                ;;
            --install-deps|-i)
                do_install=true
                shift
                ;;
            --list|-l)
                do_list=true
                shift
                ;;
            --clean|-c)
                do_clean=true
                shift
                ;;
            --help|-h)
                print_usage
                exit 0
                ;;
            *)
                log_error "Unknown option: $1"
                print_usage
                exit 1
                ;;
        esac
    done
    
    # Handle special commands
    if $do_install; then
        install_deps
        exit 0
    fi
    
    if $do_list; then
        list_scenarios
        exit 0
    fi
    
    if $do_clean; then
        clean
        exit 0
    fi
    
    if $do_update; then
        update_baseline
        exit 0
    fi
    
    if $do_verify; then
        verify_snapshots
        exit $?
    fi
    
    # Setup and check deps
    setup_dirs
    check_deps || exit 1
    build_tark
    
    echo ""
    echo "=============================================="
    echo "  TUI E2E Visual Tests (asciinema + agg)"
    echo "=============================================="
    echo ""
    
    # Run tests
    if [ -n "$scenario" ]; then
        run_scenario "$scenario"
    elif [ -n "$tier" ]; then
        run_tier "$tier"
    else
        # Default: run P0 tier
        run_tier "p0"
    fi
    
    echo ""
    log_success "Test run complete!"
    echo ""
    log_info "Outputs:"
    echo "  Recordings: $RECORDINGS_DIR/*.gif"
    echo "  Snapshots:  $CURRENT_DIR/*.png"
    echo ""
    log_info "Next steps:"
    echo "  Verify: $0 --verify"
    echo "  Update: $0 --update-baseline"
}

main "$@"
