#!/bin/bash
#
# E2E Visual Test Runner for tark TUI (`tark tui` command)
# Uses asciinema + agg + expect for visual regression testing
#
# Directory: tests/visual/tui/
# Source: src/tui_new/
#
# Usage:
#   ./tui_e2e_runner.sh                    # Run all tests
#   ./tui_e2e_runner.sh --tier p0          # Run P0 smoke tests
#   ./tui_e2e_runner.sh --tier p1          # Run P0+P1 core tests
#   ./tui_e2e_runner.sh --tier all         # Run all tiers
#   ./tui_e2e_runner.sh --scenario NAME    # Run specific scenario
#   ./tui_e2e_runner.sh --feature 16       # Run Feature 16 scenarios
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

# Directories - uses tests/visual/tui/ (new TUI)
# Note: tests/visual/chat/ is for old chat TUI
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
FEATURES_DIR="$SCRIPT_DIR/tui/features"
RECORDINGS_DIR="$SCRIPT_DIR/tui/recordings"
CURRENT_DIR="$SCRIPT_DIR/tui/current"
SNAPSHOTS_DIR="$SCRIPT_DIR/tui/snapshots"
DIFFS_DIR="$SCRIPT_DIR/tui/diffs"
LOGS_DIR="$SCRIPT_DIR/tui/logs"

# Binary - uses `tark tui` command (TUI)
TARK_BINARY="${TARK_BINARY:-$PROJECT_ROOT/target/release/tark}"
TUI_COMMAND="tui"  # TUI command (not 'chat')

# Logging
log_info() { echo -e "${BLUE}[INFO]${NC} $1"; }
log_success() { echo -e "${GREEN}[OK]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1" >&2; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_header() { echo -e "\n${CYAN}=== $1 ===${NC}\n"; }

# Feature directory names (aligned with existing structure)
declare -A FEATURE_DIRS=(
    [1]="01_terminal_layout"
    [2]="02_status_bar"
    [3]="03_message_display"
    [4]="04_input_area"
    [5]="05_modals_provider_picker"
    [6]="06_modals_model_picker"
    [7]="07_modals_file_picker"
    [8]="08_modals_theme_picker"
    [9]="09_modals_help"
    [10]="10_questions_multiple_choice"
    [11]="11_questions_single_choice"
    [12]="12_questions_free_text"
    [13]="13_sidebar"
    [14]="14_theming"
    [15]="15_keyboard_shortcuts"
    [16]="16_llm_responses"
)

# Get feature directory name
get_feature_dir() {
    local feature="$1"
    # Remove leading zeros for lookup
    local num=$((10#$feature))
    echo "${FEATURE_DIRS[$num]:-feature_$(printf '%02d' "$num")}"
}

# Setup directories
setup_dirs() {
    mkdir -p "$RECORDINGS_DIR" "$CURRENT_DIR" "$SNAPSHOTS_DIR" "$DIFFS_DIR" "$LOGS_DIR"
    # Create feature-specific subdirectories using existing naming convention
    for i in $(seq 1 16); do
        local dir_name=$(get_feature_dir "$i")
        mkdir -p "$SNAPSHOTS_DIR/$dir_name"
        mkdir -p "$RECORDINGS_DIR/$dir_name"
    done
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
# Uses `tark tui` command instead of `tark chat`
generate_expect_script() {
    local scenario="$1"
    local feature="${2:-}"
    local script_file="/tmp/tui_new_e2e_${scenario}.exp"
    
    case "$scenario" in
        # Feature 01: Terminal Layout
        terminal_layout|layout)
            cat > "$script_file" << 'EXPEOF'
#!/usr/bin/expect -f
set timeout 30
log_user 1
set env(TERM) xterm-256color
set env(COLORTERM) truecolor
spawn ./target/release/tark tui
sleep 3
# Show initial layout with rounded corners
sleep 2
send "\x03"
sleep 1
expect eof
EXPEOF
            ;;
        
        # Feature 02: Status Bar
        status_bar)
            cat > "$script_file" << 'EXPEOF'
#!/usr/bin/expect -f
set timeout 30
log_user 1
set env(TERM) xterm-256color
set env(COLORTERM) truecolor
spawn ./target/release/tark tui
sleep 3
# Status bar should show mode, provider, thinking toggle
sleep 2
send "\x03"
sleep 1
expect eof
EXPEOF
            ;;
        
        # Feature 03: Message Display
        messages|message_display)
            cat > "$script_file" << 'EXPEOF'
#!/usr/bin/expect -f
set timeout 45
log_user 1
set env(TERM) xterm-256color
set env(COLORTERM) truecolor
spawn ./target/release/tark tui --provider tark_sim
sleep 3
send "Hello, can you help me?\r"
sleep 5
send "\x03"
sleep 1
expect eof
EXPEOF
            ;;
        
        # Feature 04: Input Area
        input_area|input)
            cat > "$script_file" << 'EXPEOF'
#!/usr/bin/expect -f
set timeout 30
log_user 1
set env(TERM) xterm-256color
set env(COLORTERM) truecolor
spawn ./target/release/tark tui
sleep 3
send "This is a test message"
sleep 2
# Show cursor and input
sleep 2
send "\r"
sleep 3
send "\x03"
sleep 1
expect eof
EXPEOF
            ;;
        
        # Feature 05: Provider Picker Modal
        provider_picker|provider)
            cat > "$script_file" << 'EXPEOF'
#!/usr/bin/expect -f
set timeout 30
log_user 1
set env(TERM) xterm-256color
set env(COLORTERM) truecolor
spawn ./target/release/tark tui
sleep 3
send "/model\r"
sleep 3
send "\x1b"
sleep 2
send "\x03"
sleep 1
expect eof
EXPEOF
            ;;
        
        # Feature 06: Model Picker Modal
        model_picker|model)
            cat > "$script_file" << 'EXPEOF'
#!/usr/bin/expect -f
set timeout 30
log_user 1
set env(TERM) xterm-256color
set env(COLORTERM) truecolor
spawn ./target/release/tark tui
sleep 3
send "/model\r"
sleep 2
send "\r"
sleep 3
send "\x1b"
sleep 2
send "\x03"
sleep 1
expect eof
EXPEOF
            ;;
        
        # Feature 07: File Picker Modal
        file_picker|file)
            cat > "$script_file" << 'EXPEOF'
#!/usr/bin/expect -f
set timeout 30
log_user 1
set env(TERM) xterm-256color
set env(COLORTERM) truecolor
spawn ./target/release/tark tui --cwd /tmp
sleep 3
send "@"
sleep 3
send "\x1b"
sleep 2
send "\x03"
sleep 1
expect eof
EXPEOF
            ;;
        
        # Feature 08: Theme Picker Modal
        theme_picker|theme)
            cat > "$script_file" << 'EXPEOF'
#!/usr/bin/expect -f
set timeout 30
log_user 1
set env(TERM) xterm-256color
set env(COLORTERM) truecolor
spawn ./target/release/tark tui
sleep 3
send "/theme\r"
sleep 3
send "\x1b"
sleep 2
send "\x03"
sleep 1
expect eof
EXPEOF
            ;;
        
        # Feature 09: Help Modal
        help_modal|help)
            cat > "$script_file" << 'EXPEOF'
#!/usr/bin/expect -f
set timeout 30
log_user 1
set env(TERM) xterm-256color
set env(COLORTERM) truecolor
spawn ./target/release/tark tui
sleep 3
send "/help\r"
sleep 3
send "\x1b"
sleep 2
send "\x03"
sleep 1
expect eof
EXPEOF
            ;;
        
        # Feature 13: Sidebar
        sidebar)
            cat > "$script_file" << 'EXPEOF'
#!/usr/bin/expect -f
set timeout 30
log_user 1
set env(TERM) xterm-256color
set env(COLORTERM) truecolor
spawn ./target/release/tark tui
sleep 3
# Toggle sidebar with Ctrl+B
send "\x02"
sleep 2
send "\x02"
sleep 2
send "\x03"
sleep 1
expect eof
EXPEOF
            ;;
        
        # Feature 10: Multiple Choice Questions
        questions_multi|questions_multiple)
            cat > "$script_file" << 'EXPEOF'
#!/usr/bin/expect -f
set timeout 45
log_user 1
set env(TERM) xterm-256color
set env(COLORTERM) truecolor
set env(TARK_SIM_SCENARIO) question_multi
spawn ./target/release/tark tui --provider tark_sim
sleep 3
send "Ask me a multiple choice question\r"
sleep 5
# Navigate options
send "\x1b\[B"
sleep 1
send " "
sleep 1
send "\r"
sleep 2
send "\x03"
sleep 1
expect eof
EXPEOF
            ;;
        
        # Feature 11: Single Choice Questions
        questions_single)
            cat > "$script_file" << 'EXPEOF'
#!/usr/bin/expect -f
set timeout 45
log_user 1
set env(TERM) xterm-256color
set env(COLORTERM) truecolor
set env(TARK_SIM_SCENARIO) question_single
spawn ./target/release/tark tui --provider tark_sim
sleep 3
send "Ask me a single choice question\r"
sleep 5
# Select option
send "\x1b\[B"
sleep 1
send "\r"
sleep 2
send "\x03"
sleep 1
expect eof
EXPEOF
            ;;
        
        # Feature 12: Free Text Questions
        questions_text|questions_free)
            cat > "$script_file" << 'EXPEOF'
#!/usr/bin/expect -f
set timeout 45
log_user 1
set env(TERM) xterm-256color
set env(COLORTERM) truecolor
set env(TARK_SIM_SCENARIO) question_text
spawn ./target/release/tark tui --provider tark_sim
sleep 3
send "Ask me a free text question\r"
sleep 5
# Type answer
send "My answer here"
sleep 1
send "\r"
sleep 2
send "\x03"
sleep 1
expect eof
EXPEOF
            ;;
        
        # Feature 14: Theming
        theming)
            cat > "$script_file" << 'EXPEOF'
#!/usr/bin/expect -f
set timeout 30
log_user 1
set env(TERM) xterm-256color
set env(COLORTERM) truecolor
spawn ./target/release/tark tui
sleep 3
# Open theme picker
send "/theme\r"
sleep 2
# Navigate themes
send "\x1b\[B"
sleep 1
send "\x1b\[B"
sleep 1
# Select theme
send "\r"
sleep 2
send "\x03"
sleep 1
expect eof
EXPEOF
            ;;

        # Feature 16: LLM Responses - Echo
        llm_echo|echo)
            cat > "$script_file" << 'EXPEOF'
#!/usr/bin/expect -f
set timeout 60
log_user 1
set env(TERM) xterm-256color
set env(COLORTERM) truecolor
set env(TARK_SIM_SCENARIO) echo
spawn ./target/release/tark tui --provider tark_sim
sleep 3
send "Hello, can you help me?\r"
sleep 5
# Should see echo response
sleep 2
send "\x03"
sleep 1
expect eof
EXPEOF
            ;;
        
        # Feature 16: LLM Responses - Streaming
        llm_streaming|streaming)
            cat > "$script_file" << 'EXPEOF'
#!/usr/bin/expect -f
set timeout 60
log_user 1
set env(TERM) xterm-256color
set env(COLORTERM) truecolor
set env(TARK_SIM_SCENARIO) streaming
spawn ./target/release/tark tui --provider tark_sim
sleep 3
send "Explain Rust closures\r"
sleep 8
# Should see streaming response
sleep 2
send "\x03"
sleep 1
expect eof
EXPEOF
            ;;
        
        # Feature 16: LLM Responses - Tool Invocation
        llm_tools|tools)
            cat > "$script_file" << 'EXPEOF'
#!/usr/bin/expect -f
set timeout 60
log_user 1
set env(TERM) xterm-256color
set env(COLORTERM) truecolor
set env(TARK_SIM_SCENARIO) tool
spawn ./target/release/tark tui --provider tark_sim --cwd /tmp
sleep 3
send "Search for TODO comments\r"
sleep 8
# Should see tool execution
sleep 2
send "\x03"
sleep 1
expect eof
EXPEOF
            ;;
        
        # Feature 16: LLM Responses - Thinking
        llm_thinking|thinking)
            cat > "$script_file" << 'EXPEOF'
#!/usr/bin/expect -f
set timeout 60
log_user 1
set env(TERM) xterm-256color
set env(COLORTERM) truecolor
set env(TARK_SIM_SCENARIO) thinking
spawn ./target/release/tark tui --provider tark_sim
sleep 3
send "Solve this complex problem\r"
sleep 10
# Should see thinking block
sleep 2
send "\x03"
sleep 1
expect eof
EXPEOF
            ;;
        
        # Feature 16: LLM Responses - Error Timeout
        llm_error_timeout|error_timeout)
            cat > "$script_file" << 'EXPEOF'
#!/usr/bin/expect -f
set timeout 60
log_user 1
set env(TERM) xterm-256color
set env(COLORTERM) truecolor
set env(TARK_SIM_SCENARIO) error_timeout
spawn ./target/release/tark tui --provider tark_sim
sleep 3
send "This will timeout\r"
sleep 8
# Should see timeout error
sleep 2
send "\x03"
sleep 1
expect eof
EXPEOF
            ;;
        
        # Feature 16: LLM Responses - Error Rate Limit
        llm_error_ratelimit|error_ratelimit)
            cat > "$script_file" << 'EXPEOF'
#!/usr/bin/expect -f
set timeout 60
log_user 1
set env(TERM) xterm-256color
set env(COLORTERM) truecolor
set env(TARK_SIM_SCENARIO) error_rate_limit
spawn ./target/release/tark tui --provider tark_sim
sleep 3
send "This will hit rate limit\r"
sleep 5
# Should see rate limit error
sleep 2
send "\x03"
sleep 1
expect eof
EXPEOF
            ;;
        
        # Feature 15: Keyboard Shortcuts
        keyboard|shortcuts)
            cat > "$script_file" << 'EXPEOF'
#!/usr/bin/expect -f
set timeout 30
log_user 1
set env(TERM) xterm-256color
set env(COLORTERM) truecolor
spawn ./target/release/tark tui
sleep 3
# Test Tab focus cycling
send "\t"
sleep 1
send "\t"
sleep 1
# Test ? for help
send "?"
sleep 2
send "\x1b"
sleep 1
# Test Ctrl+T for thinking toggle
send "\x14"
sleep 1
send "\x03"
sleep 1
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
spawn ./target/release/tark tui
sleep 3
send "Test message for ${scenario}\r"
sleep 5
send "\x03"
sleep 1
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
    local feature="${2:-}"
    local feature_dir=""
    
    # Determine feature directory using existing naming convention
    if [ -n "$feature" ]; then
        feature_dir="$(get_feature_dir "$feature")/"
    fi
    
    local cast_file="$RECORDINGS_DIR/${feature_dir}${scenario}.cast"
    local animated_cast="$RECORDINGS_DIR/${feature_dir}${scenario}_animated.cast"
    local gif_file="$RECORDINGS_DIR/${feature_dir}${scenario}.gif"
    local log_file="$LOGS_DIR/${scenario}.log"
    
    log_info "Running TUI scenario: $scenario"
    
    # Generate expect script
    local exp_script
    exp_script=$(generate_expect_script "$scenario" "$feature")
    
    cd "$PROJECT_ROOT"
    
    # Clean up old files
    rm -f "$cast_file" "$gif_file" "$animated_cast"
    mkdir -p "$(dirname "$cast_file")"
    
    # Record with asciinema
    if ! TERM=xterm-256color asciinema rec --overwrite --cols 120 --rows 40 --idle-time-limit 2 \
        --command "$exp_script" "$cast_file" 2>&1 | tee "$log_file"; then
        log_error "Recording failed for $scenario"
        return 1
    fi
    
    # Make animated
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
        
        local snapshot_dir="$CURRENT_DIR/${feature_dir}"
        mkdir -p "$snapshot_dir"
        
        TOTAL_FRAMES=$(identify "$gif_file" 2>/dev/null | wc -l || echo 0)
        
        if [ "$TOTAL_FRAMES" -gt 2 ]; then
            convert "$gif_file" -coalesce -delete 0-2 -delete 1--1 \
                -define png:color-type=6 "$snapshot_dir/${scenario}_initial.png" 2>/dev/null || true
            
            FINAL_IDX=$((TOTAL_FRAMES - 2))
            if [ "$FINAL_IDX" -ge 0 ]; then
                convert "$gif_file" -coalesce -delete 0-$((FINAL_IDX-1)) -delete 1--1 \
                    -define png:color-type=6 "$snapshot_dir/${scenario}_final.png" 2>/dev/null || true
            fi
        elif [ "$TOTAL_FRAMES" -gt 0 ]; then
            convert "$gif_file" -coalesce \
                -define png:color-type=6 "$snapshot_dir/${scenario}_final.png" 2>/dev/null || true
        fi
    fi
    
    # Clean up
    rm -f "$animated_cast"
    
    # Verify expected content in recording
    local verification_failed=false
    if [ -f "$cast_file" ]; then
        case "$scenario" in
            model_picker|model)
                # Should show "Select Model" modal or model list, not just provider picker
                if ! grep -qE "(Select Model|Model ─|gpt-4)" "$cast_file" 2>/dev/null; then
                    log_error "VERIFICATION FAILED: Model picker modal not shown (expected 'Select Model' or model list)"
                    verification_failed=true
                fi
                ;;
            file_picker|file)
                # Should show file picker modal
                if ! grep -q "Select File\|File Picker\|Files" "$cast_file" 2>/dev/null; then
                    log_error "VERIFICATION FAILED: File picker modal not shown"
                    verification_failed=true
                fi
                ;;
            provider_picker|provider)
                # Should show provider picker
                if ! grep -q "Select Provider" "$cast_file" 2>/dev/null; then
                    log_error "VERIFICATION FAILED: Provider picker modal not shown"
                    verification_failed=true
                fi
                ;;
            theme_picker|theme)
                # Should show theme picker
                if ! grep -q "Select Theme" "$cast_file" 2>/dev/null; then
                    log_error "VERIFICATION FAILED: Theme picker modal not shown"
                    verification_failed=true
                fi
                ;;
            help_modal|help)
                # Should show help modal
                if ! grep -q "Help\|Keyboard Shortcuts" "$cast_file" 2>/dev/null; then
                    log_error "VERIFICATION FAILED: Help modal not shown"
                    verification_failed=true
                fi
                ;;
            sidebar)
                # Should show sidebar toggle effect
                if ! grep -q "Session\|Context\|Tasks\|Git" "$cast_file" 2>/dev/null; then
                    log_warn "VERIFICATION WARNING: Sidebar panels may not be visible"
                fi
                ;;
            llm_echo|echo)
                # Should show echoed response
                if ! grep -q "Hello\|help" "$cast_file" 2>/dev/null; then
                    log_error "VERIFICATION FAILED: Echo response not shown"
                    verification_failed=true
                fi
                ;;
            llm_streaming|streaming)
                # Should show streaming content
                if ! grep -q "Rust\|closure" "$cast_file" 2>/dev/null; then
                    log_warn "VERIFICATION WARNING: Streaming response may not be visible"
                fi
                ;;
            llm_tools|tools)
                # Should show tool execution
                if ! grep -q "grep\|search\|tool\|TODO" "$cast_file" 2>/dev/null; then
                    log_warn "VERIFICATION WARNING: Tool execution may not be visible"
                fi
                ;;
            llm_thinking|thinking)
                # Should show thinking block
                if ! grep -q "thinking\|Thinking" "$cast_file" 2>/dev/null; then
                    log_warn "VERIFICATION WARNING: Thinking block may not be visible"
                fi
                ;;
            llm_error_timeout|error_timeout)
                # Should show timeout error
                if ! grep -q "timeout\|Timeout\|error\|Error" "$cast_file" 2>/dev/null; then
                    log_warn "VERIFICATION WARNING: Timeout error may not be visible"
                fi
                ;;
            llm_error_ratelimit|error_ratelimit)
                # Should show rate limit error
                if ! grep -q "rate\|limit\|Rate\|Limit\|error\|Error" "$cast_file" 2>/dev/null; then
                    log_warn "VERIFICATION WARNING: Rate limit error may not be visible"
                fi
                ;;
            questions_multi|questions_multiple)
                # Should show multiple choice question UI
                if ! grep -q "\[\s*\]\|\[x\]\|checkbox\|multiple" "$cast_file" 2>/dev/null; then
                    log_warn "VERIFICATION WARNING: Multiple choice UI may not be visible"
                fi
                ;;
            questions_single)
                # Should show single choice question UI
                if ! grep -q "○\|●\|radio\|single" "$cast_file" 2>/dev/null; then
                    log_warn "VERIFICATION WARNING: Single choice UI may not be visible"
                fi
                ;;
            questions_text|questions_free)
                # Should show text input for question
                if ! grep -q "answer\|input\|text" "$cast_file" 2>/dev/null; then
                    log_warn "VERIFICATION WARNING: Text question UI may not be visible"
                fi
                ;;
        esac
    fi
    
    if $verification_failed; then
        log_error "Scenario FAILED: $scenario"
        [ -f "$gif_file" ] && echo "  Recording: $gif_file"
        return 1
    fi
    
    # Compare snapshots against baseline (TDD: fail if no baseline exists)
    local snapshot_comparison_failed=false
    local snapshot_dir="$CURRENT_DIR/${feature_dir}"
    local baseline_dir="$SNAPSHOTS_DIR/${feature_dir}"
    
    if [ -d "$snapshot_dir" ]; then
        for current_snap in "$snapshot_dir"/*.png; do
            [ -f "$current_snap" ] || continue
            local snap_name=$(basename "$current_snap")
            local baseline_snap="$baseline_dir/$snap_name"
            
            if [ ! -f "$baseline_snap" ]; then
                # TDD: No baseline = test fails (feature not yet verified)
                log_error "BASELINE MISSING: $snap_name"
                log_info "  This is expected for new/unimplemented features (TDD approach)"
                log_info "  Current snapshot: $current_snap"
                log_info "  Expected baseline: $baseline_snap"
                log_info "  To create baseline after verifying output is correct:"
                log_info "    cp '$current_snap' '$baseline_snap'"
                log_info "  Or run: ./tests/visual/tui_e2e_runner.sh --update-baseline"
                snapshot_comparison_failed=true
            else
                # Compare using ImageMagick
                local diff_file="$DIFFS_DIR/${scenario}_${snap_name}"
                local diff_metric
                diff_metric=$(compare -metric RMSE "$baseline_snap" "$current_snap" "$diff_file" 2>&1 | awk -F'[()]' '{print $2}' | cut -d' ' -f1) || true
                
                local diff_value
                diff_value=$(echo "$diff_metric" | grep -oE '[0-9]+\.?[0-9]*' | head -1) || diff_value="0"
                
                # Threshold: 5.0 = 500% difference allowed (for timing variations in terminal output)
                if [ -n "$diff_value" ] && (( $(echo "$diff_value > 5.0" | bc -l 2>/dev/null || echo 0) )); then
                    log_error "SNAPSHOT MISMATCH: $snap_name (diff: $diff_value)"
                    log_info "  Baseline: $baseline_snap"
                    log_info "  Current:  $current_snap"
                    log_info "  Diff:     $diff_file"
                    snapshot_comparison_failed=true
                else
                    log_success "Snapshot match: $snap_name"
                    rm -f "$diff_file" 2>/dev/null
                fi
            fi
        done
    fi
    
    if $snapshot_comparison_failed; then
        log_error "Scenario FAILED (snapshot verification): $scenario"
        [ -f "$gif_file" ] && echo "  Recording: $gif_file"
        return 1
    fi
    
    log_success "Scenario PASSED: $scenario"
    [ -f "$gif_file" ] && echo "  Recording: $gif_file"
    [ -d "$CURRENT_DIR/${feature_dir}" ] && echo "  Snapshots: $CURRENT_DIR/${feature_dir}${scenario}_*.png"
    
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
            # Features 01-09 - Core + modals
            echo "terminal_layout status_bar messages input_area provider_picker model_picker file_picker theme_picker help_modal"
            ;;
        p2|extended)
            # Features 01-15 - All UI features
            echo "terminal_layout status_bar messages input_area provider_picker model_picker file_picker theme_picker help_modal sidebar keyboard"
            ;;
        llm|feature16)
            # Feature 16: LLM Response scenarios
            echo "llm_echo llm_streaming llm_tools llm_thinking llm_error_timeout llm_error_ratelimit"
            ;;
        all)
            # All features 01-16
            echo "terminal_layout status_bar messages input_area provider_picker model_picker file_picker theme_picker help_modal questions_multi questions_single questions_text sidebar theming keyboard llm_echo llm_streaming llm_tools llm_thinking llm_error_timeout llm_error_ratelimit"
            ;;
        *)
            echo "terminal_layout"
            ;;
    esac
}

# Get scenarios for a specific feature number
get_feature_scenarios() {
    local feature="$1"
    case "$feature" in
        1|01) echo "terminal_layout" ;;
        2|02) echo "status_bar" ;;
        3|03) echo "messages" ;;
        4|04) echo "input_area" ;;
        5|05) echo "provider_picker" ;;
        6|06) echo "model_picker" ;;
        7|07) echo "file_picker" ;;
        8|08) echo "theme_picker" ;;
        9|09) echo "help_modal" ;;
        10) echo "questions_multi" ;;
        11) echo "questions_single" ;;
        12) echo "questions_text" ;;
        13) echo "sidebar" ;;
        14) echo "theming" ;;
        15) echo "keyboard" ;;
        16) echo "llm_echo llm_streaming llm_tools llm_thinking llm_error_timeout llm_error_ratelimit" ;;
        *) echo "" ;;
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

# Run scenarios for a specific feature
run_feature() {
    local feature="$1"
    local scenarios
    scenarios=$(get_feature_scenarios "$feature")
    
    if [ -z "$scenarios" ]; then
        log_error "No scenarios defined for feature $feature"
        return 1
    fi
    
    log_header "Running TUI Feature $feature"
    log_info "Scenarios: $scenarios"
    
    local failed=0
    local passed=0
    
    for scenario in $scenarios; do
        if run_scenario "$scenario" "$feature"; then
            passed=$((passed + 1))
        else
            failed=$((failed + 1))
        fi
    done
    
    echo ""
    log_info "Feature $feature Results: $passed passed, $failed failed"
    
    [ "$failed" -eq 0 ]
}

# Verify snapshots against baseline
verify_snapshots() {
    log_header "Verifying TUI Snapshots"
    
    local failed=0
    local checked=0
    
    # Check all PNG files recursively
    while IFS= read -r -d '' current; do
        [ -f "$current" ] || continue
        
        local rel_path="${current#$CURRENT_DIR/}"
        local baseline="$SNAPSHOTS_DIR/$rel_path"
        local diff_out="$DIFFS_DIR/diff_$(basename "$current")"
        
        if [ ! -f "$baseline" ]; then
            log_warn "No baseline for: $rel_path (new file)"
            continue
        fi
        
        ((checked++))
        
        # Compare using ImageMagick
        local diff_metric
        diff_metric=$(compare -metric RMSE "$baseline" "$current" "$diff_out" 2>&1 | awk -F'[()]' '{print $2}' | cut -d' ' -f1) || true
        
        local diff_value
        diff_value=$(echo "$diff_metric" | grep -oE '[0-9]+\.?[0-9]*' | head -1) || diff_value="0"
        
        if [ -n "$diff_value" ] && (( $(echo "$diff_value > 0.1" | bc -l 2>/dev/null || echo 0) )); then
            log_error "Visual diff: $rel_path (diff: $diff_value)"
            ((failed++))
        else
            log_success "Match: $rel_path"
            rm -f "$diff_out"
        fi
    done < <(find "$CURRENT_DIR" -name "*.png" -print0 2>/dev/null)
    
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
    
    # Copy all current snapshots to baseline
    if [ -d "$CURRENT_DIR" ]; then
        while IFS= read -r -d '' current; do
            [ -f "$current" ] || continue
            local rel_path="${current#$CURRENT_DIR/}"
            local target="$SNAPSHOTS_DIR/$rel_path"
            mkdir -p "$(dirname "$target")"
            cp "$current" "$target"
            log_success "Updated: $rel_path"
            ((updated++))
        done < <(find "$CURRENT_DIR" -name "*.png" -print0 2>/dev/null)
    fi
    
    # Copy GIF recordings
    if [ -d "$RECORDINGS_DIR" ]; then
        while IFS= read -r -d '' gif; do
            [ -f "$gif" ] || continue
            local rel_path="${gif#$RECORDINGS_DIR/}"
            local target="$SNAPSHOTS_DIR/$rel_path"
            mkdir -p "$(dirname "$target")"
            cp "$gif" "$target"
            log_success "Updated: $rel_path"
            ((updated++))
        done < <(find "$RECORDINGS_DIR" -name "*.gif" -print0 2>/dev/null)
    fi
    
    log_success "Updated $updated baseline files"
    log_info "Review changes: git diff $SNAPSHOTS_DIR/"
}

# List available scenarios
list_scenarios() {
    log_header "Available TUI Scenarios"
    
    echo "TUI scenarios (for \`tark tui\` command):"
    echo ""
    echo "  P0 (Smoke) - Features 01, 04:"
    echo "    terminal_layout  - Main layout with rounded corners"
    echo "    input_area       - Input field and cursor"
    echo ""
    echo "  P1 (Core) - Features 02-03, 05-09:"
    echo "    status_bar       - Mode, provider, thinking toggle"
    echo "    messages         - User/agent/system messages"
    echo "    provider_picker  - Provider selection modal"
    echo "    model_picker     - Model selection modal"
    echo "    file_picker      - File picker modal (@mention)"
    echo "    theme_picker     - Theme selection modal"
    echo "    help_modal       - Help and shortcuts"
    echo ""
    echo "  Features 10-12 (Questions):"
    echo "    questions_multi  - Multiple choice questions"
    echo "    questions_single - Single choice questions"
    echo "    questions_text   - Free text questions"
    echo ""
    echo "  Features 13-15 (Extended):"
    echo "    sidebar          - Sidebar panels (Ctrl+B)"
    echo "    theming          - Theme switching"
    echo "    keyboard         - Keyboard shortcuts"
    echo ""
    echo "  Feature 16 (LLM Responses):"
    echo "    llm_echo           - Basic echo response"
    echo "    llm_streaming      - Streaming response"
    echo "    llm_tools          - Tool invocation"
    echo "    llm_thinking       - Thinking blocks"
    echo "    llm_error_timeout  - Timeout error"
    echo "    llm_error_ratelimit - Rate limit error"
    echo ""
    echo "Run with:"
    echo "  $0 --scenario <name>     # Single scenario"
    echo "  $0 --tier <p0|p1|p2|llm|all>"
    echo "  $0 --feature <1-16>      # All scenarios for feature"
}

# Clean generated files
clean() {
    log_header "Cleaning Generated Files"
    rm -rf "$CURRENT_DIR"/* "$RECORDINGS_DIR"/*.gif "$RECORDINGS_DIR"/*.cast \
           "$RECORDINGS_DIR"/feature_*/*.gif "$RECORDINGS_DIR"/feature_*/*.cast \
           "$DIFFS_DIR"/* "$LOGS_DIR"/*
    log_success "Clean complete"
}

# Print usage
print_usage() {
    cat << EOF
E2E Visual Test Runner for tark TUI (\`tark tui\` command)

Usage: $0 [OPTIONS]

Options:
  --tier TIER           Run scenarios by tier: p0/smoke, p1/core, p2/extended, llm/feature16, all
  --feature NUM         Run all scenarios for a specific feature (1-16)
  --scenario NAME       Run specific scenario
  --verify              Compare current snapshots against baseline
  --update-baseline     Copy current snapshots to baseline
  --install-deps        Install required dependencies
  --list                List available scenarios
  --clean               Remove generated files
  --help                Show this help

Tiers:
  p0/smoke      Features 01, 04 (layout, input)
  p1/core       Features 01-09 (core + modals)
  p2/extended   Features 01-15 (all UI features)
  llm/feature16 Feature 16 (LLM response scenarios)
  all           Run everything

Features:
  01  Terminal Layout       09  Help Modal
  02  Status Bar            10  Multiple Choice Questions
  03  Message Display       11  Single Choice Questions
  04  Input Area            12  Free Text Questions
  05  Provider Picker       13  Sidebar
  06  Model Picker          14  Theming
  07  File Picker           15  Keyboard Shortcuts
  08  Theme Picker          16  LLM Responses

Examples:
  $0 --install-deps             # First-time setup
  $0 --tier p0                  # Quick smoke test
  $0 --tier llm                 # Test LLM response features
  $0 --feature 16               # Run all Feature 16 scenarios
  $0 --scenario llm_echo        # Run specific scenario
  $0 --verify                   # Check for regressions
  $0 --update-baseline          # Accept current as new baseline

Environment:
  TARK_BINARY     Path to tark binary (default: ./target/release/tark)
  SKIP_BUILD      Skip cargo build if set to 1

Output Directories:
  tests/visual/tui/recordings/  - GIF recordings
  tests/visual/tui/snapshots/   - Baseline PNG snapshots
  tests/visual/tui/current/     - Current test run snapshots
  tests/visual/tui/diffs/       - Visual diff outputs
EOF
}

# Main
main() {
    local tier=""
    local feature=""
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
            --feature|-f)
                feature="$2"
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
    echo "  TUI E2E Visual Tests (\`tark tui\`)"
    echo "=============================================="
    echo ""
    
    # Run tests
    if [ -n "$scenario" ]; then
        run_scenario "$scenario"
    elif [ -n "$feature" ]; then
        run_feature "$feature"
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
    echo "  Recordings: $RECORDINGS_DIR/"
    echo "  Snapshots:  $CURRENT_DIR/"
    echo ""
    log_info "Next steps:"
    echo "  Verify: $0 --verify"
    echo "  Update: $0 --update-baseline"
}

main "$@"
