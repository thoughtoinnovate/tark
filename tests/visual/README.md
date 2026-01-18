# Visual E2E Test Framework

End-to-end visual testing for tark using asciinema recordings.

## Quick Start

```bash
# First-time setup (installs all dependencies)
./e2e_runner.sh --install-deps

# Run smoke tests (P0 - fastest)
./e2e_runner.sh --tier p0

# Run core tests (P1 - for PRs)
./e2e_runner.sh --tier p1

# Run specific scenario
./e2e_runner.sh --scenario basic
```

## How It Works

1. **Records** terminal sessions using asciinema + expect scripts
2. **Converts** recordings to GIF using agg with proper fonts
3. **Extracts** PNG snapshots at key frames
4. **Compares** against baseline snapshots for regression detection

## Test Tiers

| Tier | Name | Purpose | When to Run |
|------|------|---------|-------------|
| P0 | Smoke | Quick sanity check | Every commit |
| P1 | Core | Main functionality | PRs |
| P2 | Extended | Edge cases | Release |

## Commands

```bash
# List available scenarios
./e2e_runner.sh --list

# Run and verify against baselines
./e2e_runner.sh --tier p0
./e2e_runner.sh --verify

# Update baselines after intentional changes
./e2e_runner.sh --update-baseline

# Clean generated files
./e2e_runner.sh --clean
```

## Directory Structure

```
tests/visual/
├── e2e_runner.sh           # Main test runner
├── compare_snapshots.sh    # Snapshot comparison tool
├── make_animated.py        # Cast animation tool
├── chat/
│   ├── features/           # BDD scenarios (.feature files)
│   ├── snapshots/          # Baseline PNGs (tracked in git)
│   ├── current/            # Generated PNGs (gitignored)
│   ├── recordings/         # GIFs (gitignored, except demo)
│   └── diffs/              # Visual diffs (gitignored)
├── tui/
│   └── features/           # TUI-specific scenarios
└── logs/                   # Test logs (gitignored)
```

## Requirements

### Local Development

```bash
# Debian/Ubuntu
sudo apt-get install -y asciinema expect imagemagick bc \
  fonts-noto-color-emoji fonts-symbola fontconfig
cargo install agg

# macOS
brew install asciinema expect imagemagick
cargo install agg
```

### CI (GitHub Actions)

The CI workflow automatically installs all dependencies. See `.github/workflows/ci.yml`.

## Writing Tests

### Feature Files (BDD)

Feature files define test scenarios in Gherkin syntax:

```gherkin
Feature: Basic Chat
  Scenario: User sends a message
    Given tark is running with tark_sim provider
    When I send "Hello"
    Then I see a response from tark_sim
```

### Adding a New Scenario

1. Create/update feature file in `chat/features/`
2. Add scenario to `e2e_runner.sh` (see `run_scenario()` function)
3. Run the test: `./e2e_runner.sh --scenario your_scenario`
4. Verify output: `./e2e_runner.sh --verify`
5. Update baselines: `./e2e_runner.sh --update-baseline`

## Color & Icon Support

The framework supports full color and icon rendering:

- **Colors**: Enabled via `TARK_FORCE_COLOR=1` environment variable
- **Icons**: Rendered using Symbola and Noto Color Emoji fonts

### Fonts Used

| Font | Purpose |
|------|---------|
| DejaVu Sans Mono | Primary text |
| Symbola | Unicode symbols (◆, ◇, ▶, ✓) |
| Noto Color Emoji | Emoji (⚠️, 🔧, 🟡) |

## Troubleshooting

### No colors in recordings

Ensure `TARK_FORCE_COLOR=1` is set. The e2e_runner.sh sets this automatically.

### Missing icons

Install font packages:
```bash
sudo apt-get install fonts-noto-color-emoji fonts-symbola
fc-cache -f
```

### Snapshot comparison fails

1. View the diff: `ls tests/visual/chat/diffs/`
2. If intentional, update baselines: `./e2e_runner.sh --update-baseline`

### Tests hang

Check for interactive prompts. All expect scripts should have proper timeouts.

## Local vs CI

| Feature | Local | CI |
|---------|-------|-----|
| Build binary | Auto | Auto |
| Install deps | `--install-deps` | Auto |
| Font support | Manual install | Auto |
| Artifacts | Local files | Uploaded |
| Baseline updates | `--update-baseline` | Manual commit |
