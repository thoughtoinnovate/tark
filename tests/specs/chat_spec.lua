-- Tests for chat agent mode
-- Tests window management, token display in title, and session stats

local chat = require('tark.chat')

describe('chat - agent mode', function()
    after_each(function()
        -- Clean up: close chat if open
        if chat.is_open() then
            chat.close()
        end
    end)

    describe('window management', function()
        it('opens chat window', function()
            chat.open()
            assert.is_true(chat.is_open())
        end)

        it('closes chat window', function()
            chat.open()
            assert.is_true(chat.is_open())
            chat.close()
            assert.is_false(chat.is_open())
        end)

        it('toggles chat window', function()
            -- Ensure closed first
            if chat.is_open() then
                chat.close()
            end
            
            -- Toggle open
            chat.toggle()
            assert.is_true(chat.is_open())
            
            -- Toggle close
            chat.toggle()
            assert.is_false(chat.is_open())
        end)

        it('is_open returns false when closed', function()
            if chat.is_open() then
                chat.close()
            end
            assert.is_false(chat.is_open())
        end)

        it('can open with initial message', function()
            chat.open('test message')
            assert.is_true(chat.is_open())
        end)
    end)

    describe('module functions exist', function()
        it('has open function', function()
            assert.is_function(chat.open)
        end)

        it('has close function', function()
            assert.is_function(chat.close)
        end)

        it('has toggle function', function()
            assert.is_function(chat.toggle)
        end)

        it('has is_open function', function()
            assert.is_function(chat.is_open)
        end)

        it('has setup function', function()
            assert.is_function(chat.setup)
        end)

        it('has show_diff export', function()
            assert.is_function(chat.show_diff)
        end)
    end)

    describe('config', function()
        it('has default config', function()
            assert.is_table(chat.config)
        end)

        it('has server_url config (nil by default, resolved dynamically)', function()
            -- server_url is nil by default - resolved dynamically via port file
            -- This is to support auto port selection
            assert.is_nil(chat.config.server_url)
        end)

        it('has window config', function()
            assert.is_table(chat.config.window)
            assert.is_string(chat.config.window.style)
            assert.is_string(chat.config.window.border)
        end)

        it('has auto_show_diff config', function()
            assert.is_not_nil(chat.config.auto_show_diff)
        end)

        it('has lsp_proxy config', function()
            assert.is_not_nil(chat.config.lsp_proxy)
        end)
    end)

    describe('window state', function()
        it('is_open returns boolean', function()
            local result = chat.is_open()
            assert.is_boolean(result)
        end)

        it('can check state multiple times', function()
            local state1 = chat.is_open()
            local state2 = chat.is_open()
            assert.equals(state1, state2)
        end)
    end)

    describe('error handling', function()
        it('close does not error when already closed', function()
            if chat.is_open() then
                chat.close()
            end
            -- Should not error
            assert.has_no_errors(function()
                chat.close()
            end)
        end)

        it('toggle works from any state', function()
            assert.has_no_errors(function()
                chat.toggle()
                chat.toggle()
            end)
        end)
    end)

    describe('provider state tracking', function()
        it('has test helper functions', function()
            assert.is_function(chat._test_get_current_provider)
            assert.is_function(chat._test_get_current_provider_id)
            assert.is_function(chat._test_get_current_model)
            assert.is_function(chat._test_set_provider_state)
        end)

        it('tracks backend provider separately from display provider', function()
            -- Simulate selecting Gemini (uses OpenAI-compatible API)
            chat._test_set_provider_state('openai', 'google', 'google/gemini-1.5-flash')
            
            -- Backend provider should be 'openai' (for API routing)
            assert.equals('openai', chat._test_get_current_provider())
            
            -- Display provider should be 'google' (for UI)
            assert.equals('google', chat._test_get_current_provider_id())
            
            -- Model should be full ID
            assert.equals('google/gemini-1.5-flash', chat._test_get_current_model())
        end)

        it('tracks OpenAI provider correctly', function()
            chat._test_set_provider_state('openai', 'openai', 'openai/gpt-4o')
            
            assert.equals('openai', chat._test_get_current_provider())
            assert.equals('openai', chat._test_get_current_provider_id())
            assert.equals('openai/gpt-4o', chat._test_get_current_model())
        end)

        it('tracks Claude provider correctly', function()
            chat._test_set_provider_state('claude', 'anthropic', 'anthropic/claude-sonnet-4')
            
            assert.equals('claude', chat._test_get_current_provider())
            assert.equals('anthropic', chat._test_get_current_provider_id())
            assert.equals('anthropic/claude-sonnet-4', chat._test_get_current_model())
        end)

        it('tracks Ollama provider correctly', function()
            chat._test_set_provider_state('ollama', 'ollama', 'ollama/codellama')
            
            assert.equals('ollama', chat._test_get_current_provider())
            assert.equals('ollama', chat._test_get_current_provider_id())
            assert.equals('ollama/codellama', chat._test_get_current_model())
        end)

        it('provider_id defaults to provider when not set', function()
            chat._test_set_provider_state('openai', nil, 'openai/gpt-4o')
            
            assert.equals('openai', chat._test_get_current_provider())
            assert.is_nil(chat._test_get_current_provider_id())
        end)
    end)

    describe('task queue functionality', function()
        before_each(function()
            -- Ensure chat is open for task queue tests
            if not chat.is_open() then
                chat.open()
            end
        end)

        it('has task queue test helpers', function()
            assert.is_function(chat._test_get_task_queue)
            assert.is_function(chat._test_add_to_queue)
            assert.is_function(chat._test_clear_queue)
            assert.is_function(chat._test_get_agent_running)
        end)

        it('starts with empty task queue', function()
            chat._test_clear_queue()
            local queue = chat._test_get_task_queue()
            assert.is_table(queue)
            assert.equals(0, #queue)
        end)

        it('adds tasks to queue', function()
            chat._test_clear_queue()
            local task_id = chat._test_add_to_queue('test task 1')
            
            local queue = chat._test_get_task_queue()
            assert.equals(1, #queue)
            assert.equals('test task 1', queue[1].prompt)
            assert.equals('queued', queue[1].status)
            assert.equals(task_id, queue[1].id)
        end)

        it('adds multiple tasks to queue', function()
            chat._test_clear_queue()
            chat._test_add_to_queue('task 1')
            chat._test_add_to_queue('task 2')
            chat._test_add_to_queue('task 3')
            
            local queue = chat._test_get_task_queue()
            assert.equals(3, #queue)
            assert.equals('task 1', queue[1].prompt)
            assert.equals('task 2', queue[2].prompt)
            assert.equals('task 3', queue[3].prompt)
        end)

        it('task has required fields', function()
            chat._test_clear_queue()
            chat._test_add_to_queue('test task')
            
            local queue = chat._test_get_task_queue()
            local task = queue[1]
            
            assert.is_number(task.id)
            assert.is_string(task.prompt)
            assert.is_string(task.status)
            assert.is_number(task.timestamp)
        end)

        it('removes tasks from queue', function()
            chat._test_clear_queue()
            chat._test_add_to_queue('task 1')
            chat._test_add_to_queue('task 2')
            chat._test_add_to_queue('task 3')
            
            -- Remove middle task
            chat._test_remove_from_queue(2)
            
            local queue = chat._test_get_task_queue()
            assert.equals(2, #queue)
            assert.equals('task 1', queue[1].prompt)
            assert.equals('task 3', queue[2].prompt)
        end)

        it('clears all tasks', function()
            chat._test_clear_queue()
            chat._test_add_to_queue('task 1')
            chat._test_add_to_queue('task 2')
            
            chat._test_clear_queue()
            
            local queue = chat._test_get_task_queue()
            assert.equals(0, #queue)
        end)

        it('tracks agent running state', function()
            local is_running = chat._test_get_agent_running()
            assert.is_boolean(is_running)
        end)
    end)

    describe('mode switching prevention', function()
        before_each(function()
            if not chat.is_open() then
                chat.open()
            end
        end)

        it('has mode test helpers', function()
            assert.is_function(chat._test_get_current_mode)
            assert.is_function(chat._test_set_mode)
            assert.is_function(chat._test_can_switch_mode)
        end)

        it('has valid default mode', function()
            local mode = chat._test_get_current_mode()
            assert.is_string(mode)
            assert.is_true(
                mode == 'plan' or mode == 'build' or mode == 'review',
                'Mode should be plan, build, or review'
            )
        end)

        it('allows mode switching when agent not running', function()
            -- Ensure agent is not running and no queue processing
            chat._test_set_agent_running(false)
            
            local can_switch = chat._test_can_switch_mode()
            assert.is_true(can_switch, 'Should allow mode switch when agent not running')
        end)

        it('blocks mode switching when agent is running', function()
            -- Simulate agent running state
            chat._test_set_agent_running(true)
            
            local can_switch = chat._test_can_switch_mode()
            assert.is_false(can_switch, 'Should block mode switch when agent is running')
            
            -- Clean up
            chat._test_set_agent_running(false)
        end)

        it('switches between modes when allowed', function()
            -- Ensure agent not running
            chat._test_set_agent_running(false)
            
            -- Switch to plan mode
            chat._test_set_mode('plan')
            assert.equals('plan', chat._test_get_current_mode())
            
            -- Switch to build mode
            chat._test_set_mode('build')
            assert.equals('build', chat._test_get_current_mode())
            
            -- Switch to review mode
            chat._test_set_mode('review')
            assert.equals('review', chat._test_get_current_mode())
        end)

        it('only accepts valid modes', function()
            local valid_modes = {'plan', 'build', 'review'}
            
            for _, mode in ipairs(valid_modes) do
                assert.has_no_errors(function()
                    chat._test_set_mode(mode)
                end)
                assert.equals(mode, chat._test_get_current_mode())
            end
        end)
    end)
end)

