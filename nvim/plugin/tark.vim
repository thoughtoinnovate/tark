" tark (तर्क) Neovim plugin auto-loader
" Tark = Logic/Reasoning in Sanskrit
" This file ensures the plugin is loaded on startup

if exists('g:loaded_tark')
    finish
endif
let g:loaded_tark = 1

" The plugin is configured via lua
" Add to your init.lua:
"
" require('tark').setup({
"     lsp = { enabled = true },
"     ghost_text = {
"         enabled = true,
"         server_url = 'http://localhost:8765',
"     },
"     chat = { enabled = true },
" })

