-- tark (तर्क) Neovim plugin
-- Tark = Logic/Reasoning in Sanskrit
-- Provides AI-powered completions and chat functionality
-- 
-- Works seamlessly with LazyVim and blink.cmp - no manual config needed!

local M = {}

-- Server job ID (for managing the background server process)
M.server_job_id = nil

-- Default configuration
M.config = {
    -- Server settings
    server = {
        auto_start = true,           -- Auto-start server when plugin loads
        binary = 'tark',             -- Path to tark binary (or just 'tark' if in PATH)
        host = '127.0.0.1',
        port = 8765,
        stop_on_exit = true,         -- Stop server when Neovim exits
    },
    -- LSP settings
    lsp = {
        enabled = false,             -- Disabled by default to avoid conflicts with existing LSP
        cmd = { 'tark', 'lsp' },
        filetypes = { '*' },
    },
    -- Ghost text (inline completions) settings
    ghost_text = {
        enabled = true,
        server_url = 'http://localhost:8765',
        debounce_ms = 150,
        hl_group = 'Comment',
    },
    -- Chat settings
    chat = {
        enabled = true,
        window = {
            width = 80,
            height = 20,
            border = 'rounded',
        },
    },
}

-- Check if the tark binary is available
function M.check_binary()
    local binary = M.config.server.binary
    local handle = io.popen('which ' .. binary .. ' 2>/dev/null || where ' .. binary .. ' 2>nul')
    if handle then
        local result = handle:read('*a')
        handle:close()
        return result and result ~= ''
    end
    return false
end

-- Check if server is running
function M.is_server_running()
    local url = string.format('http://%s:%d/health', 
        M.config.server.host, M.config.server.port)
    local handle = io.popen('curl -s --max-time 1 ' .. url .. ' 2>/dev/null')
    if handle then
        local result = handle:read('*a')
        handle:close()
        return result and result:match('"status":"ok"')
    end
    return false
end

-- Start the tark server
function M.start_server()
    -- Check if already running
    if M.is_server_running() then
        vim.notify('tark server is already running', vim.log.levels.INFO)
        return true
    end

    -- Check if binary exists
    if not M.check_binary() then
        vim.notify(
            'tark binary not found. Install with:\n' ..
            '  cargo install --git https://github.com/thoughtoinnovate/tark.git\n' ..
            'Or download from GitHub releases.',
            vim.log.levels.ERROR
        )
        return false
    end

    local cmd = string.format('%s serve --host %s --port %d',
        M.config.server.binary,
        M.config.server.host,
        M.config.server.port
    )

    -- Start server as background job
    M.server_job_id = vim.fn.jobstart(cmd, {
        detach = true,
        on_exit = function(_, code)
            if code ~= 0 and code ~= 143 then  -- 143 = SIGTERM (normal stop)
                vim.schedule(function()
                    vim.notify('tark server exited with code ' .. code, vim.log.levels.WARN)
                end)
            end
            M.server_job_id = nil
        end,
        on_stderr = function(_, data)
            -- Log errors but don't spam notifications
            if data and data[1] and data[1] ~= '' then
                vim.schedule(function()
                    -- Only log, don't notify for every stderr line
                end)
            end
        end,
    })

    if M.server_job_id and M.server_job_id > 0 then
        -- Wait a moment for server to start, then verify
        vim.defer_fn(function()
            if M.is_server_running() then
                vim.notify('tark server started on port ' .. M.config.server.port, vim.log.levels.INFO)
            else
                vim.notify('tark server may have failed to start. Check :TarkServerStatus', vim.log.levels.WARN)
            end
        end, 500)
        return true
    else
        vim.notify('Failed to start tark server', vim.log.levels.ERROR)
        return false
    end
end

-- Stop the tark server
function M.stop_server()
    if M.server_job_id and M.server_job_id > 0 then
        vim.fn.jobstop(M.server_job_id)
        M.server_job_id = nil
        vim.notify('tark server stopped', vim.log.levels.INFO)
        return true
    else
        -- Try to find and kill any running tark serve process
        local handle = io.popen('pkill -f "tark serve" 2>/dev/null')
        if handle then
            handle:close()
            vim.notify('tark server stopped', vim.log.levels.INFO)
            return true
        end
    end
    vim.notify('No tark server to stop', vim.log.levels.INFO)
    return false
end

-- Get server status
function M.server_status()
    local running = M.is_server_running()
    local binary_found = M.check_binary()
    
    local lines = {
        '=== tark Server Status ===',
        '',
        'Binary: ' .. (binary_found and 'Found (' .. M.config.server.binary .. ')' or 'NOT FOUND'),
        'Server: ' .. (running and 'Running' or 'Not running'),
        'URL: http://' .. M.config.server.host .. ':' .. M.config.server.port,
        '',
    }
    
    if running then
        -- Get more details from health endpoint
        local url = string.format('http://%s:%d/health', M.config.server.host, M.config.server.port)
        local handle = io.popen('curl -s ' .. url .. ' 2>/dev/null')
        if handle then
            local result = handle:read('*a')
            handle:close()
            local provider = result:match('"current_provider":"([^"]+)"')
            local version = result:match('"version":"([^"]+)"')
            if provider then
                table.insert(lines, 'Provider: ' .. provider)
            end
            if version then
                table.insert(lines, 'Version: ' .. version)
            end
        end
    else
        table.insert(lines, 'Start with :TarkServerStart')
    end
    
    if not binary_found then
        table.insert(lines, '')
        table.insert(lines, 'Install tark:')
        table.insert(lines, '  cargo install --git https://github.com/thoughtoinnovate/tark.git')
        table.insert(lines, '  Or download binary from GitHub releases')
    end
    
    vim.notify(table.concat(lines, '\n'), running and vim.log.levels.INFO or vim.log.levels.WARN)
end

-- Setup function (LazyVim compatible - accepts opts table)
function M.setup(opts)
    -- Handle both direct opts and lazy.nvim style { opts = {...} }
    if opts and opts.opts then
        opts = opts.opts
    end
    M.config = vim.tbl_deep_extend('force', M.config, opts or {})

    -- Update ghost_text server_url based on server config
    M.config.ghost_text.server_url = string.format('http://%s:%d',
        M.config.server.host, M.config.server.port)

    -- Register commands first (so they're available even if server isn't running)
    M.register_commands()

    -- Auto-start server if enabled
    if M.config.server.auto_start then
        -- Defer to not block startup
        vim.defer_fn(function()
            if not M.is_server_running() then
                M.start_server()
            end
        end, 100)
    end

    -- Setup LSP if enabled
    if M.config.lsp.enabled then
        require('tark.lsp').setup(M.config.lsp)
    end

    -- Setup ghost text if enabled
    if M.config.ghost_text.enabled then
        require('tark.ghost').setup(M.config.ghost_text)
    end

    -- Setup chat if enabled
    if M.config.chat.enabled then
        require('tark.chat').setup(M.config.chat)
    end

    -- Stop server on Neovim exit if we started it
    if M.config.server.stop_on_exit then
        vim.api.nvim_create_autocmd('VimLeavePre', {
            callback = function()
                if M.server_job_id and M.server_job_id > 0 then
                    vim.fn.jobstop(M.server_job_id)
                end
            end,
        })
    end
end

-- Register user commands
function M.register_commands()
    -- Server management commands
    vim.api.nvim_create_user_command('TarkServerStart', function()
        M.start_server()
    end, { desc = 'Start tark server' })

    vim.api.nvim_create_user_command('TarkServerStop', function()
        M.stop_server()
    end, { desc = 'Stop tark server' })

    vim.api.nvim_create_user_command('TarkServerStatus', function()
        M.server_status()
    end, { desc = 'Show tark server status' })

    vim.api.nvim_create_user_command('TarkServerRestart', function()
        M.stop_server()
        vim.defer_fn(function()
            M.start_server()
        end, 500)
    end, { desc = 'Restart tark server' })

    -- Chat command (open)
    vim.api.nvim_create_user_command('TarkChat', function(opts)
        require('tark.chat').open(opts.args)
    end, { nargs = '*', desc = 'Open tark chat' })

    -- Chat toggle command
    vim.api.nvim_create_user_command('TarkChatToggle', function()
        require('tark.chat').toggle()
    end, { desc = 'Toggle tark chat window' })

    -- Toggle ghost text
    vim.api.nvim_create_user_command('TarkGhostToggle', function()
        require('tark.ghost').toggle()
    end, { desc = 'Toggle ghost text completions' })

    -- Trigger completion manually
    vim.api.nvim_create_user_command('TarkComplete', function()
        require('tark.ghost').trigger()
    end, { desc = 'Trigger AI completion' })

    -- Setup default keybindings
    vim.keymap.set('n', '<leader>ec', function()
        require('tark.chat').toggle()
    end, { silent = true, desc = 'Toggle tark chat' })

    vim.keymap.set('n', '<leader>es', function()
        M.server_status()
    end, { silent = true, desc = 'tark server status' })
end

return M

