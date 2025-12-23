-- Ghost text (inline completions) for tark
-- With LSP context integration for better completions

local M = {}

local ns_id = vim.api.nvim_create_namespace('tark_ghost_text')
local current_completion = nil
local debounce_timer = nil
local enabled = true

M.config = {
    server_url = 'http://localhost:8765',
    debounce_ms = 150,
    hl_group = 'Comment',
    lsp_context = true,  -- Include LSP context in requests
}

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

    local bufnr = vim.api.nvim_get_current_buf()
    local cursor = vim.api.nvim_win_get_cursor(0)
    local lines = vim.api.nvim_buf_get_lines(bufnr, 0, -1, false)
    local file_path = vim.api.nvim_buf_get_name(bufnr)
    local file_content = table.concat(lines, '\n')
    local line = cursor[1] - 1  -- 0-indexed
    local col = cursor[2]

    -- If LSP context is disabled, send request immediately
    if not M.config.lsp_context then
        send_completion_request(bufnr, cursor, file_path, file_content, nil)
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
    lsp.get_completion_context_async(bufnr, line, col, function(context)
        -- Check if cursor is still in the same position before sending
        vim.schedule(function()
            local new_cursor = vim.api.nvim_win_get_cursor(0)
            if new_cursor[1] == cursor[1] and new_cursor[2] == cursor[2] then
                send_completion_request(bufnr, cursor, file_path, file_content, context)
            end
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

    -- Clear ghost text
    M.dismiss()

    return true
end

-- Dismiss the current completion
function M.dismiss()
    vim.api.nvim_buf_clear_namespace(0, ns_id, 0, -1)
    current_completion = nil
end

-- Trigger completion manually
function M.trigger()
    request_completion()
end

-- Toggle ghost text on/off
function M.toggle()
    enabled = not enabled
    if not enabled then
        M.dismiss()
    end
    vim.notify('Ghost text ' .. (enabled and 'enabled' or 'disabled'), vim.log.levels.INFO)
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

