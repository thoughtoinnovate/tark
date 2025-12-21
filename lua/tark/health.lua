-- Health check for tark
local M = {}

-- Check if a command exists
local function command_exists(cmd)
    local handle = io.popen('which ' .. cmd .. ' 2>/dev/null || where ' .. cmd .. ' 2>nul')
    if handle then
        local result = handle:read('*a')
        handle:close()
        return result and result ~= ''
    end
    return false
end

-- Get OS type
local function get_os()
    local uname = io.popen('uname -s 2>/dev/null')
    if uname then
        local os_name = uname:read('*l')
        uname:close()
        if os_name then
            if os_name:match('Darwin') then return 'macos'
            elseif os_name:match('Linux') then return 'linux'
            end
        end
    end
    -- Fallback: check for Windows
    if os.getenv('OS') and os.getenv('OS'):match('Windows') then
        return 'windows'
    end
    return 'unknown'
end

function M.check()
    vim.health.start("tark")
    
    -- 1. Check if tark binary is installed
    vim.health.start("Binary Installation")
    
    local binary_found = command_exists('tark')
    if binary_found then
        vim.health.ok("tark binary found in PATH")
        
        -- Get version
        local version_handle = io.popen('tark --version 2>/dev/null')
        if version_handle then
            local version = version_handle:read('*l')
            version_handle:close()
            if version then
                vim.health.info("Version: " .. version)
            end
        end
        
        -- Get binary path and show checksum for verification
        local which_handle = io.popen('which tark 2>/dev/null || where tark 2>nul')
        if which_handle then
            local binary_path = which_handle:read('*l')
            which_handle:close()
            if binary_path and binary_path ~= '' then
                vim.health.info("Binary path: " .. binary_path)
                
                -- Calculate SHA256 for verification
                local sha_handle = io.popen('sha256sum "' .. binary_path .. '" 2>/dev/null || shasum -a 256 "' .. binary_path .. '" 2>/dev/null')
                if sha_handle then
                    local sha_output = sha_handle:read('*l')
                    sha_handle:close()
                    if sha_output then
                        local sha256 = sha_output:match('^(%w+)')
                        if sha256 then
                            vim.health.info("SHA256: " .. sha256)
                            vim.health.info("Verify at: https://github.com/thoughtoinnovate/tark/releases")
                        end
                    end
                end
            end
        end
    else
        local os_type = get_os()
        local install_hints = {
            "tark binary not found in PATH",
            "",
            "Install options:",
        }
        
        -- Add OS-specific installation instructions
        if os_type == 'macos' then
            table.insert(install_hints, "  brew install thoughtoinnovate/tap/tark  (coming soon)")
        elseif os_type == 'linux' then
            table.insert(install_hints, "  Download from: https://github.com/thoughtoinnovate/tark/releases")
        end
        
        table.insert(install_hints, "  cargo install --git https://github.com/thoughtoinnovate/tark.git")
        table.insert(install_hints, "")
        table.insert(install_hints, "Or build from source:")
        table.insert(install_hints, "  git clone https://github.com/thoughtoinnovate/tark.git")
        table.insert(install_hints, "  cd tark && cargo install --path .")
        
        vim.health.error("tark binary not found", install_hints)
    end
    
    -- 2. Check if server is running
    vim.health.start("Server Status")
    
    local config = require('tark').config
    local server_url = string.format('http://%s:%d/health', 
        config.server.host or '127.0.0.1', 
        config.server.port or 8765)
    
    local handle = io.popen('curl -s --max-time 2 ' .. server_url .. ' 2>/dev/null')
    if handle then
        local result = handle:read('*a')
        handle:close()
        
        if result and result:match('"status":"ok"') then
            vim.health.ok("tark server is running")
            
            -- Extract details
            local provider = result:match('"current_provider":"([^"]+)"')
            local version = result:match('"version":"([^"]+)"')
            if provider then
                vim.health.info("Current provider: " .. provider)
            end
            if version then
                vim.health.info("Server version: " .. version)
            end
        else
            vim.health.warn("tark server is not running", {
                "Auto-start is " .. (config.server.auto_start and "enabled" or "disabled"),
                "",
                "To start manually:",
                "  :TarkServerStart",
                "  or run: tark serve",
            })
        end
    else
        vim.health.error("Could not check server status (curl failed)")
    end
    
    -- 3. Check for API keys
    vim.health.start("LLM Providers")
    
    local openai_key = os.getenv("OPENAI_API_KEY")
    local anthropic_key = os.getenv("ANTHROPIC_API_KEY")
    local ollama_running = false
    
    -- Check Ollama
    local ollama_handle = io.popen('curl -s --max-time 1 http://localhost:11434/api/tags 2>/dev/null')
    if ollama_handle then
        local ollama_result = ollama_handle:read('*a')
        ollama_handle:close()
        ollama_running = ollama_result and ollama_result:match('"models"')
    end
    
    if ollama_running then
        vim.health.ok("Ollama is running (local models available)")
    else
        vim.health.info("Ollama not running (optional, for local models)")
    end
    
    if openai_key then
        vim.health.ok("OPENAI_API_KEY is set")
    else
        vim.health.warn("OPENAI_API_KEY not set", {
            "Set with: export OPENAI_API_KEY='sk-...'",
        })
    end
    
    if anthropic_key then
        vim.health.ok("ANTHROPIC_API_KEY is set")
    else
        vim.health.info("ANTHROPIC_API_KEY not set (optional, for Claude)")
    end
    
    if not openai_key and not anthropic_key and not ollama_running then
        vim.health.error("No LLM provider available", {
            "You need at least one of:",
            "  - OPENAI_API_KEY environment variable",
            "  - ANTHROPIC_API_KEY environment variable", 
            "  - Ollama running locally (ollama serve)",
        })
    end
    
    -- 4. Check plugin integration
    vim.health.start("Plugin Integration")
    
    local has_blink = pcall(require, 'blink.cmp')
    if has_blink then
        vim.health.ok("blink.cmp detected - Tab integration enabled")
    else
        vim.health.info("blink.cmp not found - using standalone Tab mapping")
    end
    
    local has_lspconfig = pcall(require, 'lspconfig')
    if has_lspconfig then
        vim.health.ok("nvim-lspconfig available")
    else
        vim.health.info("nvim-lspconfig not found (optional, for tark LSP)")
    end
    
    -- Config summary
    vim.health.start("Configuration")
    vim.health.info("Ghost text: " .. (config.ghost_text.enabled and "enabled" or "disabled"))
    vim.health.info("Chat: " .. (config.chat.enabled and "enabled" or "disabled"))
    vim.health.info("LSP: " .. (config.lsp.enabled and "enabled" or "disabled"))
    vim.health.info("Auto-start server: " .. (config.server.auto_start and "yes" or "no"))
end

return M

