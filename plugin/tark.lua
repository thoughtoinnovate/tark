-- tark.nvim plugin loader
-- This file is auto-loaded by Neovim/lazy.nvim

if vim.g.loaded_tark then
    return
end
vim.g.loaded_tark = true

-- Register health check (handles both :checkhealth tark and :checkhealth Tark)
if vim.health and vim.health.register then
    -- Neovim 0.10+
    vim.health.register('tark', function()
        require('tark.health').check()
    end)
else
    -- Older Neovim - create module alias
    package.preload['Tark.health'] = function()
        return require('tark.health')
    end
    package.preload['Tark'] = function()
        return require('tark')
    end
end
