-- tark health check module
-- Used by :checkhealth tark

local M = {}

local health = vim.health or require('health')
local start = health.start or health.report_start
local ok = health.ok or health.report_ok
local warn = health.warn or health.report_warn
local error_fn = health.error or health.report_error
local info = health.info or health.report_info

function M.check()
    start('tark')
    
    -- Check Neovim version
    start('Neovim')
    local nvim_version = vim.version()
    if nvim_version.major >= 0 and nvim_version.minor >= 9 then
        ok('Neovim version: ' .. nvim_version.major .. '.' .. nvim_version.minor .. '.' .. nvim_version.patch)
    else
        warn('Neovim 0.9+ recommended, you have: ' .. nvim_version.major .. '.' .. nvim_version.minor)
    end
    
    -- Check curl (required for HTTP communication)
    start('Dependencies')
    local curl_ok = vim.fn.executable('curl') == 1
    if curl_ok then
        ok('curl is available')
    else
        error_fn('curl not found - required for server communication')
    end
    
    -- Check tark module loaded
    local tark_ok, tark = pcall(require, 'tark')
    if tark_ok then
        ok('tark module loaded (v' .. (tark.version or 'unknown') .. ')')
    else
        error_fn('Failed to load tark module')
        return
    end
    
    -- Check binary availability
    start('tark Binary')
    local server = require('tark.server')
    local binary_ok, binary_info = server.binary_available()
    if binary_ok then
        ok('Binary found: ' .. binary_info)
        
        -- Get binary hash for verification
        local binary_path = vim.fn.exepath('tark')
        if binary_path and binary_path ~= '' then
            local hash_cmd = vim.fn.has('mac') == 1 and 'shasum -a 256' or 'sha256sum'
            local hash = vim.fn.system(hash_cmd .. ' ' .. binary_path .. ' 2>/dev/null')
            if hash and hash ~= '' then
                local sha = hash:match('^(%S+)')
                if sha then
                    info('SHA256: ' .. sha)
                end
            end
        end
    else
        info('Binary not installed: ' .. (binary_info or 'not found'))
        info('Install with: curl -fsSL https://raw.githubusercontent.com/thoughtoinnovate/tark/main/install.sh | bash')
    end
    
    -- Check Docker availability
    start('Docker')
    local docker_ok, docker_info = server.docker_available()
    if docker_ok then
        ok('Docker is available')
        
        -- Check if tark image exists
        local image = server.config.docker.image
        local handle = io.popen('docker images -q ' .. image .. ' 2>/dev/null')
        if handle then
            local result = handle:read('*a')
            handle:close()
            if result and result ~= '' then
                ok('Docker image present: ' .. image)
            else
                info('Docker image not pulled yet: ' .. image)
                info('Run :TarkDockerPull to download')
            end
        end
        
        -- Check if container is running
        if server.container_running() then
            ok('Docker container running: ' .. server.config.docker.container_name)
        else
            info('Docker container not running')
        end
    else
        info('Docker: ' .. (docker_info or 'not available'))
        info('Docker is optional - you can use the binary instead')
    end
    
    -- Check server status
    start('Server')
    local healthy, resp = server.health_check()
    if healthy then
        ok('Server is running and healthy')
        if resp then
            if resp.version then
                info('Server version: ' .. resp.version)
            end
            if resp.provider then
                info('Current provider: ' .. resp.provider)
            end
        end
    else
        warn('Server is not running')
        info('Start with :TarkServerStart')
    end
    
    -- Check API keys
    start('API Keys')
    local openai_key = os.getenv('OPENAI_API_KEY')
    local anthropic_key = os.getenv('ANTHROPIC_API_KEY')
    local ollama_host = os.getenv('OLLAMA_HOST') or 'http://localhost:11434'
    
    if openai_key and openai_key ~= '' then
        ok('OPENAI_API_KEY is set')
    else
        info('OPENAI_API_KEY not set')
    end
    
    if anthropic_key and anthropic_key ~= '' then
        ok('ANTHROPIC_API_KEY is set')
    else
        info('ANTHROPIC_API_KEY not set')
    end
    
    -- Check Ollama
    local ollama_ok = vim.fn.system('curl -s --connect-timeout 2 ' .. ollama_host .. '/api/tags 2>/dev/null')
    if ollama_ok and ollama_ok ~= '' and not ollama_ok:match('error') then
        ok('Ollama is running at ' .. ollama_host)
    else
        info('Ollama not detected at ' .. ollama_host)
    end
    
    if not openai_key and not anthropic_key and (not ollama_ok or ollama_ok == '') then
        warn('No LLM provider configured')
        info('Set OPENAI_API_KEY, ANTHROPIC_API_KEY, or start Ollama')
    end
    
    -- Configuration info
    start('Configuration')
    local config = tark.config
    info('Server mode: ' .. config.server.mode)
    info('Server URL: http://' .. config.server.host .. ':' .. config.server.port)
    info('Auto-start: ' .. (config.server.auto_start and 'yes' or 'no'))
    info('Ghost text: ' .. (config.ghost_text.enabled and 'enabled' or 'disabled'))
    info('Chat: ' .. (config.chat.enabled and 'enabled' or 'disabled'))
    info('LSP: ' .. (config.lsp.enabled and 'enabled' or 'disabled'))
end

return M
