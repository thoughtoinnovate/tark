-- tark (तर्क) Neovim plugin
-- Tark = Logic/Reasoning in Sanskrit
-- Provides AI-powered completions and chat functionality
-- 
-- Works seamlessly with LazyVim and blink.cmp - no manual config needed!
-- Supports both binary and Docker modes for maximum flexibility.

local M = {}

-- Server job ID (for managing the background server process)
M.server_job_id = nil
M.docker_container_id = nil

-- Default configuration
M.config = {
    -- Server settings
    server = {
        auto_start = true,           -- Auto-start server when plugin loads
        mode = 'auto',               -- 'auto', 'binary', or 'docker'
        binary = 'tark',             -- Path to tark binary (or just 'tark' if in PATH)
        host = '127.0.0.1',
        port = 8765,
        stop_on_exit = true,         -- Stop server when Neovim exits
    },
    -- Docker settings (used when mode = 'docker' or 'auto' with no binary)
    docker = {
        image = 'ghcr.io/thoughtoinnovate/tark:latest',
        container_name = 'tark-server',
        pull_on_start = true,        -- Pull latest image before starting
        build_local = false,         -- Build from local Dockerfile instead of pulling
        dockerfile = 'minimal',      -- 'minimal' (scratch, ~15MB) or 'alpine' (~30MB, has shell)
        mount_workspace = true,      -- Mount current directory into container
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

-- Check if Docker is available
function M.check_docker()
    local handle = io.popen('docker --version 2>/dev/null')
    if handle then
        local result = handle:read('*a')
        handle:close()
        return result and result:match('Docker')
    end
    return false
end

-- Check if Docker container is running
function M.is_docker_running()
    local handle = io.popen('docker ps --filter "name=' .. M.config.docker.container_name .. '" --format "{{.ID}}" 2>/dev/null')
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

-- Determine which mode to use
function M.get_server_mode()
    local mode = M.config.server.mode
    
    if mode == 'binary' then
        return 'binary'
    elseif mode == 'docker' then
        return 'docker'
    else -- 'auto'
        -- Prefer binary if available, fallback to Docker
        if M.check_binary() then
            return 'binary'
        elseif M.check_docker() then
            return 'docker'
        else
            return nil
        end
    end
end

-- Get the plugin installation directory
function M.get_plugin_dir()
    -- Find where this plugin is installed
    local source = debug.getinfo(1, 'S').source
    if source:sub(1, 1) == '@' then
        source = source:sub(2)
    end
    -- Go up from lua/tark/init.lua to plugin root
    local plugin_dir = vim.fn.fnamemodify(source, ':h:h:h')
    return plugin_dir
end

-- Get host architecture
function M.get_host_arch()
    local handle = io.popen('uname -m 2>/dev/null')
    if handle then
        local result = handle:read('*a'):gsub('%s+', '')
        handle:close()
        -- Normalize architecture names
        if result == 'x86_64' or result == 'amd64' then
            return 'x86_64'
        elseif result == 'aarch64' or result == 'arm64' then
            return 'arm64'
        end
        return result
    end
    return 'unknown'
end

-- Build Docker image locally from Dockerfile (with optional callback)
function M.docker_build_async(on_complete)
    local plugin_dir = M.get_plugin_dir()
    
    -- Determine which Dockerfile to use
    local dockerfile_type = M.config.docker.dockerfile or 'minimal'
    local dockerfile_path
    local image_tag
    
    if dockerfile_type == 'alpine' then
        dockerfile_path = plugin_dir .. '/Dockerfile.alpine'
        image_tag = 'tark:local-alpine'
    else
        -- 'minimal' or default - use scratch-based Dockerfile
        dockerfile_path = plugin_dir .. '/Dockerfile'
        image_tag = 'tark:local'
    end
    
    -- Check if Dockerfile exists
    if vim.fn.filereadable(dockerfile_path) ~= 1 then
        -- Fallback to main Dockerfile
        dockerfile_path = plugin_dir .. '/Dockerfile'
        if vim.fn.filereadable(dockerfile_path) ~= 1 then
            vim.notify('Dockerfile not found at: ' .. dockerfile_path, vim.log.levels.ERROR)
            if on_complete then on_complete(false) end
            return false
        end
    end
    
    -- Get just the filename for docker build -f
    local dockerfile_name = vim.fn.fnamemodify(dockerfile_path, ':t')
    
    -- Build in background with progress
    local cmd = string.format('cd %s && docker build -f %s -t %s . 2>&1', 
        vim.fn.shellescape(plugin_dir), dockerfile_name, image_tag)
    
    local output_lines = {}
    local job_id = vim.fn.jobstart(cmd, {
        on_stdout = function(_, data)
            for _, line in ipairs(data) do
                if line ~= '' then
                    table.insert(output_lines, line)
                    -- Show build steps (both buildkit and legacy formats)
                    if line:match('^Step') or line:match('^%[%d+/%d+%]') or line:match('^Successfully') or line:match('CACHED') then
                        vim.schedule(function()
                            vim.notify(line, vim.log.levels.INFO)
                        end)
                    end
                end
            end
        end,
        on_stderr = function(_, data)
            for _, line in ipairs(data) do
                if line ~= '' then
                    table.insert(output_lines, line)
                    -- BuildKit outputs to stderr
                    if line:match('^%[%d+/%d+%]') or line:match('CACHED') then
                        vim.schedule(function()
                            vim.notify(line, vim.log.levels.INFO)
                        end)
                    end
                end
            end
        end,
        on_exit = function(_, code)
            vim.schedule(function()
                if code == 0 then
                    M.config.docker.image = image_tag
                    
                    -- Get image size
                    local size_handle = io.popen('docker images ' .. image_tag .. ' --format "{{.Size}}"')
                    local size = size_handle and size_handle:read('*a'):gsub('%s+', '') or 'unknown'
                    if size_handle then size_handle:close() end
                    
                    vim.notify('Docker image built: ' .. image_tag .. ' (' .. size .. ')', vim.log.levels.INFO)
                    if on_complete then on_complete(true) end
                else
                    vim.notify('Docker build failed.\n' .. 
                        table.concat(output_lines, '\n'):sub(-500), vim.log.levels.ERROR)
                    if on_complete then on_complete(false) end
                end
            end)
        end,
    })
    
    return job_id > 0
end

-- Synchronous wrapper for manual builds
function M.docker_build()
    vim.notify('Building tark Docker image from source...\nThis may take a few minutes.', vim.log.levels.INFO)
    return M.docker_build_async(nil)
end

-- Pull Docker image
function M.docker_pull()
    vim.notify('Pulling tark Docker image...', vim.log.levels.INFO)
    local handle = io.popen('docker pull ' .. M.config.docker.image .. ' 2>&1')
    if handle then
        local result = handle:read('*a')
        handle:close()
        if result:match('Status:') or result:match('up to date') or result:match('Downloaded') then
            vim.notify('Docker image ready', vim.log.levels.INFO)
            return true
        end
    end
    return false
end

-- Check if local Docker image exists
function M.docker_image_exists(image_name)
    local handle = io.popen('docker images -q ' .. image_name .. ' 2>/dev/null')
    if handle then
        local result = handle:read('*a')
        handle:close()
        return result and result ~= ''
    end
    return false
end

-- Start Docker container
function M.start_docker()
    -- Check if already running
    if M.is_docker_running() then
        vim.notify('tark Docker container is already running', vim.log.levels.INFO)
        return true
    end
    
    -- Remove any stopped container with same name
    os.execute('docker rm -f ' .. M.config.docker.container_name .. ' 2>/dev/null')
    
    -- Handle image: build local or pull from registry
    if M.config.docker.build_local then
        -- Determine the local image tag based on dockerfile type
        local dockerfile_type = M.config.docker.dockerfile or 'minimal'
        local local_image_tag = dockerfile_type == 'alpine' and 'tark:local-alpine' or 'tark:local'
        
        -- Build from local Dockerfile
        if not M.docker_image_exists(local_image_tag) then
            local arch = M.get_host_arch()
            local dockerfile_desc = dockerfile_type == 'alpine' 
                and 'Alpine-based image (~30MB, includes shell)' 
                or 'Minimal scratch image (~15MB, binary only)'
            
            vim.notify(string.format([[
Building tark Docker image from source...
Type: %s
Arch: %s

This takes 3-5 minutes on first run.
Run :TarkServerStatus to check progress.
The server will start automatically when build completes.
]], dockerfile_desc, arch), vim.log.levels.WARN)
            
            -- Start async build with completion callback
            M.docker_build_async(function(success)
                if success then
                    M.start_docker()
                end
            end)
            return true
        end
        M.config.docker.image = local_image_tag
    elseif M.config.docker.pull_on_start then
        -- Pull from registry
        M.docker_pull()
    end
    
    -- Build docker run command
    local cwd = vim.fn.getcwd()
    local is_linux = vim.loop.os_uname().sysname == 'Linux'
    
    local cmd_parts = {
        'docker', 'run', '-d',
        '--name', M.config.docker.container_name,
    }
    
    -- Network configuration differs by OS
    if is_linux then
        -- On Linux, use host network for direct localhost access (Ollama, etc.)
        table.insert(cmd_parts, '--network')
        table.insert(cmd_parts, 'host')
    else
        -- On macOS/Windows, need port mapping and host.docker.internal
        table.insert(cmd_parts, '-p')
        table.insert(cmd_parts, M.config.server.port .. ':8765')
        table.insert(cmd_parts, '--add-host')
        table.insert(cmd_parts, 'host.docker.internal:host-gateway')
    end
    
    -- Mount workspace if configured
    if M.config.docker.mount_workspace then
        table.insert(cmd_parts, '-v')
        table.insert(cmd_parts, cwd .. ':/workspace')
        table.insert(cmd_parts, '-w')
        table.insert(cmd_parts, '/workspace')
    end
    
    -- Pass API keys from environment
    local openai_key = os.getenv('OPENAI_API_KEY')
    local anthropic_key = os.getenv('ANTHROPIC_API_KEY')
    
    if openai_key then
        table.insert(cmd_parts, '-e')
        table.insert(cmd_parts, 'OPENAI_API_KEY=' .. openai_key)
    end
    if anthropic_key then
        table.insert(cmd_parts, '-e')
        table.insert(cmd_parts, 'ANTHROPIC_API_KEY=' .. anthropic_key)
    end
    
    -- Set Ollama host based on OS
    table.insert(cmd_parts, '-e')
    if is_linux then
        table.insert(cmd_parts, 'OLLAMA_HOST=http://127.0.0.1:11434')
    else
        table.insert(cmd_parts, 'OLLAMA_HOST=http://host.docker.internal:11434')
    end
    
    -- Add image name
    table.insert(cmd_parts, M.config.docker.image)
    
    local cmd = table.concat(cmd_parts, ' ')
    
    vim.notify('Starting tark Docker container...', vim.log.levels.INFO)
    
    local handle = io.popen(cmd .. ' 2>&1')
    if handle then
        local result = handle:read('*a')
        handle:close()
        M.docker_container_id = result:gsub('%s+', '')
        
        -- Wait for container to be ready
        vim.defer_fn(function()
            if M.is_server_running() then
                vim.notify('tark Docker container started on port ' .. M.config.server.port, vim.log.levels.INFO)
            else
                vim.notify('tark container may have failed. Check: docker logs ' .. M.config.docker.container_name, vim.log.levels.WARN)
            end
        end, 2000)  -- Docker needs more time to start
        
        return true
    end
    
    vim.notify('Failed to start Docker container', vim.log.levels.ERROR)
    return false
end

-- Stop Docker container
function M.stop_docker()
    if M.is_docker_running() then
        os.execute('docker stop ' .. M.config.docker.container_name .. ' 2>/dev/null')
        os.execute('docker rm ' .. M.config.docker.container_name .. ' 2>/dev/null')
        vim.notify('tark Docker container stopped', vim.log.levels.INFO)
        M.docker_container_id = nil
        return true
    end
    vim.notify('No Docker container to stop', vim.log.levels.INFO)
    return false
end

-- Start the tark server (binary or Docker)
function M.start_server()
    -- Check if already running
    if M.is_server_running() then
        vim.notify('tark server is already running', vim.log.levels.INFO)
        return true
    end

    local mode = M.get_server_mode()
    
    if mode == 'docker' then
        return M.start_docker()
    elseif mode == 'binary' then
        return M.start_binary()
    else
        vim.notify(
            'No tark server available. Install options:\n\n' ..
            '1. Docker (easiest):\n' ..
            '   docker pull ghcr.io/thoughtoinnovate/tark:latest\n\n' ..
            '2. Binary:\n' ..
            '   curl -fsSL https://raw.githubusercontent.com/thoughtoinnovate/tark/main/install.sh | bash\n\n' ..
            '3. From source:\n' ..
            '   cargo install --git https://github.com/thoughtoinnovate/tark.git',
            vim.log.levels.ERROR
        )
        return false
    end
end

-- Start binary server
function M.start_binary()
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

-- Stop the tark server (binary or Docker)
function M.stop_server()
    -- Try stopping Docker container first
    if M.is_docker_running() then
        return M.stop_docker()
    end
    
    -- Try stopping binary process
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
    local docker_available = M.check_docker()
    local docker_running = M.is_docker_running()
    local mode = M.get_server_mode()
    local arch = M.get_host_arch()
    
    local lines = {
        '=== tark Server Status ===',
        '',
        'Mode: ' .. (M.config.server.mode == 'auto' and 'auto (' .. (mode or 'none') .. ')' or M.config.server.mode),
        'Server: ' .. (running and 'Running' or 'Not running'),
        'URL: http://' .. M.config.server.host .. ':' .. M.config.server.port,
        'Host Arch: ' .. arch,
        '',
        '--- Backends ---',
        'Binary: ' .. (binary_found and 'Available (' .. M.config.server.binary .. ')' or 'Not found'),
        'Docker: ' .. (docker_available and (docker_running and 'Running' or 'Available') or 'Not installed'),
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
    
    if not binary_found and not docker_available then
        table.insert(lines, '')
        table.insert(lines, 'Install options:')
        table.insert(lines, '  Docker: docker pull ghcr.io/thoughtoinnovate/tark:latest')
        table.insert(lines, '  Binary: curl -fsSL .../install.sh | bash')
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
                -- Stop binary process
                if M.server_job_id and M.server_job_id > 0 then
                    vim.fn.jobstop(M.server_job_id)
                end
                -- Stop Docker container
                if M.docker_container_id then
                    os.execute('docker stop ' .. M.config.docker.container_name .. ' 2>/dev/null')
                    os.execute('docker rm ' .. M.config.docker.container_name .. ' 2>/dev/null')
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

    -- Docker-specific commands
    vim.api.nvim_create_user_command('TarkDockerPull', function()
        M.docker_pull()
    end, { desc = 'Pull latest tark Docker image' })

    vim.api.nvim_create_user_command('TarkDockerBuild', function()
        M.docker_build()
    end, { desc = 'Build tark Docker image from source' })

    vim.api.nvim_create_user_command('TarkDockerLogs', function()
        local handle = io.popen('docker logs --tail 50 ' .. M.config.docker.container_name .. ' 2>&1')
        if handle then
            local logs = handle:read('*a')
            handle:close()
            -- Open in a new buffer
            vim.cmd('new')
            vim.api.nvim_buf_set_lines(0, 0, -1, false, vim.split(logs, '\n'))
            vim.bo.buftype = 'nofile'
            vim.bo.bufhidden = 'wipe'
            vim.bo.filetype = 'log'
            vim.api.nvim_buf_set_name(0, 'tark-docker-logs')
        end
    end, { desc = 'Show tark Docker container logs' })

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

