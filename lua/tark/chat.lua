-- Chat interface for tark with provider selection and slash commands
-- With LSP proxy integration for enhanced agent capabilities

local M = {}

M.config = {
    server_url = 'http://localhost:8765',
    auto_show_diff = true,  -- Show inline diff in chat when files are modified
    docker_mode = false,    -- If true, server is in Docker (use /workspace as cwd)
    lsp_proxy = true,       -- Enable LSP proxy for agent tools
    window = {
        style = 'split',     -- 'split' (docked), 'sidepane' (floating overlay), or 'popup'
        position = 'right',  -- 'right' or 'left' (for split/sidepane)
        width = 80,          -- for popup mode
        height = 20,         -- for popup mode
        sidepane_width = 0.35,  -- 35% of editor width (or fixed number like 60)
        split_width = 80,    -- fixed width for split mode (columns)
        min_width = 50,
        max_width = 100,
        border = 'rounded',
    },
}

-- LSP proxy port (dynamically assigned when chat opens)
local lsp_proxy_port = nil

-- Get the cwd to send to server (handles Docker mode)
local function get_server_cwd()
    if M.config.docker_mode then
        -- Docker container has workspace mounted at /workspace
        return '/workspace'
    else
        return vim.fn.getcwd()
    end
end

local chat_buf = nil
local chat_win = nil
local current_provider = 'ollama'      -- Backend protocol for API calls
local current_provider_id = 'ollama'   -- Actual provider identity for display
local prompt_line_start = nil  -- Track where prompt starts in buffer

-- Store last modified files for diff view
local last_modified_files = {}

-- Open side-by-side diff view using git or vimdiff
local function show_diff_view(filepath)
    -- Close chat windows temporarily
    local chat_was_open = M.is_open and M.is_open()
    if chat_was_open then
        M.close()
    end
    
    local cwd = vim.fn.getcwd()
    local target_file = filepath
    
    -- If no file specified, try to use last modified or show git status
    if not target_file or target_file == '' then
        if #last_modified_files > 0 then
            target_file = last_modified_files[#last_modified_files]
        else
            -- Show all git changes
            vim.cmd('tabnew')
            vim.cmd('terminal git diff --color')
            vim.notify('Showing all git changes (press q to close)', vim.log.levels.INFO)
            return
        end
    end
    
    -- Check if file exists and has git changes
    local has_git = vim.fn.isdirectory(cwd .. '/.git') == 1
    
    if has_git then
        -- Use git diff in split view
        local git_show = vim.fn.system('cd ' .. vim.fn.shellescape(cwd) .. ' && git show HEAD:' .. vim.fn.shellescape(target_file) .. ' 2>/dev/null')
        local git_exit = vim.v.shell_error
        
        if git_exit == 0 then
            -- File exists in git, show proper diff
            local old_buf = vim.api.nvim_create_buf(false, true)
            local new_buf = vim.api.nvim_create_buf(false, true)
            
            -- Read current file content
            local current_content = vim.fn.readfile(cwd .. '/' .. target_file)
            
            -- Parse git content
            local old_lines = {}
            for line in git_show:gmatch('[^\r\n]*') do
                table.insert(old_lines, line)
            end
            
            -- Set buffer contents
            vim.api.nvim_buf_set_lines(old_buf, 0, -1, false, old_lines)
            vim.api.nvim_buf_set_lines(new_buf, 0, -1, false, current_content)
            
            -- Set buffer names with clear OLD/NEW labels
            local short_name = target_file:match('[^/]+$') or target_file
            vim.api.nvim_buf_set_name(old_buf, '[OLD] ' .. short_name .. ' (git HEAD)')
            vim.api.nvim_buf_set_name(new_buf, '[NEW] ' .. short_name .. ' (modified)')
            
            -- Detect filetype
            local ext = target_file:match('%.([^%.]+)$') or ''
            local ft_map = {
                rs = 'rust', lua = 'lua', py = 'python', js = 'javascript',
                ts = 'typescript', md = 'markdown', json = 'json', toml = 'toml',
                yaml = 'yaml', yml = 'yaml', html = 'html', css = 'css',
                go = 'go', java = 'java', c = 'c', cpp = 'cpp', h = 'c',
            }
            local filetype = ft_map[ext] or ''
            
            vim.api.nvim_buf_set_option(old_buf, 'filetype', filetype)
            vim.api.nvim_buf_set_option(new_buf, 'filetype', filetype)
            vim.api.nvim_buf_set_option(old_buf, 'buftype', 'nofile')
            vim.api.nvim_buf_set_option(new_buf, 'buftype', 'nofile')
            vim.api.nvim_buf_set_option(old_buf, 'modifiable', false)
            vim.api.nvim_buf_set_option(new_buf, 'modifiable', false)
            
            -- Open in vertical split with diff mode
            vim.cmd('tabnew')
            
            -- Configure diff display for clear +/- visibility
            vim.opt_local.diffopt = 'internal,filler,closeoff,algorithm:histogram,indent-heuristic'
            
            -- LEFT side: OLD file (from git HEAD)
            vim.api.nvim_win_set_buf(0, old_buf)
            vim.cmd('diffthis')
            vim.opt_local.number = true
            vim.opt_local.signcolumn = 'yes'
            vim.opt_local.foldcolumn = '0'
            vim.opt_local.cursorline = true
            
            -- RIGHT side: NEW file (modified)
            vim.cmd('vsplit')
            vim.api.nvim_win_set_buf(0, new_buf)
            vim.cmd('diffthis')
            vim.opt_local.number = true
            vim.opt_local.signcolumn = 'yes'
            vim.opt_local.foldcolumn = '0'
            vim.opt_local.cursorline = true
            
            -- Define clear highlight groups for diff
            vim.cmd([[
                highlight DiffAdd guibg=#1e3a1e guifg=NONE gui=NONE
                highlight DiffDelete guibg=#3a1e1e guifg=#666666 gui=NONE
                highlight DiffChange guibg=#1e2a3a guifg=NONE gui=NONE
                highlight DiffText guibg=#2e4a5a guifg=NONE gui=bold
            ]])
            
            -- Add signs for +/- indicators
            vim.fn.sign_define('DiffAdd', { text = '+', texthl = 'DiffAdd' })
            vim.fn.sign_define('DiffDelete', { text = '-', texthl = 'DiffDelete' })
            vim.fn.sign_define('DiffChange', { text = '~', texthl = 'DiffChange' })
            
            -- Set up keymaps to close
            local close_diff = function()
                vim.cmd('diffoff!')
                vim.cmd('tabclose')
                if chat_was_open then
                    vim.defer_fn(function() M.open() end, 50)
                end
            end
            
            vim.keymap.set('n', 'q', close_diff, { buffer = old_buf, silent = true })
            vim.keymap.set('n', 'q', close_diff, { buffer = new_buf, silent = true })
            vim.keymap.set('n', '<Esc>', close_diff, { buffer = old_buf, silent = true })
            vim.keymap.set('n', '<Esc>', close_diff, { buffer = new_buf, silent = true })
            
            -- Navigation keymaps
            vim.keymap.set('n', ']c', ']c', { buffer = old_buf, silent = true, desc = 'Next change' })
            vim.keymap.set('n', '[c', '[c', { buffer = new_buf, silent = true, desc = 'Previous change' })
            
            -- Show help message
            local short_file = target_file:match('[^/]+$') or target_file
            vim.notify(string.format(
                '📊 Diff: %s\n   LEFT = OLD (HEAD)  |  RIGHT = NEW (modified)\n   ]c = next change  |  [c = prev change  |  q = close',
                short_file
            ), vim.log.levels.INFO)
        else
            -- New file, not in git yet
            vim.cmd('tabnew')
            vim.cmd('edit ' .. vim.fn.fnameescape(cwd .. '/' .. target_file))
            vim.notify('New file (not yet in git): ' .. target_file, vim.log.levels.INFO)
        end
    else
        -- No git, just open the file
        vim.cmd('tabnew')
        vim.cmd('edit ' .. vim.fn.fnameescape(cwd .. '/' .. target_file))
        vim.notify('No git repo. Opened: ' .. target_file, vim.log.levels.INFO)
    end
end

-- Track modified files from tool calls
local function track_modified_file(filepath)
    -- Remove if already in list
    for i, f in ipairs(last_modified_files) do
        if f == filepath then
            table.remove(last_modified_files, i)
            break
        end
    end
    -- Add to end (most recent)
    table.insert(last_modified_files, filepath)
    -- Keep only last 10
    while #last_modified_files > 10 do
        table.remove(last_modified_files, 1)
    end
end

-- Export for external use
M.show_diff = show_diff_view

-- Token and cost tracking
local session_stats = {
    input_tokens = 0,
    output_tokens = 0,
    total_cost = 0,
}

-- Models database from models.dev
local models_db = {
    data = nil,
    timestamp = 0,
    cache_duration = 3600, -- 1 hour cache
}

-- Model mappings (our provider names to models.dev model IDs)
-- Current model (provider/model format)
local current_model = nil

-- Thinking mode - show verbose tool call info
local thinking_mode = false

-- Agent mode: plan (read-only), build (all tools), review (approval required)
local current_mode = 'build'

-- Loading animation state
local loading_timer = nil
local loading_frame = 1
local spinner_frames = { '⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏' }

-- Calculate display width (handles Unicode properly)
local function display_width(str)
    return vim.fn.strdisplaywidth(str)
end

-- Get loading message based on current mode
local function get_loading_message()
    local messages = {
        plan = {
            '🔍 Exploring codebase...',
            '📖 Reading files...',
            '🧠 Analyzing structure...',
            '🔎 Searching...',
        },
        build = {
            '🔨 Working...',
            '📝 Preparing changes...',
            '⚙️ Processing...',
            '🛠️ Building...',
        },
        review = {
            '👁️ Reviewing...',
            '🔍 Checking changes...',
            '✅ Validating...',
            '📋 Preparing actions...',
        },
    }
    local mode_messages = messages[current_mode] or messages.build
    return mode_messages[math.random(#mode_messages)]
end

-- Model mappings (our provider names to models.dev model IDs)
local model_mappings = {
    openai = 'openai/gpt-4o',
    claude = 'anthropic/claude-sonnet-4-20250514',
    ollama = nil, -- Local, no cost
}

-- Provider info for model picker
local providers_info = {
    {
        id = 'openai',
        name = 'OpenAI',
        icon = '🧠',
        api_key = 'openai',  -- Key in models.dev API
        our_provider = 'openai',
        env = 'OPENAI_API_KEY',
    },
    {
        id = 'anthropic',
        name = 'Anthropic (Claude)',
        icon = '🤖',
        api_key = 'anthropic',
        our_provider = 'claude',
        env = 'ANTHROPIC_API_KEY',
    },
    {
        id = 'google',
        name = 'Google (Gemini)',
        icon = '🔷',
        api_key = 'google',
        our_provider = 'openai',  -- Uses OpenAI-compatible
        env = 'GOOGLE_API_KEY',
    },
    {
        id = 'ollama',
        name = 'Ollama (Local)',
        icon = '🦙',
        api_key = 'ollama',
        our_provider = 'ollama',
        env = nil,
        local_fetch = true,
    },
}

-- Fetch models database from models.dev
-- SECURITY NOTE: This is a PUBLIC API - NO API keys are sent here!
-- API keys are ONLY sent to official provider endpoints in the Rust backend:
--   - OpenAI: https://api.openai.com/v1/chat/completions
--   - Anthropic: https://api.anthropic.com/v1/messages
--   - Ollama: http://localhost:11434 (local)
local function fetch_models_db()
    local now = os.time()
    if models_db.data and (now - models_db.timestamp) < models_db.cache_duration then
        return models_db.data
    end
    
    -- Async fetch - PUBLIC endpoint, no authentication
    vim.fn.jobstart({
        'curl', '-s', 'https://models.dev/api.json'
    }, {
        stdout_buffered = true,
        on_stdout = function(_, data)
            if data and data[1] and data[1] ~= '' then
                local ok, parsed = pcall(vim.fn.json_decode, table.concat(data, ''))
                if ok and parsed then
                    models_db.data = parsed
                    models_db.timestamp = now
                end
            end
        end,
    })
    
    return models_db.data
end

-- Get model info from database
local function get_model_info(provider)
    local model_id = model_mappings[provider]
    if not model_id then return nil end
    
    local db = fetch_models_db()
    if not db then return nil end
    
    -- Parse model_id (format: provider/model)
    local provider_key, model_key = model_id:match('^([^/]+)/(.+)$')
    if not provider_key then return nil end
    
    -- Map our provider names to models.dev provider keys
    local provider_map = {
        openai = 'openai',
        anthropic = 'anthropic',
        google = 'google',
    }
    provider_key = provider_map[provider_key] or provider_key
    
    -- Look up in nested structure
    local provider_data = db[provider_key]
    if provider_data and provider_data.models then
        return provider_data.models[model_key]
    end
    
    return nil
end

-- Estimate tokens (rough: ~4 chars per token for English)
local function estimate_tokens(text)
    if not text then return 0 end
    return math.ceil(#text / 4)
end

-- Format cost
local function format_cost(cost)
    if cost < 0.01 then
        return string.format('$%.4f', cost)
    elseif cost < 1 then
        return string.format('$%.3f', cost)
    else
        return string.format('$%.2f', cost)
    end
end

-- Format number with K/M suffix
local function format_number(n)
    if n >= 1000000 then
        local val = n / 1000000
        return val == math.floor(val) and string.format('%dM', val) or string.format('%.1fM', val)
    elseif n >= 1000 then
        local val = n / 1000
        return val == math.floor(val) and string.format('%dK', val) or string.format('%.1fK', val)
    else
        return tostring(n)
    end
end

-- Calculate cost for tokens
local function calculate_cost(input_tokens, output_tokens, model_info)
    if not model_info or not model_info.cost then return 0 end
    
    local input_cost = (model_info.cost.input or 0) * input_tokens / 1000000
    local output_cost = (model_info.cost.output or 0) * output_tokens / 1000000
    return input_cost + output_cost
end

-- Reset session stats
local function reset_stats()
    session_stats.input_tokens = 0
    session_stats.output_tokens = 0
    session_stats.total_cost = 0
end

-- Default context windows (fallback if models.dev not loaded)
local default_context_windows = {
    openai = 128000,   -- GPT-4o: 128K
    claude = 200000,   -- Claude: 200K
    ollama = 32000,    -- Most local models: ~32K
}

-- Build window title with stats
-- Get model info by looking up current_model directly
local function get_current_model_info()
    if not current_model then return nil end
    
    local db = fetch_models_db()
    if not db then return nil end
    
    -- Parse model_id (format: provider/model)
    local provider_key, model_key = current_model:match('^([^/]+)/(.+)$')
    if not provider_key then return nil end
    
    -- Look up in nested structure
    local provider_data = db[provider_key]
    if provider_data and provider_data.models then
        return provider_data.models[model_key]
    end
    
    return nil
end

local function build_chat_title()
    -- Use current_model directly for pricing lookup
    local model_info = get_current_model_info()
    
    -- Model name (short, clean)
    local model_name = current_model or model_mappings[current_provider] or current_provider
    local model_short = model_name:match('[^/]+$') or model_name
    
    -- Mode icons (distinctive shapes)
    local mode_icons = { plan = '◇', build = '◆', review = '◈' }
    local mode_icon = mode_icons[current_mode] or '◆'
    local mode_label = current_mode:sub(1,1):upper() .. current_mode:sub(2)
    
    local left_title = string.format(' %s %s · %s ', mode_icon, mode_label, model_short)
    
    -- Context info
    local used = session_stats.input_tokens + session_stats.output_tokens
    local context_max = default_context_windows[current_provider] or 128000
    if model_info and model_info.limit and model_info.limit.context then
        context_max = model_info.limit.context
    end
    
    local percent = context_max > 0 and math.floor((used / context_max) * 100) or 0
    
    -- Simple clean progress bar (like OpenCode)
    local bar_width = 15
    local filled = math.floor((percent / 100) * bar_width)
    local bar = '⚪' .. string.rep('▓', filled) .. string.rep('░', bar_width - filled)
    
    local right_title = string.format(' %s %s/%s ', bar, format_number(used), format_number(context_max))
    
    return left_title, right_title
end

-- Get mode-specific highlight group
local function get_mode_highlight(mode, with_bg)
    local suffix = with_bg and 'Bg' or ''
    local mode_highlights = {
        plan = 'TarkModePlan' .. suffix,
        build = 'TarkModeBuild' .. suffix,
        review = 'TarkModeReview' .. suffix,
    }
    return mode_highlights[mode] or ('TarkModeBuild' .. suffix)
end

-- Check if a window is a floating window
local function is_floating_window(win)
    if not win or not vim.api.nvim_win_is_valid(win) then
        return false
    end
    local config = vim.api.nvim_win_get_config(win)
    return config.relative ~= nil and config.relative ~= ''
end

-- Note: update_input_window_title removed - no longer needed with single-window design

-- Update chat window title with current stats
local function update_chat_window_title()
    if not chat_win or not vim.api.nvim_win_is_valid(chat_win) then
        return
    end
    
    -- Build title components
    local model_info = get_current_model_info()
    local model_name = current_model or model_mappings[current_provider] or current_provider
    local model_short = model_name:match('[^/]+$') or model_name
    
    -- Context info
    local used = session_stats.input_tokens + session_stats.output_tokens
    local context_max = default_context_windows[current_provider] or 128000
    if model_info and model_info.limit and model_info.limit.context then
        context_max = model_info.limit.context
    end
    
    local percent = context_max > 0 and math.floor((used / context_max) * 100) or 0
    local remaining = context_max - used
    
    -- Left: Model name
    local model_part = string.format(' %s ', model_short)
    
    -- Center: Context usage (simple format: [1%] 1K/128K)
    local context_part = string.format('[%d%%] %s/%s', percent, format_number(used), format_number(context_max))
    
    -- Right: Cost
    local cost_part = string.format('$%.4f ', session_stats.total_cost)
    
    -- Check if this is a floating window or a split window
    if is_floating_window(chat_win) then
        -- Floating window: use title config
        local config = vim.api.nvim_win_get_config(chat_win)
        local width = config.width or 80
        
        -- Calculate display widths
        local left_width = display_width(model_part)
        local center_width = display_width(context_part)
        local right_width = display_width(cost_part)
        
        -- INTELLIGENT PADDING: Left-align left, Center center, Right-align right
        local usable_width = width - 2  -- Account for border chars
        
        -- Where should center START to be truly centered?
        local center_start = math.floor((usable_width - center_width) / 2)
        -- Where should right START to be truly right-aligned?
        local right_start = usable_width - right_width
        
        -- Calculate padding needed
        local left_pad = center_start - left_width
        local right_pad = right_start - (center_start + center_width)
        
        -- Ensure minimum padding
        if left_pad < 1 then left_pad = 1 end
        if right_pad < 1 then right_pad = 1 end
        
        -- Color the context bar based on usage
        local context_hl = percent >= 90 and 'ErrorMsg' or (percent >= 75 and 'WarningMsg' or 'Comment')
        
        config.title = {
            { model_part, 'FloatTitle' },  -- Left: Model name
            { string.rep(' ', left_pad), 'FloatBorder' },
            { context_part, context_hl },  -- Center: Context usage (truly centered)
            { string.rep(' ', right_pad), 'FloatBorder' },
            { cost_part, 'String' },  -- Right: Cost (right-aligned)
        }
        config.title_pos = 'left'
        
        vim.api.nvim_win_set_config(chat_win, config)
    else
        -- Split window: use statusline
        -- Color the context based on usage
        local context_hl = percent >= 90 and 'ErrorMsg' or (percent >= 75 and 'WarningMsg' or 'Comment')
        vim.api.nvim_win_set_option(chat_win, 'statusline',
            string.format('%%#FloatTitle#%s%%#Normal# %%#%s#%s%%#Normal# %%#String#%s%%#Normal#',
                model_part, context_hl, context_part, cost_part))
    end
end

-- Update session stats and refresh UI
local function update_stats(input_text, output_text)
    local input_tokens = estimate_tokens(input_text)
    local output_tokens = estimate_tokens(output_text)
    
    session_stats.input_tokens = session_stats.input_tokens + input_tokens
    session_stats.output_tokens = session_stats.output_tokens + output_tokens
    
    local model_info = get_model_info(current_provider)
    if model_info then
        local cost = calculate_cost(input_tokens, output_tokens, model_info)
        session_stats.total_cost = session_stats.total_cost + cost
    end
    
    -- Update the window title with new stats
    vim.schedule(function()
        update_chat_window_title()
    end)
end

-- Slash commands registry
local slash_commands = {}

-- Create or get the chat buffer
local function get_chat_buffer()
    if chat_buf and vim.api.nvim_buf_is_valid(chat_buf) then
        return chat_buf
    end

    chat_buf = vim.api.nvim_create_buf(false, true)
    vim.api.nvim_buf_set_option(chat_buf, 'buftype', 'nofile')
    vim.api.nvim_buf_set_option(chat_buf, 'bufhidden', 'hide')
    vim.api.nvim_buf_set_option(chat_buf, 'filetype', 'markdown')
    vim.api.nvim_buf_set_name(chat_buf, 'tark-chat')
    
    -- Start with buffer modifiable (will be protected after adding prompt)
    vim.api.nvim_buf_set_option(chat_buf, 'modifiable', true)
    
    -- Add buffer protection: only allow editing the prompt line (last line)
    local augroup = vim.api.nvim_create_augroup('TarkChatProtection', { clear = true })
    
    vim.api.nvim_create_autocmd({'TextChanged', 'TextChangedI'}, {
        group = augroup,
        buffer = chat_buf,
        callback = function()
            -- Always keep buffer modifiable for the prompt line
            -- Protection is handled by making buffer non-modifiable when needed
            local total_lines = vim.api.nvim_buf_line_count(chat_buf)
            local last_line = vim.api.nvim_buf_get_lines(chat_buf, total_lines - 1, total_lines, false)[1] or ''
            
            -- Ensure prompt line exists
            if not last_line:match('^> ') then
                vim.schedule(function()
                    add_prompt_line()
                end)
            end
        end,
    })
    
    -- Protect against deleting lines above the prompt
    vim.api.nvim_create_autocmd('BufModifiedSet', {
        group = augroup,
        buffer = chat_buf,
        callback = function()
            -- Keep track of valid prompt line
            local total_lines = vim.api.nvim_buf_line_count(chat_buf)
            if total_lines < 3 then
                -- Buffer too small, restore prompt
                vim.schedule(function()
                    add_prompt_line()
                end)
            end
        end,
    })

    return chat_buf
end

-- Update chat header (no in-chat header, all info in window chrome)
local function update_chat_header()
    local buf = get_chat_buffer()
    
    -- Check if buffer needs initialization
    local line_count = vim.api.nvim_buf_line_count(buf)
    local current_lines = vim.api.nvim_buf_get_lines(buf, 0, -1, false)
    
    if line_count <= 1 and (not current_lines[1] or current_lines[1] == '') then
        -- Just add a blank line to start
        vim.api.nvim_buf_set_lines(buf, 0, -1, false, { '' })
    end
    
    -- Update window title bar (this has the model + context info)
    update_chat_window_title()
end

-- Slash command completion items
local function get_command_completions()
    return {
        { word = '/model', menu = 'Switch AI provider (picker)', kind = '🔄' },
        { word = '/model openai', menu = 'Switch to OpenAI GPT-4', kind = '🧠' },
        { word = '/model claude', menu = 'Switch to Claude', kind = '🤖' },
        { word = '/model ollama', menu = 'Switch to Ollama (local)', kind = '🦙' },
        { word = '/m', menu = 'Switch AI provider (short)', kind = '🔄' },
        { word = '/openai', menu = 'Quick switch to OpenAI', kind = '🧠' },
        { word = '/claude', menu = 'Quick switch to Claude', kind = '🤖' },
        { word = '/ollama', menu = 'Quick switch to Ollama', kind = '🦙' },
        { word = '/gpt', menu = 'Quick switch to OpenAI', kind = '🧠' },
        { word = '/clear', menu = 'Clear chat history & stats', kind = '🗑️' },
        { word = '/c', menu = 'Clear (short)', kind = '🗑️' },
        { word = '/stats', menu = 'Show session statistics', kind = '📊' },
        { word = '/s', menu = 'Stats (short)', kind = '📊' },
        { word = '/cost', menu = 'Show model pricing info', kind = '💰' },
        { word = '/compact', menu = 'Summarize to save context', kind = '🗜️' },
        { word = '/thinking', menu = 'Toggle verbose tool call output', kind = '🧠' },
        { word = '/verbose', menu = 'Toggle verbose mode', kind = '🧠' },
        { word = '/plan', menu = 'Plan mode: read-only', kind = '📝' },
        { word = '/build', menu = 'Build mode: full access', kind = '🔨' },
        { word = '/review', menu = 'Review mode: approval required', kind = '👁️' },
        { word = '/help', menu = 'Show all commands', kind = '📚' },
        { word = '/?', menu = 'Help (short)', kind = '📚' },
        { word = '/exit', menu = 'Close chat window', kind = '🚪' },
        { word = '/quit', menu = 'Close chat window', kind = '🚪' },
        { word = '/split', menu = 'Split layout (docked)', kind = '📐' },
        { word = '/sidepane', menu = 'Sidepane layout (floating)', kind = '📐' },
        { word = '/popup', menu = 'Popup layout (centered)', kind = '📐' },
        { word = '/layout', menu = 'Toggle split/sidepane/popup', kind = '📐' },
        { word = '/diff', menu = 'Side-by-side diff view', kind = '📊' },
        { word = '/autodiff', menu = 'Toggle auto diff on changes', kind = '📊' },
    }
end

-- File cache for @ completion
local file_cache = {
    files = {},
    cwd = nil,
    timestamp = 0,
}

-- Scan files in directory (with gitignore support)
local function scan_files(dir, max_files)
    max_files = max_files or 500
    local files = {}
    local count = 0
    
    -- Use fd if available (faster, respects gitignore), fallback to find
    local cmd
    if vim.fn.executable('fd') == 1 then
        cmd = string.format('cd %s && fd --type f --hidden --exclude .git --max-results %d 2>/dev/null', 
            vim.fn.shellescape(dir), max_files)
    elseif vim.fn.executable('find') == 1 then
        cmd = string.format('cd %s && find . -type f -not -path "*/.git/*" 2>/dev/null | head -n %d', 
            vim.fn.shellescape(dir), max_files)
    else
        return files
    end
    
    local handle = io.popen(cmd)
    if handle then
        for line in handle:lines() do
            -- Clean up path (remove ./ prefix)
            local path = line:gsub('^%./', '')
            if path ~= '' then
                table.insert(files, path)
                count = count + 1
                if count >= max_files then break end
            end
        end
        handle:close()
    end
    
    -- Sort files for consistent ordering
    table.sort(files)
    return files
end

-- Get file completions with caching
local function get_file_completions(base)
    local cwd = vim.fn.getcwd()
    local now = os.time()
    
    -- Refresh cache if cwd changed or cache is older than 30 seconds
    if file_cache.cwd ~= cwd or (now - file_cache.timestamp) > 30 then
        file_cache.files = scan_files(cwd, 500)
        file_cache.cwd = cwd
        file_cache.timestamp = now
    end
    
    -- Filter files based on input
    local matches = {}
    local search = base:lower():gsub('^@', '')
    
    for _, file in ipairs(file_cache.files) do
        -- Match anywhere in the path
        if file:lower():find(search, 1, true) then
            -- Determine icon based on extension
            local ext = file:match('%.([^%.]+)$') or ''
            local icon = ({
                lua = '🌙',
                rs = '🦀',
                py = '🐍',
                js = '📜',
                ts = '📘',
                tsx = '⚛️',
                jsx = '⚛️',
                json = '📋',
                toml = '⚙️',
                yaml = '⚙️',
                yml = '⚙️',
                md = '📝',
                txt = '📄',
                sh = '🐚',
                bash = '🐚',
                zsh = '🐚',
                vim = '💚',
                go = '🐹',
                rb = '💎',
                css = '🎨',
                html = '🌐',
                sql = '🗃️',
            })[ext] or '📁'
            
            table.insert(matches, {
                word = '@' .. file,
                menu = file,
                kind = icon,
            })
            
            if #matches >= 50 then break end
        end
    end
    
    return matches
end

-- Custom omnifunc for slash commands
local function slash_command_complete(findstart, base)
    if findstart == 1 then
        -- Find the start of the word
        local line = vim.api.nvim_get_current_line()
        local col = vim.fn.col('.') - 1
        -- If line starts with /, return 0 to complete from start
        if line:sub(1, 1) == '/' then
            return 0
        end
        return -3 -- No completion
    else
        -- Return matching completions
        local completions = get_command_completions()
        local matches = {}
        for _, item in ipairs(completions) do
            if item.word:lower():find(base:lower(), 1, true) == 1 then
                table.insert(matches, item)
            end
        end
        return matches
    end
end

-- Custom completefunc for @ file references
local function file_reference_complete(findstart, base)
    if findstart == 1 then
        local line = vim.api.nvim_get_current_line()
        local col = vim.fn.col('.') - 1
        -- Find the @ that starts this reference
        local at_pos = nil
        for i = col, 1, -1 do
            local char = line:sub(i, i)
            if char == '@' then
                at_pos = i - 1
                break
            elseif char == ' ' or char == '\t' then
                break
            end
        end
        if at_pos then
            return at_pos
        end
        return -3
    else
        return get_file_completions(base)
    end
end

-- Make completion functions globally accessible
_G.tark_slash_complete = slash_command_complete
_G.tark_file_complete = file_reference_complete

-- Create the input buffer
-- Add prompt line at bottom of chat buffer
local function add_prompt_line()
    if not chat_buf or not vim.api.nvim_buf_is_valid(chat_buf) then
        return
    end
    
    local lines = vim.api.nvim_buf_get_lines(chat_buf, 0, -1, false)
    local last_line = lines[#lines] or ''
    
    -- Add separator and prompt if not already there
    if not last_line:match('^> ') then
        local separator = string.rep('─', 80)
        vim.api.nvim_buf_set_option(chat_buf, 'modifiable', true)
        vim.api.nvim_buf_set_lines(chat_buf, -1, -1, false, {'', separator, '> '})
        vim.api.nvim_buf_set_option(chat_buf, 'modifiable', false)
        prompt_line_start = vim.api.nvim_buf_line_count(chat_buf)
    end
end

-- Get text from prompt line
local function get_prompt_text()
    if not chat_buf or not vim.api.nvim_buf_is_valid(chat_buf) then
        return ''
    end
    
    local last_line_num = vim.api.nvim_buf_line_count(chat_buf)
    local last_line = vim.api.nvim_buf_get_lines(chat_buf, last_line_num - 1, last_line_num, false)[1] or ''
    
    -- Extract text after '> '
    local text = last_line:match('^> (.*)') or ''
    return text
end

-- Clear prompt line
local function clear_prompt_line()
    if not chat_buf or not vim.api.nvim_buf_is_valid(chat_buf) then
        return
    end
    
    local last_line_num = vim.api.nvim_buf_line_count(chat_buf)
    vim.api.nvim_buf_set_option(chat_buf, 'modifiable', true)
    vim.api.nvim_buf_set_lines(chat_buf, last_line_num - 1, last_line_num, false, {'> '})
    vim.api.nvim_buf_set_option(chat_buf, 'modifiable', false)
end

-- Append a message to the chat
-- Scroll chat window to bottom
local function scroll_to_bottom()
    if not chat_win or not vim.api.nvim_win_is_valid(chat_win) then
        return
    end
    
    local buf = get_chat_buffer()
    local line_count = vim.api.nvim_buf_line_count(buf)
    
    -- Set cursor to last line
    vim.api.nvim_win_set_cursor(chat_win, { line_count, 0 })
    
    -- Also scroll the view to ensure it's visible
    vim.api.nvim_win_call(chat_win, function()
        vim.cmd('normal! zb')  -- Put current line at bottom of window
    end)
end

-- Track response start time for timing display
local response_start_time = nil

-- Create namespace for extmarks
local ns_id = vim.api.nvim_create_namespace('tark_chat_highlights')

-- Get user accent highlight based on current mode
local function get_user_accent_hl()
    local accent_map = {
        plan = 'TarkUserAccentPlan',
        build = 'TarkUserAccentBuild',
        review = 'TarkUserAccentReview',
    }
    return accent_map[current_mode] or 'TarkUserAccentBuild'
end

-- Format message with OpenCode-style design
local function append_message(role, content)
    local buf = get_chat_buffer()
    local lines = vim.api.nvim_buf_get_lines(buf, 0, -1, false)
    local start_line = #lines

    local new_lines = {}
    local highlight_ranges = {}  -- Track where to add highlights
    
    if role == 'user' then
        -- User message with left accent bar (clean, no username)
        table.insert(new_lines, '')
        table.insert(new_lines, '  ┃ ' .. content)
        table.insert(new_lines, '')
        -- Track accent bar position for highlighting
        table.insert(highlight_ranges, { line = start_line + 1, col_start = 2, col_end = 5, hl = get_user_accent_hl() })
        -- Start timing for response
        response_start_time = vim.fn.reltime()
        
    elseif role == 'assistant' then
        -- Assistant response - clean, no border
        table.insert(new_lines, '')
        for line in content:gmatch('[^\r\n]+') do
            table.insert(new_lines, line)
        end
        -- Simple footer: just timing (mode already shown in input bar)
        local elapsed = response_start_time and vim.fn.reltimestr(vim.fn.reltime(response_start_time)) or '0.0'
        elapsed = string.format('%.1fs', tonumber(elapsed) or 0)
        table.insert(new_lines, '')
        table.insert(new_lines, string.format('  ⏱ %s', elapsed))
        table.insert(new_lines, '')
        
    elseif role == 'thinking' then
        -- Thinking section - subtle gray style
        table.insert(new_lines, '')
        for line in content:gmatch('[^\r\n]+') do
            table.insert(new_lines, '    ' .. line)
        end
        
    elseif role == 'tools' then
        -- Tool actions - subtle
        for line in content:gmatch('[^\r\n]+') do
            table.insert(new_lines, '    ' .. line)
        end
        
    elseif role == 'diff' then
        -- Clean diff display with clear +/- markers
        table.insert(new_lines, '')
        
        -- Parse the diff content
        local removed_lines = {}
        local added_lines = {}
        local filename = ''
        local is_new_file = false
        
        local in_code_block = false
        for line in content:gmatch('[^\r\n]*') do
            if line:match('^```') then
                in_code_block = not in_code_block
            elseif in_code_block then
                if line:match('^# New file:') then
                    filename = line:match('^# New file: (.+)') or ''
                    is_new_file = true
                elseif line:match('^# Changes to') then
                    filename = line:match('^# Changes to (.+)') or ''
                elseif line:match('^%+ ') then
                    table.insert(added_lines, line:sub(3))
                elseif line:match('^%- ') then
                    table.insert(removed_lines, line:sub(3))
                elseif line:match('^%+[^%+]') then
                    table.insert(added_lines, line:sub(2))
                elseif line:match('^%-[^%-]') then
                    table.insert(removed_lines, line:sub(2))
                end
            end
        end
        
        -- Header with filename
        table.insert(new_lines, '  ╭─ 📊 ' .. (is_new_file and 'New file: ' or 'Modified: ') .. filename .. ' ─╮')
        
        -- Show removed lines (if any)
        if #removed_lines > 0 then
            table.insert(new_lines, '  │')
            table.insert(new_lines, '  │ ── Removed ──')
            for i, line in ipairs(removed_lines) do
                if i > 15 then
                    table.insert(new_lines, '  │   ... (' .. (#removed_lines - 15) .. ' more)')
                    break
                end
                local display = line
                if #display > 50 then display = display:sub(1, 47) .. '...' end
                table.insert(new_lines, '  │ - ' .. display)
                local line_num = #new_lines - 1 + start_line
                table.insert(highlight_ranges, { line = line_num, col_start = 4, col_end = 6, hl = 'ErrorMsg' })
            end
        end
        
        -- Show added lines
        if #added_lines > 0 then
            table.insert(new_lines, '  │')
            table.insert(new_lines, '  │ ── Added ──')
            for i, line in ipairs(added_lines) do
                if i > 15 then
                    table.insert(new_lines, '  │   ... (' .. (#added_lines - 15) .. ' more)')
                    break
                end
                local display = line
                if #display > 50 then display = display:sub(1, 47) .. '...' end
                table.insert(new_lines, '  │ + ' .. display)
                local line_num = #new_lines - 1 + start_line
                table.insert(highlight_ranges, { line = line_num, col_start = 4, col_end = 6, hl = 'String' })
            end
        end
        
        table.insert(new_lines, '  │')
        table.insert(new_lines, '  ╰─ `/diff` for full side-by-side view ─╯')
        table.insert(new_lines, '')
        
    elseif role == 'system' then
        -- System messages - very subtle
        for line in content:gmatch('[^\r\n]+') do
            table.insert(new_lines, '  ' .. line)
        end
        
    elseif role == 'separator' then
        table.insert(new_lines, '')
    end

    if #new_lines > 0 then
        vim.api.nvim_buf_set_option(buf, 'modifiable', true)
        
        -- Find where to insert (before prompt line)
        local total_lines = vim.api.nvim_buf_line_count(buf)
        local insert_pos = total_lines
        
        -- Check if last line is prompt (starts with '> ')
        local last_line = vim.api.nvim_buf_get_lines(buf, total_lines - 1, total_lines, false)[1] or ''
        if last_line:match('^> ') then
            -- Insert before the separator and prompt (3 lines: empty, separator, prompt)
            insert_pos = total_lines - 3
            if insert_pos < 0 then insert_pos = 0 end
        end
        
        vim.api.nvim_buf_set_lines(buf, insert_pos, insert_pos, false, new_lines)
        
        vim.api.nvim_buf_set_option(buf, 'modifiable', false)
        
        -- Apply highlights using extmarks
        for _, range in ipairs(highlight_ranges) do
            pcall(vim.api.nvim_buf_add_highlight, buf, ns_id, range.hl, range.line + insert_pos, range.col_start, range.col_end)
        end
    end

    -- Auto-scroll to bottom
    vim.schedule(function()
        scroll_to_bottom()
    end)
end

-- Thinking state for Cursor-like display
local thinking_state = {
    start_time = nil,
    actions = {},      -- List of actions taken
    current_action = nil,
    expanded = true,   -- Whether thinking section is expanded
    line_start = nil,  -- Where thinking section starts in buffer
}

-- Format thinking time
local function format_thinking_time()
    if not thinking_state.start_time then return '0s' end
    local elapsed = vim.fn.reltimefloat(vim.fn.reltime(thinking_state.start_time))
    return string.format('%.0fs', elapsed)
end

-- Stop loading animation and finalize display
local function stop_loading_animation()
    if loading_timer then
        vim.fn.timer_stop(loading_timer)
        loading_timer = nil
    end
    
    -- Finalize the thinking line with summary
    local buf = get_chat_buffer()
    if buf and vim.api.nvim_buf_is_valid(buf) and thinking_state.line_start then
        local time_str = format_thinking_time()
        local action_count = #thinking_state.actions
        
        -- Final summary: "*Thought for 5s* ▸ 8 actions"
        local summary = string.format('*Thought for %s* ▸ %d action%s', 
            time_str, 
            action_count,
            action_count == 1 and '' or 's'
        )
        
        pcall(function()
            vim.api.nvim_buf_set_lines(buf, thinking_state.line_start - 1, thinking_state.line_start, false, { summary })
        end)
    end
end

-- Update the thinking display (single line, updates in-place)
local function update_thinking_display()
    local buf = get_chat_buffer()
    if not buf or not vim.api.nvim_buf_is_valid(buf) then return end
    
    local line_start = thinking_state.line_start
    if not line_start then return end
    
    -- Build single line display
    local time_str = format_thinking_time()
    loading_frame = (loading_frame % #spinner_frames) + 1
    local spinner = spinner_frames[loading_frame]
    
    -- Single line: "*Thinking for 3s* ⠋ Grepped AdminService"
    local action_text = thinking_state.current_action or 'Processing...'
    local line = string.format('*Thinking for %s* %s %s', time_str, spinner, action_text)
    
    -- Update just this one line
    pcall(function()
        vim.api.nvim_buf_set_lines(buf, line_start - 1, line_start, false, { line })
    end)
end

-- Add completed action to history (for final summary)
local function add_thinking_action(icon, text)
    table.insert(thinking_state.actions, { icon = icon, text = text })
    -- Don't clear current_action - just track for summary
end

-- Set current action (updates single line in-place)
local function set_thinking_action(text)
    thinking_state.current_action = text
    update_thinking_display()
end

-- Start loading animation (Cursor-style thinking)
local function start_loading_animation()
    stop_loading_animation()
    loading_frame = 1
    
    -- Reset thinking state
    thinking_state = {
        start_time = vim.fn.reltime(),
        actions = {},
        current_action = get_loading_message(),
        expanded = true,
        line_start = nil,
    }
    
    local buf = get_chat_buffer()
    local loading_line_idx = vim.api.nvim_buf_line_count(buf)
    
    -- Add single thinking line
    local initial_line = '*Thinking for 0s* ' .. spinner_frames[1] .. ' ' .. thinking_state.current_action
    vim.api.nvim_buf_set_lines(buf, loading_line_idx, loading_line_idx, false, { '', initial_line, '' })
    thinking_state.line_start = loading_line_idx + 2  -- Point to the single thinking line
    
    -- Start animation timer (updates spinner and time)
    loading_timer = vim.fn.timer_start(100, function()
        vim.schedule(function()
            if not buf or not vim.api.nvim_buf_is_valid(buf) then
                stop_loading_animation()
                return
            end
            update_thinking_display()
        end)
    end, { ['repeat'] = -1 })
    
    return loading_line_idx
end

-- Status polling timer
local status_timer = nil
local last_status = ''

-- Map tool actions to display-friendly format with icons
local function format_action(action, tool_arg)
    local action_map = {
        ['Searching'] = { icon = '🔍', verb = 'Searched' },
        ['Reading'] = { icon = '📖', verb = 'Read' },
        ['Writing'] = { icon = '✏️', verb = 'Wrote' },
        ['Executing'] = { icon = '⚡', verb = 'Executed' },
        ['Analyzing'] = { icon = '🧠', verb = 'Analyzed' },
        ['Grepping'] = { icon = '🔎', verb = 'Grepped' },
        ['Listing'] = { icon = '📂', verb = 'Listed' },
        ['Deleting'] = { icon = '🗑️', verb = 'Deleted' },
        ['Planning'] = { icon = '📋', verb = 'Planned' },
    }
    
    local info = action_map[action] or { icon = '•', verb = action }
    local display_text = action or ''
    
    -- Ensure tool_arg is a string (could be vim.NIL from JSON)
    if tool_arg and type(tool_arg) == 'string' and tool_arg ~= '' then
        -- Shorten long paths
        local short_arg = tostring(tool_arg)
        if #short_arg > 40 then
            short_arg = '...' .. short_arg:sub(-37)
        end
        display_text = display_text .. ' `' .. short_arg .. '`'
    end
    
    return info.icon, display_text, info.verb
end

-- Poll agent status and update display
local function poll_status()
    local url = M.config.server_url .. '/chat/status'
    vim.fn.jobstart({
        'curl', '-s', '--connect-timeout', '1', url
    }, {
        stdout_buffered = true,
        on_stdout = function(_, data)
            vim.schedule(function()
                if data and data[1] and data[1] ~= '' then
                    local response = table.concat(data, '')
                    local ok, status = pcall(vim.fn.json_decode, response)
                    if ok and status then
                        if status.active and status.current_action and status.current_action ~= '' then
                            local icon, display_text, verb = format_action(status.current_action, status.tool_arg)
                            
                            -- Check if this is a new action
                            if display_text ~= last_status then
                                -- If action changed, add previous to history
                                if last_status ~= '' then
                                    local prev_action = last_status:match('^(%w+)') or ''
                                    local prev_arg = last_status:match('`([^`]+)`') or ''
                                    if prev_action ~= '' then
                                        local prev_icon, _, prev_verb = format_action(prev_action, nil)
                                        local history_text = prev_verb
                                        if prev_arg ~= '' then
                                            history_text = history_text .. ' `' .. prev_arg .. '`'
                                        end
                                        add_thinking_action(prev_icon, history_text)
                                    end
                                end
                                
                                last_status = display_text
                                set_thinking_action(display_text)
                            end
                        end
                    end
                end
            end)
        end,
    })
end

-- Start status polling
local function start_status_polling()
    if status_timer then
        vim.fn.timer_stop(status_timer)
    end
    last_status = ''
    status_timer = vim.fn.timer_start(200, function()
        vim.schedule(poll_status)
    end, { ['repeat'] = -1 })
end

-- Stop status polling
local function stop_status_polling()
    if status_timer then
        vim.fn.timer_stop(status_timer)
        status_timer = nil
    end
    last_status = ''
end

-- Send a message to the chat server
local function send_message(message)
    if not message or message == '' then
        return
    end

    append_message('user', message)

    -- Show animated loading indicator
    local buf = get_chat_buffer()
    start_loading_animation()
    
    -- Start polling for status updates
    start_status_polling()
    
    -- Scroll to show loading indicator
    vim.schedule(function()
        scroll_to_bottom()
    end)

    -- Get editor's current working directory
    local server_cwd = get_server_cwd()
    local clean_cwd = server_cwd:gsub('\\', '\\\\'):gsub('"', '\\"')

    -- Ensure message is a proper string (escape special characters)
    local clean_message = message:gsub('\\', '\\\\'):gsub('"', '\\"'):gsub('\n', '\\n'):gsub('\r', '\\r'):gsub('\t', '\\t')
    
    -- Build request body with optional LSP proxy port
    local req_parts = {
        '"message": "' .. clean_message .. '"',
        '"clear_history": false',
        '"provider": "' .. current_provider .. '"',
        '"cwd": "' .. clean_cwd .. '"',
        '"mode": "' .. current_mode .. '"',
    }
    
    -- Include LSP proxy port if available
    if lsp_proxy_port then
        table.insert(req_parts, '"lsp_proxy_port": ' .. lsp_proxy_port)
    end
    
    local req_body = '{' .. table.concat(req_parts, ', ') .. '}'

    vim.fn.jobstart({
        'curl',
        '-s',
        '-X', 'POST',
        '-H', 'Content-Type: application/json',
        '-d', req_body,
        M.config.server_url .. '/chat',
    }, {
        stdout_buffered = true,
        on_stdout = function(_, data)
            vim.schedule(function()
                -- Stop loading animation and status polling
                stop_loading_animation()
                stop_status_polling()
                
                -- Finalize thinking section - replace with collapsed summary
                local current_lines = vim.api.nvim_buf_get_lines(buf, 0, -1, false)
                local thinking_start = nil
                local thinking_end = nil
                
                -- Find thinking section boundaries
                for i = 1, #current_lines do
                    if current_lines[i]:match('^%*Thinking') then
                        thinking_start = i
                    elseif thinking_start and (current_lines[i] == '' and not current_lines[i]:match('^%s+[⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏•🔍📖✏️⚡🧠🔎📂🗑️📋▼▶]')) then
                        thinking_end = i
                        break
                    end
                end
                
                if thinking_start then
                    thinking_end = thinking_end or #current_lines
                    local final_time = format_thinking_time()
                    local action_count = #thinking_state.actions
                    
                    -- Create collapsed summary
                    local summary_lines = {}
                    if action_count > 0 then
                        table.insert(summary_lines, string.format('*Thought for %s* ▶ _%d actions_', final_time, action_count))
                    else
                        table.insert(summary_lines, string.format('*Thought for %s*', final_time))
                    end
                    table.insert(summary_lines, '')
                    
                    pcall(function()
                        vim.api.nvim_buf_set_lines(buf, thinking_start - 1, thinking_end, false, summary_lines)
                    end)
                end

                if data and data[1] and data[1] ~= '' then
                    local response_text = table.concat(data, '')
                    local ok, resp = pcall(vim.fn.json_decode, response_text)
                    if ok and resp and resp.response then
                        -- Track token usage and update stats
                        update_stats(message, resp.response)
                        
                        -- Check if context is getting full and warn user
                        local context_max = model_info and model_info.limit and model_info.limit.context
                            or default_context_windows[current_provider] or 128000
                        local used = session_stats.input_tokens + session_stats.output_tokens
                        local percent = math.floor((used / context_max) * 100)
                        
                        -- Show auto-compaction status from server
                        if resp.auto_compacted then
                            append_message('system', '🔄 **Context auto-compacted!** Previous conversation summarized to save space.')
                        elseif percent >= 70 then
                            append_message('system', '📦 **Context at ' .. percent .. '%** - Auto-compact will trigger at 80%')
                        end
                        
                        -- Update with actual usage from server if available
                        if resp.context_usage_percent then
                            -- Server reports actual usage, use that for UI
                            percent = resp.context_usage_percent
                        end
                        
                        -- Track modified files (always, regardless of thinking mode)
                        local modified_files = {}
                        if resp.tool_call_log and #resp.tool_call_log > 0 then
                            for _, tc in ipairs(resp.tool_call_log) do
                                local args = tc.args or {}
                                if tc.tool == 'write_file' or tc.tool == 'patch_file' or tc.tool == 'propose_change' then
                                    local path = args.path or nil
                                    if path and path ~= '?' then
                                        track_modified_file(path)
                                        table.insert(modified_files, path)
                                    end
                                end
                            end
                        end
                        
                        -- Show verbose tool call info if thinking mode is on
                        if thinking_mode and resp.tool_call_log and #resp.tool_call_log > 0 then
                            -- Build thinking content
                            local thinking_lines = {}
                            table.insert(thinking_lines, 'User asked: ' .. message:sub(1, 50) .. (message:len() > 50 and '...' or ''))
                            
                            -- Categorize and display tool calls
                            local reads = {}
                            local writes = {}
                            local searches = {}
                            
                            for _, tc in ipairs(resp.tool_call_log) do
                                local args = tc.args or {}
                                if tc.tool == 'read_file' or tc.tool == 'read_files' then
                                    local path = args.path or (args.paths and table.concat(args.paths, ', ')) or '?'
                                    table.insert(reads, path)
                                elseif tc.tool == 'write_file' or tc.tool == 'patch_file' or tc.tool == 'delete_file' or tc.tool == 'propose_change' then
                                    local path = args.path or '?'
                                    table.insert(writes, path)
                                elseif tc.tool == 'file_search' or tc.tool == 'grep' or tc.tool == 'find_references' then
                                    table.insert(searches, args.pattern or args.symbol or '?')
                                elseif tc.tool == 'codebase_overview' then
                                    table.insert(thinking_lines, 'Analyzing codebase structure...')
                                elseif tc.tool == 'list_directory' then
                                    table.insert(thinking_lines, 'Listing ' .. (args.path or '.') .. '/')
                                end
                            end
                            
                            if #searches > 0 then
                                table.insert(thinking_lines, '🔍 Searching: ' .. table.concat(searches, ', '))
                            end
                            
                            if #reads > 0 then
                                table.insert(thinking_lines, '📖 Reading:')
                                for _, f in ipairs(reads) do
                                    local short = f:match('[^/]+$') or f
                                    table.insert(thinking_lines, '   • ' .. short)
                                end
                            end
                            
                            if #writes > 0 then
                                table.insert(thinking_lines, '✏️ Modified:')
                                for _, f in ipairs(writes) do
                                    local short = f:match('[^/]+$') or f
                                    table.insert(thinking_lines, '   • ' .. short)
                                end
                                -- Add diff prompt
                                table.insert(thinking_lines, '')
                                table.insert(thinking_lines, '💡 Type `/diff` to view side-by-side changes')
                            end
                            
                            -- Display as thinking section
                            append_message('thinking', table.concat(thinking_lines, '\n'))
                        end
                        
                        -- Show inline diff in chat for modified files (if enabled)
                        if #modified_files > 0 and M.config.auto_show_diff then
                            for _, filepath in ipairs(modified_files) do
                                local cwd = vim.fn.getcwd()
                                local full_path = cwd .. '/' .. filepath
                                local has_git = vim.fn.isdirectory(cwd .. '/.git') == 1
                                local diff_shown = false
                                
                                if has_git then
                                    -- Try git diff first (for tracked files)
                                    local diff_output = vim.fn.system('cd ' .. vim.fn.shellescape(cwd) .. ' && git diff --no-color -- ' .. vim.fn.shellescape(filepath) .. ' 2>/dev/null')
                                    
                                    if diff_output and diff_output ~= '' and vim.v.shell_error == 0 then
                                        -- Format diff for display
                                        local diff_lines = {}
                                        local short_name = filepath:match('[^/]+$') or filepath
                                        table.insert(diff_lines, '```diff')
                                        table.insert(diff_lines, '# Changes to ' .. short_name)
                                        
                                        -- Parse and include relevant diff lines
                                        local line_count = 0
                                        for line in diff_output:gmatch('[^\r\n]+') do
                                            -- Skip diff header lines, keep the actual changes
                                            if line:match('^[+-]') and not line:match('^[+-][+-][+-]') then
                                                table.insert(diff_lines, line)
                                                line_count = line_count + 1
                                            elseif line:match('^@@') then
                                                table.insert(diff_lines, line)
                                            end
                                            -- Limit to prevent huge diffs
                                            if line_count > 50 then
                                                table.insert(diff_lines, '... (truncated, use /diff for full view)')
                                                break
                                            end
                                        end
                                        
                                        table.insert(diff_lines, '```')
                                        
                                        if line_count > 0 then
                                            append_message('diff', table.concat(diff_lines, '\n'))
                                            diff_shown = true
                                        end
                                    end
                                    
                                    -- If no diff (new file or already staged), show file content as "new"
                                    if not diff_shown then
                                        -- Check if it's a new untracked file
                                        local is_untracked = vim.fn.system('cd ' .. vim.fn.shellescape(cwd) .. ' && git ls-files -- ' .. vim.fn.shellescape(filepath)):gsub('%s+', '') == ''
                                        
                                        if is_untracked and vim.fn.filereadable(full_path) == 1 then
                                            local content = vim.fn.readfile(full_path)
                                            if #content > 0 then
                                                local diff_lines = {}
                                                local short_name = filepath:match('[^/]+$') or filepath
                                                table.insert(diff_lines, '```diff')
                                                table.insert(diff_lines, '# New file: ' .. short_name)
                                                
                                                local line_count = 0
                                                for _, line in ipairs(content) do
                                                    table.insert(diff_lines, '+ ' .. line)
                                                    line_count = line_count + 1
                                                    if line_count > 30 then
                                                        table.insert(diff_lines, '+ ... (' .. (#content - 30) .. ' more lines)')
                                                        break
                                                    end
                                                end
                                                
                                                table.insert(diff_lines, '```')
                                                append_message('diff', table.concat(diff_lines, '\n'))
                                                diff_shown = true
                                            end
                                        end
                                    end
                                end
                                
                                -- If still no diff shown (no git or other issue), just note the file was modified
                                if not diff_shown then
                                    local short_name = filepath:match('[^/]+$') or filepath
                                    append_message('system', '📝 Modified: `' .. short_name .. '` (use `/diff` to view changes)')
                                end
                            end
                        end
                        
                        append_message('assistant', resp.response)
                        
                        -- Update provider if changed
                        if resp.provider then
                            current_provider = resp.provider
                        end
                        
                        -- Always update header to show new stats
                        update_chat_header()
                        
                        -- Show tool call summary only if not in thinking mode
                        if resp.tool_calls_made and resp.tool_calls_made > 0 and not thinking_mode then
                            append_message('system', '(' .. resp.tool_calls_made .. ' actions performed)')
                        end
                        -- Final scroll to ensure we see the end
                        scroll_to_bottom()
                    elseif ok and resp and resp.error then
                        append_message('system', '❌ *Error: ' .. resp.error .. '*')
                        scroll_to_bottom()
                    else
                        append_message('system', '❌ *Failed to parse response*')
                        scroll_to_bottom()
                    end
                end
            end)
        end,
        on_stderr = function(_, data)
            vim.schedule(function()
                stop_status_polling()
                stop_loading_animation()
                if data and data[1] and data[1] ~= '' then
                    append_message('system', '❌ *Error: Could not connect to server. Run: tark serve*')
                    scroll_to_bottom()
                end
            end)
        end,
    })
end

-- Switch to a specific provider
local function switch_provider(new_provider, skip_message)
    if new_provider == current_provider then
        -- Still update header in case model changed
        update_chat_header()
        if not skip_message then
            append_message('system', '✓ *Already using ' .. new_provider .. '*')
        end
        return
    end

    local req_body = '{"provider": "' .. new_provider .. '"}'
    vim.fn.jobstart({
        'curl', '-s', '-X', 'POST',
        '-H', 'Content-Type: application/json',
        '-d', req_body,
        M.config.server_url .. '/provider',
    }, {
        stdout_buffered = true,
        on_stdout = function(_, data)
            vim.schedule(function()
                if data and data[1] then
                    local ok, resp = pcall(vim.fn.json_decode, table.concat(data, ''))
                    if ok and resp and resp.success then
                        current_provider = new_provider
                        update_chat_header()
                        append_message('system', '✅ *Switched to ' .. new_provider .. '*')
                        -- Clear history with new provider
                        local clear_body = '{"message": "", "clear_history": true, "provider": "' .. new_provider .. '"}'
                        vim.fn.system('curl -s -X POST -H "Content-Type: application/json" -d \'' .. clear_body .. '\' ' .. M.config.server_url .. '/chat')
                    elseif ok and resp and resp.error then
                        append_message('system', '❌ *Error: ' .. resp.error .. '*')
                    end
                end
            end)
        end,
    })
end

-- Get models for a specific provider from models.dev
local function get_models_for_provider(provider_key)
    local db = models_db.data
    if not db then return {} end
    
    -- Remove trailing slash from prefix if present
    provider_key = provider_key:gsub('/$', '')
    
    local provider_data = db[provider_key]
    if not provider_data or not provider_data.models then
        return {}
    end
    
    local models = {}
    for model_key, model in pairs(provider_data.models) do
        -- Skip deprecated models
        if not model.status or model.status ~= 'deprecated' then
            -- Add full id if not present
            local model_copy = vim.tbl_deep_extend('force', {}, model)
            model_copy.id = model_copy.id or (provider_key .. '/' .. model_key)
            table.insert(models, model_copy)
        end
    end
    
    -- Sort by name
    table.sort(models, function(a, b)
        return (a.name or a.id) < (b.name or b.id)
    end)
    
    return models
end

-- Fetch Ollama models from local server
local function fetch_ollama_models(callback)
    vim.fn.jobstart({
        'curl', '-s', 'http://localhost:11434/api/tags'
    }, {
        stdout_buffered = true,
        on_stdout = function(_, data)
            vim.schedule(function()
                local models = {}
                if data and data[1] and data[1] ~= '' then
                    local ok, resp = pcall(vim.fn.json_decode, table.concat(data, ''))
                    if ok and resp and resp.models then
                        for _, m in ipairs(resp.models) do
                            table.insert(models, {
                                id = 'ollama/' .. m.name,
                                name = m.name,
                                size = m.size,
                            })
                        end
                    end
                end
                callback(models)
            end)
        end,
        on_exit = function(_, code)
            if code ~= 0 then
                vim.schedule(function()
                    callback({})
                end)
            end
        end,
    })
end

-- Show model picker for a specific provider
local function show_model_picker(provider_info, models)
    if #models == 0 then
        append_message('system', '❌ *No models found for ' .. provider_info.name .. '*')
        return
    end
    
    local items = {}
    for _, m in ipairs(models) do
        local display = m.name or m.id
        -- Add context window info if available
        if m.limit and m.limit.context then
            display = display .. string.format(' (%s ctx)', format_number(m.limit.context))
        end
        -- Add cost info if available
        if m.cost and m.cost.input then
            display = display .. string.format(' - $%.2f/1M in', m.cost.input)
        end
        -- Add size for Ollama
        if m.size then
            local size_gb = m.size / (1024 * 1024 * 1024)
            display = display .. string.format(' (%.1fGB)', size_gb)
        end
        table.insert(items, provider_info.icon .. ' ' .. display)
    end
    
    vim.ui.select(items, {
        prompt = 'Select ' .. provider_info.name .. ' Model:',
    }, function(choice, idx)
        if choice and idx then
            local selected_model = models[idx]
            local model_id = selected_model.id
            local provider_name = provider_info.our_provider or provider_info.id
            
            -- Update model mapping
            model_mappings[provider_name] = model_id
            current_model = model_id
            current_provider_id = provider_info.id  -- Track actual provider for display
            
            -- Switch provider (skip duplicate message since we show our own)
            switch_provider(provider_name, true)
            
            -- Update header to show new model
            update_chat_header()
            
            -- Save session to persist selection
            vim.defer_fn(function()
                if M.save_session then M.save_session() end
            end, 100)
            
            append_message('system', '📦 *Selected: ' .. (selected_model.name or model_id) .. '*')
        end
        -- Return focus to input window
        if input_win and vim.api.nvim_win_is_valid(input_win) then
            vim.api.nvim_set_current_win(input_win)
            vim.cmd('startinsert')
        end
    end)
end

-- Show provider selection menu (first step)
local function show_provider_menu()
    local items = {}
    for _, p in ipairs(providers_info) do
        local marker = ''
        if current_provider == p.id or 
           (current_provider == 'claude' and p.id == 'anthropic') or
           (current_provider == 'openai' and p.id == 'openai') then
            marker = '✓ '
        end
        table.insert(items, marker .. p.icon .. ' ' .. p.name)
    end

    vim.ui.select(items, {
        prompt = 'Select AI Provider:',
    }, function(choice, idx)
        if choice and idx then
            local provider = providers_info[idx]
            
            -- Show loading message
            append_message('system', '🔍 *Fetching models for ' .. provider.name .. '...*')
            
            if provider.local_fetch then
                -- Fetch from local Ollama
                fetch_ollama_models(function(models)
                    if #models > 0 then
                        show_model_picker(provider, models)
                    else
                        append_message('system', '❌ *No Ollama models found. Is Ollama running?*')
                        -- Return focus
                        if input_win and vim.api.nvim_win_is_valid(input_win) then
                            vim.api.nvim_set_current_win(input_win)
                            vim.cmd('startinsert')
                        end
                    end
                end)
            else
                -- Get from models.dev cache
                local models = get_models_for_provider(provider.api_key)
                if #models > 0 then
                    show_model_picker(provider, models)
                else
                    -- Try fetching fresh data
                    append_message('system', '⏳ *Loading models database...*')
                    fetch_models_db()
                    vim.defer_fn(function()
                        local fresh_models = get_models_for_provider(provider.api_key)
                        if #fresh_models > 0 then
                            show_model_picker(provider, fresh_models)
                        else
                            append_message('system', '❌ *No models found. Try again in a few seconds.*')
                            -- Return focus
                            if input_win and vim.api.nvim_win_is_valid(input_win) then
                                vim.api.nvim_set_current_win(input_win)
                                vim.cmd('startinsert')
                            end
                        end
                    end, 3000)
                end
            end
        else
            -- Return focus to input window
            if input_win and vim.api.nvim_win_is_valid(input_win) then
                vim.api.nvim_set_current_win(input_win)
                vim.cmd('startinsert')
            end
        end
    end)
end

-- Quick provider switch (for backwards compatibility)
local function quick_switch_provider(provider_name)
    switch_provider(provider_name)
end

-- Clear chat history
local function clear_chat()
    local buf = get_chat_buffer()
    vim.api.nvim_buf_set_lines(buf, 0, -1, false, {})
    -- Reset session stats
    reset_stats()
    update_chat_header()
    update_chat_window_title()
    
    -- Also clear server-side history
    local clear_body = '{"message": "", "clear_history": true, "provider": "' .. current_provider .. '"}'
    vim.fn.system('curl -s -X POST -H "Content-Type: application/json" -d \'' .. clear_body .. '\' ' .. M.config.server_url .. '/chat')
    
    append_message('system', '🗑️ *Chat history cleared*')
end

-- Define slash commands
slash_commands = {
    -- Model/Provider commands
    ['model'] = {
        description = 'Switch AI model/provider',
        usage = '/model [ollama|claude|openai]',
        handler = function(args)
            if args and args ~= '' then
                local provider = args:lower():gsub('^%s+', ''):gsub('%s+$', '')
                if provider == 'ollama' or provider == 'claude' or provider == 'openai' or
                   provider == 'gpt' or provider == 'anthropic' or provider == 'local' then
                    -- Normalize aliases
                    if provider == 'gpt' then provider = 'openai' end
                    if provider == 'anthropic' then provider = 'claude' end
                    if provider == 'local' then provider = 'ollama' end
                    switch_provider(provider)
                else
                    append_message('system', '❌ *Unknown provider: ' .. provider .. '. Use: ollama, claude, or openai*')
                end
            else
                show_provider_menu()
            end
        end,
    },
    ['m'] = { alias = 'model' },
    
    -- Clear command
    ['clear'] = {
        description = 'Clear chat history',
        usage = '/clear',
        handler = function()
            clear_chat()
        end,
    },
    ['c'] = { alias = 'clear' },
    
    -- Help command
    ['help'] = {
        description = 'Show available commands',
        usage = '/help',
        handler = function()
            local help_lines = {
                '📚 **Available Commands:**',
                '',
                '`/model [provider]` or `/m` - Switch AI provider',
                '  • `/model` - Open provider picker',
                '  • `/model openai` - Switch to OpenAI',
                '  • `/model claude` - Switch to Claude',
                '  • `/model ollama` - Switch to Ollama (local)',
                '',
                '`/clear` or `/c` - Clear chat history and reset stats',
                '',
                '`/compact` - Summarize conversation to save context (for large codebases)',
                '',
                '`/thinking` or `/verbose` - Toggle verbose mode (show agent tool calls)',
                '',
                '**Agent Modes** (or press `Tab` to toggle plan/build):',
                '`/plan` - Plan mode: read-only exploration, no file modifications',
                '`/build` - Build mode: full access to all tools',
                '`/review` - Review mode: approval required before each action',
                '',
                '`/stats` or `/s` - Show detailed session statistics',
                '',
                '`/cost` - Show model pricing info',
                '',
                '`/usage` - Show usage summary (tokens, costs, sessions)',
                '`/usage-open` - Open usage dashboard in browser',
                '',
                '`/exit` or `/q` - Close chat window',
                '',
                '**Layout:**',
                '`/sidepane` - Right-side panel (full height)',
                '`/popup` - Centered floating window',
                '`/layout` - Toggle between layouts',
                '',
                '**Diff View:**',
                '`/diff [file]` - Side-by-side diff (git HEAD vs current)',
                '  • No args: diff last modified file',
                '  • With file: diff specific file',
                '`/autodiff` - Toggle automatic diff on file changes (ON by default)',
                '',
                '`/help` or `/?` - Show this help',
                '',
                '**References:**',
                '• `@filename` - Reference a file for context',
                '',
                '**Keyboard Shortcuts:**',
                '• `Enter` - Send message',
                '• `Tab` - Toggle plan/build mode',
            }
            append_message('system', table.concat(help_lines, '\n'))
        end,
    },
    ['?'] = { alias = 'help' },
    
    -- Stats command
    ['stats'] = {
        description = 'Show session statistics',
        usage = '/stats',
        handler = function()
            local model_info = get_model_info(current_provider)
            local total_tokens = session_stats.input_tokens + session_stats.output_tokens
            
            -- Get context window
            local context_max = nil
            if model_info and model_info.limit and model_info.limit.context then
                context_max = model_info.limit.context
            else
                context_max = default_context_windows[current_provider]
            end
            
            local stats_lines = {
                '📊 **Session Statistics:**',
                '',
                string.format('**Provider:** %s', current_provider),
            }
            
            -- Context window info
            if context_max then
                local percent = context_max > 0 and math.floor((total_tokens / context_max) * 100) or 0
                local remaining = context_max - total_tokens
                table.insert(stats_lines, string.format('**Context Window:** %s tokens', format_number(context_max)))
                table.insert(stats_lines, string.format('**Context Used:** %s (%d%%)', format_number(total_tokens), percent))
                table.insert(stats_lines, string.format('**Context Remaining:** %s tokens', format_number(remaining)))
                
                if percent >= 90 then
                    table.insert(stats_lines, '')
                    table.insert(stats_lines, '⚠️ **Warning:** Context nearly full! Use `/clear` to reset.')
                elseif percent >= 75 then
                    table.insert(stats_lines, '')
                    table.insert(stats_lines, '⚡ **Note:** Context filling up. Consider `/clear` soon.')
                end
            end
            
            table.insert(stats_lines, '')
            table.insert(stats_lines, '**Token Breakdown:**')
            table.insert(stats_lines, string.format('• Input tokens: %s', format_number(session_stats.input_tokens)))
            table.insert(stats_lines, string.format('• Output tokens: %s', format_number(session_stats.output_tokens)))
            table.insert(stats_lines, string.format('• Total tokens: %s', format_number(total_tokens)))
            
            if session_stats.total_cost > 0 then
                table.insert(stats_lines, '')
                table.insert(stats_lines, string.format('**Estimated Cost:** %s', format_cost(session_stats.total_cost)))
            elseif current_provider == 'ollama' then
                table.insert(stats_lines, '')
                table.insert(stats_lines, '**Cost:** Free (local model)')
            end
            
            table.insert(stats_lines, '')
            table.insert(stats_lines, '*Context is stored server-side. Use `/clear` to reset.*')
            
            append_message('system', table.concat(stats_lines, '\n'))
        end,
    },
    ['s'] = { alias = 'stats' },
    
    -- Compact/summarize context command
    ['compact'] = {
        description = 'Summarize conversation to save context',
        usage = '/compact',
        handler = function()
            append_message('system', '🗜️ *Compacting conversation...*')
            
            -- Send request to compact the context
            local server_cwd = get_server_cwd()
            local clean_cwd = server_cwd:gsub('\\', '\\\\'):gsub('"', '\\"')
            local req_body = '{"message": "Please provide a brief summary of our conversation so far in 2-3 sentences. Focus on: what files were discussed, what changes were made, and what the user wanted to accomplish. Start with: CONTEXT SUMMARY:", "clear_history": false, "provider": "' .. current_provider .. '", "cwd": "' .. clean_cwd .. '"}'
            
            vim.fn.jobstart({
                'curl', '-s', '-X', 'POST',
                '-H', 'Content-Type: application/json',
                '-d', req_body,
                M.config.server_url .. '/chat',
            }, {
                stdout_buffered = true,
                on_stdout = function(_, data)
                    vim.schedule(function()
                        if data and data[1] and data[1] ~= '' then
                            local ok, resp = pcall(vim.fn.json_decode, table.concat(data, ''))
                            if ok and resp and resp.response then
                                local summary = resp.response
                                
                                -- Now clear and start fresh with summary
                                local clear_body = '{"message": "Previous context summary: ' .. summary:gsub('"', '\\"'):gsub('\n', ' ') .. '", "clear_history": true, "provider": "' .. current_provider .. '", "cwd": "' .. clean_cwd .. '"}'
                                vim.fn.system('curl -s -X POST -H "Content-Type: application/json" -d \'' .. clear_body .. '\' ' .. M.config.server_url .. '/chat')
                                
                                -- Reset local stats
                                local old_tokens = session_stats.input_tokens + session_stats.output_tokens
                                reset_stats()
                                update_chat_window_title()
                                
                                append_message('system', '✅ *Context compacted! Reduced from ~' .. format_number(old_tokens) .. ' tokens to summary.*')
                                append_message('system', '📝 *Summary preserved: ' .. summary:sub(1, 200) .. (string.len(summary) > 200 and '...' or '') .. '*')
                            end
                        end
                    end)
                end,
            })
        end,
    },
    
    -- Cost/pricing command
    ['cost'] = {
        description = 'Show model pricing info',
        usage = '/cost',
        handler = function()
            local model_info = get_model_info(current_provider)
            
            local cost_lines = {
                '💰 **Model Pricing Info:**',
                '',
                string.format('**Provider:** %s', current_provider),
            }
            
            if current_provider == 'ollama' then
                table.insert(cost_lines, '')
                table.insert(cost_lines, 'Ollama runs locally - **no API costs!**')
                table.insert(cost_lines, '')
                table.insert(cost_lines, 'You only pay for electricity and hardware.')
            elseif model_info and model_info.cost then
                local model_id = model_mappings[current_provider] or 'unknown'
                table.insert(cost_lines, string.format('**Model:** %s', model_id))
                table.insert(cost_lines, '')
                table.insert(cost_lines, '**Pricing (per 1M tokens):**')
                table.insert(cost_lines, string.format('• Input: $%.2f', model_info.cost.input or 0))
                table.insert(cost_lines, string.format('• Output: $%.2f', model_info.cost.output or 0))
                if model_info.cost.cache_read then
                    table.insert(cost_lines, string.format('• Cache read: $%.2f', model_info.cost.cache_read))
                end
                if model_info.cost.cache_write then
                    table.insert(cost_lines, string.format('• Cache write: $%.2f', model_info.cost.cache_write))
                end
                table.insert(cost_lines, '')
                table.insert(cost_lines, '*Data from [models.dev](https://models.dev)*')
            else
                table.insert(cost_lines, '')
                table.insert(cost_lines, 'Pricing info not available for this model.')
            end
            
            append_message('system', table.concat(cost_lines, '\n'))
        end,
    },
    
    -- Provider shortcuts
    ['ollama'] = {
        description = 'Switch to Ollama',
        usage = '/ollama',
        handler = function()
            switch_provider('ollama')
        end,
    },
    ['claude'] = {
        description = 'Switch to Claude',
        usage = '/claude',
        handler = function()
            switch_provider('claude')
        end,
    },
    ['openai'] = {
        description = 'Switch to OpenAI',
        usage = '/openai',
        handler = function()
            switch_provider('openai')
        end,
    },
    ['gpt'] = { alias = 'openai' },
    
    -- Thinking/verbose mode
    ['thinking'] = {
        description = 'Toggle verbose mode (show tool calls)',
        usage = '/thinking',
        handler = function()
            thinking_mode = not thinking_mode
            local status = thinking_mode and '**ON** 🔍' or '**OFF**'
            append_message('system', '🧠 Thinking mode: ' .. status)
            if thinking_mode then
                append_message('system', '_Tool calls will be shown in detail_')
            end
            scroll_to_bottom()
        end,
    },
    ['verbose'] = { alias = 'thinking' },
    ['debug'] = { alias = 'thinking' },
    
    -- Agent mode commands
    ['plan'] = {
        description = 'Plan mode: read-only, no file modifications',
        usage = '/plan',
        handler = function()
            current_mode = 'plan'
            update_chat_header()
            if M.save_session then M.save_session() end
            append_message('system', '📝 **PLAN mode** - Read-only exploration. Use `/build` to make changes.')
            append_message('system', '_Tools: file_search, grep, read_file_')
            scroll_to_bottom()
        end,
    },
    ['build'] = {
        description = 'Build mode: full access to all tools',
        usage = '/build',
        handler = function()
            current_mode = 'build'
            update_chat_header()
            if M.save_session then M.save_session() end
            append_message('system', '🔨 **BUILD mode** - Full access to modify files.')
            append_message('system', '_Tools: all (read, write, shell)_')
            scroll_to_bottom()
        end,
    },
    ['review'] = {
        description = 'Review mode: approval required before each action',
        usage = '/review',
        handler = function()
            current_mode = 'review'
            update_chat_header()
            if M.save_session then M.save_session() end
            append_message('system', '👁️ **REVIEW mode** - Approval required for each action.')
            append_message('system', '_Tools will show pending actions before execution_')
            scroll_to_bottom()
        end,
    },
    
    -- Exit/Quit commands
    ['exit'] = {
        description = 'Close the chat window',
        usage = '/exit',
        handler = function()
            M.close()
        end,
    },
    ['quit'] = { alias = 'exit' },
    ['q'] = { alias = 'exit' },
    ['close'] = { alias = 'exit' },
    
    -- Layout commands
    ['split'] = {
        description = 'Switch to split layout (docked, resizes other windows)',
        usage = '/split [left|right]',
        handler = function(args)
            M.config.window.style = 'split'
            if args and args:match('left') then
                M.config.window.position = 'left'
            else
                M.config.window.position = 'right'
            end
            -- Reopen to apply
            M.close()
            vim.defer_fn(function() M.open() end, 50)
            append_message('system', '📐 Switched to split layout (docked)')
        end,
    },
    ['sidepane'] = {
        description = 'Switch to sidepane layout (floating overlay)',
        usage = '/sidepane [left|right]',
        handler = function(args)
            M.config.window.style = 'sidepane'
            if args and args:match('left') then
                M.config.window.position = 'left'
            else
                M.config.window.position = 'right'
            end
            -- Reopen to apply
            M.close()
            vim.defer_fn(function() M.open() end, 50)
            append_message('system', '📐 Switched to sidepane layout (floating)')
        end,
    },
    ['popup'] = {
        description = 'Switch to popup layout (centered)',
        usage = '/popup',
        handler = function()
            M.config.window.style = 'popup'
            -- Reopen to apply
            M.close()
            vim.defer_fn(function() M.open() end, 50)
            append_message('system', '📐 Switched to popup layout')
        end,
    },
    ['layout'] = {
        description = 'Cycle through split/sidepane/popup layouts',
        usage = '/layout',
        handler = function()
            -- Cycle: split -> sidepane -> popup -> split
            if M.config.window.style == 'split' then
                M.config.window.style = 'sidepane'
                append_message('system', '📐 Layout: sidepane (floating)')
            elseif M.config.window.style == 'sidepane' then
                M.config.window.style = 'popup'
                append_message('system', '📐 Layout: popup (centered)')
            else
                M.config.window.style = 'split'
                append_message('system', '📐 Layout: split (docked)')
            end
            -- Reopen to apply
            M.close()
            vim.defer_fn(function() M.open() end, 50)
        end,
    },
    
    -- Diff view command
    ['diff'] = {
        description = 'Show side-by-side diff view (like vimdiff)',
        usage = '/diff [filepath]',
        handler = function(args)
            show_diff_view(args)
        end,
    },
    
    -- Toggle auto-diff
    ['autodiff'] = {
        description = 'Toggle inline diff display in chat',
        usage = '/autodiff',
        handler = function()
            M.config.auto_show_diff = not M.config.auto_show_diff
            local status = M.config.auto_show_diff and 'ON' or 'OFF'
            append_message('system', '📊 Inline diff in chat: **' .. status .. '**')
        end,
    },
    
    -- Usage commands
    ['usage'] = {
        description = 'Show usage stats',
        usage = '/usage',
        handler = function()
            require('tark.usage').show_summary()
        end,
    },
    ['usage-open'] = {
        description = 'Open usage dashboard in browser',
        usage = '/usage-open',
        handler = function()
            vim.cmd('TarkUsageOpen')
        end,
    },
}

-- Parse and execute slash commands
local function handle_slash_command(input)
    -- Parse command and args
    local cmd, args = input:match('^/(%S+)%s*(.*)')
    if not cmd then
        return false
    end
    
    cmd = cmd:lower()
    local command = slash_commands[cmd]
    
    if not command then
        append_message('system', '❌ *Unknown command: /' .. cmd .. '. Type `/help` for available commands.*')
        return true
    end
    
    -- Handle aliases
    if command.alias then
        command = slash_commands[command.alias]
    end
    
    if command and command.handler then
        command.handler(args)
    end
    
    return true
end

-- Process input (check for slash commands or send message)
local function process_input(message)
    if not message or message == '' then
        return
    end
    
    -- Check if it's a slash command
    if message:sub(1, 1) == '/' then
        handle_slash_command(message)
    else
        send_message(message)
    end
end

function M.open(initial_message)
    -- Start LSP proxy server if enabled
    if M.config.lsp_proxy then
        local ok, lsp_server = pcall(require, 'tark.lsp_server')
        if ok then
            lsp_proxy_port = lsp_server.start()
            if lsp_proxy_port then
                vim.notify('tark: LSP proxy started on port ' .. lsp_proxy_port, vim.log.levels.DEBUG)
            end
        end
    end

    local buf = get_chat_buffer()

    -- Initialize header and add prompt line
    update_chat_header()
    add_prompt_line()

    -- Calculate window dimensions
    local editor_width = vim.o.columns
    local editor_height = vim.o.lines
    local width, height, col, row

    -- SPLIT MODE: Create proper docked split that resizes other content
    if M.config.window.style == 'split' then
        -- Create vertical split on right or left
        if M.config.window.position == 'left' then
            vim.cmd('topleft vsplit')
        else
            vim.cmd('botright vsplit')
        end
        
        -- Now we're in the new split window
        chat_win = vim.api.nvim_get_current_win()
        vim.api.nvim_win_set_buf(chat_win, buf)
        
        -- Set the width
        local split_width = M.config.window.split_width or 80
        vim.api.nvim_win_set_width(chat_win, split_width)
        
        -- Configure window options
        vim.api.nvim_win_set_option(chat_win, 'number', false)
        vim.api.nvim_win_set_option(chat_win, 'relativenumber', false)
        vim.api.nvim_win_set_option(chat_win, 'signcolumn', 'no')
        vim.api.nvim_win_set_option(chat_win, 'wrap', true)
        vim.api.nvim_win_set_option(chat_win, 'linebreak', true)
        vim.api.nvim_win_set_option(chat_win, 'winfixwidth', true)
        
        -- Set statusline for split mode
        local mode_icons = { plan = '◇', build = '◆', review = '◈' }
        local mode_icon = mode_icons[current_mode] or '◆'
        local mode_label = current_mode:sub(1,1):upper() .. current_mode:sub(2)
        local model_name = (current_model or model_mappings[current_provider] or current_provider):match('[^/]+$') or current_provider
        
        local provider_display_sl = current_provider_id or current_provider
        for _, p in ipairs(providers_info) do
            if p.id == current_provider_id then
                provider_display_sl = p.name:match('^(%w+)') or p.id
                break
            end
        end
        
        vim.api.nvim_win_set_option(chat_win, 'statusline',
            string.format('%%#TarkMode%s# %s %s %%#Comment# %s | %s %%#FloatBorder# [0%%%%] 0/128K %%#String# $0.0000 %%#Normal#',
                mode_label, mode_icon, mode_label, model_name,
                provider_display_sl:sub(1,1):upper() .. provider_display_sl:sub(2)))
    
    -- FLOATING MODES (sidepane/popup)
    else
        -- Calculate dimensions based on style
        if M.config.window.style == 'sidepane' then
            -- SIDEPANE MODE: Full height, intelligent width on right
            local sidepane_width = M.config.window.sidepane_width
            if sidepane_width <= 1 then
                -- Percentage of editor width
                width = math.floor(editor_width * sidepane_width)
            else
                -- Fixed width
                width = math.floor(sidepane_width)
            end
            -- Apply min/max constraints
            width = math.max(M.config.window.min_width or 50, width)
            width = math.min(M.config.window.max_width or 100, width)
            
            -- Full height (minus statusline, cmdline, tabline)
            height = editor_height - 4
            
            -- Position on right edge
            if M.config.window.position == 'left' then
                col = 0
            else
                col = editor_width - width - 2
            end
            row = 0
        else
            -- POPUP MODE: Centered floating window (60% of screen)
            width = math.floor(editor_width * 0.6)
            height = math.floor(editor_height * 0.6)
            col = math.floor((editor_width - width) / 2)
            row = math.floor((editor_height - height) / 2)
        end

        -- Build title (model | context | cost | mode)
        local model_name_t = (current_model or model_mappings[current_provider] or current_provider):match('[^/]+$') or current_provider
        local mode_icons = { plan = '◇', build = '◆', review = '◈' }
        local mode_icon = mode_icons[current_mode] or '◆'
        local mode_label = current_mode:sub(1,1):upper() .. current_mode:sub(2)
        local input_mode_hl = get_mode_highlight(current_mode, true)
        
        -- Layout: [Mode Icon Mode] Model ... [0%] 0/128K ... [$0.0000]
        local mode_part_t = string.format(' %s %s ', mode_icon, mode_label)
        local model_part_t = string.format(' %s ', model_name_t)
        local context_part_t = string.format('[0%%] 0/%s', format_number(128000))
        local cost_part_t = '$0.0000 '
        
        -- Get provider display name
        local provider_display_t = current_provider_id or current_provider
        for _, p in ipairs(providers_info) do
            if p.id == current_provider_id then
                provider_display_t = p.name:match('^(%w+)') or p.id
                break
            end
        end
        local provider_label_t = provider_display_t:sub(1,1):upper() .. provider_display_t:sub(2)
        
        -- INTELLIGENT PADDING for title
        local left_width_t = display_width(mode_part_t) + display_width(model_part_t)
        local center_width_t = display_width(context_part_t)
        local right_width_t = display_width(cost_part_t)
        local usable_width_t = width - 2  -- Account for border chars
        
        -- Where should center START to be truly centered?
        local center_start_t = math.floor((usable_width_t - center_width_t) / 2)
        -- Where should right START to be truly right-aligned?
        local right_start_t = usable_width_t - right_width_t
        
        -- Calculate padding needed
        local left_pad_t = center_start_t - left_width_t
        local right_pad_t = right_start_t - (center_start_t + center_width_t)
        if left_pad_t < 1 then left_pad_t = 1 end
        if right_pad_t < 1 then right_pad_t = 1 end
        
        local title_config = {
            { mode_part_t, input_mode_hl },
            { model_part_t, 'FloatTitle' },
            { string.rep(' ', left_pad_t), 'FloatBorder' },
            { context_part_t, 'Comment' },
            { string.rep(' ', right_pad_t), 'FloatBorder' },
            { cost_part_t, 'String' },
        }
        
        chat_win = vim.api.nvim_open_win(buf, true, {
            relative = 'editor',
            width = width,
            height = height,
            col = col,
            row = row,
            style = 'minimal',
            border = M.config.window.border,
            title = title_config,
            title_pos = 'left',
            footer = {
                { ' ↵ send ', 'Comment' },
                { ' tab ', 'Comment' },
                { 'mode', 'FloatBorder' },
                { '  /', 'Comment' },
                { 'commands ', 'FloatBorder' },
                { ' q ', 'Comment' },
                { 'close', 'FloatBorder' },
            },
            footer_pos = 'left',
        })
        
        -- Configure window options
        vim.api.nvim_win_set_option(chat_win, 'number', false)
        vim.api.nvim_win_set_option(chat_win, 'relativenumber', false)
        vim.api.nvim_win_set_option(chat_win, 'signcolumn', 'no')
        vim.api.nvim_win_set_option(chat_win, 'wrap', true)
        vim.api.nvim_win_set_option(chat_win, 'linebreak', true)
    end

    -- Helper function to process input from prompt line
    local function do_send()
        vim.schedule(function()
            local message = get_prompt_text()
            if message and message ~= '' then
                clear_prompt_line()
                process_input(message)
            end
        end)
    end

    -- Set up keybindings on the chat buffer
    -- Move cursor to prompt line for typing
    local function go_to_prompt()
        local last_line = vim.api.nvim_buf_line_count(buf)
        vim.api.nvim_win_set_cursor(chat_win, {last_line, 2})  -- Position after '> '
    end

    -- Enter in Normal mode - send message
    vim.keymap.set('n', '<CR>', function()
        -- If on prompt line, send
        local cursor = vim.api.nvim_win_get_cursor(chat_win)
        local last_line = vim.api.nvim_buf_line_count(buf)
        if cursor[1] == last_line then
            do_send()
        else
            -- Otherwise go to prompt
            go_to_prompt()
            vim.cmd('startinsert!')
        end
    end, { buffer = buf, silent = true, nowait = true })

    -- Enter in Insert mode - send
    vim.keymap.set('i', '<CR>', function()
        if vim.fn.pumvisible() == 1 then
            -- Accept the selected completion
            return vim.api.nvim_replace_termcodes('<C-y>', true, false, true)
        else
            vim.cmd('stopinsert')
            do_send()
            return ''
        end
    end, { buffer = buf, expr = true, silent = true, nowait = true })

    -- Ctrl+Enter as alternative
    vim.keymap.set('i', '<C-CR>', function()
        vim.cmd('stopinsert')
        do_send()
    end, { buffer = buf, silent = true, nowait = true })

    -- i in normal mode - go to prompt and insert
    vim.keymap.set('n', 'i', function()
        go_to_prompt()
        vim.cmd('startinsert!')
    end, { buffer = buf, silent = true, nowait = true })
    
    -- a in normal mode - go to end of prompt and insert
    vim.keymap.set('n', 'a', function()
        go_to_prompt()
        vim.cmd('startinsert!')
        vim.api.nvim_feedkeys(vim.api.nvim_replace_termcodes('<End>', true, false, true), 'n', false)
    end, { buffer = buf, silent = true, nowait = true })

    -- Set custom omnifunc for slash commands (triggered with C-x C-o)
    vim.api.nvim_buf_set_option(buf, 'omnifunc', 'v:lua.tark_slash_complete')
    -- Set custom completefunc for @ file references (triggered with C-x C-u)
    vim.api.nvim_buf_set_option(buf, 'completefunc', 'v:lua.tark_file_complete')

    -- '/' triggers slash command completion
    vim.keymap.set('i', '/', function()
        local line = vim.api.nvim_get_current_line()
        local col = vim.fn.col('.') - 1
        -- Insert the slash first
        vim.api.nvim_put({ '/' }, 'c', false, true)
        -- Check if we're on the prompt line and near the start
        local cursor = vim.api.nvim_win_get_cursor(chat_win)
        local last_line = vim.api.nvim_buf_line_count(buf)
        if cursor[1] == last_line and (col <= 2 or line:match('^> *$')) then
            vim.schedule(function()
                vim.fn.feedkeys(vim.api.nvim_replace_termcodes('<C-x><C-o>', true, false, true), 'n')
            end)
        end
    end, { buffer = buf, silent = true, nowait = true })

    -- '@' triggers file reference completion
    vim.keymap.set('i', '@', function()
        -- Insert the @ first
        vim.api.nvim_put({ '@' }, 'c', false, true)
        -- Trigger file completion
        vim.schedule(function()
            vim.fn.feedkeys(vim.api.nvim_replace_termcodes('<C-x><C-u>', true, false, true), 'n')
        end)
    end, { buffer = buf, silent = true, nowait = true })

    -- Tab to accept completion or toggle mode
    vim.keymap.set('i', '<Tab>', function()
        if vim.fn.pumvisible() == 1 then
            return vim.api.nvim_replace_termcodes('<C-n>', true, false, true)
        else
            return vim.api.nvim_replace_termcodes('<Tab>', true, false, true)
        end
    end, { buffer = buf, expr = true, silent = true })

    -- Shift-Tab to cycle backwards
    vim.keymap.set('i', '<S-Tab>', function()
        if vim.fn.pumvisible() == 1 then
            return vim.api.nvim_replace_termcodes('<C-p>', true, false, true)
        else
            return vim.api.nvim_replace_termcodes('<S-Tab>', true, false, true)
        end
    end, { buffer = buf, expr = true, silent = true })

    -- q to close (only in normal mode)
    vim.keymap.set('n', 'q', function()
        M.close()
    end, { buffer = buf, silent = true, nowait = true })

    -- Tab in normal mode to toggle between plan and build modes
    vim.keymap.set('n', '<Tab>', function()
        if current_mode == 'plan' then
            current_mode = 'build'
            append_message('system', '🔨 **BUILD mode** - Full access to modify files.')
        else
            current_mode = 'plan'
            append_message('system', '📝 **PLAN mode** - Read-only exploration.')
        end
        update_chat_header()
        if M.save_session then M.save_session() end
        scroll_to_bottom()
    end, { buffer = buf, silent = true, desc = 'Toggle plan/build mode' })

    -- Handle initial message if provided
    if initial_message and initial_message ~= '' then
        process_input(initial_message)
    end

    -- Go to prompt and enter insert mode
    go_to_prompt()
    vim.cmd('startinsert!')
end

-- Close the chat window
function M.close()
    if chat_win and vim.api.nvim_win_is_valid(chat_win) then
        vim.api.nvim_win_close(chat_win, true)
        chat_win = nil
    end
    
    -- Stop LSP proxy server
    if lsp_proxy_port then
        local ok, lsp_server = pcall(require, 'tark.lsp_server')
        if ok then
            lsp_server.stop()
        end
        lsp_proxy_port = nil
    end
end

-- Check if chat is open
function M.is_open()
    return (chat_win and vim.api.nvim_win_is_valid(chat_win))
end

-- Toggle chat window
function M.toggle()
    if M.is_open() then
        M.close()
    else
        M.open()
    end
end

-- Get current provider
function M.get_provider()
    return current_provider
end

-- Set provider programmatically
function M.set_provider(provider)
    current_provider = provider
end

-- Setup function
function M.setup(opts)
    M.config = vim.tbl_deep_extend('force', M.config, opts or {})
    
    -- Setup mode-specific highlight groups (distinctive colors)
    -- Plan mode: Blue/Cyan - analytical, read-only
    vim.api.nvim_set_hl(0, 'TarkModePlan', { fg = '#61afef', bold = true })
    vim.api.nvim_set_hl(0, 'TarkModePlanBg', { fg = '#282c34', bg = '#61afef', bold = true })
    -- Build mode: Green - constructive, active  
    vim.api.nvim_set_hl(0, 'TarkModeBuild', { fg = '#98c379', bold = true })
    vim.api.nvim_set_hl(0, 'TarkModeBuildBg', { fg = '#282c34', bg = '#98c379', bold = true })
    -- Review mode: Yellow/Orange - caution, approval required
    vim.api.nvim_set_hl(0, 'TarkModeReview', { fg = '#e5c07b', bold = true })
    vim.api.nvim_set_hl(0, 'TarkModeReviewBg', { fg = '#282c34', bg = '#e5c07b', bold = true })
    -- Accent colors for user messages per mode
    vim.api.nvim_set_hl(0, 'TarkUserAccentPlan', { fg = '#61afef' })
    vim.api.nvim_set_hl(0, 'TarkUserAccentBuild', { fg = '#98c379' })
    vim.api.nvim_set_hl(0, 'TarkUserAccentReview', { fg = '#e5c07b' })
    
    -- Pre-fetch models database from models.dev
    fetch_models_db()
    
    -- Restore session from server (provider, model, mode)
    vim.fn.jobstart({
        'curl', '-s', M.config.server_url .. '/session',
    }, {
        stdout_buffered = true,
        on_stdout = function(_, data)
            if data and data[1] then
                local ok, resp = pcall(vim.fn.json_decode, table.concat(data, ''))
                if ok and resp then
                    if resp.provider then
                        current_provider = resp.provider
                    end
                    if resp.model then
                        current_model = resp.model
                    end
                    if resp.mode then
                        current_mode = resp.mode
                    end
                end
            end
        end,
    })
end

-- Save session to server (called when model/provider changes)
local function save_session()
    local body = vim.fn.json_encode({
        provider = current_provider,
        model = current_model,
        mode = current_mode,
    })
    
    vim.fn.jobstart({
        'curl', '-s', '-X', 'POST',
        '-H', 'Content-Type: application/json',
        '-d', body,
        M.config.server_url .. '/session/save',
    }, {
        stdout_buffered = true,
        on_stdout = function(_, data)
            -- Silent save - no notification needed
        end,
    })
end

-- Export save function
M.save_session = save_session

-- Test helpers (always export, but document as test-only)
-- These functions should only be used in automated tests
M._test_get_current_provider = function() return current_provider end
M._test_get_current_provider_id = function() return current_provider_id end
M._test_get_current_model = function() return current_model end
M._test_set_provider_state = function(provider, provider_id, model)
    current_provider = provider
    current_provider_id = provider_id
    current_model = model
end

return M
