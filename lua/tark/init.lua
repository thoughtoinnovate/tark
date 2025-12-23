-- tark.nvim - AI-powered coding assistant
-- Main entry point

local M = {}

-- Version
M.version = '0.1.0'

-- Default configuration
M.config = {
    -- Server settings
    server = {
        mode = 'auto',           -- 'auto', 'binary', 'docker'
        binary = 'tark',         -- Path to tark binary
        host = '127.0.0.1',
        port = 8765,
        auto_start = true,       -- Auto-start server when Neovim opens
        stop_on_exit = true,     -- Stop server when Neovim exits
        channel = 'stable',      -- 'stable' (pinned version) or 'nightly' (latest)
    },
    -- Docker settings
    docker = {
        image = 'ghcr.io/thoughtoinnovate/tark:alpine',
        container_name = 'tark-server',
        pull_on_start = true,    -- Pull latest image before starting
        build_local = false,     -- Build from plugin's Dockerfile
        dockerfile = 'alpine',   -- 'alpine' (~30MB) or 'minimal' (~15MB)
        mount_workspace = true,  -- Mount cwd into container
    },
    -- Ghost text (inline completions)
    ghost_text = {
        enabled = true,
        debounce_ms = 150,
        hl_group = 'Comment',
    },
    -- Chat window
    chat = {
        enabled = true,
        window = {
            style = 'sidepane',
            sidepane_width = 0.35,
            border = 'rounded',
        },
        lsp_proxy = true,  -- Enable LSP proxy for agent tools
    },
    -- LSP integration
    lsp = {
        enabled = true,                  -- Enable LSP context in completions/chat
        context_in_completions = true,   -- Send LSP context with ghost text requests
        context_in_chat = true,          -- Include buffer context in chat
        proxy_timeout_ms = 50,           -- Fast fallback to tree-sitter
    },
}

-- Lazy-loaded modules
local server = nil
local ghost = nil
local chat = nil
local lsp = nil

-- Get server module
local function get_server()
    if not server then
        server = require('tark.server')
    end
    return server
end

-- Get ghost text module
local function get_ghost()
    if not ghost then
        ghost = require('tark.ghost')
    end
    return ghost
end

-- Get chat module
local function get_chat()
    if not chat then
        chat = require('tark.chat')
    end
    return chat
end

-- Get LSP module
local function get_lsp()
    if not lsp then
        lsp = require('tark.lsp')
    end
    return lsp
end

-- Setup commands
local function setup_commands()
    -- Server management commands
    vim.api.nvim_create_user_command('TarkServerStart', function()
        get_server().start()
    end, { desc = 'Start tark server' })
    
    vim.api.nvim_create_user_command('TarkServerStop', function()
        get_server().stop()
    end, { desc = 'Stop tark server' })
    
    vim.api.nvim_create_user_command('TarkServerRestart', function()
        get_server().restart()
    end, { desc = 'Restart tark server' })
    
    vim.api.nvim_create_user_command('TarkServerStatus', function()
        local status = get_server().status()
        local lines = {
            '=== tark Server Status ===',
            '',
            'Running: ' .. (status.running and '✅ Yes' or '❌ No'),
            'Mode: ' .. (status.mode or 'not started'),
            'URL: ' .. status.url,
        }
        
        if status.version then
            table.insert(lines, 'Version: ' .. status.version)
        end
        if status.provider then
            table.insert(lines, 'Provider: ' .. status.provider)
        end
        
        table.insert(lines, '')
        table.insert(lines, '--- Platform ---')
        table.insert(lines, 'Detected: ' .. (status.platform or 'unknown'))
        table.insert(lines, 'Binary Name: ' .. (status.binary_name or 'unknown'))
        table.insert(lines, 'Channel: ' .. (status.channel or 'stable'))
        
        table.insert(lines, '')
        table.insert(lines, '--- Availability ---')
        table.insert(lines, 'Binary: ' .. (status.binary_available and '✅ ' .. (status.binary_info or 'available') or '❌ ' .. (status.binary_info or 'not found')))
        table.insert(lines, 'Docker: ' .. (status.docker_available and '✅ available' or '❌ ' .. (status.docker_info or 'not available')))
        
        if status.container_name then
            table.insert(lines, '')
            table.insert(lines, '--- Docker ---')
            table.insert(lines, 'Image: ' .. status.image)
            table.insert(lines, 'Container: ' .. status.container_name)
            table.insert(lines, 'Container Running: ' .. (status.container_running and '✅ Yes' or '❌ No'))
        end
        
        vim.notify(table.concat(lines, '\n'), vim.log.levels.INFO)
    end, { desc = 'Show tark server status' })
    
    -- Binary commands
    vim.api.nvim_create_user_command('TarkBinaryDownload', function(opts)
        -- Allow switching channel: :TarkBinaryDownload nightly
        if opts.args and opts.args ~= '' then
            local channel = opts.args:lower()
            if channel == 'nightly' or channel == 'latest' or channel == 'stable' then
                get_server().config.channel = channel
                vim.notify('tark: Switched to ' .. channel .. ' channel', vim.log.levels.INFO)
            else
                vim.notify('tark: Invalid channel. Use: stable, nightly, or latest', vim.log.levels.ERROR)
                return
            end
        end
        
        -- Remove existing binary to force re-download
        local data_dir = vim.fn.stdpath('data') .. '/tark'
        os.remove(data_dir .. '/tark')
        
        get_server().download_binary(function(success)
            if success then
                vim.notify('tark: Binary ready! Run :TarkServerRestart', vim.log.levels.INFO)
            end
        end)
    end, { 
        desc = 'Download tark binary (optional: stable/nightly)',
        nargs = '?',
        complete = function() return { 'stable', 'nightly', 'latest' } end,
    })
    
    -- Docker commands
    vim.api.nvim_create_user_command('TarkDockerPull', function()
        get_server().docker_pull()
    end, { desc = 'Pull tark Docker image' })
    
    vim.api.nvim_create_user_command('TarkDockerBuild', function()
        get_server().docker_build()
    end, { desc = 'Build tark Docker image locally' })
    
    -- Ghost text commands
    vim.api.nvim_create_user_command('TarkGhostToggle', function()
        if M.config.ghost_text.enabled then
            get_ghost().toggle()
        else
            vim.notify('Ghost text is disabled in config', vim.log.levels.WARN)
        end
    end, { desc = 'Toggle ghost text' })
    
    vim.api.nvim_create_user_command('TarkGhostTrigger', function()
        if M.config.ghost_text.enabled then
            get_ghost().trigger()
        end
    end, { desc = 'Trigger ghost text completion' })
    
    -- Chat commands
    vim.api.nvim_create_user_command('TarkChatToggle', function()
        if M.config.chat.enabled then
            get_chat().toggle()
        else
            vim.notify('Chat is disabled in config', vim.log.levels.WARN)
        end
    end, { desc = 'Toggle chat window' })
    
    vim.api.nvim_create_user_command('TarkChatOpen', function()
        if M.config.chat.enabled then
            get_chat().open()
        end
    end, { desc = 'Open chat window' })
    
    vim.api.nvim_create_user_command('TarkChatClose', function()
        if M.config.chat.enabled then
            get_chat().close()
        end
    end, { desc = 'Close chat window' })
end

-- Setup keymaps (user can override via opts.keys)
local function setup_keymaps()
    -- Default keymaps (users typically set these in lazy.nvim keys = {})
    -- These are just fallbacks if user doesn't configure keys
end

-- Main setup function
function M.setup(opts)
    -- Merge user config with defaults
    M.config = vim.tbl_deep_extend('force', M.config, opts or {})
    
    -- Smart defaults: keep Docker container running (faster reconnect)
    -- Only apply if user didn't explicitly set stop_on_exit
    local user_set_stop = opts and opts.server and opts.server.stop_on_exit ~= nil
    if not user_set_stop then
        local is_docker = M.config.server.mode == 'docker' or 
            (M.config.server.mode == 'auto' and M.config.docker.build_local)
        if is_docker then
            M.config.server.stop_on_exit = false  -- Keep container running
        end
    end
    
    -- Setup commands
    setup_commands()
    
    -- Setup keymaps
    setup_keymaps()
    
    -- Build server config from merged config
    local server_config = vim.tbl_deep_extend('force', M.config.server, {
        docker = M.config.docker,
    })
    
    -- Setup server management
    get_server().setup(server_config)
    
    -- Setup ghost text if enabled
    if M.config.ghost_text.enabled then
        local ghost_config = vim.tbl_deep_extend('force', M.config.ghost_text, {
            server_url = string.format('http://%s:%d', M.config.server.host, M.config.server.port),
            lsp_context = M.config.lsp.enabled and M.config.lsp.context_in_completions,
        })
        get_ghost().setup(ghost_config)
    end
    
    -- Setup chat if enabled
    if M.config.chat.enabled then
        -- Determine if we're using Docker mode
        local is_docker_mode = M.config.server.mode == 'docker' or 
            (M.config.server.mode == 'auto' and M.config.docker.build_local)
        
        local chat_config = vim.tbl_deep_extend('force', M.config.chat, {
            server_url = string.format('http://%s:%d', M.config.server.host, M.config.server.port),
            docker_mode = is_docker_mode,
            lsp_proxy = M.config.lsp.enabled and M.config.chat.lsp_proxy,
        })
        -- Chat setup is called when first opened, but we can pass config
        get_chat().setup(chat_config)
    end
end

-- Expose sub-modules for direct access
function M.server()
    return get_server()
end

function M.ghost()
    return get_ghost()
end

function M.chat()
    return get_chat()
end

function M.lsp()
    return get_lsp()
end

-- Convenience functions
function M.start_server(callback)
    get_server().start(callback)
end

function M.stop_server(callback)
    get_server().stop(callback)
end

function M.server_status()
    return get_server().status()
end

function M.toggle_chat()
    if M.config.chat.enabled then
        get_chat().toggle()
    end
end

function M.toggle_ghost()
    if M.config.ghost_text.enabled then
        get_ghost().toggle()
    end
end

return M
