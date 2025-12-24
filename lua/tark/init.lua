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

-- Get the global port file path
local function get_port_file_path()
    local data_dir = vim.fn.stdpath('data')
    return data_dir .. '/tark/server.port'
end

-- Read the server port from the global port file
-- Returns the port from file if exists, otherwise returns configured port
local function get_server_port()
    local port_file = get_port_file_path()
    if vim.fn.filereadable(port_file) == 1 then
        local content = vim.fn.readfile(port_file)
        if content and content[1] then
            local port = tonumber(content[1])
            if port then
                return port
            end
        end
    end
    -- Fallback to configured port
    return M.config.server.port
end

-- Get the server URL (uses dynamic port discovery)
function M.get_server_url()
    local port = get_server_port()
    return string.format('http://%s:%d', M.config.server.host, port)
end

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
        
        -- Clean up all old binaries and related files
        local data_dir = vim.fn.stdpath('data') .. '/tark'
        local old_files = {
            '/tark',                    -- Main binary
            '/tark-darwin-arm64',       -- macOS ARM
            '/tark-darwin-x86_64',      -- macOS Intel
            '/tark-linux-x86_64',       -- Linux x64
            '/tark-linux-arm64',        -- Linux ARM
            '/tark-windows-x86_64.exe', -- Windows x64
            '/tark-windows-arm64.exe',  -- Windows ARM
            '/tark.sha256',             -- Checksum file
            '/tark.tmp',                -- Temp download file
        }
        for _, file in ipairs(old_files) do
            os.remove(data_dir .. file)
        end
        vim.notify('tark: Cleaned up old binaries', vim.log.levels.DEBUG)
        
        get_server().download_binary(function(success)
            if success then
                -- Auto-restart server if it was running (to use the new binary)
                local status = get_server().status()
                if status.running then
                    vim.notify('tark: Restarting server with new binary...', vim.log.levels.INFO)
                    get_server().restart(function(restart_success)
                        if restart_success then
                            vim.notify('tark: Server restarted with new binary ✓', vim.log.levels.INFO)
                        else
                            vim.notify('tark: Server restart failed. Try :TarkServerRestart manually', vim.log.levels.WARN)
                        end
                    end)
                else
                    vim.notify('tark: Binary ready! Run :TarkServerStart', vim.log.levels.INFO)
                end
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

    vim.api.nvim_create_user_command('TarkCompletionEnable', function()
        if M.config.ghost_text.enabled then
            get_ghost().enable()
        else
            vim.notify('Ghost text is disabled in config', vim.log.levels.WARN)
        end
    end, { desc = 'Enable ghost text completions' })

    vim.api.nvim_create_user_command('TarkCompletionDisable', function()
        if M.config.ghost_text.enabled then
            get_ghost().disable()
        else
            vim.notify('Ghost text is disabled in config', vim.log.levels.WARN)
        end
    end, { desc = 'Disable ghost text completions' })
    
    vim.api.nvim_create_user_command('TarkGhostTrigger', function()
        if M.config.ghost_text.enabled then
            get_ghost().trigger()
        end
    end, { desc = 'Trigger ghost text completion' })
    
    vim.api.nvim_create_user_command('TarkCompletionStats', function()
        local ghost = get_ghost()
        local lines = ghost.stats_lines()
        vim.notify(table.concat(lines, '\n'), vim.log.levels.INFO)
    end, { desc = 'Show completion session stats' })
    
    vim.api.nvim_create_user_command('TarkCompletionStatsReset', function()
        get_ghost().reset_stats()
        vim.notify('Completion stats reset', vim.log.levels.INFO)
    end, { desc = 'Reset completion session stats' })
    
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
    
    vim.api.nvim_create_user_command('TarkMaximize', function()
        if M.config.chat.enabled then
            get_chat().maximize()
        end
    end, { desc = 'Toggle maximize tark windows' })
    
    -- Usage commands
    vim.api.nvim_create_user_command('TarkUsage', function()
        local usage = require('tark.usage')
        usage.show_summary()
    end, { desc = 'Show usage summary' })
    
    vim.api.nvim_create_user_command('TarkUsageOpen', function()
        local url = string.format('http://%s:%d/usage', 
            M.config.server.host, 
            M.config.server.port)
        
        -- Cross-platform browser open
        local cmd
        if vim.fn.has('mac') == 1 then
            cmd = 'open'
        elseif vim.fn.has('unix') == 1 then
            cmd = 'xdg-open'
        else
            cmd = 'start'
        end
        
        vim.fn.jobstart({cmd, url}, {detach = true})
        vim.notify('Opening usage dashboard: ' .. url, vim.log.levels.INFO)
    end, { desc = 'Open usage dashboard in browser' })
    
    vim.api.nvim_create_user_command('TarkUsageCleanup', function(opts)
        local days = tonumber(opts.args) or 30
        local usage = require('tark.usage')
        usage.cleanup(days)
    end, { 
        nargs = '?',
        desc = 'Cleanup usage logs older than N days (default: 30)' 
    })
end

-- Setup keymaps (user can override via opts.keys)
local function setup_keymaps()
    -- Default keymaps (users typically set these in lazy.nvim keys = {})
    -- These are just fallbacks if user doesn't configure keys
end

-- Main setup function
function M.setup(opts)
    -- Check for required dependencies
    local plenary_ok, _ = pcall(require, 'plenary')
    if not plenary_ok then
        vim.notify(
            'tark.nvim requires plenary.nvim\n\n' ..
            'Install with lazy.nvim:\n' ..
            '  { "nvim-lua/plenary.nvim" }\n\n' ..
            'Or add as tark dependency:\n' ..
            '  dependencies = { "nvim-lua/plenary.nvim" }',
            vim.log.levels.ERROR
        )
        return
    end
    
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
            server_url = M.get_server_url(),  -- Uses dynamic port discovery
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

---Get completion stats for statusline
---@return table stats Session stats for completion mode
function M.completion_stats()
    return get_ghost().get_stats()
end

---Get completion statusline component
---Returns a formatted string suitable for statusline display
---@return string statusline Formatted stats (empty if no completions)
function M.completion_statusline()
    return get_ghost().statusline()
end

return M
