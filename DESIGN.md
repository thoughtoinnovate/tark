# DESIGN.md

## Introduction and Overview

`tark` is an AI-powered CLI agent integrated with an LSP server designed for code completion, hover, diagnostics, and chat. The project aims to enhance the developer's workflow by providing advanced AI tools directly within the code editor and command-line interface. 

### Key Features
- **Ghost Text Completions**: Offers cursor-style inline completions with the ability to accept suggestions.
- **Chat Agent**: Enables interactive chat with functionality for file operations and shell commands.
- **Multi-Provider Support**: Interfaces with multiple AI models including Claude, OpenAI, and local Ollama models.
- **Usage Dashboard**: Offers real-time tracking of costs, tokens, and sessions via an interactive web dashboard.

## Architectural Diagram

```plaintext
┌─────────────────────────────────────────┐
│              Neovim                      │
│  ┌─────────────┐  ┌─────────────────┐   │
│  │ Ghost Text  │  │      Chat       │   │
│  │   (Tab)     │  │   (<leader>ec)  │   │
│  └──────┬──────┘  └────────┬────────┘   │
└─────────┼──────────────────┼────────────┘
          │                  │
          └────────┬─────────┘
                   ▼
       ┌───────────────────────┐
       │    tark serve         │
       │    (HTTP :8765)       │
       ├───────────────────────┤
       │  ┌─────┐ ┌─────────┐  │
       │  │ FIM │ │  Agent  │  │
       │  │     │ │ + Tools │  │
       │  └──┬──┘ └────┬────┘  │
       └─────┼─────────┼───────┘
             │         │
     ┌───────┴─────────┴───────┐
     │      LLM Providers       │
     │  OpenAI │ Claude │ Ollama│
     └─────────────────────────┘
```

## Components Details

### CLI Agent
- Provides command-line access to AI-powered features and integrates seamlessly with Neovim for an enhanced development experience.

### LSP Server
- Offers code navigation features such as go-to definition, find references, and call hierarchy, enhancing the code comprehension process.

### TUI Interface
- A new Rust-based text user interface that replaces the original Lua chat, bringing better performance and features like image and file attachments.

### Neovim Plugin Integration
- Provides a smooth in-editor experience, enabling features without additional configuration and seamlessly starting required services.

## Feature Description

- **Ghost Text Completions**: Facilitates code completion within the editor, allowing quick insertion of AI-suggested code lines.
- **Interactive Chat Agent**: Offers a dialogue interface for executing commands and interacting with the codebase.
- **Multi-Model Support**: Enables using various AI models, allowing flexibility and choice for users' preference or costs.
  
## Installation and Configuration
- **Installation Methods**: Available via Docker, direct binary installation, or building from source.
- **Configuration Options**: Highly configurable to match user preferences, supports setting API keys, server settings, and mode selection.

## Security Considerations

- **Binary Verification**: Each binary release is accompanied by SHA256 checksums for integrity verification.
- **Privacy Practices**: API keys are securely handled, and no telemetry data is collected, safeguarding user privacy.