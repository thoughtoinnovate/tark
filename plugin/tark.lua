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
