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