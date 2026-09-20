# Editor Compatibility Contract

Backend/editor version handshake for Tark's editor-neutral protocols (R10, R11, R12).

## Handshake

Every ACP session starts with `initialize`:

| Direction | Field | Contract |
|-----------|-------|----------|
| Client → backend | `protocolVersion` | Must be `1`. Anything else is rejected with `unsupported_version` naming the supported revision. |
| Backend → client | `agentInfo.{name,version}` | `tark` + `CARGO_PKG_VERSION` (currently `0.12.7`). Clients must compare major/minor against their supported range and surface a remediation path on mismatch. |
| Backend → client | `agentCapabilities` | Exactly the implemented subset (see below). Clients must not send methods outside it. |
| Either | `_meta.tark.completion` | Optional `_tark/inlineCompletion` extension contract `{method, version: 1}`. Chat works identically with or without it. |

## Supported backend surface

- **ACP v1, newline-delimited JSON stdio.** The legacy `Content-Length` envelope is rejected, never reinterpreted.
- `initialize`, `session/new`, `session/prompt` (+ `session/update` stream), `session/cancel`, `session/close`, `session/set_mode`, `session/set_config_option` (`mode` only), `session/request_permission` (approvals + single single-select elicitation).
- `session/load` is **not** offered (`loadSession: false`); `session/new` `mcpServers` entries are **rejected** (configure via `mcp/servers.toml` + `tark mcp` CLI).
- **LSP subset:** `initialize`/`shutdown`/`exit`, incremental + full sync, UTF-16 positions, `completion`, `hover`, `codeAction` (`refactor` suggestion edits applied by the client), cancellable debounced `publishDiagnostics`, `workspace/didChangeWorkspaceFolders` with fail-closed scoping. Complete LSP 3.18 is explicitly not claimed.
- **MCP client:** explicit revision `2026-07-28`, stdio + Streamable HTTP transports, lifecycle CLI (`tark mcp …`), Apps behind `TARK_MCP_APPS=1` consent, Tasks experimental behind `TARK_MCP_TASKS_EXPERIMENTAL=1`.

## Editor packages

Production clients (Neovim adapter, VS Code extension) live outside this repository and must:

1. Verify backend version + protocol capabilities at startup (`:checkhealth` or equivalent) with an actionable remediation per failure (upgrade backend / upgrade plugin / rollback to previous compatible pair).
2. Never send `Content-Length`-framed ACP, legacy `session/create` calls, or MCP passthrough in `session/new`.
3. Treat `_tark/inlineCompletion` as optional: degrade to standard chat when unadvertised, and discard responses whose `clientRequestId`/`completionEpoch` is stale.
4. A future editor (e.g. Sublime) integrates against this same contract with no backend changes (NG4: no additional production client is in scope for this initiative).

## Rollback

If a backend/editor pair misbehaves after upgrade: disable the new subsystem
(`tark mcp disable …`, editor plugin pin) or reinstall the previous compatible
release pair from release artifacts. Downgrades never rewrite user data:
`mcp/servers.toml` writes keep a `.toml.bak` generation, and trust records
(`.tark/mcp_trust.json`) are additive.
