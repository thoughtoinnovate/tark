-- tark (तर्क) Neovim plugin
-- Tark = Logic/Reasoning in Sanskrit
-- Provides AI-powered completions and chat functionality
-- 
-- Works seamlessly with LazyVim and blink.cmp - no manual config needed!

local M = {}

-- Default configuration
M.config = {
    -- LSP settings
    lsp = {
        enabled = true,
        cmd = { 'tark', 'lsp' },
        filetypes = { '*' },
    },
    -- Ghost text (inline completions) settings
    ghost_text = {
        enabled = true,
        server_url = 'http://localhost:8765',
        debounce_ms = 150,
        hl_group = 'Comment',
    },
    -- Chat settings
    chat = {
        enabled = true,
        window = {
            width = 80,
            height = 20,
            border = 'rounded',
        },
    },
}

-- Setup function (LazyVim compatible - accepts opts table)
function M.setup(opts)
    -- Handle both direct opts and lazy.nvim style { opts = {...} }
    if opts and opts.opts then
        opts = opts.opts
    end
    M.config = vim.tbl_deep_extend('force', M.config, opts or {})

    -- Setup LSP if enabled
    if M.config.lsp.enabled then
        require('tark.lsp').setup(M.config.lsp)
    end

    -- Setup ghost text if enabled
    if M.config.ghost_text.enabled then
        require('tark.ghost').setup(M.config.ghost_text)
    end

    -- Setup chat if enabled
    if M.config.chat.enabled then
        require('tark.chat').setup(M.config.chat)
    end

    -- Register commands
    M.register_commands()
end

-- Register user commands
function M.register_commands()
    -- Chat command (open)
    vim.api.nvim_create_user_command('TarkChat', function(opts)
        require('tark.chat').open(opts.args)
    end, { nargs = '*', desc = 'Open tark chat' })

    -- Chat toggle command
    vim.api.nvim_create_user_command('TarkChatToggle', function()
        require('tark.chat').toggle()
    end, { desc = 'Toggle tark chat window' })

    -- Toggle ghost text
    vim.api.nvim_create_user_command('TarkGhostToggle', function()
        require('tark.ghost').toggle()
    end, { desc = 'Toggle ghost text completions' })

    -- Trigger completion manually
    vim.api.nvim_create_user_command('TarkComplete', function()
        require('tark.ghost').trigger()
    end, { desc = 'Trigger AI completion' })

    -- Setup default keybindings
    vim.keymap.set('n', '<leader>ec', function()
        require('tark.chat').toggle()
    end, { silent = true, desc = 'Toggle tark chat' })
end

return M

