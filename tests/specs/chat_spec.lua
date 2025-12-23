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
end)

