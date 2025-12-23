# Neovim Tests

Automated tests for the tark Neovim plugin using plenary.nvim.

## Requirements

- **Neovim 0.8.0+** (required for plenary.nvim)
- Git (for cloning plenary.nvim)

## Running Tests

### Run All Tests

```bash
nvim --headless -u tests/minimal_init.lua \
  -c "PlenaryBustedDirectory tests/specs/ {minimal_init = 'tests/minimal_init.lua'}"
```

### Run Specific Test File

```bash
# Ghost text tests
nvim --headless -u tests/minimal_init.lua \
  -c "PlenaryBustedFile tests/specs/ghost_spec.lua"

# Chat window tests
nvim --headless -u tests/minimal_init.lua \
  -c "PlenaryBustedFile tests/specs/chat_spec.lua"

# Init module tests
nvim --headless -u tests/minimal_init.lua \
  -c "PlenaryBustedFile tests/specs/init_spec.lua"

# Server tests
nvim --headless -u tests/minimal_init.lua \
  -c "PlenaryBustedFile tests/specs/server_spec.lua"

# LSP tests
nvim --headless -u tests/minimal_init.lua \
  -c "PlenaryBustedFile tests/specs/lsp_spec.lua"
```

### Interactive Test Run

```bash
# See test output in real-time
nvim -u tests/minimal_init.lua \
  -c "PlenaryBustedFile tests/specs/ghost_spec.lua"
```

## Test Structure

```
tests/
├── minimal_init.lua    # Test environment setup
├── README.md           # This file
└── specs/
    ├── ghost_spec.lua  # Ghost text & completion stats tests
    ├── chat_spec.lua   # Chat window & agent stats tests
    ├── init_spec.lua   # Main module & commands tests
    ├── server_spec.lua # Platform detection & server tests
    └── lsp_spec.lua    # LSP helpers tests
```

## Test Coverage

### Ghost Text Module (`ghost_spec.lua`)
- ✅ Stats tracking (requests, accepted, dismissed, tokens, cost)
- ✅ Statusline formatting
- ✅ Stats reset functionality
- ✅ Module functions exist
- ✅ Config validation

### Chat Module (`chat_spec.lua`)
- ✅ Window open/close/toggle
- ✅ Window state management
- ✅ Module functions exist
- ✅ Config validation
- ✅ Error handling

### Init Module (`init_spec.lua`)
- ✅ Module loading
- ✅ API functions (completion_stats, completion_statusline)
- ✅ Commands registration (12+ commands)
- ✅ Config structure
- ✅ Version info

### Server Module (`server_spec.lua`)
- ✅ Platform detection (Linux, macOS, Windows, FreeBSD)
- ✅ Architecture detection (x86_64, arm64)
- ✅ Binary name generation
- ✅ Channel configuration (stable, nightly, latest)
- ✅ Status reporting
- ✅ Binary/Docker availability checks

### LSP Module (`lsp_spec.lua`)
- ✅ LSP client detection
- ✅ Async helper functions
- ✅ Error handling (no LSP attached)
- ✅ Callback execution
- ✅ Graceful degradation

## CI Integration

Tests run automatically on every push/PR via GitHub Actions:

```yaml
neovim-tests:
  name: Neovim Tests
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v4
    - name: Install Neovim
      uses: rhysd/action-setup-vim@v1
      with:
        neovim: true
        version: stable
    - name: Run Neovim tests
      run: |
        nvim --version
        nvim --headless -u tests/minimal_init.lua \
          -c "PlenaryBustedDirectory tests/specs/ {minimal_init = 'tests/minimal_init.lua'}"
```

## Troubleshooting

### Error: "log" is not a valid stdpath

This means your Neovim version is too old. Upgrade to Neovim 0.8.0 or later:

```bash
# Check version
nvim --version

# Ubuntu/Debian
sudo add-apt-repository ppa:neovim-ppa/stable
sudo apt update
sudo apt install neovim

# macOS
brew install neovim

# From source
git clone https://github.com/neovim/neovim
cd neovim && make CMAKE_BUILD_TYPE=Release
sudo make install
```

### Tests Hang or Timeout

Some tests involve async operations. If tests hang:
- Ensure tark server is not running (`pkill tark`)
- Check for zombie Neovim processes (`ps aux | grep nvim`)
- Increase timeout: `timeout 60 nvim --headless ...`

### Plenary Not Found

The minimal_init.lua automatically clones plenary.nvim. If it fails:

```bash
# Manual clone
git clone --depth=1 https://github.com/nvim-lua/plenary.nvim \
  ~/.local/share/nvim/plenary.nvim
```

## Writing New Tests

Follow the existing test structure:

```lua
describe('module - feature', function()
    before_each(function()
        -- Setup before each test
    end)

    after_each(function()
        -- Cleanup after each test
    end)

    describe('sub-feature', function()
        it('does something', function()
            assert.equals(expected, actual)
        end)
    end)
end)
```

### Assertion API

```lua
assert.equals(expected, actual)
assert.is_true(value)
assert.is_false(value)
assert.is_nil(value)
assert.is_not_nil(value)
assert.is_table(value)
assert.is_string(value)
assert.is_number(value)
assert.is_boolean(value)
assert.is_function(value)
assert.has_no_errors(function() ... end)
```

## Resources

- [plenary.nvim](https://github.com/nvim-lua/plenary.nvim)
- [Neovim Lua Guide](https://neovim.io/doc/user/lua-guide.html)
- [tark Documentation](../README.md)

