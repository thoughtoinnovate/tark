# Editor Adapter API v1

`Editor Adapter API v1` defines how external editor adapters (Neovim, future VS Code/Sublime) expose editor state to core `tark`.

## `/chat` request payload

Core `/chat` accepts an optional `editor` object:

```json
{
  "editor": {
    "adapter_id": "tark.nvim",
    "adapter_version": "0.11.4",
    "api_version": "v1",
    "endpoint": {
      "base_url": "http://127.0.0.1:8787",
      "auth_token": null
    },
    "capabilities": {
      "definition": true,
      "references": true,
      "hover": true,
      "symbols": true,
      "diagnostics": true,
      "open_file": true,
      "cursor": true,
      "buffers": true,
      "buffer_content": true
    }
  }
}
```

`api_version` must be `v1`.

## Adapter endpoints

All endpoints are rooted at `editor.endpoint.base_url`.

- `GET /editor/health`
- `POST /editor/definition` body: `{ "file": string, "line": number, "col": number }`
- `POST /editor/references` body: `{ "file": string, "line": number, "col": number }`
- `POST /editor/hover` body: `{ "file": string, "line": number, "col": number }`
- `POST /editor/symbols` body: `{ "file": string }`
- `POST /editor/diagnostics` body: `{ "path"?: string }`
- `GET /editor/cursor`
- `GET /editor/buffers`
- `POST /editor/buffer-content` body: `{ "path": string }`
- `POST /editor/open-file` body: `{ "path": string, "line"?: number, "col"?: number }`

## Response shape (core-used fields)

- `definition`: `{ "locations": [{ file, line, col, preview? }] }`
- `references`: `{ "references": [{ file, line, col, preview? }] }`
- `hover`: `{ "hover": string | null }`
- `symbols`: `{ "symbols": [{ name, kind, line, detail? }] }`
- `diagnostics`: `{ "diagnostics": [{ path, line, col, severity, message, source? }] }`
- `cursor`: `{ path, line, col }`
- `buffers`: `{ "buffers": [{ id, path, name, modified, filetype }] }`

## Fallback behavior

Core adapter calls are best-effort. On timeout, transport error, non-2xx status, or malformed payload, core falls back to non-adapter behavior (tree-sitter/file-system paths) where available.

## TUI session wiring

For editor-embedded TUI sessions, adapters can pass a serialized `EditorContextV1` via:

- `TARK_EDITOR_CONTEXT_JSON`

When set, `tark tui` validates and activates this context for the full TUI process lifetime so editor-aware tools can use adapter capabilities.
