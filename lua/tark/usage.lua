-- lua/tark/usage.lua
local M = {}

-- Get server URL (uses dynamic port discovery from tark module)
local function get_url(path)
    local tark = require('tark')
    return tark.get_server_url() .. path
end

-- Format cost
local function format_cost(cost)
    if not cost or type(cost) ~= 'number' then return '$0.00' end
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
    if not n or type(n) ~= 'number' then return '0' end
    if n >= 1000000 then
        return string.format('%.1fM', n / 1000000)
    elseif n >= 1000 then
        return string.format('%.1fK', n / 1000)
    else
        return tostring(n)
    end
end

-- Show usage summary in floating window
function M.show_summary()
    local ok, curl = pcall(require, 'plenary.curl')
    if not ok then
        vim.notify('plenary.nvim is required for :TarkUsage. Install it or use :TarkUsageOpen instead.', vim.log.levels.ERROR)
        return
    end
    
    -- Check if server is reachable before making async requests
    -- This is the real protection against connection errors since plenary.curl
    -- is async and pcall won't catch errors during HTTP execution
    local server = require('tark.server')
    local healthy = server.health_check()
    if not healthy then
        vim.notify('Tark server is not running. Start it with :TarkServerStart or run the binary manually.', vim.log.levels.ERROR)
        return
    end
    
    local url = get_url('/api/usage/summary')
    
    curl.get(url, {
        callback = function(response)
            vim.schedule(function()
                if not response or response.status ~= 200 then
                    local status = response and response.status or 'no response'
                    vim.notify(string.format('Failed to fetch usage data (status: %s, url: %s)', status, url), vim.log.levels.ERROR)
                    return
                end
                
                local parse_ok, data = pcall(vim.fn.json_decode, response.body)
                if not parse_ok or not data then
                    vim.notify('Failed to parse usage data', vim.log.levels.ERROR)
                    return
                end
                
                -- Fetch model data (nested async call)
                curl.get(get_url('/api/usage/models'), {
                    callback = function(models_response)
                        vim.schedule(function()
                            local models = {}
                            if models_response and models_response.status == 200 then
                                local ok2, models_data = pcall(vim.fn.json_decode, models_response.body)
                                if ok2 then
                                    models = models_data
                                end
                            end
                            
                            local lines = {
                                '╭─────────────────────────────────────────╮',
                                '│         TARK USAGE SUMMARY              │',
                                '├─────────────────────────────────────────┤',
                                string.format('│ Total Cost: %-10s Tokens: %-8s│', 
                                    format_cost(data.total_cost),
                                    format_number(data.total_tokens)),
                                string.format('│ Sessions: %-12s DB: %-10s│',
                                    data.session_count or 0,
                                    data.db_size_human or '0 B'),
                                '├─────────────────────────────────────────┤',
                                '│ BY MODEL                                │',
                            }
                            
                            for _, model in ipairs(models or {}) do
                                local model_str = string.format('%s/%s', model.provider or '?', model.model or '?')
                                if #model_str > 20 then
                                    model_str = string.sub(model_str, 1, 17) .. '...'
                                end
                                table.insert(lines, string.format('│  %-20s %8s│', 
                                    model_str, 
                                    format_cost(model.cost)))
                            end
                            
                            table.insert(lines, '├─────────────────────────────────────────┤')
                            table.insert(lines, '│ :TarkUsageOpen for detailed dashboard   │')
                            table.insert(lines, '╰─────────────────────────────────────────╯')
                            
                            -- Show in floating window
                            local buf = vim.api.nvim_create_buf(false, true)
                            vim.api.nvim_buf_set_lines(buf, 0, -1, false, lines)
                            
                            local width = 45
                            local height = #lines
                            local win = vim.api.nvim_open_win(buf, true, {
                                relative = 'editor',
                                width = width,
                                height = height,
                                col = (vim.o.columns - width) / 2,
                                row = (vim.o.lines - height) / 2,
                                style = 'minimal',
                                border = 'rounded',
                            })
                            
                            -- Close on any key
                            vim.keymap.set('n', 'q', function()
                                vim.api.nvim_win_close(win, true)
                            end, { buffer = buf })
                            vim.keymap.set('n', '<Esc>', function()
                                vim.api.nvim_win_close(win, true)
                            end, { buffer = buf })
                        end)
                    end
                })
            end)
        end
    })
end

-- Cleanup old logs
function M.cleanup(days)
    -- Check if server is reachable before making async requests
    local server = require('tark.server')
    local healthy = server.health_check()
    if not healthy then
        vim.notify('Tark server is not running. Start it with :TarkServerStart', vim.log.levels.ERROR)
        return
    end
    
    vim.ui.select({'Yes', 'No'}, {
        prompt = string.format('Delete logs older than %d days?', days),
    }, function(choice)
        if choice ~= 'Yes' then return end
        
        local ok, curl = pcall(require, 'plenary.curl')
        if not ok then
            vim.notify('plenary.nvim is required', vim.log.levels.ERROR)
            return
        end
        
        curl.post(get_url('/api/usage/cleanup'), {
            body = vim.fn.json_encode({ older_than_days = days }),
            headers = { ['Content-Type'] = 'application/json' },
            callback = function(response)
                vim.schedule(function()
                    if response and response.status == 200 then
                        local parse_ok, data = pcall(vim.fn.json_decode, response.body)
                        if parse_ok and data then
                            vim.notify(string.format(
                                'Cleaned up %d logs, freed %s',
                                data.deleted_logs or 0,
                                data.freed_human or '0 B'
                            ), vim.log.levels.INFO)
                        else
                            vim.notify('Cleanup completed', vim.log.levels.INFO)
                        end
                    else
                        vim.notify('Cleanup failed', vim.log.levels.ERROR)
                    end
                end)
            end
        })
    end)
end

return M