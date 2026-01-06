-- tark.nvim - AI-powered coding assistant
-- Provides TUI chat interface and LSP completions

local M = {}

M.version = '0.4.1'

-- Default configuration
M.config = {
    -- Binary path (auto-detected if nil)
    binary = nil,
    
    -- Window settings for TUI
    window = {
        position = 'right',  -- 'right', 'left', 'bottom', 'top', 'float'
        width = 0.4,         -- 40% of screen for vertical splits, or columns if > 1
        height = 0.5,        -- 50% of screen for horizontal splits, or rows if > 1
    },
    
    -- Auto-download binary if not found
    auto_download = true,
    
    -- LSP settings for completions (menu-based)
    lsp = {
        -- Enable LSP for completions
        enabled = false,  -- Disabled by default, use ghost text instead
        -- Excluded filetypes
        exclude_filetypes = { 'TelescopePrompt', 'NvimTree', 'neo-tree', 'dashboard', 'alpha' },
    },
    
    -- Ghost text settings (inline suggestions)
    ghost = {
        -- Enable ghost text completions
        enabled = true,
        -- Auto-trigger on typing
        auto_trigger = true,
        -- Debounce delay in ms
        debounce_ms = 300,
        -- Accept key (default Tab)
        accept_key = '<Tab>',
    },
}

-- Lazy-loaded modules
local tui = nil
local binary = nil
local lsp = nil
local ghost = nil

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

local function get_lsp()
    if not lsp then
        lsp = require('tark.lsp')
    end
    return lsp
end

local function get_ghost()
    if not ghost then
        ghost = require('tark.ghost')
    end
    return ghost
end

-- Commands are registered in plugin/tark.lua for lazy-loading
-- No need to register them again here

-- Main setup function
function M.setup(opts)
    M.config = vim.tbl_deep_extend('force', M.config, opts or {})
    
    -- Pass config to submodules
    get_tui().setup(M.config)
    get_binary().setup(M.config)
    
    -- Auto-download if binary not found
    if M.config.auto_download then
        local bin = get_binary().find()
        if not bin then
            vim.notify('tark: Binary not found. Downloading...', vim.log.levels.INFO)
            get_binary().download(function(success)
                -- Start LSP after download completes
                if success and M.config.lsp.enabled then
                    get_lsp().setup(M.config.lsp)
                end
            end)
            return
        end
    end
    
    -- Setup LSP if enabled and binary exists
    if M.config.lsp.enabled then
        get_lsp().setup(M.config.lsp)
    end
    
    -- Setup ghost text if enabled
    if M.config.ghost.enabled then
        get_ghost().setup(M.config.ghost)
    end
end

-- Public API: TUI
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

-- Public API: LSP
function M.lsp_start()
    return get_lsp().start()
end

function M.lsp_stop()
    get_lsp().stop()
end

function M.lsp_restart()
    get_lsp().restart()
end

function M.lsp_status()
    return get_lsp().status()
end

function M.lsp_enable()
    get_lsp().enable()
end

function M.lsp_disable()
    get_lsp().disable()
end

function M.lsp_toggle()
    get_lsp().toggle()
end

function M.lsp_usage()
    return get_lsp().format_usage()
end

-- Public API: Ghost Text
function M.ghost_enable()
    get_ghost().enable()
end

function M.ghost_disable()
    get_ghost().disable()
end

function M.ghost_toggle()
    get_ghost().toggle()
end

function M.ghost_usage()
    return get_ghost().format_usage()
end

function M.ghost_accept()
    return get_ghost().accept()
end

return M
