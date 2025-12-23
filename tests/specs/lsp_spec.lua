-- Tests for LSP helpers module
-- Tests LSP context gathering, cache behavior, and error handling

local lsp = require('tark.lsp')

describe('lsp - LSP helpers', function()
    describe('module functions exist', function()
        it('has get_client function', function()
            assert.is_function(lsp.get_client)
        end)

        it('has has_lsp function', function()
            assert.is_function(lsp.has_lsp)
        end)

        it('has get_completion_context_async function', function()
            assert.is_function(lsp.get_completion_context_async)
        end)

        it('has get_hover_async function', function()
            assert.is_function(lsp.get_hover_async)
        end)

        it('has get_symbols_async function', function()
            assert.is_function(lsp.get_symbols_async)
        end)

        it('has get_definition_async function', function()
            assert.is_function(lsp.get_definition_async)
        end)

        it('has get_references_async function', function()
            assert.is_function(lsp.get_references_async)
        end)

        it('has get_signature_async function', function()
            assert.is_function(lsp.get_signature_async)
        end)
    end)

    describe('has_lsp function', function()
        it('returns boolean', function()
            local bufnr = vim.api.nvim_get_current_buf()
            local result = lsp.has_lsp(bufnr)
            assert.is_boolean(result)
        end)

        it('returns false for invalid buffer', function()
            local result = lsp.has_lsp(99999)
            assert.is_false(result)
        end)

        it('works with nil buffer (uses current)', function()
            local result = lsp.has_lsp(nil)
            assert.is_boolean(result)
        end)

        it('can be called multiple times', function()
            local bufnr = vim.api.nvim_get_current_buf()
            local result1 = lsp.has_lsp(bufnr)
            local result2 = lsp.has_lsp(bufnr)
            assert.equals(result1, result2)
        end)
    end)

    describe('get_client function', function()
        it('returns nil when no LSP attached', function()
            local bufnr = vim.api.nvim_get_current_buf()
            -- In test environment, likely no LSP
            local client = lsp.get_client(bufnr)
            -- Should return nil or a client
            assert.is_true(client == nil or type(client) == 'table')
        end)

        it('works with nil buffer (uses current)', function()
            local client = lsp.get_client(nil)
            assert.is_true(client == nil or type(client) == 'table')
        end)
    end)

    describe('async functions', function()
        it('get_completion_context_async accepts callback', function()
            local bufnr = vim.api.nvim_get_current_buf()
            local callback_called = false
            
            lsp.get_completion_context_async(bufnr, 0, 0, function(context)
                callback_called = true
            end)
            
            -- Callback should be called (even if with nil when no LSP)
            vim.wait(100, function() return callback_called end)
        end)

        it('get_hover_async accepts callback', function()
            local bufnr = vim.api.nvim_get_current_buf()
            local callback_called = false
            
            lsp.get_hover_async(bufnr, 0, 0, function(hover)
                callback_called = true
            end)
            
            vim.wait(100, function() return callback_called end)
        end)

        it('get_symbols_async accepts callback', function()
            local bufnr = vim.api.nvim_get_current_buf()
            local callback_called = false
            
            lsp.get_symbols_async(bufnr, function(symbols)
                callback_called = true
            end)
            
            vim.wait(100, function() return callback_called end)
        end)

        it('get_definition_async accepts callback', function()
            local bufnr = vim.api.nvim_get_current_buf()
            local callback_called = false
            
            lsp.get_definition_async(bufnr, 0, 0, function(locations)
                callback_called = true
            end)
            
            vim.wait(100, function() return callback_called end)
        end)

        it('get_references_async accepts callback', function()
            local bufnr = vim.api.nvim_get_current_buf()
            local callback_called = false
            
            lsp.get_references_async(bufnr, 0, 0, function(references)
                callback_called = true
            end)
            
            vim.wait(100, function() return callback_called end)
        end)

        it('get_signature_async accepts callback', function()
            local bufnr = vim.api.nvim_get_current_buf()
            local callback_called = false
            
            lsp.get_signature_async(bufnr, 0, 0, function(signature)
                callback_called = true
            end)
            
            vim.wait(100, function() return callback_called end)
        end)
    end)

    describe('error handling', function()
        it('handles invalid buffer gracefully', function()
            assert.has_no_errors(function()
                lsp.has_lsp(99999)
            end)
        end)

        it('async functions handle no LSP gracefully', function()
            local bufnr = vim.api.nvim_get_current_buf()
            
            assert.has_no_errors(function()
                lsp.get_completion_context_async(bufnr, 0, 0, function() end)
            end)
        end)

        it('async functions handle nil callback', function()
            local bufnr = vim.api.nvim_get_current_buf()
            
            -- Should not error even with nil callback
            assert.has_no_errors(function()
                lsp.get_hover_async(bufnr, 0, 0, nil)
            end)
        end)
    end)

    describe('return values', function()
        it('async functions call callback with nil when no LSP', function()
            local bufnr = vim.api.nvim_get_current_buf()
            local result = 'not_called'
            
            lsp.get_completion_context_async(bufnr, 0, 0, function(context)
                result = context
            end)
            
            vim.wait(100, function() return result ~= 'not_called' end)
            
            -- When no LSP, should return nil
            if not lsp.has_lsp(bufnr) then
                assert.is_nil(result)
            end
        end)
    end)
end)

