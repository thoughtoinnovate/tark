# Tark Platform Hardening and Editor Integration

**Artifact:** Product requirements and canonical acceptance scenarios  
**Status:** Proposed freeze  
**Last meaningful update:** 2026-08-31  
**Approval:** Not yet approved  

## Purpose

Make Tark safe and reliable as a terminal agent, add a standards-based MCP client, replace the incompatible editor transport with ACP v1, expose a deliberately limited LSP surface, and ship production-quality Neovim and VS Code integrations.

This proposal corrects the earlier conversational plan by removing unverified dependency pins, removing a claim of complete LSP 3.18 support, treating MCP Tasks as experimental while its official specification remains draft, replacing path canonicalization with race-resistant workspace confinement as an observable security requirement, and separating the work into release-sized outcomes.

## Desired outcome

A user can run Tark interactively or from a supported editor without terminal hangs, accidental filesystem escape, silent execution of untrusted MCP server code, incompatible protocol framing, or plugin/backend version confusion. Tark should interoperate through current published standards and expose only capabilities it actually implements and tests.

## Primary users and workflows

- Terminal users use mouse and keyboard navigation across long, streaming conversations without the TUI becoming unresponsive.
- Agent users allow Tark to read, modify, and run processes inside a selected workspace while retaining explicit control over risky actions.
- MCP users configure trusted servers, inspect their capabilities, approve sensitive operations, and recover from server or transport failure.
- Neovim and VS Code users start a Tark subprocess, chat with the agent, review permission requests, and receive completion assistance.
- Maintainers upgrade dependencies and protocol integrations using repeatable conformance, compatibility, migration, security, and rollback evidence.

## Release boundaries

The initiative is delivered in this order. A release may not consume an incomplete prerequisite.

1. **Release 1 — Security foundation:** workspace confinement, process controls, secret handling, and MCP process trust.
2. **Release 2 — TUI reliability:** event ingestion, rendering, hit testing, scrolling, shutdown, and stress coverage.
3. **Release 3 — MCP client:** MCP 2026-07-28 core, stdio, Streamable HTTP, lifecycle commands, conformance, authentication, and separately gated extensions.
4. **Release 4 — Editor backend:** ACP v1 and the explicitly supported LSP capability subset.
5. **Release 5 — Neovim:** production plugin, distribution, real-editor tests, and migration.
6. **Release 6 — VS Code:** production extension with behavior equivalent to the supported Neovim workflows.

Security controls from Release 1 apply to every later release. MCP Apps and experimental MCP Tasks must not block the core MCP release. The editor-neutral backend must be releasable before either editor package.

## Requirements

### R1 — Race-resistant workspace confinement

All native filesystem operations must remain within explicitly granted workspace roots, including reads, writes, creation, rename, and deletion. Traversal, absolute-path escape, symlink escape, and path replacement between authorization and use must fail closed. A user may grant another root only through an explicit permission interaction that clearly identifies the target.

### R2 — Controlled command and process execution

Ask/Plan or equivalent safe modes must not expose a generic shell. Any mode that can launch a process must display the effective command and working directory when approval is required, enforce execution-time and output bounds, support cancellation, terminate descendant processes, and return a bounded diagnostic result. Read-only workflows should use native operations rather than command-line emulation.

### R3 — Secret and trust-boundary protection

Tokens, provider credentials, authorization codes, command environments, and sensitive MCP headers must not appear in normal logs, traces, crash reports, or UI history. An MCP stdio server is treated as executable third-party code: Tark must show its command, source/provenance information when available, working directory, and inherited environment categories before first launch. The user must explicitly trust it, and Tark must inherit only the environment entries required by its configuration.

### R4 — Responsive and correct TUI interaction

Mouse and keyboard input must continue to be processed while output streams, history is long, the window is resized, or background events arrive rapidly. Click actions must use the layout actually rendered on screen. Scrolling must use one authoritative offset, preserve intentional user position, resume follow-tail only through an explicit follow-tail action or documented boundary behavior, and never make cancellation or exit unreachable. Terminal state must be restored after normal exit, cancellation, panic, or initialization failure.

### R5 — Current MCP client core

Tark must act as an MCP client for the published MCP `2026-07-28` revision and explicitly select that revision rather than relying on an SDK default. It must support stdio and Streamable HTTP transports, lifecycle management, tools, resources, prompts, required notifications/subscriptions, and the client behavior required by the official conformance suite. A server that cannot use the required revision must receive a clear incompatibility error; Tark must not silently fall back to legacy standalone HTTP+SSE or an older protocol revision.

Users must be able to list, add, inspect, enable, disable, connect, disconnect, reconnect, and remove server definitions, and inspect discovered capabilities before use. TOML is the canonical configuration format; an explicit JSON import command may create equivalent TOML without silently overwriting an existing server.

### R6 — Secure MCP HTTP and authorization

Streamable HTTP connections must reject insecure non-loopback endpoints by default. Local loopback service endpoints must use a per-start bearer credential stored through a symlink-safe, atomic, owner-only mechanism, rotated on restart, compared without data-dependent early exit, and excluded from diagnostics. Remote authentication must implement only published stable MCP/OAuth extension contracts that Tark advertises, including client-credentials and enterprise-managed authorization when those extensions remain stable at implementation time. Tokens must use an appropriate secure credential store when the platform provides one.

### R7 — MCP extensions with explicit maturity gates

MCP Apps may be enabled only through a user-visible consent flow and an isolated browser host with a distinct origin, restrictive content policy, explicit network grants, bounded lifetime, and cleanup. MCP Tasks remains disabled by default and marked experimental while the official specification is labelled draft. No draft extension may be presented as stable. Extension status must be rechecked against primary documentation before implementation begins and before release.

### R8 — ACP v1 editor-agent transport

Editor chat and agent control must use the official stable ACP v1 protocol over newline-delimited JSON stdio through a published ACP SDK. Tark must advertise only implemented capabilities. The supported product surface includes initialization, session lifecycle, prompts and streamed updates, cancellation, permission and elicitation interactions, configuration options, and MCP server information required by editor sessions. The existing `Content-Length` framing must not remain as an alternative ACP mode.

Inline completion may use one documented, optional ACP extension whose method begins with `_` and whose capability is advertised through `_meta`. Clients that do not understand the extension must still retain standard ACP chat behavior.

### R9 — Deliberate LSP capability subset

Tark must provide a stateless, read-only language-server subprocess with these tested capabilities: initialize/shutdown/exit, incremental document synchronization, UTF-16 position conversion, completion, hover, code actions, diagnostics, cancellation, and workspace-folder handling. Tark must not claim complete LSP 3.18 compliance. LSP must not own agent sessions, MCP connections, permission state, or persistent policy state.

### R10 — Production Neovim integration

The Neovim plugin must install through normal plugin-manager workflows, locate or securely install a compatible Tark binary, start and stop ACP/LSP subprocesses without blocking Neovim, expose health diagnostics, support chat and permission interactions, and render inline completions without corrupting the buffer or leaking stale results. It must recover when Tark exits and explain version incompatibility with a direct remediation path.

### R11 — Production VS Code integration and editor-neutrality

The VS Code extension must provide the same supported chat, permission, cancellation, and completion workflows through ACP and LSP while using native VS Code interaction surfaces. The backend protocols and compatibility contract must not depend on editor-specific payloads. A future editor such as Sublime should be able to integrate without changing Tark's core protocol contracts, although no additional production client is included in this initiative.

### R12 — Compatibility, migration, distribution, and rollback

Existing MCP TOML configuration and supported session data must either continue to work or be migrated with a dry-run preview and backup. Removed configuration, old ACP framing, and plugin/backend version requirements must have documented migration errors rather than silent reinterpretation. Backend and editor artifacts must use compatible version metadata and independently verifiable release evidence. A failed rollout must be containable by disabling the new subsystem or installing the previous compatible release without corrupting user data.

### R13 — Repeatable verification and operational evidence

Every release must retain automated evidence for its governing scenarios. The MCP release must pass the official client conformance suite for `2026-07-28` through a dedicated Tark conformance entry point and must interoperate with the official Everything test server; the official Filesystem server is the initial practical server, restricted to a disposable test directory. ACP and editor behavior must be exercised against real subprocesses and real headless editors. TUI responsiveness must be proved through deterministic starvation and state-invariant tests, with latency measured separately as a benchmark instead of a flaky wall-clock unit assertion.

Dependency versions and artifact integrity values must be pinned only after registry/release verification. CI must cover supported operating systems, retain failure diagnostics without secrets, and block release on protocol, migration, security, or compatibility regressions.

## Non-functional requirements

### NFR1 — Fail-closed security

Authorization, path validation, protocol-version validation, extension maturity, artifact verification, and secret handling fail closed. A diagnostic explains denial without revealing sensitive values.

### NFR2 — Input fairness

No render, layout, output-stream, provider, or background task may monopolize the TUI event loop. Cancellation and exit remain reachable under sustained load.

### NFR3 — Bounded resources

Channels, process output, retained render data, retries, browser sessions, and transport queues have explicit bounds or coalescing behavior. Exceeding a bound produces controlled degradation and a diagnostic rather than unbounded growth or deadlock.

### NFR4 — Isolation

Concurrent ACP editor processes have isolated session state. LSP remains stateless. MCP servers and MCP Apps receive only explicitly granted capabilities and data. A failure in one integration must not freeze unrelated TUI input or other editor sessions.

### NFR5 — Standards evidence

Standards compliance is claimed per tested capability and protocol revision, never from dependency names alone. Draft extensions are identified as draft.

### NFR6 — Cross-platform support

Release verification covers the operating systems and architectures for which binaries and editor packages are published. Platform limitations are detected before installation and explained to the user.

### NFR7 — Maintainability for weak execution agents

Downstream work packages must have one owner boundary, explicit files or modules, prerequisites, forbidden changes, failing tests to add first, exact validation commands, expected evidence, rollback instructions, and a stop condition. No work package may require its executor to invent a shared protocol, security rule, migration contract, or release format.

## Non-goals

- **NG1:** Implement or depend on A2A.
- **NG2:** Claim support for every LSP 3.18 method.
- **NG3:** Implement every optional ACP feature merely because it exists.
- **NG4:** Add production Sublime or other editor clients in these releases.
- **NG5:** Support legacy MCP standalone HTTP+SSE or silently downgrade from MCP `2026-07-28`.
- **NG6:** Add new Tark dependencies on deprecated MCP Roots, Sampling, or Logging features.
- **NG7:** Present MCP Tasks as stable while its official specification remains draft.
- **NG8:** Claim that MCP tool-call policy alone sandboxes the MCP server process.
- **NG9:** Allow a generic shell in Ask/Plan or equivalent safe modes.
- **NG10:** Rewrite unrelated provider, storage, theme, channel, or plugin-runtime features.

## User-visible interaction requirements

- Permission prompts identify the actor, operation, target, scope, and whether approval is one-time or persisted.
- TUI mouse targets match visible controls, including after resize, modal changes, and wrapped text changes.
- MCP connection states distinguish disabled, untrusted, starting, connected, degraded, incompatible, failed, and stopped.
- MCP App consent explains that content opens in an isolated browser session and lists requested network access.
- Neovim and VS Code show connection state, backend version, actionable health failures, cancellation state, and stale-completion suppression.
- Errors provide the next safe action; they do not recommend disabling verification or broadening trust as the default fix.

## Canonical BDD scenario manifest

The canonical manifest proposed for freeze is **S1–S33**.

### S1 — Allowed workspace file operation (`R1`)

**Given** a user selected a workspace root and a file exists beneath it, **when** an authorized native file tool opens that file, **then** the operation succeeds using only the granted workspace capability and reports the workspace-relative target.

### S2 — Traversal and absolute escape are denied (`R1`, `NFR1`)

**Given** a tool argument resolves outside every granted workspace root through `..`, an absolute path, or platform-specific path syntax, **when** the tool is invoked, **then** Tark performs no target operation and returns a denial that identifies the missing scope without exposing unrelated filesystem content.

### S3 — Symlink replacement cannot escape authorization (`R1`, `NFR1`)

**Given** an attacker replaces a path component or symlink after a request is authorized, **when** the operation opens, writes, renames, or deletes the target, **then** it remains confined to the pre-granted directory capability or fails without affecting the external target.

### S4 — Safe mode has no generic shell (`R2`, `NG9`)

**Given** Ask/Plan or an equivalent safe mode, **when** the agent asks to inspect or search workspace content, **then** only bounded native read-only operations are available and no generic command interpreter can be invoked.

### S5 — Risky process is approved, bounded, and cancelled (`R2`, `NFR3`)

**Given** a mode permits a risky command, **when** the user approves it and later cancels it or a configured bound is exceeded, **then** Tark stops the process and descendants, caps captured output, reports the reason, and keeps the UI responsive.

### S6 — Secrets remain redacted (`R3`, `R6`)

**Given** provider or MCP credentials are present in configuration, environment, headers, or responses, **when** Tark logs, traces, displays an error, or produces a crash diagnostic, **then** the credential value is absent while enough non-secret context remains to diagnose the failure.

### S7 — MCP stdio launch requires informed trust (`R3`)

**Given** a configured stdio MCP server has not been trusted, **when** a connection is requested, **then** Tark displays its executable command, working directory, configured source/provenance when available, and environment categories; no process starts until the user approves.

### S8 — Mouse hit testing follows rendered layout (`R4`)

**Given** wrapped content, a resized terminal, or a modal changes visible geometry, **when** the user clicks a visible control or text region, **then** Tark dispatches only the action associated with the exact region produced by the most recent render.

### S9 — Long-history scrolling does not hang (`R4`, `NFR2`, `NFR3`)

**Given** a long conversation with continuing streamed output, **when** the user repeatedly scrolls, clicks, types, cancels, and toggles follow-tail, **then** the viewport moves predictably, input continues to be handled, retained state remains bounded, and the TUI does not freeze.

### S10 — Background event storm cannot starve input (`R4`, `NFR2`)

**Given** sustained background updates and render invalidations, **when** input, cancellation, resize, and shutdown events arrive, **then** each priority input class is serviced according to the documented fairness invariant and redundant render work is coalesced.

### S11 — Terminal restoration after failure (`R4`)

**Given** Tark entered alternate-screen or raw terminal mode, **when** startup fails, the event loop panics, or the user exits, **then** the terminal mode, cursor, mouse capture, and screen state are restored through the same cleanup guard.

### S12 — MCP stdio interoperability (`R5`, `R13`)

**Given** a pinned official Everything test server, **when** Tark connects using explicit MCP `2026-07-28`, discovers capabilities, invokes representative tools, reads representative resources, and exercises supported notifications, **then** results match the server contract and the server shuts down without orphaned processes.

### S13 — Practical Filesystem server is confined (`R1`, `R3`, `R5`, `R13`)

**Given** the official Filesystem MCP server is configured for a disposable directory, **when** Tark discovers and invokes its operations, **then** user approval and MCP policy are enforced, operations outside the disposable directory are rejected by the server configuration, and no production or home-directory path is used by the test.

### S14 — Streamable HTTP connection is secure (`R5`, `R6`)

**Given** a Streamable HTTP server definition, **when** Tark connects, **then** insecure non-loopback transport is rejected by default, required standard headers and authentication are sent, transport errors are bounded, and credentials are redacted. For a Tark-owned loopback bridge, its per-start bearer credential is created atomically without following symlinks, is readable only by the owner, changes after restart, is compared without data-dependent early exit, and is removed or invalidated at shutdown.

### S15 — Legacy or incompatible MCP version is rejected (`R5`, `NG5`)

**Given** a server cannot communicate using MCP `2026-07-28`, **when** connection negotiation completes or fails, **then** Tark reports the supported and required revisions and does not silently retry with an older revision or legacy SSE transport.

### S16 — MCP server failure is isolated and recoverable (`R5`, `NFR4`)

**Given** one MCP server exits, hangs, returns malformed messages, or exceeds a response bound, **when** Tark detects the fault, **then** that server enters a clear degraded or failed state, its requests are cancelled, unrelated input and servers remain usable, and an explicit reconnect can create a clean session.

### S17 — Stable authorization extension succeeds without credential leakage (`R6`)

**Given** a server requires a supported published authorization extension, **when** the user or enterprise flow completes, **then** Tark stores and applies the token through the platform-appropriate secure mechanism, scopes it to the correct issuer/server, refreshes or reauthorizes according to the contract, and never displays the token.

### S18 — MCP App requires consent and isolation (`R7`)

**Given** an MCP result requests an App UI, **when** the user reviews and accepts the App request, **then** Tark opens a bounded isolated browser session with a distinct origin and only approved network access; closing or expiring the session revokes its credential and removes its transient state.

### S19 — Draft Tasks extension remains gated (`R7`, `NG7`)

**Given** the Tasks specification is still labelled draft, **when** a normal production user connects to a Tasks-capable server, **then** Tark does not advertise Tasks unless the explicit experimental flag is enabled and clearly identifies the feature as experimental.

### S20 — ACP v1 chat session uses standard framing (`R8`)

**Given** a conforming ACP v1 client, **when** it starts Tark over stdio, initializes, creates a session, sends a prompt, receives streamed updates, and ends the session, **then** every message uses newline-delimited JSON, advertised capabilities match observed behavior, and no `Content-Length` framing is required.

### S21 — ACP permission and cancellation are round-tripped (`R8`, `NFR4`)

**Given** an editor session triggers a permission or elicitation request and a long-running agent action, **when** the user responds or cancels from the editor, **then** Tark associates the response with the correct isolated session, stops the affected action, and sends the terminal ACP update without changing another session.

### S22 — Optional completion extension degrades safely (`R8`)

**Given** a client does not advertise Tark's completion extension, **when** it uses standard ACP chat, **then** chat remains fully usable and Tark sends no extension messages; when both sides advertise the documented extension, stale or cancelled completion results are never applied.

### S23 — LSP incremental UTF-16 behavior is correct (`R9`)

**Given** an open document containing multibyte and non-BMP characters, **when** the client sends incremental edits and requests completion, hover, code actions, or diagnostics using UTF-16 positions, **then** Tark interprets ranges correctly and never edits or diagnoses the wrong text span.

### S24 — LSP cancellation and ownership boundaries hold (`R9`, `NFR4`)

**Given** overlapping requests for multiple documents or workspaces, **when** one request is cancelled or a document closes, **then** only the relevant computation and diagnostics are affected, stale diagnostics are not published, and no ACP, MCP, or persistent agent state is created by LSP.

### S25 — Neovim installs and diagnoses compatibility (`R10`, `R12`)

**Given** a supported Neovim version and plugin manager, **when** the user installs and configures the plugin, **then** `:checkhealth` or the documented equivalent verifies the backend, protocol capabilities, version compatibility, executable integrity, and configuration with actionable remediation for each failure.

### S26 — Neovim completes the primary editor workflow (`R10`)

**Given** a valid Tark configuration, **when** the user opens chat, sends context, handles a permission request, cancels an action, and requests inline completion, **then** Neovim remains responsive, ACP/LSP subprocesses are cleaned up, and accepted completion text is applied only to the intended buffer version.

### S27 — VS Code completes the equivalent editor workflow (`R11`)

**Given** a supported VS Code version and a verified compatible Tark binary, **when** the user performs the supported chat, permission, cancellation, and completion workflow, **then** behavior and backend protocol contracts match Neovim while presentation uses native VS Code surfaces.

### S28 — Concurrent editors remain isolated (`R8`, `R9`, `R11`, `NFR4`)

**Given** multiple ACP and LSP subprocesses from one or more editors, **when** prompts, permissions, completions, diagnostics, cancellations, and shutdowns overlap, **then** session ownership is unambiguous, persistent writes are serialized or atomic, LSP stays stateless, and one process failure does not corrupt another session.

### S29 — Migration and rollback preserve user state (`R12`)

**Given** existing configuration, sessions, or an older editor plugin, **when** the user previews and performs an upgrade, **then** Tark creates required backups, reports transformed and unsupported fields, refuses ambiguous conversion, and permits return to the documented previous compatible release without corrupting the original data.

### S30 — Release evidence blocks unsupported claims (`R5`, `R7`, `R8`, `R9`, `R12`, `R13`, `NFR5`)

**Given** a release candidate, **when** CI and release validation run, **then** dependency versions and integrity are verified, MCP conformance runs through the dedicated Tark client adapter, real ACP/editor scenarios run, deterministic TUI/security/migration tests pass, draft extension labels are rechecked, and release metadata lists only the capabilities actually proved.

### S31 — MCP configuration lifecycle is reviewable (`R5`, `R12`)

**Given** valid global or workspace MCP definitions, **when** the user lists, adds, inspects, enables, disables, connects, disconnects, reconnects, removes, or imports a server, **then** the command reports the effective source and trust state, preserves unrelated configuration, refuses a silent overwrite, and writes canonical TOML only after successful validation.

### S32 — Published platform artifacts are actually supported (`R10`, `R11`, `R12`, `R13`, `NFR6`)

**Given** every advertised operating-system, architecture, Neovim, and VS Code combination, **when** its release installation and primary smoke workflow run, **then** the artifact verifies, starts, negotiates compatible capabilities, completes the smoke workflow, and shuts down cleanly; a combination without this evidence is not advertised as supported.

### S33 — A work package can be executed without conversation history (`NFR7`)

**Given** an execution agent has only the repository and locked planning bundle, **when** it accepts one downstream work package, **then** it can identify owned files or modules, prerequisites, inputs, outputs, forbidden changes, tests to add first, validation commands, expected evidence, rollback, and stop conditions without inventing a shared contract; otherwise that package remains blocked at plan review.

## Success and release evidence

- Every scenario `S1–S33` has an assigned automated validation layer before implementation planning is locked.
- MCP client conformance for the explicitly selected revision passes without unapproved fallback.
- TUI stress tests prove input fairness and bounded state; separately retained benchmarks show whether responsiveness regressed from the recorded baseline.
- Real Neovim and VS Code smoke/E2E runs exercise subprocess startup, protocol initialization, primary workflow, cancellation, failure recovery, and shutdown.
- Security regression tests cover traversal, absolute paths, symlink/path replacement, subprocess descendants, secret redaction, untrusted MCP launch, token-file attacks, and release artifact verification.
- Migration tests start from representative current user configuration and prove both forward conversion and documented rollback.
- Release notes disclose experimental extensions, unsupported protocol features, platform support, configuration changes, and compatibility requirements.

## Decisions included in this proposal

### D1 — Protocol boundaries

**Status:** Proposed  
MCP connects Tark to tool/context servers, ACP connects editors to the agent, and LSP provides the narrow language-feature subset. A2A is out of scope.

### D2 — Editor process model

**Status:** Proposed  
Editors launch standard ACP and LSP subprocesses. LSP is stateless and ACP owns editor-agent sessions. There is no unauthenticated general-purpose local editor HTTP API in the initial production design.

### D3 — MCP revision policy

**Status:** Proposed  
The implementation target is MCP `2026-07-28`, explicitly selected and conformance-tested. The target is fixed for this initiative; a newer revision discovered later requires reopening this requirement rather than silently moving the goalpost.

### D4 — Extension maturity policy

**Status:** Proposed  
Only published stable extensions may be enabled by default. Apps and stable authorization extensions are separately gated product features. Tasks remains experimental while its official page identifies the specification as draft.

### D5 — LSP claims

**Status:** Proposed  
Tark claims compatibility only for the exact method and behavior subset in `R9`, not complete LSP 3.18 support.

### D6 — Safe-mode process policy

**Status:** Proposed  
Generic `safe_shell` behavior is removed from safe modes. Native workspace-confined operations replace read-only shell use; risky process execution exists only behind the applicable mode and approval policy.

### D7 — Configuration source

**Status:** Proposed  
TOML remains canonical for MCP configuration. JSON is accepted only by an explicit import operation that produces reviewable TOML.

### D8 — First production editor clients

**Status:** Proposed  
Neovim and VS Code are production deliverables. Backend contracts remain editor-neutral, but other clients are follow-on work.

## Assumptions requiring validation

### A1 — MCP Rust SDK release

A published Rust SDK version exists that can be pinned and made to explicitly use MCP `2026-07-28`. Validate against the package registry, source tag, lockfile, security advisories, and official conformance suite before selecting a version. If false, stop MCP implementation for an SDK assessment rather than reviving the handwritten protocol.

### A2 — ACP Rust SDK suitability

The published ACP Rust 1.0 SDK covers the required stable session surface and supports Tark's optional extension mechanism. Validate with a minimal newline-delimited stdio spike before deleting compatibility code.

### A3 — Editor release ownership

The project can publish Neovim and VS Code packages plus compatible Tark binaries and signed or attested release metadata. Repository/mirror ownership and signing authority must be resolved before distribution design is locked.

### A4 — Supported platform matrix

The supported OS, architecture, Neovim, and VS Code versions will be derived from current release automation and user support commitments before regression strategy lock. No platform may be advertised without CI or documented manual release evidence.

## Risks

### K1 — Initiative size

Combining releases would exceed the context and verification capacity of a weak execution model. Mitigation: downstream planning must create atomic work packages and enforce release gates.

### K2 — MCP server code execution

Even perfect MCP call policy does not sandbox a stdio server. Mitigation: informed trust, minimal environment, provenance, and an optional OS/container sandbox profile; documentation must preserve this limitation.

### K3 — TUI regression from shared mutable state

Splitting ingestion and rendering can add deadlocks or stale layout data. Mitigation: a later design must define immutable render snapshots, ownership, queue priority, and coalescing before implementation.

### K4 — Protocol dependency drift

SDK defaults or published extension maturity may not match the desired revision. Mitigation: explicit protocol constants, pinned verified releases, maturity recheck, and conformance gates.

### K5 — Editor/backend distribution mismatch

Independently updated editor packages can start incompatible binaries. Mitigation: capability/version handshake, published compatibility metadata, health diagnostics, and rollback packages.

### K6 — Migration damage

Existing custom ACP/config behavior may have users despite being nonstandard. Mitigation: detect, preview, back up, migrate only unambiguous data, and retain a rollback route.

## Material unresolved items

No product-scope choice is intentionally left unresolved in this candidate. SDK versions, exact supported-platform versions, repository/mirror ownership, signing mechanism, queue implementation, browser host implementation, and test-framework selection are downstream design or validation decisions constrained by these requirements. They must not be guessed inside an execution work package.

## Proposed freeze record

Approval of this proposal freezes:

- requirements `R1–R13`;
- non-functional requirements `NFR1–NFR7`;
- non-goals `NG1–NG10`;
- decisions `D1–D8`;
- assumptions `A1–A4` as explicit validation obligations;
- risks `K1–K6`; and
- the exact canonical scenario manifest `S1–S33` with the observable behavior written above.

No downstream HLD, LLD, regression strategy, implementation work package, dependency version, or distribution mechanism is approved by freezing this artifact. A material change to scope or scenario behavior reopens this artifact and requires a new freeze.

## Primary standards references

- [MCP 2026-07-28 release](https://blog.modelcontextprotocol.io/posts/2026-07-28/)
- [MCP specification changelog](https://github.com/modelcontextprotocol/modelcontextprotocol/blob/main/docs/specification/2026-07-28/changelog.mdx)
- [Official MCP conformance suite](https://github.com/modelcontextprotocol/conformance)
- [Official MCP Rust SDK](https://github.com/modelcontextprotocol/rust-sdk)
- [MCP Apps overview](https://modelcontextprotocol.io/extensions/apps/overview)
- [MCP Tasks specification](https://tasks.extensions.modelcontextprotocol.io/specification/draft/tasks)
- [ACP v1 transports](https://agentclientprotocol.com/protocol/v1/transports)
- [ACP v1 extensibility](https://agentclientprotocol.com/protocol/v1/extensibility)
- [Language Server Protocol specification](https://microsoft.github.io/language-server-protocol/)
