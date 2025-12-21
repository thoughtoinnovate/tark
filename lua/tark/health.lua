-- Health check for tark
local M = {}

function M.check()
    vim.health.start("tark")
    
    -- Check if server is running
    local handle = io.popen("curl -s http://localhost:8765/health 2>/dev/null")
    if handle then
        local result = handle:read("*a")
        handle:close()
        
        if result and result:match('"status":"ok"') then
            vim.health.ok("tark server is running")
            
            -- Extract provider
            local provider = result:match('"current_provider":"([^"]+)"')
            if provider then
                vim.health.info("Current provider: " .. provider)
            end
        else
            vim.health.error("tark server is not running", {
                "Start with: tark serve",
                "Or: cd /path/to/tark-cli && cargo run -- serve",
            })
        end
    else
        vim.health.error("Could not check server status")
    end
    
    -- Check for API keys
    local openai_key = os.getenv("OPENAI_API_KEY")
    local anthropic_key = os.getenv("ANTHROPIC_API_KEY")
    
    if openai_key then
        vim.health.ok("OPENAI_API_KEY is set")
    else
        vim.health.warn("OPENAI_API_KEY not set (needed for OpenAI provider)")
    end
    
    if anthropic_key then
        vim.health.ok("ANTHROPIC_API_KEY is set")
    else
        vim.health.info("ANTHROPIC_API_KEY not set (optional, for Claude provider)")
    end
    
    -- Check blink.cmp integration
    local has_blink = pcall(require, 'blink.cmp')
    if has_blink then
        vim.health.ok("blink.cmp detected - Tab integration enabled")
    else
        vim.health.info("blink.cmp not found - using standalone Tab mapping")
    end
end

return M

