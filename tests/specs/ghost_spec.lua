-- Tests for ghost text completion mode
-- Tests stats tracking, token display, and cost calculation

local ghost = require('tark.ghost')

describe('ghost - completion mode', function()
    before_each(function()
        -- Reset stats before each test
        ghost.reset_stats()
    end)

    describe('stats tracking', function()
        it('initializes with zero values', function()
            local stats = ghost.get_stats()
            assert.equals(0, stats.requests)
            assert.equals(0, stats.accepted)
            assert.equals(0, stats.dismissed)
            assert.equals(0, stats.input_tokens)
            assert.equals(0, stats.output_tokens)
            assert.equals(0, stats.total_cost)
            assert.equals(0, stats.chars_generated)
            assert.equals(0, stats.chars_accepted)
        end)

        it('resets all stats to zero', function()
            -- Manually set some stats (simulating usage)
            local stats = ghost.get_stats()
            -- Since we can't directly set stats, we verify reset works
            ghost.reset_stats()
            stats = ghost.get_stats()
            assert.equals(0, stats.requests)
            assert.equals(0, stats.accepted)
        end)

        it('returns stats as a table', function()
            local stats = ghost.get_stats()
            assert.is_table(stats)
            assert.is_number(stats.requests)
            assert.is_number(stats.input_tokens)
            assert.is_number(stats.output_tokens)
            assert.is_number(stats.total_cost)
        end)
    end)

    describe('token display', function()
        it('returns empty string when no completions', function()
            ghost.reset_stats()
            local statusline = ghost.statusline()
            assert.equals('', statusline)
        end)

        it('formats stats lines for display', function()
            ghost.reset_stats()
            local lines = ghost.stats_lines()
            assert.is_table(lines)
            assert.is_true(#lines > 0)
            -- Should contain header
            assert.is_true(lines[1]:find('tark Completion Stats') ~= nil)
        end)

        it('shows no completions message when empty', function()
            ghost.reset_stats()
            local lines = ghost.stats_lines()
            local has_no_completions = false
            for _, line in ipairs(lines) do
                if line:find('No completions') then
                    has_no_completions = true
                    break
                end
            end
            assert.is_true(has_no_completions)
        end)
    end)

    describe('stats_lines formatting', function()
        it('returns array of strings', function()
            local lines = ghost.stats_lines()
            assert.is_table(lines)
            for _, line in ipairs(lines) do
                assert.is_string(line)
            end
        end)

        it('includes header line', function()
            local lines = ghost.stats_lines()
            assert.is_true(lines[1]:find('===') ~= nil)
        end)
    end)

    describe('module functions exist', function()
        it('has get_stats function', function()
            assert.is_function(ghost.get_stats)
        end)

        it('has reset_stats function', function()
            assert.is_function(ghost.reset_stats)
        end)

        it('has statusline function', function()
            assert.is_function(ghost.statusline)
        end)

        it('has stats_lines function', function()
            assert.is_function(ghost.stats_lines)
        end)

        it('has accept function', function()
            assert.is_function(ghost.accept)
        end)

        it('has dismiss function', function()
            assert.is_function(ghost.dismiss)
        end)

        it('has trigger function', function()
            assert.is_function(ghost.trigger)
        end)

        it('has toggle function', function()
            assert.is_function(ghost.toggle)
        end)

        it('has has_completion function', function()
            assert.is_function(ghost.has_completion)
        end)

        it('has setup function', function()
            assert.is_function(ghost.setup)
        end)
    end)

    describe('config', function()
        it('has default config', function()
            assert.is_table(ghost.config)
            assert.is_string(ghost.config.server_url)
            assert.is_number(ghost.config.debounce_ms)
            assert.is_string(ghost.config.hl_group)
        end)

        it('has lsp_context option', function()
            assert.is_not_nil(ghost.config.lsp_context)
        end)
    end)
end)

