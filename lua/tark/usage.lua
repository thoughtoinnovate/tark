-- lua/tark/usage.lua
local M = {}

local config = require('tark').config

-- Get server URL
local function get_url(path)
    return string.format('http://%s:%d%s', 
        config.server.host, 
        config.server.port, 
        path)
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
        return string.format('%.1fM', n / 1000000)
    elseif n >= 1000 then
        return string.format('%.1fK', n / 1000)
    else
        return tostring(n)
    end
end

-- Show usage summary in floating window
function M.show_summary()
    local curl = require('plenary.curl')
    
    curl.get(get_url('/api/usage/summary'), {
        callback = function(response)
            vim.schedule(function()
                if response.status ~= 200 then
                    vim.notify('Failed to fetch usage data', vim.log.levels.ERROR)
                    return
                end
                
                local ok, data = pcall(vim.fn.json_decode, response.body)
                if not ok then
                    vim.notify('Failed to parse usage data', vim.log.levels.ERROR)
                    return
                end
                
                -- Fetch model data
                curl.get(get_url('/api/usage/models'), {
                    callback = function(models_response)
                        vim.schedule(function()
                            local models = {}
                            if models_response.status == 200 then
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
                                    data.session_count,
                                    data.db_size_human),
                                '├─────────────────────────────────────────┤',
                                '│ BY MODEL                                │',
                            }
                            
                            for _, model in ipairs(models or {}) do
                                local model_str = string.format('%s/%s', model.provider, model.model)
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
    vim.ui.select({'Yes', 'No'}, {
        prompt = string.format('Delete logs older than %d days?', days),
    }, function(choice)
        if choice ~= 'Yes' then return end
        
        local curl = require('plenary.curl')
        curl.post(get_url('/api/usage/cleanup'), {
            body = vim.fn.json_encode({ older_than_days = days }),
            headers = { ['Content-Type'] = 'application/json' },
            callback = function(response)
                vim.schedule(function()
                    if response.status == 200 then
                        local ok, data = pcall(vim.fn.json_decode, response.body)
                        if ok then
                            vim.notify(string.format(
                                'Cleaned up %d logs, freed %s',
                                data.deleted_logs,
                                data.freed_human
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

