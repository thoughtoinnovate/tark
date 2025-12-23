-- Tests for main tark module
-- Tests module loading, commands, and API functions

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

    describe('API functions', function()
        it('has completion_stats function', function()
            assert.is_function(tark.completion_stats)
        end)

        it('has completion_statusline function', function()
            assert.is_function(tark.completion_statusline)
        end)

        it('completion_stats returns table', function()
            local stats = tark.completion_stats()
            assert.is_table(stats)
        end)

        it('completion_stats has required fields', function()
            local stats = tark.completion_stats()
            assert.is_number(stats.requests)
            assert.is_number(stats.accepted)
            assert.is_number(stats.dismissed)
            assert.is_number(stats.input_tokens)
            assert.is_number(stats.output_tokens)
            assert.is_number(stats.total_cost)
        end)

        it('completion_statusline returns string', function()
            local statusline = tark.completion_statusline()
            assert.is_string(statusline)
        end)
    end)

    describe('module access functions', function()
        it('has ghost function', function()
            assert.is_function(tark.ghost)
        end)

        it('has chat function', function()
            assert.is_function(tark.chat)
        end)

        it('has lsp function', function()
            assert.is_function(tark.lsp)
        end)

        it('ghost returns module', function()
            local ghost = tark.ghost()
            assert.is_table(ghost)
        end)

        it('chat returns module', function()
            local chat = tark.chat()
            assert.is_table(chat)
        end)

        it('lsp returns module', function()
            local lsp = tark.lsp()
            assert.is_table(lsp)
        end)
    end)

    describe('server functions', function()
        it('has start_server function', function()
            assert.is_function(tark.start_server)
        end)

        it('has stop_server function', function()
            assert.is_function(tark.stop_server)
        end)

        it('has server_status function', function()
            assert.is_function(tark.server_status)
        end)

        it('server_status returns table', function()
            local status = tark.server_status()
            assert.is_table(status)
        end)
    end)

    describe('convenience functions', function()
        it('has toggle_chat function', function()
            assert.is_function(tark.toggle_chat)
        end)

        it('has toggle_ghost function', function()
            assert.is_function(tark.toggle_ghost)
        end)
    end)

    describe('commands registration', function()
        -- Commands are only registered after setup() is called
        before_each(function()
            tark.setup({})
        end)

        it('TarkServerStart command exists', function()
            local commands = vim.api.nvim_get_commands({})
            assert.is_not_nil(commands.TarkServerStart)
        end)

        it('TarkServerStop command exists', function()
            local commands = vim.api.nvim_get_commands({})
            assert.is_not_nil(commands.TarkServerStop)
        end)

        it('TarkServerStatus command exists', function()
            local commands = vim.api.nvim_get_commands({})
            assert.is_not_nil(commands.TarkServerStatus)
        end)

        it('TarkServerRestart command exists', function()
            local commands = vim.api.nvim_get_commands({})
            assert.is_not_nil(commands.TarkServerRestart)
        end)

        it('TarkBinaryDownload command exists', function()
            local commands = vim.api.nvim_get_commands({})
            assert.is_not_nil(commands.TarkBinaryDownload)
        end)

        it('TarkGhostToggle command exists', function()
            local commands = vim.api.nvim_get_commands({})
            assert.is_not_nil(commands.TarkGhostToggle)
        end)

        it('TarkGhostTrigger command exists', function()
            local commands = vim.api.nvim_get_commands({})
            assert.is_not_nil(commands.TarkGhostTrigger)
        end)

        it('TarkCompletionStats command exists', function()
            local commands = vim.api.nvim_get_commands({})
            assert.is_not_nil(commands.TarkCompletionStats)
        end)

        it('TarkCompletionStatsReset command exists', function()
            local commands = vim.api.nvim_get_commands({})
            assert.is_not_nil(commands.TarkCompletionStatsReset)
        end)

        it('TarkChatToggle command exists', function()
            local commands = vim.api.nvim_get_commands({})
            assert.is_not_nil(commands.TarkChatToggle)
        end)

        it('TarkChatOpen command exists', function()
            local commands = vim.api.nvim_get_commands({})
            assert.is_not_nil(commands.TarkChatOpen)
        end)

        it('TarkChatClose command exists', function()
            local commands = vim.api.nvim_get_commands({})
            assert.is_not_nil(commands.TarkChatClose)
        end)
    end)

    describe('config structure', function()
        it('has server config', function()
            assert.is_table(tark.config.server)
        end)

        it('has ghost_text config', function()
            assert.is_table(tark.config.ghost_text)
        end)

        it('has chat config', function()
            assert.is_table(tark.config.chat)
        end)

        it('has lsp config', function()
            assert.is_table(tark.config.lsp)
        end)

        it('server config has expected fields', function()
            assert.is_not_nil(tark.config.server.auto_start)
            assert.is_not_nil(tark.config.server.mode)
            assert.is_not_nil(tark.config.server.channel)
        end)
    end)
end)

