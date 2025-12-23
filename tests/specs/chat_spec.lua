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

        it('has server_url config', function()
            assert.is_string(chat.config.server_url)
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
end)

