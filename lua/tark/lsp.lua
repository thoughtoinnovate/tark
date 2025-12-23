-- LSP integration helpers for tark
-- All LSP calls are ASYNC to prevent blocking Neovim's UI

local M = {}

-- Cache for symbols to avoid repeated LSP calls
local symbol_cache = {}
local SYMBOL_CACHE_TTL = 5000  -- 5 seconds

---Check if any LSP client is attached to buffer
---@param bufnr number|nil Buffer number (default: current)
---@return boolean
function M.has_lsp(bufnr)
    bufnr = bufnr or vim.api.nvim_get_current_buf()
    local clients = vim.lsp.get_active_clients({ bufnr = bufnr })
    return #clients > 0
end

---Check if any LSP client supports a specific method
---@param bufnr number|nil Buffer number
---@param method string LSP method name
---@return boolean
function M.supports_method(bufnr, method)
    bufnr = bufnr or vim.api.nvim_get_current_buf()
    local clients = vim.lsp.get_active_clients({ bufnr = bufnr })
    for _, client in ipairs(clients) do
        if client.supports_method(method) then
            return true
        end
    end
    return false
end

---Get the first LSP client for buffer
---@param bufnr number|nil
---@return table|nil
function M.get_client(bufnr)
    bufnr = bufnr or vim.api.nvim_get_current_buf()
    local clients = vim.lsp.get_active_clients({ bufnr = bufnr })
    return clients[1]
end

---Get diagnostics near a line (synchronous but fast - vim.diagnostic is already cached)
---@param bufnr number|nil Buffer number
---@param line number|nil Center line (0-indexed)
---@param radius number|nil Lines above/below to include (default: 10)
---@return table[] List of diagnostics
function M.get_diagnostics(bufnr, line, radius)
    bufnr = bufnr or vim.api.nvim_get_current_buf()
    radius = radius or 10
    
    local all_diags = vim.diagnostic.get(bufnr)
    
    -- If no line specified, return all (limited)
    if not line then
        local result = {}
        for i, d in ipairs(all_diags) do
            if i > 20 then break end  -- Limit to 20 diagnostics
            table.insert(result, {
                line = d.lnum,
                col = d.col,
                message = d.message,
                severity = d.severity,
                source = d.source,
            })
        end
        return result
    end
    
    -- Filter to within radius of cursor
    local result = {}
    for _, d in ipairs(all_diags) do
        if math.abs(d.lnum - line) <= radius then
            table.insert(result, {
                line = d.lnum,
                col = d.col,
                message = d.message,
                severity = d.severity,
                source = d.source,
            })
        end
    end
    
    return result
end

---Get hover information (type/docs) at position - ASYNC
---@param bufnr number|nil Buffer number
---@param line number Line (0-indexed)
---@param col number Column (0-indexed)
---@param callback function Callback(result: string|nil)
function M.get_hover_async(bufnr, line, col, callback)
    bufnr = bufnr or vim.api.nvim_get_current_buf()
    
    if not M.has_lsp(bufnr) or not M.supports_method(bufnr, 'textDocument/hover') then
        vim.schedule(function() callback(nil) end)
        return
    end
    
    local params = {
        textDocument = vim.lsp.util.make_text_document_params(bufnr),
        position = { line = line, character = col },
    }
    
    vim.lsp.buf_request(bufnr, 'textDocument/hover', params, function(err, result, _, _)
        if err or not result or not result.contents then
            callback(nil)
            return
        end
        
        -- Extract text from hover result
        local contents = result.contents
        local text
        
        if type(contents) == 'string' then
            text = contents
        elseif type(contents) == 'table' then
            if contents.value then
                text = contents.value
            elseif contents.kind == 'markdown' then
                text = contents.value
            elseif #contents > 0 then
                -- Array of MarkedString
                local parts = {}
                for _, part in ipairs(contents) do
                    if type(part) == 'string' then
                        table.insert(parts, part)
                    elseif part.value then
                        table.insert(parts, part.value)
                    end
                end
                text = table.concat(parts, '\n')
            end
        end
        
        callback(text)
    end)
end

---Get document symbols - ASYNC
---@param bufnr number|nil Buffer number
---@param callback function Callback(symbols: table[])
function M.get_symbols_async(bufnr, callback)
    bufnr = bufnr or vim.api.nvim_get_current_buf()
    
    -- Check cache first
    local cache_key = bufnr .. ':' .. vim.api.nvim_buf_get_name(bufnr)
    local cached = symbol_cache[cache_key]
    if cached and (vim.loop.now() - cached.time) < SYMBOL_CACHE_TTL then
        vim.schedule(function() callback(cached.symbols) end)
        return
    end
    
    if not M.has_lsp(bufnr) then
        vim.schedule(function() callback({}) end)
        return
    end
    
    -- Check if any LSP server supports documentSymbol
    if not M.supports_method(bufnr, 'textDocument/documentSymbol') then
        vim.schedule(function() callback({}) end)
        return
    end
    
    local params = {
        textDocument = vim.lsp.util.make_text_document_params(bufnr),
    }
    
    vim.lsp.buf_request(bufnr, 'textDocument/documentSymbol', params, function(err, result, _, _)
        if err or not result then
            callback({})
            return
        end
        
        -- Flatten nested symbols
        local symbols = {}
        local function flatten(items, parent_name)
            for _, item in ipairs(items or {}) do
                local name = item.name
                if parent_name then
                    name = parent_name .. '.' .. name
                end
                
                table.insert(symbols, {
                    name = name,
                    kind = vim.lsp.protocol.SymbolKind[item.kind] or 'Unknown',
                    line = item.range and item.range.start.line or (item.location and item.location.range.start.line) or 0,
                    detail = item.detail,
                })
                
                -- Recurse into children
                if item.children then
                    flatten(item.children, name)
                end
            end
        end
        
        flatten(result, nil)
        
        -- Cache the result
        symbol_cache[cache_key] = {
            symbols = symbols,
            time = vim.loop.now(),
        }
        
        callback(symbols)
    end)
end

---Go to definition - ASYNC
---@param bufnr number|nil Buffer number
---@param line number Line (0-indexed)
---@param col number Column (0-indexed)
---@param callback function Callback(locations: table[]|nil)
function M.get_definition_async(bufnr, line, col, callback)
    bufnr = bufnr or vim.api.nvim_get_current_buf()
    
    if not M.has_lsp(bufnr) or not M.supports_method(bufnr, 'textDocument/definition') then
        vim.schedule(function() callback(nil) end)
        return
    end
    
    local params = {
        textDocument = vim.lsp.util.make_text_document_params(bufnr),
        position = { line = line, character = col },
    }
    
    vim.lsp.buf_request(bufnr, 'textDocument/definition', params, function(err, result, _, _)
        if err or not result then
            callback(nil)
            return
        end
        
        -- Normalize result to array
        local locations = {}
        if result.uri then
            -- Single Location
            locations = { result }
        elseif #result > 0 then
            locations = result
        end
        
        -- Convert to simple format
        local formatted = {}
        for _, loc in ipairs(locations) do
            local uri = loc.uri or loc.targetUri
            local range = loc.range or loc.targetRange
            
            if uri and range then
                table.insert(formatted, {
                    file = vim.uri_to_fname(uri),
                    line = range.start.line,
                    col = range.start.character,
                    end_line = range['end'].line,
                    end_col = range['end'].character,
                })
            end
        end
        
        callback(#formatted > 0 and formatted or nil)
    end)
end

---Find all references - ASYNC
---@param bufnr number|nil Buffer number
---@param line number Line (0-indexed)
---@param col number Column (0-indexed)
---@param callback function Callback(locations: table[]|nil)
function M.get_references_async(bufnr, line, col, callback)
    bufnr = bufnr or vim.api.nvim_get_current_buf()
    
    if not M.has_lsp(bufnr) or not M.supports_method(bufnr, 'textDocument/references') then
        vim.schedule(function() callback(nil) end)
        return
    end
    
    local params = {
        textDocument = vim.lsp.util.make_text_document_params(bufnr),
        position = { line = line, character = col },
        context = { includeDeclaration = true },
    }
    
    vim.lsp.buf_request(bufnr, 'textDocument/references', params, function(err, result, _, _)
        if err or not result then
            callback(nil)
            return
        end
        
        local formatted = {}
        for _, loc in ipairs(result) do
            if loc.uri and loc.range then
                table.insert(formatted, {
                    file = vim.uri_to_fname(loc.uri),
                    line = loc.range.start.line,
                    col = loc.range.start.character,
                })
            end
        end
        
        callback(#formatted > 0 and formatted or nil)
    end)
end

---Get signature help - ASYNC
---@param bufnr number|nil Buffer number
---@param line number Line (0-indexed)
---@param col number Column (0-indexed)
---@param callback function Callback(signature: string|nil)
function M.get_signature_async(bufnr, line, col, callback)
    bufnr = bufnr or vim.api.nvim_get_current_buf()
    
    if not M.has_lsp(bufnr) or not M.supports_method(bufnr, 'textDocument/signatureHelp') then
        vim.schedule(function() callback(nil) end)
        return
    end
    
    local params = {
        textDocument = vim.lsp.util.make_text_document_params(bufnr),
        position = { line = line, character = col },
    }
    
    vim.lsp.buf_request(bufnr, 'textDocument/signatureHelp', params, function(err, result, _, _)
        if err or not result or not result.signatures or #result.signatures == 0 then
            callback(nil)
            return
        end
        
        local sig = result.signatures[1]
        callback(sig.label)
    end)
end

---Gather all relevant context for completion - ASYNC
---Collects diagnostics, hover type, and nearby symbols
---@param bufnr number|nil Buffer number
---@param line number Line (0-indexed)
---@param col number Column (0-indexed)
---@param callback function Callback(context: table)
function M.get_completion_context_async(bufnr, line, col, callback)
    bufnr = bufnr or vim.api.nvim_get_current_buf()
    
    local context = {
        language = vim.bo[bufnr].filetype,
        diagnostics = M.get_diagnostics(bufnr, line, 10),
        has_lsp = M.has_lsp(bufnr),
        cursor_type = nil,
        symbols = nil,
    }
    
    -- If no LSP, return immediately with basic context
    if not context.has_lsp then
        vim.schedule(function() callback(context) end)
        return
    end
    
    -- Gather hover and symbols in parallel
    local pending = 2
    local function check_done()
        pending = pending - 1
        if pending == 0 then
            callback(context)
        end
    end
    
    M.get_hover_async(bufnr, line, col, function(hover)
        context.cursor_type = hover
        check_done()
    end)
    
    M.get_symbols_async(bufnr, function(symbols)
        -- Only include nearby symbols (within 50 lines)
        context.symbols = {}
        for _, sym in ipairs(symbols) do
            if math.abs(sym.line - line) <= 50 then
                table.insert(context.symbols, sym)
            end
            if #context.symbols >= 20 then break end  -- Limit
        end
        check_done()
    end)
end

---Clear symbol cache (call on buffer change)
function M.clear_cache(bufnr)
    if bufnr then
        local cache_key = bufnr .. ':' .. vim.api.nvim_buf_get_name(bufnr)
        symbol_cache[cache_key] = nil
    else
        symbol_cache = {}
    end
end

return M
