-- tark chat integration for Neovim
-- Thin Lua layer that bridges the Rust TUI with Neovim
-- Handles socket server, terminal management, and RPC commands
-- (Renamed from tui.lua as part of migration cleanup)

local M = {}

-- Module state
M.state = {
    socket_path = nil,      -- Path to Unix socket
    socket_server = nil,    -- Socket server handle
    client_conn = nil,      -- Connected TUI client
    terminal_buf = nil,     -- Terminal buffer
    terminal_win = nil,     -- Terminal window
    terminal_job = nil,     -- Terminal job ID
}

-- Configuration
M.config = {
    binary = 'tark',        -- Path to tark binary
    window = {
        position = 'right', -- 'right', 'left', 'bottom', 'top'
        -- TUI layout requires: 70% chat column + 30% panel (horizontal split)
        -- Minimum recommended: 120 columns for proper panel display
        -- For vertical splits (right/left), use wider window
        width = 120,        -- Width for vertical splits (was 80, too narrow for 70/30 layout)
        -- For horizontal splits (bottom/top), need enough height for:
        -- messages (75%) + status (5%) + input (20%) = minimum ~30 lines
        height = 35,        -- Height for horizontal splits (was 20, too cramped)
    },
    context_sync = {
        enabled = true,     -- Send buffer/diagnostic updates to TUI
        debounce_ms = 100,  -- Debounce context updates
    },
}

-- Generate unique socket path for this Neovim instance
local function get_socket_path()
    local pid = vim.fn.getpid()
    local tmpdir = os.getenv('TMPDIR') or os.getenv('TMP') or '/tmp'
    return string.format('%s/tark-nvim-%d.sock', tmpdir, pid)
end

-- JSON encode helper
local function json_encode(data)
    return vim.fn.json_encode(data)
end

-- JSON decode helper
local function json_decode(str)
    local ok, result = pcall(vim.fn.json_decode, str)
    if ok then
        return result
    end
    return nil
end

-- Send RPC message to connected TUI client
local function send_rpc(msg)
    if not M.state.client_conn then
        return false
    end
    
    local json = json_encode(msg)
    if not json then
        return false
    end
    
    -- Send message with newline delimiter
    local ok, err = pcall(function()
        M.state.client_conn:write(json .. '\n')
    end)
    
    if not ok then
        vim.notify('tark chat: Failed to send RPC: ' .. tostring(err), vim.log.levels.DEBUG)
        return false
    end
    
    return true
end

-- Send response to a request
local function send_response(id, result)
    send_rpc({
        type = 'response',
        id = id,
        result = result,
    })
end

-- Send error response
local function send_error(id, message, code)
    send_rpc({
        type = 'error',
        id = id,
        message = message,
        code = code,
    })
end

-- ============================================================================
-- RPC Command Handlers
-- ============================================================================

-- Handle open_file command
local function handle_open_file(msg, request_id)
    local path = msg.path
    local line = msg.line
    local col = msg.col
    
    if not path or path == '' then
        send_error(request_id, 'Path is required', -1)
        return
    end
    
    -- Schedule to run in main thread
    vim.schedule(function()
        -- Open the file
        local ok, err = pcall(function()
            vim.cmd('edit ' .. vim.fn.fnameescape(path))
            
            -- Jump to line/column if specified
            if line and line > 0 then
                local target_col = (col and col > 0) and col or 1
                vim.api.nvim_win_set_cursor(0, {line, target_col - 1})
            end
        end)
        
        if ok then
            send_response(request_id, { success = true })
        else
            send_error(request_id, tostring(err), -1)
        end
    end)
end

-- Handle goto_line command
local function handle_goto_line(msg, request_id)
    local line = msg.line
    local col = msg.col
    
    if not line or line < 1 then
        send_error(request_id, 'Valid line number is required', -1)
        return
    end
    
    vim.schedule(function()
        local ok, err = pcall(function()
            local target_col = (col and col > 0) and col or 1
            vim.api.nvim_win_set_cursor(0, {line, target_col - 1})
        end)
        
        if ok then
            send_response(request_id, { success = true })
        else
            send_error(request_id, tostring(err), -1)
        end
    end)
end

-- Handle apply_diff command
local function handle_apply_diff(msg, request_id)
    local path = msg.path
    local diff = msg.diff
    
    if not path or path == '' then
        send_error(request_id, 'Path is required', -1)
        return
    end
    
    if not diff or diff == '' then
        send_error(request_id, 'Diff content is required', -1)
        return
    end
    
    vim.schedule(function()
        -- Apply diff using patch command
        local tmpfile = vim.fn.tempname()
        local f = io.open(tmpfile, 'w')
        if not f then
            send_error(request_id, 'Failed to create temp file', -1)
            return
        end
        f:write(diff)
        f:close()
        
        local result = vim.fn.system(string.format(
            'patch -p1 < %s 2>&1',
            vim.fn.shellescape(tmpfile)
        ))
        local exit_code = vim.v.shell_error
        
        os.remove(tmpfile)
        
        if exit_code == 0 then
            -- Reload the buffer if it's open
            local bufnr = vim.fn.bufnr(path)
            if bufnr ~= -1 then
                vim.api.nvim_buf_call(bufnr, function()
                    vim.cmd('edit!')
                end)
            end
            send_response(request_id, { success = true })
        else
            send_error(request_id, 'Patch failed: ' .. result, -1)
        end
    end)
end

-- Handle show_diff command
local function handle_show_diff(msg, request_id)
    local path = msg.path
    local original = msg.original
    local modified = msg.modified
    
    if not path or path == '' then
        send_error(request_id, 'Path is required', -1)
        return
    end
    
    vim.schedule(function()
        local ok, err = pcall(function()
            -- If original/modified provided, create temp buffers
            if original and modified then
                -- Create two scratch buffers for diff view
                local old_buf = vim.api.nvim_create_buf(false, true)
                local new_buf = vim.api.nvim_create_buf(false, true)
                
                local old_lines = vim.split(original, '\n')
                local new_lines = vim.split(modified, '\n')
                
                vim.api.nvim_buf_set_lines(old_buf, 0, -1, false, old_lines)
                vim.api.nvim_buf_set_lines(new_buf, 0, -1, false, new_lines)
                
                local short_name = path:match('[^/]+$') or path
                vim.api.nvim_buf_set_name(old_buf, '[OLD] ' .. short_name)
                vim.api.nvim_buf_set_name(new_buf, '[NEW] ' .. short_name)
                
                vim.api.nvim_buf_set_option(old_buf, 'buftype', 'nofile')
                vim.api.nvim_buf_set_option(new_buf, 'buftype', 'nofile')
                vim.api.nvim_buf_set_option(old_buf, 'modifiable', false)
                vim.api.nvim_buf_set_option(new_buf, 'modifiable', false)
                
                -- Open in vertical split with diff mode
                vim.cmd('tabnew')
                vim.api.nvim_win_set_buf(0, old_buf)
                vim.cmd('diffthis')
                vim.cmd('vsplit')
                vim.api.nvim_win_set_buf(0, new_buf)
                vim.cmd('diffthis')
            else
                -- Use git diff for the file
                local cwd = vim.fn.getcwd()
                local has_git = vim.fn.isdirectory(cwd .. '/.git') == 1
                
                if has_git then
                    vim.cmd('tabnew')
                    vim.cmd('terminal git diff --color ' .. vim.fn.shellescape(path))
                else
                    vim.cmd('edit ' .. vim.fn.fnameescape(path))
                end
            end
        end)
        
        if ok then
            send_response(request_id, { success = true })
        else
            send_error(request_id, tostring(err), -1)
        end
    end)
end

-- Handle get_buffers command
local function handle_get_buffers(request_id)
    vim.schedule(function()
        local buffers = {}
        
        for _, bufnr in ipairs(vim.api.nvim_list_bufs()) do
            if vim.api.nvim_buf_is_loaded(bufnr) then
                local name = vim.api.nvim_buf_get_name(bufnr)
                local modified = vim.api.nvim_buf_get_option(bufnr, 'modified')
                local filetype = vim.api.nvim_buf_get_option(bufnr, 'filetype')
                local buftype = vim.api.nvim_buf_get_option(bufnr, 'buftype')
                
                -- Skip special buffers
                if buftype == '' or buftype == 'acwrite' then
                    table.insert(buffers, {
                        id = bufnr,
                        path = name ~= '' and name or nil,
                        name = name ~= '' and vim.fn.fnamemodify(name, ':t') or '[No Name]',
                        modified = modified,
                        filetype = filetype ~= '' and filetype or nil,
                    })
                end
            end
        end
        
        send_response(request_id, { buffers = buffers })
    end)
end

-- Handle get_diagnostics command
local function handle_get_diagnostics(msg, request_id)
    local filter_path = msg.path
    
    vim.schedule(function()
        local diagnostics = {}
        local all_diags = vim.diagnostic.get()
        
        for _, diag in ipairs(all_diags) do
            local bufnr = diag.bufnr
            local path = vim.api.nvim_buf_get_name(bufnr)
            
            -- Filter by path if specified
            if not filter_path or path == filter_path or path:match(filter_path .. '$') then
                local severity_map = {
                    [vim.diagnostic.severity.ERROR] = 'error',
                    [vim.diagnostic.severity.WARN] = 'warning',
                    [vim.diagnostic.severity.INFO] = 'info',
                    [vim.diagnostic.severity.HINT] = 'hint',
                }
                
                table.insert(diagnostics, {
                    path = path,
                    line = diag.lnum + 1,  -- Convert to 1-indexed
                    col = diag.col + 1,
                    end_line = diag.end_lnum and (diag.end_lnum + 1) or nil,
                    end_col = diag.end_col and (diag.end_col + 1) or nil,
                    severity = severity_map[diag.severity] or 'error',
                    message = diag.message,
                    source = diag.source,
                    code = diag.code and tostring(diag.code) or nil,
                })
            end
        end
        
        send_response(request_id, { diagnostics = diagnostics })
    end)
end

-- Handle get_buffer_content command
local function handle_get_buffer_content(msg, request_id)
    local path = msg.path
    
    if not path or path == '' then
        send_error(request_id, 'Path is required', -1)
        return
    end
    
    vim.schedule(function()
        local bufnr = vim.fn.bufnr(path)
        local content
        local truncated = false
        
        if bufnr ~= -1 and vim.api.nvim_buf_is_loaded(bufnr) then
            -- Get from buffer
            local lines = vim.api.nvim_buf_get_lines(bufnr, 0, -1, false)
            content = table.concat(lines, '\n')
        else
            -- Read from file
            local f = io.open(path, 'r')
            if f then
                content = f:read('*a')
                f:close()
            else
                send_error(request_id, 'File not found: ' .. path, -1)
                return
            end
        end
        
        -- Truncate if too large (> 100KB)
        local max_size = 100 * 1024
        if #content > max_size then
            content = content:sub(1, max_size)
            truncated = true
        end
        
        send_response(request_id, {
            path = path,
            content = content,
            truncated = truncated,
        })
    end)
end

-- Handle get_cursor command
local function handle_get_cursor(request_id)
    vim.schedule(function()
        local path = vim.api.nvim_buf_get_name(0)
        local cursor = vim.api.nvim_win_get_cursor(0)
        
        send_response(request_id, {
            path = path,
            line = cursor[1],
            col = cursor[2] + 1,  -- Convert to 1-indexed
        })
    end)
end

-- Handle ping command
local function handle_ping(msg)
    send_rpc({
        type = 'pong',
        seq = msg.seq,
    })
end

-- ============================================================================
-- RPC Message Router
-- ============================================================================

-- Route incoming RPC message to appropriate handler
local function handle_rpc_message(msg)
    if not msg or not msg.type then
        return
    end
    
    -- Handle request wrapper
    local request_id = nil
    local command = msg
    
    if msg.type == 'request' then
        request_id = msg.id
        command = msg.command
        if not command then
            send_error(request_id, 'Missing command in request', -1)
            return
        end
    end
    
    -- Route to handler based on type
    local msg_type = command.type
    
    if msg_type == 'open_file' then
        handle_open_file(command, request_id)
    elseif msg_type == 'goto_line' then
        handle_goto_line(command, request_id)
    elseif msg_type == 'apply_diff' then
        handle_apply_diff(command, request_id)
    elseif msg_type == 'show_diff' then
        handle_show_diff(command, request_id)
    elseif msg_type == 'get_buffers' then
        handle_get_buffers(request_id)
    elseif msg_type == 'get_diagnostics' then
        handle_get_diagnostics(command, request_id)
    elseif msg_type == 'get_buffer_content' then
        handle_get_buffer_content(command, request_id)
    elseif msg_type == 'get_cursor' then
        handle_get_cursor(request_id)
    elseif msg_type == 'ping' then
        handle_ping(command)
    else
        if request_id then
            send_error(request_id, 'Unknown command type: ' .. tostring(msg_type), -1)
        end
    end
end

-- ============================================================================
-- Context Synchronization
-- ============================================================================

local context_timer = nil

-- Debounced context send
local function send_context_debounced(msg)
    if not M.config.context_sync.enabled then
        return
    end
    
    if context_timer then
        vim.fn.timer_stop(context_timer)
    end
    
    context_timer = vim.fn.timer_start(M.config.context_sync.debounce_ms, function()
        send_rpc(msg)
        context_timer = nil
    end)
end

-- Send buffer changed notification
local function send_buffer_changed(bufnr)
    local path = vim.api.nvim_buf_get_name(bufnr)
    if path == '' then
        return
    end
    
    -- Get content (truncate if large)
    local lines = vim.api.nvim_buf_get_lines(bufnr, 0, -1, false)
    local content = table.concat(lines, '\n')
    local truncated = false
    
    local max_size = 50 * 1024  -- 50KB
    if #content > max_size then
        content = content:sub(1, max_size)
        truncated = true
    end
    
    send_context_debounced({
        type = 'buffer_changed',
        path = path,
        content = content,
        truncated = truncated,
    })
end

-- Send diagnostics updated notification
local function send_diagnostics_updated()
    local diagnostics = {}
    local all_diags = vim.diagnostic.get()
    
    for _, diag in ipairs(all_diags) do
        local bufnr = diag.bufnr
        local path = vim.api.nvim_buf_get_name(bufnr)
        
        if path ~= '' then
            local severity_map = {
                [vim.diagnostic.severity.ERROR] = 'error',
                [vim.diagnostic.severity.WARN] = 'warning',
                [vim.diagnostic.severity.INFO] = 'info',
                [vim.diagnostic.severity.HINT] = 'hint',
            }
            
            table.insert(diagnostics, {
                path = path,
                line = diag.lnum + 1,
                col = diag.col + 1,
                end_line = diag.end_lnum and (diag.end_lnum + 1) or nil,
                end_col = diag.end_col and (diag.end_col + 1) or nil,
                severity = severity_map[diag.severity] or 'error',
                message = diag.message,
                source = diag.source,
                code = diag.code and tostring(diag.code) or nil,
            })
        end
    end
    
    send_context_debounced({
        type = 'diagnostics_updated',
        diagnostics = diagnostics,
    })
end

-- Send cursor moved notification
local function send_cursor_moved()
    local path = vim.api.nvim_buf_get_name(0)
    if path == '' then
        return
    end
    
    local cursor = vim.api.nvim_win_get_cursor(0)
    
    send_context_debounced({
        type = 'cursor_moved',
        path = path,
        line = cursor[1],
        col = cursor[2] + 1,
    })
end

-- Send buffer entered notification
local function send_buffer_entered()
    local path = vim.api.nvim_buf_get_name(0)
    if path == '' then
        return
    end
    
    local filetype = vim.api.nvim_buf_get_option(0, 'filetype')
    
    send_rpc({
        type = 'buffer_entered',
        path = path,
        filetype = filetype ~= '' and filetype or nil,
    })
end

-- ============================================================================
-- Socket Server
-- ============================================================================

-- Start the Unix socket server
local function start_socket_server()
    local socket_path = get_socket_path()
    M.state.socket_path = socket_path
    
    -- Remove existing socket file
    os.remove(socket_path)
    
    -- Create socket server using vim.loop (libuv)
    local server = vim.loop.new_pipe(false)
    if not server then
        vim.notify('tark chat: Failed to create socket server', vim.log.levels.ERROR)
        return false
    end
    
    local ok, err = pcall(function()
        server:bind(socket_path)
    end)
    
    if not ok then
        vim.notify('tark chat: Failed to bind socket: ' .. tostring(err), vim.log.levels.ERROR)
        server:close()
        return false
    end
    
    server:listen(1, function(listen_err)
        if listen_err then
            vim.notify('tark chat: Socket listen error: ' .. listen_err, vim.log.levels.ERROR)
            return
        end
        
        local client = vim.loop.new_pipe(false)
        server:accept(client)
        
        -- Store client connection
        M.state.client_conn = client
        
        -- Buffer for incomplete messages
        local buffer = ''
        
        -- Handle incoming data
        client:read_start(function(read_err, data)
            if read_err then
                vim.schedule(function()
                    vim.notify('tark chat: Read error: ' .. read_err, vim.log.levels.DEBUG)
                end)
                return
            end
            
            if data then
                buffer = buffer .. data
                
                -- Process complete messages (newline-delimited JSON)
                while true do
                    local newline_pos = buffer:find('\n')
                    if not newline_pos then
                        break
                    end
                    
                    local line = buffer:sub(1, newline_pos - 1)
                    buffer = buffer:sub(newline_pos + 1)
                    
                    if line ~= '' then
                        local msg = json_decode(line)
                        if msg then
                            vim.schedule(function()
                                handle_rpc_message(msg)
                            end)
                        end
                    end
                end
            else
                -- Client disconnected
                vim.schedule(function()
                    M.state.client_conn = nil
                end)
            end
        end)
        
        -- Send initial context
        vim.schedule(function()
            send_buffer_entered()
            send_diagnostics_updated()
        end)
    end)
    
    M.state.socket_server = server
    return true
end

-- Stop the socket server
local function stop_socket_server()
    if M.state.client_conn then
        M.state.client_conn:close()
        M.state.client_conn = nil
    end
    
    if M.state.socket_server then
        M.state.socket_server:close()
        M.state.socket_server = nil
    end
    
    if M.state.socket_path then
        os.remove(M.state.socket_path)
        M.state.socket_path = nil
    end
end

-- ============================================================================
-- Terminal Management
-- ============================================================================

-- Get the tark binary path
local function get_binary_path()
    -- Check if configured binary exists
    if vim.fn.executable(M.config.binary) == 1 then
        return M.config.binary
    end
    
    -- Check local binary (downloaded by plugin)
    local data_dir = vim.fn.stdpath('data') .. '/tark'
    local local_binary = data_dir .. '/tark'
    if vim.fn.filereadable(local_binary) == 1 then
        return local_binary
    end
    
    return nil
end

-- Open chat in terminal split
function M.open()
    -- Check if already open
    if M.state.terminal_win and vim.api.nvim_win_is_valid(M.state.terminal_win) then
        vim.api.nvim_set_current_win(M.state.terminal_win)
        return
    end
    
    -- Get binary path
    local binary = get_binary_path()
    if not binary then
        vim.notify('tark chat: Binary not found. Run :TarkBinaryDownload first.', vim.log.levels.ERROR)
        return
    end
    
    -- Start socket server
    if not start_socket_server() then
        return
    end
    
    -- Build command
    local cmd = string.format('%s chat --socket %s', binary, M.state.socket_path)
    
    -- Open terminal split based on position
    -- Note: TUI layout requires 70% chat + 30% panel (horizontal split)
    -- For vertical splits: need at least 120 columns (56 chat + 36 panel + borders)
    -- For horizontal splits: need at least 35 lines (messages + status + input)
    local pos = M.config.window.position
    if pos == 'right' then
        vim.cmd('botright vsplit')
        vim.cmd('vertical resize ' .. M.config.window.width)
    elseif pos == 'left' then
        vim.cmd('topleft vsplit')
        vim.cmd('vertical resize ' .. M.config.window.width)
    elseif pos == 'bottom' then
        vim.cmd('botright split')
        vim.cmd('resize ' .. M.config.window.height)
    elseif pos == 'top' then
        vim.cmd('topleft split')
        vim.cmd('resize ' .. M.config.window.height)
    else
        vim.cmd('botright vsplit')
        vim.cmd('vertical resize ' .. M.config.window.width)
    end
    
    -- Open terminal
    M.state.terminal_job = vim.fn.termopen(cmd, {
        on_exit = function(_, exit_code, _)
            vim.schedule(function()
                M.cleanup()
                if exit_code ~= 0 then
                    vim.notify('tark chat: Exited with code ' .. exit_code, vim.log.levels.DEBUG)
                end
            end)
        end,
    })
    
    M.state.terminal_buf = vim.api.nvim_get_current_buf()
    M.state.terminal_win = vim.api.nvim_get_current_win()
    
    -- Set buffer options
    vim.api.nvim_buf_set_option(M.state.terminal_buf, 'buflisted', false)
    vim.api.nvim_buf_set_name(M.state.terminal_buf, 'tark-chat')
    
    -- Enter insert mode
    vim.cmd('startinsert')
    
    -- Setup autocmds for context sync
    M.setup_autocmds()
end

-- Close chat
function M.close()
    if M.state.terminal_win and vim.api.nvim_win_is_valid(M.state.terminal_win) then
        vim.api.nvim_win_close(M.state.terminal_win, true)
    end
    M.cleanup()
end

-- Toggle chat
function M.toggle()
    if M.state.terminal_win and vim.api.nvim_win_is_valid(M.state.terminal_win) then
        M.close()
    else
        M.open()
    end
end

-- Check if chat is open
function M.is_open()
    return M.state.terminal_win and vim.api.nvim_win_is_valid(M.state.terminal_win)
end

-- Cleanup resources
function M.cleanup()
    stop_socket_server()
    M.state.terminal_buf = nil
    M.state.terminal_win = nil
    M.state.terminal_job = nil
end

-- ============================================================================
-- Autocmds for Context Sync
-- ============================================================================

local autocmd_group = nil

function M.setup_autocmds()
    -- Create autocmd group
    if autocmd_group then
        vim.api.nvim_del_augroup_by_id(autocmd_group)
    end
    autocmd_group = vim.api.nvim_create_augroup('TarkChat', { clear = true })
    
    -- Buffer enter
    vim.api.nvim_create_autocmd('BufEnter', {
        group = autocmd_group,
        callback = function()
            if M.state.client_conn then
                send_buffer_entered()
            end
        end,
    })
    
    -- Buffer write
    vim.api.nvim_create_autocmd('BufWritePost', {
        group = autocmd_group,
        callback = function(args)
            if M.state.client_conn then
                send_buffer_changed(args.buf)
            end
        end,
    })
    
    -- Diagnostics changed
    vim.api.nvim_create_autocmd('DiagnosticChanged', {
        group = autocmd_group,
        callback = function()
            if M.state.client_conn then
                send_diagnostics_updated()
            end
        end,
    })
    
    -- Cursor moved (debounced via send_context_debounced)
    vim.api.nvim_create_autocmd('CursorHold', {
        group = autocmd_group,
        callback = function()
            if M.state.client_conn then
                send_cursor_moved()
            end
        end,
    })
    
    -- Terminal close
    vim.api.nvim_create_autocmd('TermClose', {
        group = autocmd_group,
        buffer = M.state.terminal_buf,
        callback = function()
            vim.schedule(function()
                M.cleanup()
            end)
        end,
    })
end

-- ============================================================================
-- Setup and Commands
-- ============================================================================

-- Setup function
function M.setup(opts)
    M.config = vim.tbl_deep_extend('force', M.config, opts or {})
    
    -- Cleanup on Neovim exit
    vim.api.nvim_create_autocmd('VimLeavePre', {
        callback = function()
            -- Send editor closed notification
            if M.state.client_conn then
                send_rpc({ type = 'editor_closed' })
            end
            M.cleanup()
        end,
    })
end

-- Register commands (called from init.lua)
function M.register_commands()
    vim.api.nvim_create_user_command('TarkChatOpen', function()
        M.open()
    end, { desc = 'Open tark chat' })
    
    vim.api.nvim_create_user_command('TarkChatClose', function()
        M.close()
    end, { desc = 'Close tark chat' })
    
    vim.api.nvim_create_user_command('TarkChatToggle', function()
        M.toggle()
    end, { desc = 'Toggle tark chat' })
end

return M
