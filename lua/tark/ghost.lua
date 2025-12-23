-- Ghost text (inline completions) for tark
-- With LSP context integration for better completions

local M = {}

local ns_id = vim.api.nvim_create_namespace('tark_ghost_text')
local current_completion = nil
local debounce_timer = nil
local enabled = true
local function set_enabled(state)
    enabled = state
    if not enabled then
        M.dismiss()
    end
end

M.config = {
    server_url = 'http://localhost:8765',
    debounce_ms = 150,
    hl_group = 'Comment',
    lsp_context = true,  -- Include LSP context in requests
}

-- Session stats for completion mode (similar to chat mode)
local session_stats = {
    requests = 0,           -- Number of completion requests
    accepted = 0,           -- Number of accepted completions
    dismissed = 0,          -- Number of dismissed completions
    input_tokens = 0,       -- Estimated input tokens (file context sent)
    output_tokens = 0,      -- Estimated output tokens (completions received)
    total_cost = 0,         -- Estimated cost
    chars_generated = 0,    -- Total characters generated
    chars_accepted = 0,     -- Characters actually accepted
}

-- Models database (shared with chat, lazy-loaded)
local models_db = nil

---Estimate tokens (rough: ~4 chars per token for code)
---@param text string
---@return number
local function estimate_tokens(text)
    if not text then return 0 end
    return math.ceil(#text / 4)
end

---Format number with K/M suffix
---@param n number
---@return string
local function format_number(n)
    if n >= 1000000 then
        return string.format('%.1fM', n / 1000000)
    elseif n >= 1000 then
        return string.format('%.1fK', n / 1000)
    else
        return tostring(n)
    end
end

---Format cost
---@param cost number
---@return string
local function format_cost(cost)
    if cost < 0.01 then
        return string.format('$%.4f', cost)
    elseif cost < 1 then
        return string.format('$%.3f', cost)
    else
        return string.format('$%.2f', cost)
    end
end

---Fetch models database for pricing info
---@return table|nil
local function get_models_db()
    if models_db then return models_db end
    
    -- Try to get from chat module if loaded
    local ok, chat = pcall(require, 'tark.chat')
    if ok and chat.get_models_db then
        models_db = chat.get_models_db()
    end
    
    return models_db
end

---Calculate cost for tokens based on provider
---@param input_tokens number
---@param output_tokens number
---@param provider string
---@return number
local function calculate_cost(input_tokens, output_tokens, provider)
    -- Default pricing (per 1M tokens) - conservative estimates
    local pricing = {
        openai = { input = 2.50, output = 10.00 },   -- GPT-4o
        claude = { input = 3.00, output = 15.00 },   -- Claude 3.5
        ollama = { input = 0, output = 0 },          -- Free (local)
    }
    
    local rates = pricing[provider] or pricing.ollama
    local input_cost = (rates.input * input_tokens) / 1000000
    local output_cost = (rates.output * output_tokens) / 1000000
    return input_cost + output_cost
end

---Update session stats after a completion
---@param input_text string The file content sent
---@param output_text string The completion received
---@param provider string|nil The provider used
local function update_stats(input_text, output_text, provider)
    provider = provider or 'ollama'
    
    local input_tokens = estimate_tokens(input_text)
    local output_tokens = estimate_tokens(output_text)
    
    session_stats.requests = session_stats.requests + 1
    session_stats.input_tokens = session_stats.input_tokens + input_tokens
    session_stats.output_tokens = session_stats.output_tokens + output_tokens
    session_stats.chars_generated = session_stats.chars_generated + #output_text
    session_stats.total_cost = session_stats.total_cost + calculate_cost(input_tokens, output_tokens, provider)
end

---Get current session stats
---@return table
function M.get_stats()
    return vim.tbl_extend('force', {}, session_stats)
end

---Reset session stats
function M.reset_stats()
    session_stats.requests = 0
    session_stats.accepted = 0
    session_stats.dismissed = 0
    session_stats.input_tokens = 0
    session_stats.output_tokens = 0
    session_stats.total_cost = 0
    session_stats.chars_generated = 0
    session_stats.chars_accepted = 0
end

---Get statusline component string
---@return string
function M.statusline()
    local icon = enabled and '🛰' or '⏻'
    if session_stats.requests == 0 then
        return icon
    end

    local total_tokens = session_stats.input_tokens + session_stats.output_tokens
    local accept_rate = session_stats.requests > 0 
        and math.floor((session_stats.accepted / session_stats.requests) * 100) 
        or 0
    
    -- Format: "⚡ 5K tokens | 3/10 accepted | $0.02"
    local parts = {}
    table.insert(parts, string.format('⚡%s', format_number(total_tokens)))
    
    if session_stats.accepted > 0 or session_stats.dismissed > 0 then
        table.insert(parts, string.format('%d/%d (%d%%)', 
            session_stats.accepted, 
            session_stats.requests,
            accept_rate))
    end
    
    if session_stats.total_cost > 0 then
        table.insert(parts, format_cost(session_stats.total_cost))
    end
    
    return icon .. ' ' .. table.concat(parts, ' · ')
end

---Get detailed stats as formatted lines
---@return string[]
function M.stats_lines()
    local lines = {
        '=== tark Completion Stats ===',
        '',
    }
    
    if session_stats.requests == 0 then
        table.insert(lines, 'No completions this session.')
        return lines
    end
    
    local total_tokens = session_stats.input_tokens + session_stats.output_tokens
    local accept_rate = math.floor((session_stats.accepted / session_stats.requests) * 100)
    
    table.insert(lines, string.format('**Requests:** %d', session_stats.requests))
    table.insert(lines, string.format('**Accepted:** %d (%d%%)', session_stats.accepted, accept_rate))
    table.insert(lines, string.format('**Dismissed:** %d', session_stats.dismissed))
    table.insert(lines, '')
    table.insert(lines, string.format('**Input Tokens:** %s', format_number(session_stats.input_tokens)))
    table.insert(lines, string.format('**Output Tokens:** %s', format_number(session_stats.output_tokens)))
    table.insert(lines, string.format('**Total Tokens:** %s', format_number(total_tokens)))
    table.insert(lines, '')
    table.insert(lines, string.format('**Characters Generated:** %s', format_number(session_stats.chars_generated)))
    table.insert(lines, string.format('**Characters Accepted:** %s', format_number(session_stats.chars_accepted)))
    
    if session_stats.total_cost > 0 then
        table.insert(lines, '')
        table.insert(lines, string.format('**Estimated Cost:** %s', format_cost(session_stats.total_cost)))
    end
    
    return lines
end

-- Display ghost text at current cursor position
local function show_ghost_text(completion, line, col)
    -- Clear existing ghost text
    vim.api.nvim_buf_clear_namespace(0, ns_id, 0, -1)

    if not completion or completion == '' then
        current_completion = nil
        return
    end

    local lines = vim.split(completion, '\n')

    -- First line: virtual text after cursor (overlay)
    if #lines > 0 and lines[1] ~= '' then
        vim.api.nvim_buf_set_extmark(0, ns_id, line, col, {
            virt_text = { { lines[1], M.config.hl_group } },
            virt_text_pos = 'overlay',
        })
    end

    -- Remaining lines: virtual lines below
    if #lines > 1 then
        local virt_lines = {}
        for i = 2, #lines do
            table.insert(virt_lines, { { lines[i], M.config.hl_group } })
        end
        vim.api.nvim_buf_set_extmark(0, ns_id, line, 0, {
            virt_lines = virt_lines,
        })
    end

    current_completion = {
        text = completion,
        line = line,
        col = col,
    }
end

-- Send the completion request to server
local function send_completion_request(bufnr, cursor, file_path, file_content, context)
    local req_data = {
        file_path = file_path,
        file_content = file_content,
        cursor_line = cursor[1] - 1, -- 0-indexed
        cursor_col = cursor[2],
    }
    
    -- Include LSP context if available
    if context then
        req_data.context = context
    end
    
    local req_body = vim.fn.json_encode(req_data)
    
    -- Track the input size for stats
    local input_size = #file_content

    -- Use curl to make the request asynchronously
    vim.fn.jobstart({
        'curl',
        '-s',
        '-X', 'POST',
        '-H', 'Content-Type: application/json',
        '-d', req_body,
        M.config.server_url .. '/inline-complete',
    }, {
        stdout_buffered = true,
        on_stdout = function(_, data)
            if data and data[1] and data[1] ~= '' then
                local response_text = table.concat(data, '')
                local ok, resp = pcall(vim.fn.json_decode, response_text)
                if ok and resp and resp.completion and resp.completion ~= '' then
                    -- Update stats with the completion
                    -- Use real usage from server if available, otherwise estimate
                    if resp.usage then
                        session_stats.requests = session_stats.requests + 1
                        session_stats.input_tokens = session_stats.input_tokens + (resp.usage.input_tokens or 0)
                        session_stats.output_tokens = session_stats.output_tokens + (resp.usage.output_tokens or 0)
                        session_stats.chars_generated = session_stats.chars_generated + #resp.completion
                        -- Calculate cost with real tokens
                        local cost = calculate_cost(
                            resp.usage.input_tokens or 0,
                            resp.usage.output_tokens or 0,
                            M.config.provider or 'ollama'
                        )
                        session_stats.total_cost = session_stats.total_cost + cost
                    else
                        -- Fallback to estimation
                        update_stats(file_content, resp.completion, M.config.provider)
                    end
                    
                    -- Schedule to run on main thread
                    vim.schedule(function()
                        -- Check if cursor is still in the same position
                        local new_cursor = vim.api.nvim_win_get_cursor(0)
                        if new_cursor[1] == cursor[1] and new_cursor[2] == cursor[2] then
                            show_ghost_text(resp.completion, cursor[1] - 1, cursor[2])
                        end
                    end)
                end
            end
        end,
        on_stderr = function(_, data)
            if data and data[1] and data[1] ~= '' then
                -- Silent fail - server might not be running
            end
        end,
    })
end

-- Request completion from the HTTP server
local function request_completion()
    if not enabled then
        return
    end
    
    -- Snapshot buffer state as late as possible to honor current cursor/content
    local function snapshot()
        local bufnr = vim.api.nvim_get_current_buf()
        local cursor = vim.api.nvim_win_get_cursor(0)
        local lines = vim.api.nvim_buf_get_lines(bufnr, 0, -1, false)
        return {
            bufnr = bufnr,
            cursor = cursor,
            file_path = vim.api.nvim_buf_get_name(bufnr),
            file_content = table.concat(lines, '\n'),
        }
    end

    local snap = snapshot()
    local line = snap.cursor[1] - 1  -- 0-indexed
    local col = snap.cursor[2]

    -- If LSP context is disabled, send request immediately
    if not M.config.lsp_context then
        send_completion_request(snap.bufnr, snap.cursor, snap.file_path, snap.file_content, nil)
        return
    end

    -- Gather LSP context asynchronously, then send request
    local lsp_ok, lsp = pcall(require, 'tark.lsp')
    if not lsp_ok then
        -- LSP module not available, send without context
        send_completion_request(bufnr, cursor, file_path, file_content, nil)
        return
    end

    -- Get context asynchronously (doesn't block UI)
    lsp.get_completion_context_async(snap.bufnr, line, col, function(context)
        vim.schedule(function()
            -- Re-snapshot to honor the latest cursor/content before sending
            local latest = snapshot()
            send_completion_request(latest.bufnr, latest.cursor, latest.file_path, latest.file_content, context)
        end)
    end)
end

-- Accept the current completion
function M.accept()
    if not current_completion then
        return false
    end

    local lines = vim.split(current_completion.text, '\n')

    -- Insert the completion text
    vim.api.nvim_buf_set_text(
        0,
        current_completion.line,
        current_completion.col,
        current_completion.line,
        current_completion.col,
        lines
    )

    -- Move cursor to end of inserted text
    local new_line = current_completion.line + #lines
    local new_col = #lines > 1 and #lines[#lines] or (current_completion.col + #lines[1])
    vim.api.nvim_win_set_cursor(0, { new_line, new_col })

    -- Track accepted completion
    session_stats.accepted = session_stats.accepted + 1
    session_stats.chars_accepted = session_stats.chars_accepted + #current_completion.text

    -- Clear ghost text (don't track as dismissed since we're accepting)
    M.dismiss(false)

    return true
end

-- Dismiss the current completion
---@param track_dismissed boolean|nil Whether to track as dismissed (default: true if completion exists)
function M.dismiss(track_dismissed)
    -- Track dismissed completion (only if there was one showing)
    if current_completion and track_dismissed ~= false then
        session_stats.dismissed = session_stats.dismissed + 1
    end
    
    vim.api.nvim_buf_clear_namespace(0, ns_id, 0, -1)
    current_completion = nil
end

-- Trigger completion manually
function M.trigger()
    request_completion()
end

-- Toggle ghost text on/off
function M.toggle()
    set_enabled(not enabled)
    vim.notify('Ghost text ' .. (enabled and 'enabled' or 'disabled'), vim.log.levels.INFO)
end

-- Explicit enable/disable helpers
function M.enable()
    set_enabled(true)
    vim.notify('Ghost text enabled', vim.log.levels.INFO)
end

function M.disable()
    set_enabled(false)
    vim.notify('Ghost text disabled', vim.log.levels.INFO)
end

-- Check if we have a visible ghost completion
function M.has_completion()
    return current_completion ~= nil
end

-- Setup function
function M.setup(opts)
    M.config = vim.tbl_deep_extend('force', M.config, opts or {})

    -- Auto-trigger on text change in insert mode
    vim.api.nvim_create_autocmd('TextChangedI', {
        callback = function()
            if not enabled then
                return
            end

            -- Clear existing completion
            M.dismiss()

            -- Debounce
            if debounce_timer then
                vim.fn.timer_stop(debounce_timer)
            end

            debounce_timer = vim.fn.timer_start(M.config.debounce_ms, function()
                vim.schedule(request_completion)
            end)
        end,
    })

    -- Clear on cursor move in insert mode
    vim.api.nvim_create_autocmd('CursorMovedI', {
        callback = function()
            -- Only dismiss if there's a completion showing
            if current_completion then
                M.dismiss()
            end
        end,
    })

    -- Clear on leaving insert mode
    vim.api.nvim_create_autocmd('InsertLeave', {
        callback = M.dismiss,
    })

    -- Ctrl+] to accept completion (always works, avoids conflicts)
    vim.keymap.set('i', '<C-]>', function()
        M.accept()
    end, { silent = true, desc = "Accept tark completion" })

    -- Ctrl+Space to trigger manually
    vim.keymap.set('i', '<C-Space>', M.trigger, { silent = true, desc = "Trigger tark completion" })
    
    -- Ctrl+e to dismiss
    vim.keymap.set('i', '<C-e>', function()
        if current_completion then
            M.dismiss()
            return ''
        end
        return '<C-e>'
    end, { expr = true, silent = true, desc = "Dismiss tark completion" })
    
    -- Setup Tab integration with blink.cmp (if available)
    M.setup_tab_integration()
end

-- Setup Tab key to work with blink.cmp
function M.setup_tab_integration()
    -- Defer to allow other plugins to load first
    vim.defer_fn(function()
        -- Simple Tab mapping that works with any completion plugin
        -- Priority: tark ghost text > fallback to original Tab behavior
        vim.keymap.set('i', '<Tab>', function()
            -- If tark has ghost text showing, accept it
            if current_completion then
                M.accept()
                return ''
            end
            -- Otherwise, return Tab for default behavior (blink.cmp handles it)
            return '<Tab>'
        end, { expr = true, silent = true, desc = "Accept tark completion or Tab" })
    end, 50)
end

return M

