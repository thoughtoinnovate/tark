-- tark server management (Docker and binary modes)
-- Handles starting, stopping, and health checking the tark server

local M = {}

-- Server state
M.state = {
    running = false,
    mode = nil,  -- 'binary' or 'docker'
    pid = nil,
    container_id = nil,
}

-- Default configuration
M.config = {
    mode = 'auto',  -- 'auto', 'binary', 'docker'
    binary = 'tark',
    host = '127.0.0.1',
    port = 8765,
    auto_start = true,
    stop_on_exit = true,
    docker = {
        image = 'ghcr.io/thoughtoinnovate/tark:alpine',
        container_name = 'tark-server',
        pull_on_start = true,
        build_local = false,
        dockerfile = 'alpine',  -- 'alpine' or 'minimal'
        mount_workspace = true,
    },
}

-- Check if a command exists
local function command_exists(cmd)
    local handle = io.popen('command -v ' .. cmd .. ' 2>/dev/null')
    if handle then
        local result = handle:read('*a')
        handle:close()
        return result ~= nil and result ~= ''
    end
    return false
end

-- Get the plugin's data directory for storing the binary
local function get_data_dir()
    local data_dir = vim.fn.stdpath('data') .. '/tark'
    if vim.fn.isdirectory(data_dir) == 0 then
        vim.fn.mkdir(data_dir, 'p')
    end
    return data_dir
end

-- Get the local binary path (in plugin data dir)
local function get_local_binary_path()
    return get_data_dir() .. '/tark'
end

-- Detect platform for binary download
-- Returns: os, arch, binary_name
local function detect_platform()
    local uname = vim.loop.os_uname()
    local os_name = uname.sysname
    local arch = uname.machine
    
    -- Detect OS
    local os_key
    if os_name == 'Linux' then
        os_key = 'linux'
    elseif os_name == 'Darwin' then
        os_key = 'darwin'
    elseif os_name == 'FreeBSD' then
        os_key = 'freebsd'
    elseif os_name:match('Windows') or os_name:match('MINGW') or os_name:match('MSYS') then
        os_key = 'windows'
    else
        os_key = 'linux'  -- fallback
    end
    
    -- Detect architecture
    local arch_key
    if arch == 'x86_64' or arch == 'amd64' then
        arch_key = 'x86_64'
    elseif arch == 'aarch64' or arch == 'arm64' then
        arch_key = 'arm64'
    elseif arch:match('arm') then
        arch_key = 'arm64'  -- assume ARM64 for other ARM variants
    else
        arch_key = 'x86_64'  -- fallback
    end
    
    -- Build binary name (Windows needs .exe extension)
    local binary_name
    if os_key == 'windows' then
        binary_name = 'tark-' .. os_key .. '-' .. arch_key .. '.exe'
    else
        binary_name = 'tark-' .. os_key .. '-' .. arch_key
    end
    
    return os_key, arch_key, binary_name
end

-- Download tark binary automatically with SHA256 verification
function M.download_binary(callback)
    local os_key, arch_key, binary_name = detect_platform()
    
    -- Determine download URL based on channel
    local channel = M.config.channel or 'stable'
    local base_url
    local version_display
    
    if channel == 'nightly' then
        -- Use nightly release (manual builds)
        base_url = 'https://github.com/thoughtoinnovate/tark/releases/download/nightly/'
        version_display = 'nightly'
    elseif channel == 'latest' then
        -- Use latest stable release
        base_url = 'https://github.com/thoughtoinnovate/tark/releases/latest/download/'
        version_display = 'latest'
    else
        -- Use pinned version matching plugin (stable)
        local tark = require('tark')
        local version = 'v' .. tark.version
        base_url = 'https://github.com/thoughtoinnovate/tark/releases/download/' .. version .. '/'
        version_display = version .. ' (stable)'
    end
    
    local binary_url = base_url .. binary_name
    local checksum_url = binary_url .. '.sha256'
    local dest = get_local_binary_path()
    local checksum_file = dest .. '.sha256'
    local platform_display = os_key .. '-' .. arch_key
    
    vim.notify('tark: Downloading ' .. version_display .. ' binary for ' .. platform_display .. '...', vim.log.levels.INFO)
    
    -- First check if the release exists (HEAD request)
    local check_cmd = string.format('curl -sfI "%s" >/dev/null 2>&1', binary_url)
    local check_result = os.execute(check_cmd)
    
    if check_result ~= 0 then
        local error_msg = string.format([[
tark: No binary available for download!

Channel: %s
URL: %s

]], version_display, binary_url)
        
        if channel == 'nightly' then
            error_msg = error_msg .. [[Nightly release does not exist yet.

To create nightly builds:
1. Go to GitHub → Actions → "Manual Build"
2. Click "Run workflow" → Select platforms → Run
3. Wait for build to complete (~10 min)
4. Then restart Neovim

Or switch to Docker mode:
  opts = { server = { mode = 'docker' } }]]
        elseif channel == 'stable' or channel == 'latest' then
            error_msg = error_msg .. [[No stable release exists yet.

To create a release:
  git tag v0.1.0
  git push --tags

Or use Docker mode:
  opts = { server = { mode = 'docker' } }

Or try nightly (if manual builds exist):
  opts = { server = { channel = 'nightly' } }]]
        end
        
        vim.notify(error_msg, vim.log.levels.ERROR)
        if callback then callback(false) end
        return
    end
    
    -- Download binary and checksum, then verify
    local download_cmd = string.format(
        'curl -fsSL "%s" -o "%s" && curl -fsSL "%s" -o "%s"',
        binary_url, dest, checksum_url, checksum_file
    )
    
    vim.fn.jobstart(download_cmd, {
        on_exit = function(_, code)
            vim.schedule(function()
                if code ~= 0 then
                    vim.notify('tark: Failed to download binary. Check your internet connection.', vim.log.levels.ERROR)
                    if callback then callback(false) end
                    return
                end
                
                -- Verify SHA256 checksum
                vim.notify('tark: Verifying checksum...', vim.log.levels.INFO)
                
                -- Read expected checksum
                local checksum_handle = io.open(checksum_file, 'r')
                if not checksum_handle then
                    vim.notify('tark: Could not read checksum file, skipping verification', vim.log.levels.WARN)
                else
                    local expected = checksum_handle:read('*a'):match('^(%S+)')
                    checksum_handle:close()
                    os.remove(checksum_file)
                    
                    if expected then
                        -- Calculate actual checksum
                        local sha_cmd = vim.fn.executable('sha256sum') == 1 
                            and 'sha256sum' 
                            or 'shasum -a 256'
                        local sha_handle = io.popen(sha_cmd .. ' "' .. dest .. '" 2>/dev/null')
                        if sha_handle then
                            local actual = sha_handle:read('*a'):match('^(%S+)')
                            sha_handle:close()
                            
                            if actual ~= expected then
                                vim.notify('tark: SECURITY ALERT - Checksum verification FAILED!\nExpected: ' .. expected .. '\nActual: ' .. actual, vim.log.levels.ERROR)
                                os.remove(dest)
                                if callback then callback(false) end
                                return
                            end
                            vim.notify('tark: Checksum verified ✓', vim.log.levels.INFO)
                        end
                    end
                end
                
                -- Make executable
                vim.fn.system('chmod +x "' .. dest .. '"')
                
                -- Verify the binary works
                local handle = io.popen(dest .. ' --version 2>&1')
                if handle then
                    local result = handle:read('*a')
                    handle:close()
                    if result and result:match('tark') then
                        vim.notify('tark: Binary downloaded and verified successfully!', vim.log.levels.INFO)
                        M.config.binary = dest
                        if callback then callback(true) end
                        return
                    end
                end
                
                vim.notify('tark: Downloaded file is not a valid tark binary', vim.log.levels.ERROR)
                os.remove(dest)
                if callback then callback(false) end
            end)
        end,
    })
end

-- Check if local binary exists and is valid
local function local_binary_available()
    local binary_path = get_local_binary_path()
    if vim.fn.filereadable(binary_path) == 1 then
        local handle = io.popen(binary_path .. ' --version 2>&1')
        if handle then
            local result = handle:read('*a')
            handle:close()
            if result and result:match('tark') then
                return true, binary_path
            end
        end
    end
    return false
end

-- Check if Docker is available and running
function M.docker_available()
    if not command_exists('docker') then
        return false, 'Docker not installed'
    end
    
    -- Check if Docker daemon is running
    local handle = io.popen('docker info 2>&1')
    if handle then
        local result = handle:read('*a')
        handle:close()
        if result:match('error') or result:match('Cannot connect') then
            return false, 'Docker daemon not running'
        end
        return true
    end
    return false, 'Could not check Docker status'
end

-- Check if tark binary is available (system or local)
function M.binary_available()
    -- First check system binary
    local binary = M.config.binary
    if command_exists(binary) then
        local handle = io.popen(binary .. ' --version 2>&1')
        if handle then
            local result = handle:read('*a')
            handle:close()
            if result:match('tark') then
                return true, result:gsub('%s+$', '')
            end
        end
    end
    
    -- Check local binary (downloaded by plugin)
    local local_ok, local_path = local_binary_available()
    if local_ok then
        M.config.binary = local_path
        return true, 'local: ' .. local_path
    end
    
    return false, 'Binary not found (system or local)'
end

-- Check if server is responding
function M.health_check()
    local url = string.format('http://%s:%d/health', M.config.host, M.config.port)
    local handle = io.popen('curl -s --connect-timeout 1 ' .. url .. ' 2>/dev/null')
    if handle then
        local result = handle:read('*a')
        handle:close()
        if result and result ~= '' then
            local ok, resp = pcall(vim.fn.json_decode, result)
            if ok and resp and resp.status == 'ok' then
                return true, resp
            end
        end
    end
    return false
end

-- Check if Docker container is running
function M.container_running()
    local name = M.config.docker.container_name
    local handle = io.popen('docker ps -q -f name=' .. name .. ' 2>/dev/null')
    if handle then
        local result = handle:read('*a')
        handle:close()
        if result and result ~= '' then
            M.state.container_id = result:gsub('%s+$', '')
            return true
        end
    end
    return false
end

-- Pull Docker image
function M.docker_pull(callback)
    local image = M.config.docker.image
    vim.notify('Pulling tark Docker image: ' .. image, vim.log.levels.INFO)
    
    vim.fn.jobstart({ 'docker', 'pull', image }, {
        on_exit = function(_, code)
            vim.schedule(function()
                if code == 0 then
                    vim.notify('Docker image pulled successfully', vim.log.levels.INFO)
                    if callback then callback(true) end
                else
                    vim.notify('Failed to pull Docker image', vim.log.levels.ERROR)
                    if callback then callback(false) end
                end
            end)
        end,
        on_stdout = function(_, data)
            -- Progress output
            if data and data[1] and data[1] ~= '' then
                vim.schedule(function()
                    -- Silent progress - too noisy to show every line
                end)
            end
        end,
    })
end

-- Build Docker image locally
function M.docker_build(callback)
    -- Find the plugin directory
    local plugin_dir = vim.fn.fnamemodify(debug.getinfo(1, 'S').source:sub(2), ':h:h:h')
    local dockerfile = M.config.docker.dockerfile == 'minimal' and 'Dockerfile' or 'Dockerfile.alpine'
    local image_tag = M.config.docker.image
    
    vim.notify('Building tark Docker image from ' .. dockerfile .. '...', vim.log.levels.INFO)
    
    vim.fn.jobstart({
        'docker', 'build',
        '-t', image_tag,
        '-f', plugin_dir .. '/' .. dockerfile,
        plugin_dir
    }, {
        on_exit = function(_, code)
            vim.schedule(function()
                if code == 0 then
                    vim.notify('Docker image built successfully: ' .. image_tag, vim.log.levels.INFO)
                    if callback then callback(true) end
                else
                    vim.notify('Failed to build Docker image', vim.log.levels.ERROR)
                    if callback then callback(false) end
                end
            end)
        end,
        on_stdout = function(_, data)
            if data and data[1] and data[1] ~= '' then
                vim.schedule(function()
                    -- Show build progress
                    for _, line in ipairs(data) do
                        if line:match('^Step') then
                            vim.notify(line, vim.log.levels.INFO)
                        end
                    end
                end)
            end
        end,
        on_stderr = function(_, data)
            if data and data[1] and data[1] ~= '' then
                vim.schedule(function()
                    for _, line in ipairs(data) do
                        if line:match('^Step') or line:match('Successfully') then
                            vim.notify(line, vim.log.levels.INFO)
                        end
                    end
                end)
            end
        end,
    })
end

-- Start server in Docker mode
function M.start_docker(callback)
    local docker_ok, docker_err = M.docker_available()
    if not docker_ok then
        vim.notify('Docker not available: ' .. docker_err, vim.log.levels.ERROR)
        if callback then callback(false) end
        return
    end
    
    -- Check if container already running
    if M.container_running() then
        vim.notify('tark container already running', vim.log.levels.INFO)
        M.state.running = true
        M.state.mode = 'docker'
        if callback then callback(true) end
        return
    end
    
    -- Remove old container if exists
    local name = M.config.docker.container_name
    vim.fn.system('docker rm -f ' .. name .. ' 2>/dev/null')
    
    local function do_start()
        -- Build docker run command
        local cmd = {
            'docker', 'run', '-d',
            '--name', name,
            '-p', string.format('%d:%d', M.config.port, 8765),
        }
        
        -- Add host.docker.internal for accessing host services (Ollama)
        local os_name = vim.loop.os_uname().sysname
        if os_name == 'Darwin' or os_name:match('Windows') then
            -- macOS and Windows
            table.insert(cmd, '--add-host')
            table.insert(cmd, 'host.docker.internal:host-gateway')
        elseif os_name == 'Linux' then
            -- Linux: use host network for better Ollama access
            -- Remove the -p flag and use host networking
            cmd = {
                'docker', 'run', '-d',
                '--name', name,
                '--network', 'host',
            }
        end
        
        -- Pass API keys from environment
        local api_keys = {
            'OPENAI_API_KEY',
            'ANTHROPIC_API_KEY', 
            'OLLAMA_HOST',
        }
        for _, key in ipairs(api_keys) do
            local val = os.getenv(key)
            if val and val ~= '' then
                table.insert(cmd, '-e')
                table.insert(cmd, key .. '=' .. val)
            end
        end
        
        -- Set Ollama host for Docker
        if not os.getenv('OLLAMA_HOST') then
            table.insert(cmd, '-e')
            if os_name == 'Linux' then
                table.insert(cmd, 'OLLAMA_HOST=http://127.0.0.1:11434')
            else
                table.insert(cmd, 'OLLAMA_HOST=http://host.docker.internal:11434')
            end
        end
        
        -- Mount workspace if configured
        if M.config.docker.mount_workspace then
            local cwd = vim.fn.getcwd()
            table.insert(cmd, '-v')
            table.insert(cmd, cwd .. ':/workspace:rw')
            table.insert(cmd, '-w')
            table.insert(cmd, '/workspace')
        end
        
        -- Add image and command
        table.insert(cmd, M.config.docker.image)
        table.insert(cmd, 'serve')
        table.insert(cmd, '--host')
        table.insert(cmd, '0.0.0.0')
        table.insert(cmd, '--port')
        table.insert(cmd, '8765')
        
        vim.notify('Starting tark Docker container...', vim.log.levels.INFO)
        
        vim.fn.jobstart(cmd, {
            on_exit = function(_, code)
                vim.schedule(function()
                    if code == 0 then
                        -- Wait for server to be ready
                        local attempts = 0
                        local max_attempts = 30
                        local function check_ready()
                            attempts = attempts + 1
                            if M.health_check() then
                                M.state.running = true
                                M.state.mode = 'docker'
                                vim.notify('tark server started (Docker)', vim.log.levels.INFO)
                                if callback then callback(true) end
                            elseif attempts < max_attempts then
                                vim.defer_fn(check_ready, 500)
                            else
                                vim.notify('tark server started but health check failed', vim.log.levels.WARN)
                                M.state.running = true
                                M.state.mode = 'docker'
                                if callback then callback(true) end
                            end
                        end
                        vim.defer_fn(check_ready, 1000)
                    else
                        vim.notify('Failed to start tark Docker container', vim.log.levels.ERROR)
                        if callback then callback(false) end
                    end
                end)
            end,
        })
    end
    
    -- Pull image first if configured
    if M.config.docker.pull_on_start and not M.config.docker.build_local then
        M.docker_pull(function(success)
            if success then
                do_start()
            elseif callback then
                callback(false)
            end
        end)
    elseif M.config.docker.build_local then
        M.docker_build(function(success)
            if success then
                do_start()
            elseif callback then
                callback(false)
            end
        end)
    else
        do_start()
    end
end

-- Start server in binary mode
function M.start_binary(callback)
    local binary_ok, binary_info = M.binary_available()
    if not binary_ok then
        vim.notify('tark binary not available: ' .. binary_info, vim.log.levels.ERROR)
        if callback then callback(false) end
        return
    end
    
    -- Check if server already running
    if M.health_check() then
        vim.notify('tark server already running', vim.log.levels.INFO)
        M.state.running = true
        M.state.mode = 'binary'
        if callback then callback(true) end
        return
    end
    
    local cmd = {
        M.config.binary,
        'serve',
        '--host', M.config.host,
        '--port', tostring(M.config.port),
    }
    
    vim.notify('Starting tark server (binary)...', vim.log.levels.INFO)
    
    local job_id = vim.fn.jobstart(cmd, {
        detach = true,
        on_exit = function(_, code)
            vim.schedule(function()
                if code ~= 0 and M.state.running then
                    M.state.running = false
                    vim.notify('tark server exited unexpectedly', vim.log.levels.WARN)
                end
            end)
        end,
    })
    
    if job_id > 0 then
        M.state.pid = job_id
        -- Wait for server to be ready
        local attempts = 0
        local max_attempts = 20
        local function check_ready()
            attempts = attempts + 1
            if M.health_check() then
                M.state.running = true
                M.state.mode = 'binary'
                vim.notify('tark server started (binary)', vim.log.levels.INFO)
                if callback then callback(true) end
            elseif attempts < max_attempts then
                vim.defer_fn(check_ready, 250)
            else
                vim.notify('tark server started but health check failed', vim.log.levels.WARN)
                M.state.running = true
                M.state.mode = 'binary'
                if callback then callback(true) end
            end
        end
        vim.defer_fn(check_ready, 500)
    else
        vim.notify('Failed to start tark server', vim.log.levels.ERROR)
        if callback then callback(false) end
    end
end

-- Start server (auto-detect mode)
function M.start(callback)
    local mode = M.config.mode
    
    if mode == 'binary' then
        -- Binary mode: try to find or auto-download
        local binary_ok = M.binary_available()
        if binary_ok then
            M.start_binary(callback)
        else
            -- Auto-download binary
            vim.notify('tark: Binary not found. Downloading...', vim.log.levels.INFO)
            M.download_binary(function(success)
                if success then
                    M.start_binary(callback)
                elseif callback then
                    callback(false)
                end
            end)
        end
    elseif mode == 'docker' then
        M.start_docker(callback)
    else  -- 'auto'
        -- Try binary first, then auto-download, fallback to Docker only as last resort
        local binary_ok = M.binary_available()
        if binary_ok then
            M.start_binary(callback)
        else
            -- Binary not found, try to auto-download first
            vim.notify('tark: Binary not found. Downloading...', vim.log.levels.INFO)
            M.download_binary(function(success)
                if success then
                    M.start_binary(callback)
                else
                    -- Download failed, try Docker as fallback
                    local docker_ok = M.docker_available()
                    if docker_ok then
                        vim.notify('tark: Binary download failed. Falling back to Docker...', vim.log.levels.INFO)
                        M.start_docker(callback)
                    else
                        vim.notify([[
tark: Could not start server.

Install options:
1. Binary (auto): Restart Neovim to retry download
2. Binary (manual): curl -fsSL https://raw.githubusercontent.com/thoughtoinnovate/tark/main/install.sh | bash
3. Docker: Install Docker and restart Neovim
]], vim.log.levels.ERROR)
                        if callback then callback(false) end
                    end
                end
            end)
        end
    end
end

-- Stop server
function M.stop(callback)
    if not M.state.running then
        vim.notify('tark server is not running', vim.log.levels.INFO)
        if callback then callback(true) end
        return
    end
    
    if M.state.mode == 'docker' then
        local name = M.config.docker.container_name
        vim.notify('Stopping tark Docker container...', vim.log.levels.INFO)
        
        vim.fn.jobstart({ 'docker', 'stop', name }, {
            on_exit = function(_, code)
                vim.schedule(function()
                    M.state.running = false
                    M.state.container_id = nil
                    if code == 0 then
                        vim.notify('tark server stopped', vim.log.levels.INFO)
                    end
                    if callback then callback(code == 0) end
                end)
            end,
        })
    elseif M.state.mode == 'binary' then
        -- Try graceful shutdown via curl
        local url = string.format('http://%s:%d/shutdown', M.config.host, M.config.port)
        vim.fn.system('curl -s -X POST ' .. url .. ' 2>/dev/null')
        
        -- Also try killing the process
        if M.state.pid then
            vim.fn.jobstop(M.state.pid)
        end
        
        M.state.running = false
        M.state.pid = nil
        vim.notify('tark server stopped', vim.log.levels.INFO)
        if callback then callback(true) end
    else
        M.state.running = false
        if callback then callback(true) end
    end
end

-- Restart server
function M.restart(callback)
    M.stop(function(stopped)
        if stopped then
            vim.defer_fn(function()
                M.start(callback)
            end, 500)
        elseif callback then
            callback(false)
        end
    end)
end

-- Get server status
function M.status()
    -- Detect platform
    local os_key, arch_key, binary_name = detect_platform()
    
    local status = {
        running = false,
        mode = M.state.mode,
        url = string.format('http://%s:%d', M.config.host, M.config.port),
        platform = os_key .. '-' .. arch_key,
        binary_name = binary_name,
        channel = M.config.channel or 'stable',
    }
    
    -- Check actual health
    local healthy, resp = M.health_check()
    status.running = healthy
    
    if healthy and resp then
        status.version = resp.version
        status.provider = resp.provider
    end
    
    -- Check Docker container status
    if M.config.mode == 'docker' or M.config.mode == 'auto' then
        status.container_running = M.container_running()
        status.container_name = M.config.docker.container_name
        status.image = M.config.docker.image
    end
    
    -- Check binary availability
    local binary_ok, binary_info = M.binary_available()
    status.binary_available = binary_ok
    status.binary_info = binary_info
    
    -- Check Docker availability
    local docker_ok, docker_info = M.docker_available()
    status.docker_available = docker_ok
    status.docker_info = docker_info
    
    return status
end

-- Setup server management
function M.setup(opts)
    M.config = vim.tbl_deep_extend('force', M.config, opts or {})
    
    -- Setup auto-stop on Neovim exit
    if M.config.stop_on_exit then
        vim.api.nvim_create_autocmd('VimLeavePre', {
            callback = function()
                if M.state.running and M.state.mode == 'docker' then
                    -- Sync stop for Docker (we can't do async in VimLeavePre)
                    local name = M.config.docker.container_name
                    vim.fn.system('docker stop ' .. name .. ' 2>/dev/null')
                end
            end,
        })
    end
    
    -- Auto-start if configured
    if M.config.auto_start then
        vim.defer_fn(function()
            M.start()
        end, 100)
    end
end

return M

