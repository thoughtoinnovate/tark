# Implementation Plan: Chat Session Restore

## Overview

This implementation adds session restore and management to tark's Neovim plugin. The work is primarily in Lua, leveraging existing Rust backend APIs. The implementation follows an incremental approach: core module first, then integration, then UI enhancements.

## Tasks

- [x] 1. Create session management module
  - [x] 1.1 Create `lua/tark/session.lua` with module structure and config
    - Define M.config with auto_restore, max_sessions, save_on_close
    - Define M.current_session state variable
    - Add setup() function to merge config
    - _Requirements: 7.1, 7.2, 7.4_

  - [x] 1.2 Implement HTTP API wrapper functions
    - fetch_sessions() - GET /sessions
    - fetch_current() - GET /sessions/current
    - switch_session(id) - POST /sessions/switch
    - create_session() - POST /sessions/new
    - delete_session(id) - POST /sessions/delete
    - Use vim.fn.jobstart for async HTTP calls
    - _Requirements: 2.1, 3.1, 4.1, 5.2_

  - [x] 1.3 Implement restore_to_buffer() function
    - Parse session messages array
    - Render each message using chat's append_message pattern
    - Restore session_stats (input_tokens, output_tokens, total_cost)
    - Update window titles with session info
    - _Requirements: 1.5, 1.6_

  - [x] 1.4 Write property test for session restore round-trip
    - **Property 1: Session Restore Round-Trip**
    - **Validates: Requirements 1.4, 1.5, 1.6**

- [x] 2. Integrate session module with chat
  - [x] 2.1 Add session config options to chat.lua
    - Add session = { auto_restore, max_sessions, save_on_close } to M.config
    - Pass config to session module in setup()
    - _Requirements: 7.1, 7.2, 7.4_

  - [x] 2.2 Implement auto-restore in M.open()
    - Check if auto_restore is enabled
    - Call session.restore_current() after window creation
    - Handle errors gracefully (allow fresh start if restore fails)
    - _Requirements: 1.1, 1.2, 1.3_

  - [x] 2.3 Implement session indicator in window title
    - Modify update_chat_window_title() to include session name
    - Truncate long names with ellipsis (max 20 chars)
    - _Requirements: 6.1, 6.2_

  - [x] 2.4 Write property test for session name truncation
    - **Property 8: Session Name Truncation**
    - **Validates: Requirements 6.2**

- [x] 3. Checkpoint - Ensure core restore works
  - Ensure all tests pass, ask the user if questions arise.

- [x] 4. Implement session picker UI
  - [x] 4.1 Create show_picker() function in session.lua
    - Fetch sessions list from backend
    - Format display: name, date, message count, provider
    - Mark current session with indicator (●)
    - Use vim_select pattern from chat.lua
    - _Requirements: 3.1, 3.2, 3.5_

  - [x] 4.2 Add picker keybindings
    - j/k for navigation
    - Enter to select
    - d to delete (in delete mode)
    - q/Esc to close
    - _Requirements: 3.3, 5.1_

  - [x] 4.3 Write property test for workspace isolation
    - **Property 2: Workspace Isolation**
    - **Validates: Requirements 2.1, 2.2, 2.3**

- [x] 5. Implement slash commands
  - [x] 5.1 Add /sessions command
    - Show session picker in switch mode
    - On select, call switch_session() and restore
    - _Requirements: 3.1, 3.3_

  - [x] 5.2 Add /new command
    - Call create_session() API
    - Clear chat buffer
    - Reset session_stats
    - Retain provider/mode settings
    - _Requirements: 4.1, 4.2, 4.3, 4.4_

  - [x] 5.3 Add /delete command
    - Show session picker in delete mode
    - Confirm before deletion
    - Handle current session deletion (switch to recent)
    - Handle last session deletion (create new)
    - _Requirements: 5.1, 5.2, 5.3, 5.4_

  - [x] 5.4 Write property test for save-before-switch
    - **Property 3: Save-Before-Switch Invariant**
    - **Validates: Requirements 3.4, 4.2**

  - [x] 5.5 Write property test for delete current session fallback
    - **Property 5: Delete Current Session Fallback**
    - **Validates: Requirements 5.3**

- [x] 6. Checkpoint - Ensure slash commands work
  - Ensure all tests pass, ask the user if questions arise.

- [x] 7. Implement auto-save and cleanup
  - [x] 7.1 Add save triggers in chat.lua
    - Save after message sent/received (backend handles this)
    - Save on chat close if save_on_close enabled
    - Save on VimLeavePre autocmd
    - _Requirements: 8.1, 8.2, 8.3_

  - [x] 7.2 Implement max_sessions cleanup
    - Check session count after create_session
    - Delete oldest sessions if over limit
    - _Requirements: 7.3_

  - [x] 7.3 Write property test for max sessions cleanup
    - **Property 6: Max Sessions Cleanup**
    - **Validates: Requirements 7.3**

  - [x] 7.4 Write property test for auto-save on message
    - **Property 7: Auto-Save on Message**
    - **Validates: Requirements 8.1**

- [x] 8. Add error handling and notifications
  - [x] 8.1 Add error notifications for API failures
    - Server not running
    - Session not found
    - Save/delete failures
    - _Requirements: 8.4_

  - [x] 8.2 Add success notifications
    - Session switched
    - Session created
    - Session deleted
    - _Requirements: 3.3, 4.1, 5.2_

- [x] 9. Final checkpoint - Full integration test
  - Ensure all tests pass, ask the user if questions arise.
  - Manual testing checklist:
    - [x] Open chat in new workspace → creates new session
    - [ ] Close and reopen chat → restores previous session
    - [ ] /sessions shows picker with correct sessions
    - [ ] Switch session → history changes
    - [ ] /new creates fresh session
    - [ ] /delete removes session correctly
    - [ ] Session name shows in title

## Notes

- All property-based tests are required for comprehensive coverage
- The Rust backend already has all required APIs - no backend changes needed
- Session data is stored in `.tark/sessions/` per workspace
- Each task references specific requirements for traceability
