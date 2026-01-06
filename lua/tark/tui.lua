-- tark TUI integration
-- Opens tark chat in a terminal with socket-based Neovim integration

local M = {}

-- State
M.state = {
    buf = nil,           -- Terminal buffer
    win = nil,           -- Terminal window
    job_id = nil,        -- Terminal job
    socket_path = nil,   -- Unix socket path
    socket_server = nil, -- Socket server handle
    client = nil,        -- Connected TUI client
}

-- Config (set by setup)
M.config = {
    binary = nil,
    window = {
        position = 'right',
        width = 0.4,
        height = 0.5,
    },
}

-- ============================================================================
-- Helpers
-- ============================================================================

local function get_socket_path()
    local tmpdir = os.getenv('TMPDIR') or os.getenv('TMP') or '/tmp'
    return string.format('%s/tark-nvim-%d.sock', tmpdir, vim.fn.getpid())
end

local function json_encode(data)
    return vim.fn.json_encode(data)
end

local function json_decode(str)
    local ok, result = pcall(vim.fn.json_decode, str)
    return ok and result or nil
end

-- ============================================================================
-- RPC: Send messages to TUI
-- ============================================================================

local function send(msg)
    if M.state.client then
        local ok = pcall(function()
            M.state.client:write(json_encode(msg) .. '\n')
        end)
        return ok
    end
    return false
end

local function respond(id, result)
    send({ type = 'response', id = id, result = result })
end

local function respond_error(id, message)
    send({ type = 'error', id = id, message = message })
end

-- ============================================================================
-- RPC: Handle requests from TUI
-- ============================================================================

local handlers = {}

-- Open a file in Neovim
handlers.open_file = function(msg, id)
    local path = msg.path
    if not path then
        respond_error(id, 'path required')
        return
    end
    
    vim.schedule(function()
        -- Find a non-terminal window to open the file
        local target_win = nil
        for _, win in ipairs(vim.api.nvim_list_wins()) do
            local buf = vim.api.nvim_win_get_buf(win)
            if vim.bo[buf].buftype ~= 'terminal' then
                target_win = win
                break
            end
        end
        
        if target_win then
            vim.api.nvim_set_current_win(target_win)
        end
        
        vim.cmd('edit ' .. vim.fn.fnameescape(path))
        
        if msg.line and msg.line > 0 then
            local col = (msg.col and msg.col > 0) and (msg.col - 1) or 0
            pcall(vim.api.nvim_win_set_cursor, 0, { msg.line, col })
        end
        
        respond(id, { success = true })
    end)
end

-- Get list of open buffers
handlers.get_buffers = function(_, id)
    vim.schedule(function()
        local buffers = {}
        for _, bufnr in ipairs(vim.api.nvim_list_bufs()) do
            if vim.api.nvim_buf_is_loaded(bufnr) then
                local name = vim.api.nvim_buf_get_name(bufnr)
                local bt = vim.bo[bufnr].buftype
                if bt == '' and name ~= '' then
                    table.insert(buffers, {
                        id = bufnr,
                        path = name,
                        name = vim.fn.fnamemodify(name, ':t'),
                        modified = vim.bo[bufnr].modified,
                        filetype = vim.bo[bufnr].filetype,
                    })
                end
            end
        end
        respond(id, { buffers = buffers })
    end)
end

-- Get buffer content
handlers.get_buffer_content = function(msg, id)
    local path = msg.path
    if not path then
        respond_error(id, 'path required')
        return
    end
    
    vim.schedule(function()
        local bufnr = vim.fn.bufnr(path)
        local content
        
        if bufnr ~= -1 and vim.api.nvim_buf_is_loaded(bufnr) then
            local lines = vim.api.nvim_buf_get_lines(bufnr, 0, -1, false)
            content = table.concat(lines, '\n')
        else
            local f = io.open(path, 'r')
            if f then
                content = f:read('*a')
                f:close()
            else
                respond_error(id, 'File not found: ' .. path)
                return
            end
        end
        
        -- Truncate large files
        local max_size = 100 * 1024
        local truncated = #content > max_size
        if truncated then
            content = content:sub(1, max_size)
        end
        
        respond(id, { path = path, content = content, truncated = truncated })
    end)
end

-- Get diagnostics (LSP errors/warnings)
handlers.get_diagnostics = function(msg, id)
    vim.schedule(function()
        local filter_path = msg.path
        local diagnostics = {}
        local severity_map = {
            [vim.diagnostic.severity.ERROR] = 'error',
            [vim.diagnostic.severity.WARN] = 'warning',
            [vim.diagnostic.severity.INFO] = 'info',
            [vim.diagnostic.severity.HINT] = 'hint',
        }
        
        for _, diag in ipairs(vim.diagnostic.get()) do
            local path = vim.api.nvim_buf_get_name(diag.bufnr)
            if path ~= '' and (not filter_path or path:match(filter_path .. '$')) then
                table.insert(diagnostics, {
                    path = path,
                    line = diag.lnum + 1,
                    col = diag.col + 1,
                    severity = severity_map[diag.severity] or 'error',
                    message = diag.message,
                    source = diag.source,
                })
            end
        end
        
        respond(id, { diagnostics = diagnostics })
    end)
end

-- Get cursor position
handlers.get_cursor = function(_, id)
    vim.schedule(function()
        local path = vim.api.nvim_buf_get_name(0)
        local cursor = vim.api.nvim_win_get_cursor(0)
        respond(id, { path = path, line = cursor[1], col = cursor[2] + 1 })
    end)
end

-- Ping/pong for health check
handlers.ping = function(msg, _)
    send({ type = 'pong', seq = msg.seq })
end

-- Route incoming messages
local function handle_message(msg)
    if not msg or not msg.type then return end
    
    local id = nil
    local cmd = msg
    
    -- Unwrap request envelope
    if msg.type == 'request' then
        id = msg.id
        cmd = msg.command or {}
    end
    
    local handler = handlers[cmd.type]
    if handler then
        handler(cmd, id)
    elseif id then
        respond_error(id, 'Unknown command: ' .. tostring(cmd.type))
    end
end

-- ============================================================================
-- Socket Server
-- ============================================================================

local function start_socket_server()
    local socket_path = get_socket_path()
    M.state.socket_path = socket_path
    
    -- Remove old socket
    os.remove(socket_path)
    
    local server = vim.loop.new_pipe(false)
    if not server then
        vim.notify('tark: Failed to create socket', vim.log.levels.ERROR)
        return false
    end
    
    local ok, err = pcall(function() server:bind(socket_path) end)
    if not ok then
        vim.notify('tark: Socket bind failed: ' .. tostring(err), vim.log.levels.ERROR)
        server:close()
        return false
    end
    
    server:listen(1, function(listen_err)
        if listen_err then return end
        
        local client = vim.loop.new_pipe(false)
        server:accept(client)
        M.state.client = client
        
        local buffer = ''
        client:read_start(function(read_err, data)
            if read_err or not data then
                vim.schedule(function() M.state.client = nil end)
                return
            end
            
            buffer = buffer .. data
            while true do
                local nl = buffer:find('\n')
                if not nl then break end
                
                local line = buffer:sub(1, nl - 1)
                buffer = buffer:sub(nl + 1)
                
                if line ~= '' then
                    local msg = json_decode(line)
                    if msg then
                        vim.schedule(function() handle_message(msg) end)
                    end
                end
            end
        end)
        
        -- Send initial context
        vim.schedule(function()
            local path = vim.api.nvim_buf_get_name(0)
            if path ~= '' then
                send({ type = 'buffer_entered', path = path, filetype = vim.bo.filetype })
            end
        end)
    end)
    
    M.state.socket_server = server
    return true
end

local function stop_socket_server()
    if M.state.client then
        M.state.client:close()
        M.state.client = nil
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
-- Terminal Window Management
-- ============================================================================

local function get_binary_path()
    if M.config.binary and vim.fn.executable(M.config.binary) == 1 then
        return M.config.binary
    end
    
    -- Check data directory
    local data_bin = vim.fn.stdpath('data') .. '/tark/tark'
    if vim.fn.filereadable(data_bin) == 1 then
        return data_bin
    end
    
    -- Check PATH
    if vim.fn.executable('tark') == 1 then
        return 'tark'
    end
    
    return nil
end

local function calculate_size(value, total)
    if value <= 1 then
        return math.floor(total * value)
    end
    return math.floor(value)
end

-- Prevent concurrent opens (global across all requires)
if not _G._tark_opening then
    _G._tark_opening = false
end

function M.open()
    -- Debug: log every open attempt
    vim.notify('[tark] open() called, _G._tark_opening=' .. tostring(_G._tark_opening), vim.log.levels.DEBUG)
    
    -- Prevent concurrent opens
    if _G._tark_opening then
        vim.notify('[tark] blocked concurrent open', vim.log.levels.DEBUG)
        return
    end
    _G._tark_opening = true
    
    -- Ensure we reset the flag when done
    local function done()
        _G._tark_opening = false
    end
    
    -- Find ALL existing tark windows/buffers and close duplicates
    local tark_wins = {}
    for _, win in ipairs(vim.api.nvim_list_wins()) do
        local buf = vim.api.nvim_win_get_buf(win)
        local name = vim.api.nvim_buf_get_name(buf)
        local buftype = vim.bo[buf].buftype
        -- Check for tark buffer by name or by being a terminal with tark job
        if name:match('tark://') or name:match('tark$') or 
           (buftype == 'terminal' and name:match('tark')) then
            table.insert(tark_wins, { win = win, buf = buf })
            vim.notify('[tark] found existing tark win=' .. win .. ' buf=' .. buf .. ' name=' .. name, vim.log.levels.DEBUG)
        end
    end
    
    -- If we found tark windows
    if #tark_wins > 0 then
        vim.notify('[tark] found ' .. #tark_wins .. ' existing tark windows, focusing first', vim.log.levels.DEBUG)
        -- Keep the first one, close the rest
        for i = 2, #tark_wins do
            pcall(vim.api.nvim_win_close, tark_wins[i].win, true)
        end
        -- Focus the remaining one
        M.state.win = tark_wins[1].win
        M.state.buf = tark_wins[1].buf
        vim.api.nvim_set_current_win(M.state.win)
        vim.cmd('startinsert')
        done()
        return
    end
    
    -- Also check state
    if M.state.win and vim.api.nvim_win_is_valid(M.state.win) then
        vim.api.nvim_set_current_win(M.state.win)
        vim.cmd('startinsert')
        done()
        return
    end
    
    -- Clean up any stale state
    M.cleanup()
    
    -- Find binary
    local bin = get_binary_path()
    if not bin then
        vim.notify('tark: Binary not found. Run :TarkDownload', vim.log.levels.ERROR)
        done()
        return
    end
    
    -- Start socket server
    if not start_socket_server() then
        done()
        return
    end
    
    -- Build command
    local cmd = string.format('%s chat --socket %s', bin, M.state.socket_path)
    
    -- Calculate dimensions
    local pos = M.config.window.position
    local total_width = vim.o.columns
    local total_height = vim.o.lines
    
    -- Create split
    if pos == 'float' then
        local width = calculate_size(M.config.window.width, total_width)
        local height = calculate_size(M.config.window.height, total_height)
        
        M.state.buf = vim.api.nvim_create_buf(false, true)
        M.state.win = vim.api.nvim_open_win(M.state.buf, true, {
            relative = 'editor',
            width = width,
            height = height,
            col = math.floor((total_width - width) / 2),
            row = math.floor((total_height - height) / 2),
            style = 'minimal',
            border = 'rounded',
        })
    elseif pos == 'right' then
        local width = calculate_size(M.config.window.width, total_width)
        vim.notify('[tark] creating right split, width=' .. width, vim.log.levels.DEBUG)
        vim.cmd('botright vsplit')
        vim.cmd('vertical resize ' .. width)
    elseif pos == 'left' then
        local width = calculate_size(M.config.window.width, total_width)
        vim.cmd('topleft vsplit')
        vim.cmd('vertical resize ' .. width)
    elseif pos == 'bottom' then
        local height = calculate_size(M.config.window.height, total_height)
        vim.cmd('botright split')
        vim.cmd('resize ' .. height)
    elseif pos == 'top' then
        local height = calculate_size(M.config.window.height, total_height)
        vim.cmd('topleft split')
        vim.cmd('resize ' .. height)
    else
        -- Default: right
        local width = calculate_size(M.config.window.width, total_width)
        vim.cmd('botright vsplit')
        vim.cmd('vertical resize ' .. width)
    end
    
    -- Open terminal
    M.state.job_id = vim.fn.termopen(cmd, {
        on_exit = function()
            vim.schedule(function()
                M.cleanup()
            end)
        end,
    })
    
    if pos ~= 'float' then
        M.state.buf = vim.api.nvim_get_current_buf()
        M.state.win = vim.api.nvim_get_current_win()
    end
    
    -- Configure buffer
    vim.bo[M.state.buf].buflisted = false
    pcall(vim.api.nvim_buf_set_name, M.state.buf, 'tark://chat')
    
    -- Configure window to prevent accidental duplication
    vim.wo[M.state.win].winfixwidth = true
    vim.wo[M.state.win].number = false
    vim.wo[M.state.win].relativenumber = false
    vim.wo[M.state.win].signcolumn = 'no'
    
    -- Enter terminal mode
    vim.cmd('startinsert')
    
    -- Setup autocmds for context sync
    M.setup_autocmds()
    
    done()
end

function M.close()
    if M.state.win and vim.api.nvim_win_is_valid(M.state.win) then
        vim.api.nvim_win_close(M.state.win, true)
    end
    M.cleanup()
end

function M.toggle()
    if M.is_open() then
        M.close()
    else
        M.open()
    end
end

function M.is_open()
    return M.state.win and vim.api.nvim_win_is_valid(M.state.win)
end

function M.cleanup()
    stop_socket_server()
    M.state.buf = nil
    M.state.win = nil
    M.state.job_id = nil
end

-- ============================================================================
-- Autocmds for context sync
-- ============================================================================

local augroup = nil

function M.setup_autocmds()
    if augroup then
        vim.api.nvim_del_augroup_by_id(augroup)
    end
    augroup = vim.api.nvim_create_augroup('TarkTUI', { clear = true })
    
    -- Notify TUI when buffer changes
    vim.api.nvim_create_autocmd('BufEnter', {
        group = augroup,
        callback = function()
            if M.state.client then
                local path = vim.api.nvim_buf_get_name(0)
                if path ~= '' and vim.bo.buftype == '' then
                    send({ type = 'buffer_entered', path = path, filetype = vim.bo.filetype })
                end
            end
        end,
    })
    
    -- Notify on diagnostics change
    vim.api.nvim_create_autocmd('DiagnosticChanged', {
        group = augroup,
        callback = function()
            if M.state.client then
                send({ type = 'diagnostics_changed' })
            end
        end,
    })
    
    -- Cleanup on terminal close
    if M.state.buf then
        vim.api.nvim_create_autocmd('TermClose', {
            group = augroup,
            buffer = M.state.buf,
            callback = function()
                vim.schedule(function() M.cleanup() end)
            end,
        })
    end
end

-- ============================================================================
-- Setup
-- ============================================================================

function M.setup(config)
    M.config = vim.tbl_deep_extend('force', M.config, config or {})
    
    -- Cleanup on Neovim exit
    vim.api.nvim_create_autocmd('VimLeavePre', {
        callback = function()
            if M.state.client then
                send({ type = 'editor_closed' })
            end
            M.cleanup()
        end,
    })
end

return M

