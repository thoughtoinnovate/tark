# ACP Migration Guide (Breaking)

## Method mapping

- `session/create` -> `session/new`
- `session/send_message` -> `session/prompt`
- `response/delta` / `response/final` / `tool/event` / `session/status` -> `session/update`
- `approval/request` -> `session/request_permission`

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
