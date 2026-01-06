-- tark.nvim plugin entry point
-- Lazy-loads the main module on first use

if vim.g.loaded_tark then
    return
end
vim.g.loaded_tark = true

-- Create commands that lazy-load the plugin
vim.api.nvim_create_user_command('Tark', function()
    require('tark').toggle()
end, { desc = 'Toggle tark TUI' })

vim.api.nvim_create_user_command('TarkToggle', function()
    require('tark').toggle()
end, { desc = 'Toggle tark TUI (show/hide)' })

vim.api.nvim_create_user_command('TarkOpen', function()
    require('tark').open()
end, { desc = 'Open tark TUI' })

vim.api.nvim_create_user_command('TarkClose', function()
    require('tark').close()
end, { desc = 'Close tark TUI' })

vim.api.nvim_create_user_command('TarkDownload', function()
    require('tark.binary').download()
end, { desc = 'Download tark binary' })

vim.api.nvim_create_user_command('TarkVersion', function()
    local binary = require('tark.binary')
    local bin = binary.find()
    if bin then
        local ver = binary.version() or 'unknown'
        vim.notify('tark: v' .. ver .. '\nPath: ' .. bin, vim.log.levels.INFO)
    else
        vim.notify('tark: Binary not found. Run :TarkDownload', vim.log.levels.WARN)
    end
end, { desc = 'Show tark version' })

-- LSP commands
vim.api.nvim_create_user_command('TarkLspStart', function()
    local tark = require('tark')
    local client_id = tark.lsp_start()
    if client_id then
        vim.notify('tark: LSP started (client ' .. client_id .. ')', vim.log.levels.INFO)
    else
        vim.notify('tark: Failed to start LSP', vim.log.levels.ERROR)
    end
end, { desc = 'Start tark LSP server' })

vim.api.nvim_create_user_command('TarkLspStop', function()
    require('tark').lsp_stop()
    vim.notify('tark: LSP stopped', vim.log.levels.INFO)
end, { desc = 'Stop tark LSP server' })

vim.api.nvim_create_user_command('TarkLspRestart', function()
    require('tark').lsp_restart()
    vim.notify('tark: LSP restarting...', vim.log.levels.INFO)
end, { desc = 'Restart tark LSP server' })

vim.api.nvim_create_user_command('TarkLspStatus', function()
    local status = require('tark').lsp_status()
    vim.notify('tark: LSP ' .. status, vim.log.levels.INFO)
end, { desc = 'Show tark LSP status' })

vim.api.nvim_create_user_command('TarkLspEnable', function()
    require('tark').lsp_enable()
end, { desc = 'Enable tark completions' })

vim.api.nvim_create_user_command('TarkLspDisable', function()
    require('tark').lsp_disable()
end, { desc = 'Disable tark completions' })

vim.api.nvim_create_user_command('TarkLspToggle', function()
    require('tark').lsp_toggle()
end, { desc = 'Toggle tark completions' })

vim.api.nvim_create_user_command('TarkLspUsage', function()
    local usage = require('tark').lsp_usage()
    vim.notify(usage, vim.log.levels.INFO)
end, { desc = 'Show tark completion usage stats' })

-- Ghost text commands
vim.api.nvim_create_user_command('TarkGhostEnable', function()
    require('tark').ghost_enable()
end, { desc = 'Enable tark ghost text' })

vim.api.nvim_create_user_command('TarkGhostDisable', function()
    require('tark').ghost_disable()
end, { desc = 'Disable tark ghost text' })

vim.api.nvim_create_user_command('TarkGhostToggle', function()
    require('tark').ghost_toggle()
end, { desc = 'Toggle tark ghost text' })

vim.api.nvim_create_user_command('TarkGhostUsage', function()
    local usage = require('tark').ghost_usage()
    vim.notify(usage, vim.log.levels.INFO)
end, { desc = 'Show tark ghost text usage stats' })
