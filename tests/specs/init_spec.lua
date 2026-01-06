-- Tests for main tark module
-- Tests module loading, setup, and public API

local tark = require('tark')

describe('tark - main module', function()
    describe('module loading', function()
        it('loads without error', function()
            assert.is_table(tark)
        end)

        it('has version defined', function()
            assert.is_string(tark.version)
            assert.is_true(#tark.version > 0)
        end)

        it('has config table', function()
            assert.is_table(tark.config)
        end)
    end)

    describe('public API functions', function()
        it('has open function', function()
            assert.is_function(tark.open)
        end)

        it('has close function', function()
            assert.is_function(tark.close)
        end)

        it('has toggle function', function()
            assert.is_function(tark.toggle)
        end)

        it('has is_open function', function()
            assert.is_function(tark.is_open)
        end)

        it('has setup function', function()
            assert.is_function(tark.setup)
        end)
    end)

    describe('is_open function', function()
        it('returns boolean', function()
            local result = tark.is_open()
            assert.is_boolean(result)
        end)

        it('returns false when TUI is not open', function()
            assert.is_false(tark.is_open())
        end)
    end)

    describe('config structure', function()
        it('has window config', function()
            assert.is_table(tark.config.window)
        end)

        it('has auto_download config', function()
            assert.is_boolean(tark.config.auto_download)
        end)

        it('window config has position', function()
            assert.is_string(tark.config.window.position)
        end)

        it('window config has width', function()
            assert.is_number(tark.config.window.width)
        end)

        it('window config has height', function()
            assert.is_number(tark.config.window.height)
        end)
    end)

    describe('setup function', function()
        it('accepts empty config', function()
            assert.has_no_errors(function()
                tark.setup({})
            end)
        end)

        it('merges config with defaults', function()
            tark.setup({ window = { position = 'left' } })
            assert.equals('left', tark.config.window.position)
            -- Reset
            tark.setup({ window = { position = 'right' } })
        end)

        it('preserves other config values', function()
            local original_width = tark.config.window.width
            tark.setup({ window = { position = 'bottom' } })
            assert.equals(original_width, tark.config.window.width)
            -- Reset
            tark.setup({ window = { position = 'right' } })
        end)
    end)

    describe('commands registration', function()
        it('Tark command exists', function()
            local commands = vim.api.nvim_get_commands({})
            assert.is_not_nil(commands.Tark)
        end)

        it('TarkToggle command exists', function()
            local commands = vim.api.nvim_get_commands({})
            assert.is_not_nil(commands.TarkToggle)
        end)

        it('TarkOpen command exists', function()
            local commands = vim.api.nvim_get_commands({})
            assert.is_not_nil(commands.TarkOpen)
        end)

        it('TarkClose command exists', function()
            local commands = vim.api.nvim_get_commands({})
            assert.is_not_nil(commands.TarkClose)
        end)

        it('TarkDownload command exists', function()
            local commands = vim.api.nvim_get_commands({})
            assert.is_not_nil(commands.TarkDownload)
        end)

        it('TarkVersion command exists', function()
            local commands = vim.api.nvim_get_commands({})
            assert.is_not_nil(commands.TarkVersion)
        end)
    end)
end)
