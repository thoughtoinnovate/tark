-- tark LSP configuration for Neovim

local M = {}

M.config = {}

function M.setup(opts)
    M.config = opts

    -- Check if lspconfig is available
    local ok, lspconfig = pcall(require, 'lspconfig')
    if not ok then
        vim.notify('nvim-lspconfig not found. Please install it for LSP support.', vim.log.levels.WARN)
        return
    end

    local configs = require('lspconfig.configs')

    -- Register tark as a new LSP server if not already registered
    if not configs.tark then
        configs.tark = {
            default_config = {
                cmd = opts.cmd or { 'tark', 'lsp' },
                filetypes = opts.filetypes or { '*' },
                root_dir = function(fname)
                    return lspconfig.util.find_git_ancestor(fname)
                        or lspconfig.util.path.dirname(fname)
                end,
                settings = {
                    tark = {
                        completion = { enabled = true },
                        hover = { enabled = true },
                        codeAction = { enabled = true },
                        diagnostics = { enabled = true },
                    },
                },
            },
        }
    end

    -- Setup the LSP
    lspconfig.tark.setup({
        on_attach = function(client, bufnr)
            -- Set up keymaps for LSP features
            local bufopts = { noremap = true, silent = true, buffer = bufnr }

            -- Hover documentation
            vim.keymap.set('n', 'K', vim.lsp.buf.hover, bufopts)

            -- Code actions
            vim.keymap.set('n', '<leader>ca', vim.lsp.buf.code_action, bufopts)
            vim.keymap.set('v', '<leader>ca', vim.lsp.buf.code_action, bufopts)

            vim.notify('tark LSP attached to buffer ' .. bufnr, vim.log.levels.INFO)
        end,
        capabilities = vim.lsp.protocol.make_client_capabilities(),
    })
end

return M

