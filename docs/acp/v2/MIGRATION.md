# ACP Migration Guide (Breaking)

## Method mapping

- `session/create` -> `session/new`
- `session/send_message` -> `session/prompt`
- `response/delta` / `response/final` / `tool/event` / `session/status` -> `session/update`
- `approval/request` -> `session/request_permission`
- `tark/inline_completion` added as Tark ACP extension method for ghost text.

Legacy methods above are no longer accepted by the server.

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
