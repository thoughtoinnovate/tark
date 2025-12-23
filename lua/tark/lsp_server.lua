--- LSP Proxy Server for tark
--- Provides HTTP endpoints for the tark agent to call Neovim's LSP
--- Uses dynamic port binding (port 0) to support multiple Neovim instances
---@module tark.lsp_server

local M = {}

M.port = nil
M.server = nil
M.clients = {}

-- Lazy-load LSP module
local function get_lsp()
    return require('tark.lsp')
end

-- HTTP response helpers
local function http_response(status, body)
    local json_body = vim.fn.json_encode(body)
    return string.format(
        'HTTP/1.1 %s\r\nContent-Type: application/json\r\nContent-Length: %d\r\nConnection: close\r\n\r\n%s',
        status,
        #json_body,
        json_body
    )
end

local function http_ok(body)
    return http_response('200 OK', body)
end

local function http_error(message)
    return http_response('400 Bad Request', { error = message })
end

-- Parse HTTP request
local function parse_request(data)
    -- Extract method and path
    local method, path = data:match('^(%w+)%s+([^%s]+)')
    if not method or not path then
        return nil, 'Invalid request'
    end
    
    -- Extract body (after double CRLF)
    local body_start = data:find('\r\n\r\n')
    local body = nil
    if body_start then
        local raw_body = data:sub(body_start + 4)
        if #raw_body > 0 then
            local ok, parsed = pcall(vim.fn.json_decode, raw_body)
            if ok then
                body = parsed
            end
        end
    end
    
    return {
        method = method,
        path = path,
        body = body or {},
    }
end

-- Route handlers
local handlers = {}

-- POST /lsp/diagnostics - Get diagnostics for a file
handlers['/lsp/diagnostics'] = function(req, callback)
    local file = req.body.file
    if not file then
        callback(http_error('Missing file parameter'))
        return
    end
    
    -- Find buffer for file
    local bufnr = vim.fn.bufnr(file)
    if bufnr == -1 then
        -- File not open, return empty
        callback(http_ok({ diagnostics = {} }))
        return
    end
    
    local diags = get_lsp().get_diagnostics(bufnr, nil, nil)
    callback(http_ok({ diagnostics = diags }))
end

-- POST /lsp/symbols - Get document symbols
handlers['/lsp/symbols'] = function(req, callback)
    local file = req.body.file
    if not file then
        callback(http_error('Missing file parameter'))
        return
    end
    
    local bufnr = vim.fn.bufnr(file)
    if bufnr == -1 then
        callback(http_ok({ symbols = {} }))
        return
    end
    
    get_lsp().get_symbols_async(bufnr, function(symbols)
        callback(http_ok({ symbols = symbols }))
    end)
end

-- POST /lsp/hover - Get hover information
handlers['/lsp/hover'] = function(req, callback)
    local file = req.body.file
    local line = req.body.line
    local col = req.body.col
    
    if not file or not line or not col then
        callback(http_error('Missing file, line, or col parameter'))
        return
    end
    
    local bufnr = vim.fn.bufnr(file)
    if bufnr == -1 then
        callback(http_ok({ hover = nil }))
        return
    end
    
    get_lsp().get_hover_async(bufnr, line, col, function(hover)
        callback(http_ok({ hover = hover }))
    end)
end

-- POST /lsp/definition - Go to definition
handlers['/lsp/definition'] = function(req, callback)
    local file = req.body.file
    local line = req.body.line
    local col = req.body.col
    
    if not file or not line or not col then
        callback(http_error('Missing file, line, or col parameter'))
        return
    end
    
    local bufnr = vim.fn.bufnr(file)
    if bufnr == -1 then
        callback(http_ok({ locations = nil }))
        return
    end
    
    get_lsp().get_definition_async(bufnr, line, col, function(locations)
        if locations then
            -- Read preview lines for each location
            for _, loc in ipairs(locations) do
                local lines = vim.fn.readfile(loc.file, '', loc.line + 5)
                if lines and #lines > loc.line then
                    loc.preview = lines[loc.line + 1]
                end
            end
        end
        callback(http_ok({ locations = locations }))
    end)
end

-- POST /lsp/references - Find all references
handlers['/lsp/references'] = function(req, callback)
    local file = req.body.file
    local line = req.body.line
    local col = req.body.col
    
    if not file or not line or not col then
        callback(http_error('Missing file, line, or col parameter'))
        return
    end
    
    local bufnr = vim.fn.bufnr(file)
    if bufnr == -1 then
        callback(http_ok({ references = nil }))
        return
    end
    
    get_lsp().get_references_async(bufnr, line, col, function(refs)
        if refs then
            -- Read preview lines for each reference
            for _, ref in ipairs(refs) do
                local lines = vim.fn.readfile(ref.file, '', ref.line + 5)
                if lines and #lines > ref.line then
                    ref.preview = lines[ref.line + 1]
                end
            end
        end
        callback(http_ok({ references = refs }))
    end)
end

-- POST /lsp/signature - Get signature help
handlers['/lsp/signature'] = function(req, callback)
    local file = req.body.file
    local line = req.body.line
    local col = req.body.col
    
    if not file or not line or not col then
        callback(http_error('Missing file, line, or col parameter'))
        return
    end
    
    local bufnr = vim.fn.bufnr(file)
    if bufnr == -1 then
        callback(http_ok({ signature = nil }))
        return
    end
    
    get_lsp().get_signature_async(bufnr, line, col, function(sig)
        callback(http_ok({ signature = sig }))
    end)
end

-- GET /health - Health check
handlers['/health'] = function(req, callback)
    callback(http_ok({ status = 'ok', port = M.port }))
end

-- Handle incoming client connection
local function handle_client(client)
    local buffer = ''
    
    client:read_start(function(err, data)
        if err then
            client:close()
            return
        end
        
        if not data then
            -- Connection closed
            client:close()
            return
        end
        
        buffer = buffer .. data
        
        -- Check if we have a complete request (ends with double CRLF for headers)
        -- For POST requests, we also need the body
        local header_end = buffer:find('\r\n\r\n')
        if not header_end then
            return  -- Wait for more data
        end
        
        -- Check Content-Length for body
        local content_length = buffer:match('Content%-Length:%s*(%d+)')
        if content_length then
            local body_start = header_end + 4
            local expected_length = body_start + tonumber(content_length) - 1
            if #buffer < expected_length then
                return  -- Wait for more body data
            end
        end
        
        -- Parse and handle request
        local req, parse_err = parse_request(buffer)
        if not req then
            local response = http_error(parse_err or 'Parse error')
            client:write(response, function()
                client:close()
            end)
            return
        end
        
        -- Find handler
        local handler = handlers[req.path]
        if not handler then
            local response = http_error('Unknown endpoint: ' .. req.path)
            client:write(response, function()
                client:close()
            end)
            return
        end
        
        -- Execute handler (async)
        vim.schedule(function()
            handler(req, function(response)
                client:write(response, function()
                    client:close()
                end)
            end)
        end)
    end)
end

---Start the LSP proxy server
---@return number|nil port The assigned port, or nil on failure
function M.start()
    if M.server then
        return M.port  -- Already running
    end
    
    local uv = vim.loop
    M.server = uv.new_tcp()
    
    -- Bind to port 0 = OS assigns available port
    local ok, err = pcall(function()
        M.server:bind('127.0.0.1', 0)
    end)
    
    if not ok then
        vim.notify('tark: Failed to bind LSP proxy server: ' .. tostring(err), vim.log.levels.ERROR)
        M.server:close()
        M.server = nil
        return nil
    end
    
    M.server:listen(128, function(listen_err)
        if listen_err then
            vim.notify('tark: LSP proxy listen error: ' .. listen_err, vim.log.levels.ERROR)
            return
        end
        
        local client = uv.new_tcp()
        M.server:accept(client)
        table.insert(M.clients, client)
        handle_client(client)
    end)
    
    -- Get the assigned port
    local addr = M.server:getsockname()
    M.port = addr.port
    
    vim.notify('tark: LSP proxy server started on port ' .. M.port, vim.log.levels.DEBUG)
    
    return M.port
end

---Stop the LSP proxy server
function M.stop()
    -- Close all client connections
    for _, client in ipairs(M.clients) do
        if not client:is_closing() then
            client:close()
        end
    end
    M.clients = {}
    
    -- Close server
    if M.server then
        if not M.server:is_closing() then
            M.server:close()
        end
        M.server = nil
        M.port = nil
        vim.notify('tark: LSP proxy server stopped', vim.log.levels.DEBUG)
    end
end

---Check if server is running
---@return boolean
function M.is_running()
    return M.server ~= nil and M.port ~= nil
end

---Get the current port (or nil if not running)
---@return number|nil
function M.get_port()
    return M.port
end

return M

