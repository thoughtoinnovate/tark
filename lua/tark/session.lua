-- Session management for tark chat
-- Handles session lifecycle: load, save, switch, delete
-- Integrates with Rust backend via HTTP API

local M = {}

--- Configuration (merged from tark.config.chat.session)
M.config = {
    auto_restore = true,      -- Auto-load previous session on chat open
    max_sessions = 50,        -- Max sessions per workspace
    save_on_close = true,     -- Save session when chat closes
}

-- ========== Notification System ==========
-- Centralized notifications for session operations
-- Requirements: 8.4 (error notifications), 3.3, 4.1, 5.2 (success notifications)

--- Notification levels
local notify_levels = {
    ERROR = vim.log.levels.ERROR,
    WARN = vim.log.levels.WARN,
    INFO = vim.log.levels.INFO,
    DEBUG = vim.log.levels.DEBUG,
}

--- Error type classification for user-friendly messages
local error_types = {
    SERVER_NOT_RUNNING = 'server_not_running',
    SESSION_NOT_FOUND = 'session_not_found',
    SAVE_FAILED = 'save_failed',
    DELETE_FAILED = 'delete_failed',
    SWITCH_FAILED = 'switch_failed',
    CREATE_FAILED = 'create_failed',
    FETCH_FAILED = 'fetch_failed',
    NETWORK_ERROR = 'network_error',
    UNKNOWN = 'unknown',
}

--- Classify error type from error message
---@param err string|nil Error message
---@return string Error type constant
local function classify_error(err)
    if not err then return error_types.UNKNOWN end
    
    local err_lower = err:lower()
    
    -- Server not running patterns
    if err_lower:match('connection refused') or
       err_lower:match('could not connect') or
       err_lower:match('failed to connect') or
       err_lower:match('curl.*exit') or
       err_lower:match('couldn\'t connect') or
       err_lower:match('no route to host') or
       err_lower:match('network is unreachable') then
        return error_types.SERVER_NOT_RUNNING
    end
    
    -- Session not found patterns
    if err_lower:match('not found') or
       err_lower:match('404') or
       err_lower:match('no such session') or
       err_lower:match('session does not exist') then
        return error_types.SESSION_NOT_FOUND
    end
    
    -- Network errors
    if err_lower:match('timeout') or
       err_lower:match('timed out') then
        return error_types.NETWORK_ERROR
    end
    
    return error_types.UNKNOWN
end

--- Get user-friendly error message
---@param error_type string Error type constant
---@param context string|nil Additional context (e.g., session name)
---@return string User-friendly error message
local function get_error_message(error_type, context)
    local messages = {
        [error_types.SERVER_NOT_RUNNING] = 'Server not running. Start with: tark serve',
        [error_types.SESSION_NOT_FOUND] = context 
            and string.format('Session not found: %s', context)
            or 'Session not found',
        [error_types.SAVE_FAILED] = 'Failed to save session. Changes may be lost.',
        [error_types.DELETE_FAILED] = context
            and string.format('Failed to delete session: %s', context)
            or 'Failed to delete session',
        [error_types.SWITCH_FAILED] = context
            and string.format('Failed to switch to session: %s', context)
            or 'Failed to switch session',
        [error_types.CREATE_FAILED] = 'Failed to create new session',
        [error_types.FETCH_FAILED] = 'Failed to fetch sessions',
        [error_types.NETWORK_ERROR] = 'Network error. Check your connection.',
        [error_types.UNKNOWN] = context or 'An unexpected error occurred',
    }
    return messages[error_type] or messages[error_types.UNKNOWN]
end

--- Show error notification
--- Requirements: 8.4
---@param err string|nil Raw error message
---@param operation string Operation that failed (e.g., 'save', 'delete', 'switch')
---@param context string|nil Additional context (e.g., session name)
function M.notify_error(err, operation, context)
    local error_type = classify_error(err)
    
    -- Map operation to specific error type if not already classified
    if error_type == error_types.UNKNOWN then
        local op_map = {
            save = error_types.SAVE_FAILED,
            delete = error_types.DELETE_FAILED,
            switch = error_types.SWITCH_FAILED,
            create = error_types.CREATE_FAILED,
            fetch = error_types.FETCH_FAILED,
        }
        error_type = op_map[operation] or error_type
    end
    
    local message = get_error_message(error_type, context or err)
    
    vim.schedule(function()
        vim.notify('tark: ' .. message, notify_levels.ERROR)
    end)
end

--- Show success notification
--- Requirements: 3.3, 4.1, 5.2
---@param operation string Operation that succeeded ('switched', 'created', 'deleted')
---@param context string|nil Additional context (e.g., session name)
function M.notify_success(operation, context)
    local messages = {
        switched = context 
            and string.format('Switched to session: %s', context)
            or 'Session switched',
        created = 'New session created',
        deleted = context
            and string.format('Session deleted: %s', context)
            or 'Session deleted',
        saved = 'Session saved',
    }
    
    local message = messages[operation] or string.format('Session %s', operation)
    
    vim.schedule(function()
        vim.notify('tark: ' .. message, notify_levels.INFO)
    end)
end

--- Show warning notification
---@param message string Warning message
function M.notify_warn(message)
    vim.schedule(function()
        vim.notify('tark: ' .. message, notify_levels.WARN)
    end)
end

-- Export error types for testing
M._error_types = error_types
M._classify_error = classify_error
M._get_error_message = get_error_message

--- Current session state (cached from backend)
M.current_session = nil

--- Server URL (dynamically resolved)
local function get_server_url()
    local ok, tark = pcall(require, 'tark')
    if ok and tark.get_server_url then
        return tark.get_server_url()
    end
    return 'http://127.0.0.1:8765'
end

--- Setup session module with config
---@param opts table|nil Configuration options
function M.setup(opts)
    M.config = vim.tbl_deep_extend('force', M.config, opts or {})
end

--- Make an async HTTP request using curl via vim.fn.jobstart
---@param method string HTTP method (GET, POST)
---@param endpoint string API endpoint (e.g., '/sessions')
---@param body table|nil Request body for POST requests
---@param callback function Callback with (success, data, error)
local function http_request(method, endpoint, body, callback)
    local url = get_server_url() .. endpoint
    local cmd = { 'curl', '-s', '-X', method }
    
    -- Add content type and body for POST requests
    if method == 'POST' and body then
        table.insert(cmd, '-H')
        table.insert(cmd, 'Content-Type: application/json')
        table.insert(cmd, '-d')
        table.insert(cmd, vim.fn.json_encode(body))
    end
    
    table.insert(cmd, url)
    
    local stdout_data = {}
    local stderr_data = {}
    
    vim.fn.jobstart(cmd, {
        stdout_buffered = true,
        stderr_buffered = true,
        on_stdout = function(_, data)
            if data then
                for _, line in ipairs(data) do
                    if line ~= '' then
                        table.insert(stdout_data, line)
                    end
                end
            end
        end,
        on_stderr = function(_, data)
            if data then
                for _, line in ipairs(data) do
                    if line ~= '' then
                        table.insert(stderr_data, line)
                    end
                end
            end
        end,
        on_exit = function(_, code)
            vim.schedule(function()
                if code ~= 0 then
                    local err = table.concat(stderr_data, '\n')
                    callback(false, nil, err ~= '' and err or 'Request failed')
                    return
                end
                
                local response = table.concat(stdout_data, '')
                if response == '' then
                    callback(true, nil, nil)
                    return
                end
                
                local ok, parsed = pcall(vim.fn.json_decode, response)
                if ok then
                    callback(true, parsed, nil)
                else
                    callback(false, nil, 'Failed to parse response: ' .. response)
                end
            end)
        end,
    })
end

--- Fetch all sessions for current workspace
---@param callback function Callback with (sessions, error)
function M.fetch_sessions(callback)
    http_request('GET', '/sessions', nil, function(success, data, err)
        if not success then
            callback(nil, err or 'Failed to fetch sessions')
            return
        end
        
        -- Backend returns { sessions: SessionMeta[] }
        local sessions = data and data.sessions or {}
        callback(sessions, nil)
    end)
end

--- Fetch current session with full data
---@param callback function Callback with (session, error)
function M.fetch_current(callback)
    http_request('GET', '/sessions/current', nil, function(success, data, err)
        if not success then
            callback(nil, err or 'Failed to fetch current session')
            return
        end
        
        -- Cache the current session
        M.current_session = data
        callback(data, nil)
    end)
end

--- Switch to a different session
---@param session_id string Session ID to switch to
---@param callback function Callback with (session, error)
function M.switch_session(session_id, callback)
    http_request('POST', '/sessions/switch', { session_id = session_id }, function(success, data, err)
        if not success then
            callback(nil, err or 'Failed to switch session')
            return
        end
        
        -- Cache the new current session
        M.current_session = data
        callback(data, nil)
    end)
end

--- Cleanup old sessions if over max_sessions limit
--- Deletes oldest sessions (by updated_at) until count equals max_sessions
---@param callback function|nil Optional callback with (deleted_count)
local function cleanup_old_sessions(callback)
    local max = M.config.max_sessions
    if not max or max <= 0 then
        if callback then callback(0) end
        return
    end
    
    M.fetch_sessions(function(sessions, err)
        if err or not sessions then
            if callback then callback(0) end
            return
        end
        
        local count = #sessions
        if count <= max then
            if callback then callback(0) end
            return
        end
        
        -- Sort by updated_at (oldest first)
        table.sort(sessions, function(a, b)
            local a_date = a.updated_at or a.created_at or ''
            local b_date = b.updated_at or b.created_at or ''
            return a_date < b_date
        end)
        
        -- Calculate how many to delete
        local to_delete = count - max
        local deleted = 0
        local current_id = M.current_session and M.current_session.id
        
        -- Delete oldest sessions (skip current session)
        local function delete_next(index)
            if deleted >= to_delete or index > #sessions then
                if callback then callback(deleted) end
                return
            end
            
            local session = sessions[index]
            -- Skip current session
            if session.id == current_id or session.is_current then
                delete_next(index + 1)
                return
            end
            
            M.delete_session(session.id, function(success, _)
                if success then
                    deleted = deleted + 1
                end
                delete_next(index + 1)
            end)
        end
        
        delete_next(1)
    end)
end

-- Export for testing
M._cleanup_old_sessions = cleanup_old_sessions

--- Create a new session
---@param callback function Callback with (session, error)
function M.create_session(callback)
    http_request('POST', '/sessions/new', {}, function(success, data, err)
        if not success then
            callback(nil, err or 'Failed to create session')
            return
        end
        
        -- Cache the new session
        M.current_session = data
        
        -- Cleanup old sessions if over limit (async, don't block callback)
        cleanup_old_sessions(function(deleted)
            if deleted > 0 then
                vim.schedule(function()
                    vim.notify(string.format('Cleaned up %d old session(s)', deleted), vim.log.levels.DEBUG)
                end)
            end
        end)
        
        callback(data, nil)
    end)
end

--- Delete a session
---@param session_id string Session ID to delete
---@param callback function Callback with (success, error)
function M.delete_session(session_id, callback)
    http_request('POST', '/sessions/delete', { session_id = session_id }, function(success, data, err)
        if not success then
            callback(false, err or 'Failed to delete session')
            return
        end
        
        -- If we deleted the current session, update cache
        if M.current_session and M.current_session.id == session_id then
            M.current_session = nil
        end
        
        callback(true, nil)
    end)
end

--- Restore session messages to chat buffer
--- This function renders all messages from a session into the chat buffer
--- and restores session statistics (tokens, cost)
---@param session table ChatSession data from backend
---@param chat_module table The chat module (require('tark.chat'))
function M.restore_to_buffer(session, chat_module)
    if not session then
        return
    end
    
    -- Cache the session
    M.current_session = session
    
    -- Restore session stats if available
    if chat_module._session_restore_stats then
        chat_module._session_restore_stats({
            input_tokens = session.input_tokens or 0,
            output_tokens = session.output_tokens or 0,
            total_cost = session.total_cost or 0,
        })
    end
    
    -- Restore messages to buffer
    if session.messages and #session.messages > 0 then
        for _, msg in ipairs(session.messages) do
            local role = msg.role
            local content = msg.content or ''
            
            -- Map backend roles to chat.lua roles
            if role == 'user' then
                if chat_module._session_append_message then
                    chat_module._session_append_message('user', content)
                end
            elseif role == 'assistant' then
                if chat_module._session_append_message then
                    chat_module._session_append_message('assistant', content)
                end
            elseif role == 'system' then
                if chat_module._session_append_message then
                    chat_module._session_append_message('system', content)
                end
            end
        end
    end
    
    -- Update window title with session info
    if chat_module._session_update_title then
        chat_module._session_update_title(session.name)
    end
end

--- Restore current session to chat buffer
--- Fetches current session from backend and restores it
---@param chat_module table The chat module
---@param callback function|nil Optional callback with (success, error)
function M.restore_current(chat_module, callback)
    M.fetch_current(function(session, err)
        if err then
            if callback then callback(false, err) end
            return
        end
        
        if session then
            M.restore_to_buffer(session, chat_module)
            if callback then callback(true, nil) end
        else
            -- No current session, create a new one
            M.create_session(function(new_session, create_err)
                if create_err then
                    if callback then callback(false, create_err) end
                    return
                end
                M.current_session = new_session
                if callback then callback(true, nil) end
            end)
        end
    end)
end

-- ========== Session Picker UI ==========

--- Format a date string for display (e.g., "Dec 30, 14:30")
---@param date_str string ISO date string
---@return string Formatted date
local function format_date(date_str)
    if not date_str then return '' end
    
    -- Parse ISO date: 2024-12-30T14:30:22Z
    local year, month, day, hour, min = date_str:match('(%d+)-(%d+)-(%d+)T(%d+):(%d+)')
    if not year then return date_str:sub(1, 10) end
    
    local months = { 'Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 
                     'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec' }
    local month_name = months[tonumber(month)] or month
    
    return string.format('%s %s, %s:%s', month_name, day, hour, min)
end

--- Format a session for display in the picker
---@param session table SessionMeta from backend
---@param is_current boolean Whether this is the current session
---@return string Formatted display line
local function format_session_line(session, is_current)
    -- Current session indicator
    local indicator = is_current and '● ' or '  '
    
    -- Session name (truncate if too long)
    local name = session.name or 'Unnamed Session'
    local max_name_len = 30
    if #name > max_name_len then
        name = name:sub(1, max_name_len - 3) .. '...'
    end
    
    -- Message count
    local msg_count = session.message_count or 0
    local msg_str = string.format('%d msgs', msg_count)
    
    -- Provider icon
    local provider_icons = {
        openai = '🧠',
        claude = '🤖',
        anthropic = '🤖',
        ollama = '🦙',
        google = '🔷',
    }
    local provider = session.provider or 'unknown'
    local icon = provider_icons[provider] or '💬'
    
    -- Date
    local date = format_date(session.updated_at or session.created_at)
    
    -- Format: "● Session name...          12 msgs  🧠  Dec 30"
    -- Pad name to align columns
    local padded_name = name .. string.rep(' ', max_name_len - #name)
    
    return string.format('%s%s  %8s  %s  %s', indicator, padded_name, msg_str, icon, date)
end

--- Show session picker UI
--- Displays a floating window with available sessions for selection
---@param on_select function Callback when session selected: on_select(session_id)
---@param mode string 'switch' or 'delete' - determines picker behavior
function M.show_picker(on_select, mode)
    mode = mode or 'switch'
    
    -- Fetch sessions from backend
    M.fetch_sessions(function(sessions, err)
        if err then
            -- Use centralized error notification (Requirements: 8.4)
            M.notify_error(err, 'fetch', nil)
            return
        end
        
        if not sessions or #sessions == 0 then
            M.notify_warn('No sessions found')
            return
        end
        
        -- Get current session ID for marking
        local current_id = M.current_session and M.current_session.id
        
        -- Format sessions for display
        local items = {}
        local session_map = {}  -- Map display index to session
        
        for i, session in ipairs(sessions) do
            local is_current = session.id == current_id or session.is_current
            local line = format_session_line(session, is_current)
            table.insert(items, line)
            session_map[i] = session
        end
        
        -- Calculate dimensions
        local max_width = 0
        for _, item in ipairs(items) do
            max_width = math.max(max_width, vim.fn.strdisplaywidth(item))
        end
        local width = math.min(max_width + 4, math.floor(vim.o.columns * 0.8))
        local height = math.min(#items, math.floor(vim.o.lines * 0.5))
        
        -- Create buffer
        local buf = vim.api.nvim_create_buf(false, true)
        vim.api.nvim_buf_set_lines(buf, 0, -1, false, items)
        vim.api.nvim_buf_set_option(buf, 'modifiable', false)
        vim.api.nvim_buf_set_option(buf, 'bufhidden', 'wipe')
        vim.api.nvim_buf_set_option(buf, 'buftype', 'nofile')
        
        -- Calculate position (center)
        local col = math.floor((vim.o.columns - width) / 2)
        local row = math.floor((vim.o.lines - height) / 2) - 2
        
        -- Title based on mode
        local title = mode == 'delete' and ' Delete Session ' or ' Sessions '
        
        -- Footer with keybindings
        local footer_text = mode == 'delete' 
            and ' j/k:move  Enter:delete  q:cancel '
            or ' j/k:move  Enter:select  d:delete  q:close '
        
        -- Create window
        local win = vim.api.nvim_open_win(buf, true, {
            relative = 'editor',
            width = width,
            height = height,
            col = col,
            row = row,
            style = 'minimal',
            border = 'rounded',
            title = title,
            title_pos = 'center',
            footer = { { footer_text, 'Comment' } },
            footer_pos = 'center',
        })
        
        -- Highlight current line
        vim.api.nvim_win_set_option(win, 'cursorline', true)
        vim.api.nvim_win_set_option(win, 'winhighlight', 'CursorLine:PmenuSel')
        
        -- Start in normal mode at first line
        vim.api.nvim_win_set_cursor(win, {1, 0})
        vim.cmd('stopinsert')
        
        -- Helper to close picker
        local function close_picker()
            if vim.api.nvim_win_is_valid(win) then
                vim.api.nvim_win_close(win, true)
            end
        end
        
        -- Helper to get selected session
        local function get_selected_session()
            local cursor = vim.api.nvim_win_get_cursor(win)
            local idx = cursor[1]
            return session_map[idx]
        end
        
        -- Keymaps
        local kopts = { buffer = buf, silent = true, nowait = true }
        
        -- Navigation
        vim.keymap.set('n', 'j', 'j', kopts)
        vim.keymap.set('n', 'k', 'k', kopts)
        vim.keymap.set('n', '<Down>', 'j', kopts)
        vim.keymap.set('n', '<Up>', 'k', kopts)
        vim.keymap.set('n', 'G', 'G', kopts)
        vim.keymap.set('n', 'gg', 'gg', kopts)
        vim.keymap.set('n', '<C-d>', '<C-d>', kopts)
        vim.keymap.set('n', '<C-u>', '<C-u>', kopts)
        
        -- Selection (Enter)
        vim.keymap.set('n', '<CR>', function()
            local session = get_selected_session()
            if session then
                close_picker()
                if on_select then
                    on_select(session.id)
                end
            end
        end, kopts)
        
        -- Delete mode shortcut (d key in switch mode)
        if mode == 'switch' then
            vim.keymap.set('n', 'd', function()
                local session = get_selected_session()
                if session then
                    close_picker()
                    -- Show delete confirmation
                    M.show_delete_confirm(session, function(confirmed)
                        if confirmed then
                            local session_name = session.name or session.id
                            M.delete_session(session.id, function(success, del_err)
                                if success then
                                    -- Use centralized success notification (Requirements: 5.2)
                                    M.notify_success('deleted', session_name)
                                else
                                    -- Use centralized error notification (Requirements: 8.4)
                                    M.notify_error(del_err, 'delete', session_name)
                                end
                            end)
                        end
                    end)
                end
            end, kopts)
        end
        
        -- Cancel
        vim.keymap.set('n', '<Esc>', close_picker, kopts)
        vim.keymap.set('n', 'q', close_picker, kopts)
        
        -- Close on buffer leave
        vim.api.nvim_create_autocmd('BufLeave', {
            buffer = buf,
            once = true,
            callback = function()
                if vim.api.nvim_win_is_valid(win) then
                    vim.api.nvim_win_close(win, true)
                end
            end,
        })
    end)
end

--- Show delete confirmation dialog
---@param session table Session to delete
---@param callback function Callback with (confirmed: boolean)
function M.show_delete_confirm(session, callback)
    local name = session.name or session.id
    if #name > 40 then
        name = name:sub(1, 37) .. '...'
    end
    
    local prompt = string.format('Delete session "%s"?', name)
    local items = { '  Yes, delete', '  No, cancel' }
    
    -- Create buffer
    local buf = vim.api.nvim_create_buf(false, true)
    vim.api.nvim_buf_set_lines(buf, 0, -1, false, items)
    vim.api.nvim_buf_set_option(buf, 'modifiable', false)
    vim.api.nvim_buf_set_option(buf, 'bufhidden', 'wipe')
    
    -- Calculate dimensions
    local width = math.max(#prompt + 4, 30)
    local height = 2
    
    -- Calculate position (center)
    local col = math.floor((vim.o.columns - width) / 2)
    local row = math.floor((vim.o.lines - height) / 2) - 2
    
    -- Create window
    local win = vim.api.nvim_open_win(buf, true, {
        relative = 'editor',
        width = width,
        height = height,
        col = col,
        row = row,
        style = 'minimal',
        border = 'rounded',
        title = ' ' .. prompt .. ' ',
        title_pos = 'center',
    })
    
    -- Highlight current line
    vim.api.nvim_win_set_option(win, 'cursorline', true)
    vim.api.nvim_win_set_option(win, 'winhighlight', 'CursorLine:PmenuSel')
    
    -- Start at first line
    vim.api.nvim_win_set_cursor(win, {1, 0})
    vim.cmd('stopinsert')
    
    -- Helper to close
    local function close_and_callback(confirmed)
        if vim.api.nvim_win_is_valid(win) then
            vim.api.nvim_win_close(win, true)
        end
        if callback then
            callback(confirmed)
        end
    end
    
    -- Keymaps
    local kopts = { buffer = buf, silent = true, nowait = true }
    
    vim.keymap.set('n', 'j', 'j', kopts)
    vim.keymap.set('n', 'k', 'k', kopts)
    vim.keymap.set('n', '<Down>', 'j', kopts)
    vim.keymap.set('n', '<Up>', 'k', kopts)
    
    vim.keymap.set('n', '<CR>', function()
        local cursor = vim.api.nvim_win_get_cursor(win)
        close_and_callback(cursor[1] == 1)  -- First line = Yes
    end, kopts)
    
    vim.keymap.set('n', 'y', function() close_and_callback(true) end, kopts)
    vim.keymap.set('n', 'Y', function() close_and_callback(true) end, kopts)
    vim.keymap.set('n', 'n', function() close_and_callback(false) end, kopts)
    vim.keymap.set('n', 'N', function() close_and_callback(false) end, kopts)
    vim.keymap.set('n', '<Esc>', function() close_and_callback(false) end, kopts)
    vim.keymap.set('n', 'q', function() close_and_callback(false) end, kopts)
    
    -- Close on buffer leave
    vim.api.nvim_create_autocmd('BufLeave', {
        buffer = buf,
        once = true,
        callback = function()
            if vim.api.nvim_win_is_valid(win) then
                vim.api.nvim_win_close(win, true)
            end
        end,
    })
end

-- Export helper functions for testing
M._format_date = format_date
M._format_session_line = format_session_line

--- Trigger session save on backend (async)
--- Called when chat closes with save_on_close enabled
--- Note: Chat messages are auto-saved by backend after each exchange
--- This saves the session config (provider, model, mode, etc.)
function M.trigger_save()
    -- Get chat module to access current settings
    local ok_chat, chat = pcall(require, 'tark.chat')
    if not ok_chat then return end
    
    local provider = chat._test_get_current_provider and chat._test_get_current_provider() or 'ollama'
    local model = chat._test_get_current_model and chat._test_get_current_model() or nil
    
    local body = vim.fn.json_encode({
        provider = provider,
        model = model,
    })
    
    local url = get_server_url() .. '/session/save'
    vim.fn.jobstart({
        'curl', '-s', '-X', 'POST',
        '-H', 'Content-Type: application/json',
        '-d', body,
        url,
    }, {
        stdout_buffered = true,
        on_exit = function(_, code)
            if code ~= 0 then
                -- Silent failure - don't interrupt user
                vim.schedule(function()
                    vim.notify('Session save may have failed', vim.log.levels.DEBUG)
                end)
            end
        end,
    })
end

--- Trigger session save on backend (sync)
--- Called on VimLeavePre when Neovim is exiting
--- Uses synchronous call to ensure save completes before exit
function M.trigger_save_sync()
    -- Get chat module to access current settings
    local ok_chat, chat = pcall(require, 'tark.chat')
    if not ok_chat then return end
    
    local provider = chat._test_get_current_provider and chat._test_get_current_provider() or 'ollama'
    local model = chat._test_get_current_model and chat._test_get_current_model() or nil
    
    local body = vim.fn.json_encode({
        provider = provider,
        model = model,
    })
    
    local url = get_server_url() .. '/session/save'
    -- Use vim.fn.system for synchronous execution
    vim.fn.system({
        'curl', '-s', '-X', 'POST',
        '-H', 'Content-Type: application/json',
        '-d', body,
        url,
    })
    -- Don't check exit code - we're exiting anyway
end

return M
