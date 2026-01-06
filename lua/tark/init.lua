-- tark.nvim - AI-powered coding assistant
-- Minimal plugin that opens tark TUI with Neovim integration

local M = {}

M.version = '0.5.0'

-- Default configuration
M.config = {
    -- Binary path (auto-detected if nil)
    binary = nil,
    
    -- Window settings
    window = {
        position = 'right',  -- 'right', 'left', 'bottom', 'top', 'float'
        width = 0.4,         -- 40% of screen for vertical splits, or columns if > 1
        height = 0.5,        -- 50% of screen for horizontal splits, or rows if > 1
    },
    
    -- Auto-download binary if not found
    auto_download = true,
}

-- Lazy-loaded modules
local tui = nil
local binary = nil

local function get_tui()
    if not tui then
        tui = require('tark.tui')
    end
    return tui
end

local function get_binary()
    if not binary then
        binary = require('tark.binary')
    end
    return binary
end

-- Setup commands
local function setup_commands()
    vim.api.nvim_create_user_command('Tark', function()
        get_tui().toggle()
    end, { desc = 'Toggle tark TUI' })
    
    vim.api.nvim_create_user_command('TarkOpen', function()
        get_tui().open()
    end, { desc = 'Open tark TUI' })
    
    vim.api.nvim_create_user_command('TarkClose', function()
        get_tui().close()
    end, { desc = 'Close tark TUI' })
    
    vim.api.nvim_create_user_command('TarkDownload', function()
        get_binary().download()
    end, { desc = 'Download tark binary' })
    
    vim.api.nvim_create_user_command('TarkVersion', function()
        local bin = get_binary().find()
        if bin then
            local version = vim.fn.system(bin .. ' --version'):gsub('%s+$', '')
            vim.notify('tark: ' .. version .. '\nPath: ' .. bin, vim.log.levels.INFO)
        else
            vim.notify('tark: Binary not found. Run :TarkDownload', vim.log.levels.WARN)
        end
    end, { desc = 'Show tark version' })
end

-- Main setup function
function M.setup(opts)
    M.config = vim.tbl_deep_extend('force', M.config, opts or {})
    
    setup_commands()
    
    -- Pass config to submodules
    get_tui().setup(M.config)
    get_binary().setup(M.config)
    
    -- Auto-download if binary not found
    if M.config.auto_download then
        local bin = get_binary().find()
        if not bin then
            vim.notify('tark: Binary not found. Downloading...', vim.log.levels.INFO)
            get_binary().download()
        end
    end
end

-- Public API
function M.open()
    get_tui().open()
end

function M.close()
    get_tui().close()
end

function M.toggle()
    get_tui().toggle()
end

function M.is_open()
    return get_tui().is_open()
end

return M
