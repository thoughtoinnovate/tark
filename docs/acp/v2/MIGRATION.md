# ACP Migration Guide (Breaking)

## Transport framing

- The legacy `Content-Length` envelope was removed. Every message is now one
  JSON value followed by `\n` (newline-delimited JSON stdio). Peers sending
  `Content-Length` framing receive a parse rejection naming this migration.

## Method mapping

- `session/create` -> `session/new`
- `session/send_message` -> `session/prompt`
- `response/delta` / `response/final` / `tool/event` / `session/status` -> `session/update`
- `approval/request` -> `session/request_permission`
- `tark/inline_completion` -> `_tark/inlineCompletion` (optional extension;
  advertise `{"_meta":{"tark":{"completion":{"supported":true}}}}` in
  `initialize`, otherwise standard chat only)
- `session/load` removed (`loadSession: false`; use `session/new`)
- `session/new` `mcpServers` entries are rejected (configure MCP via
  `mcp/servers.toml` + `tark mcp` CLI)
- `session/set_config_option` is effective for `mode` only; `provider`/`model`
  are fixed at session creation

Legacy methods above are no longer accepted by the server; each returns a
documented migration error naming the replacement.

## Initialize mapping

- Request fields:
  - `versions` -> `protocolVersion`
  - `client` -> `clientInfo`
  - `capabilities` -> `clientCapabilities`
- Response fields:
  - `acp_version` -> `protocolVersion`
  - `server` -> `agentInfo`
  - `capabilities` -> `agentCapabilities`
  - new: `authMethods`

## Neovim command mapping

- `:TarkChatOpen` -> `:AcpChatOpen`
- `:TarkChatClose` -> `:AcpChatClose`
- `:TarkChatToggle` -> `:AcpChatToggle`
- `:TarkChatSend` -> `:AcpSend`
- `:TarkChatCancel` -> `:AcpCancel`
- `:TarkAskBuffer` -> `:AcpAskBuffer`
- `:TarkAskSelection` -> `:AcpAskSelection`
- `:TarkMode` -> `:AcpMode`
- `:TarkUiFocus` -> `:AcpUiFocus`
- `:TarkUiNextAction` -> `:AcpUiNextAction`
- `:TarkUiPrevAction` -> `:AcpUiPrevAction`
- `:TarkUiSubmit` -> `:AcpUiSubmit`
- `:TarkUiCancel` -> `:AcpUiCancel`

ACP-related `:Tark*` commands were removed (no command aliases).

## Config mapping (Neovim)

- `binary` is still supported for Tark auto-download behavior.
- New ACP transport keys:
  - `acp.command`
  - `acp.args`
  - `acp.env`
  - `acp.cwd`
  - `acp.protocol_version`
  - `acp.client_capabilities`
  - `acp.profile`
