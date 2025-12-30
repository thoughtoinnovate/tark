# Requirements Document

## Introduction

This feature adds automatic chat session restoration and session management to tark's Neovim plugin. When users reopen Neovim and open the tark chat, their previous conversation will be automatically restored. Users can also switch between multiple sessions, view session history, and manage sessions through slash commands.

## Glossary

- **Chat_Session**: A persistent conversation context containing messages, provider settings, mode, and token usage stored in `.tark/sessions/`
- **Session_Manager**: The Lua module responsible for session lifecycle operations (load, save, switch, delete)
- **Current_Session**: The active session pointer stored in `.tark/sessions/current`
- **Session_Picker**: A floating window UI for browsing and selecting sessions
- **Auto_Restore**: The behavior of automatically loading the previous session when chat opens
- **Workspace**: The current working directory (cwd) where Neovim is opened, determining which `.tark/` folder is used

## Requirements

### Requirement 1: Automatic Session Restoration

**User Story:** As a developer, I want my previous chat session to be automatically restored when I reopen tark chat, so that I can continue my conversation without losing context.

#### Acceptance Criteria

1. WHEN the chat window opens AND a Current_Session exists for the current Workspace, THE Session_Manager SHALL automatically load and display the previous conversation
2. WHEN the chat window opens AND no Current_Session exists for the current Workspace, THE Session_Manager SHALL create a new empty session
3. WHEN auto-restore is disabled in config, THE Chat_Session SHALL start fresh without loading history
4. WHEN a session is restored, THE Chat_Session SHALL restore provider, model, mode, and window settings
5. WHEN a session is restored, THE Chat_Session SHALL display all previous messages in the chat buffer
6. WHEN a session is restored, THE Chat_Session SHALL restore token usage statistics for accurate context tracking

### Requirement 2: Workspace-Scoped Sessions

**User Story:** As a developer, I want sessions to be isolated per workspace/project folder, so that different projects have separate conversation histories.

#### Acceptance Criteria

1. THE Session_Manager SHALL only load sessions from the current Workspace's `.tark/sessions/` directory
2. THE Session_Manager SHALL only display sessions belonging to the current Workspace in the Session_Picker
3. WHEN switching Workspaces (changing cwd), THE Session_Manager SHALL load sessions from the new Workspace
4. THE Session_Manager SHALL NOT access or display sessions from other Workspaces

### Requirement 3: Session Switching

**User Story:** As a developer, I want to switch between different chat sessions, so that I can maintain separate conversations for different tasks.

#### Acceptance Criteria

1. WHEN a user executes `/sessions` command, THE Session_Picker SHALL display a list of available sessions for the current Workspace
2. WHEN displaying sessions, THE Session_Picker SHALL show session name, date, message count, and provider
3. WHEN a user selects a session from the picker, THE Session_Manager SHALL switch to that session and restore its state
4. WHEN switching sessions, THE Session_Manager SHALL save the current session before loading the new one
5. WHEN the current session is selected, THE Session_Picker SHALL indicate it is already active

### Requirement 4: Session Creation

**User Story:** As a developer, I want to create new chat sessions, so that I can start fresh conversations without losing previous ones.

#### Acceptance Criteria

1. WHEN a user executes `/new` command, THE Session_Manager SHALL create a new empty session in the current Workspace
2. WHEN a new session is created, THE Session_Manager SHALL save the current session first
3. WHEN a new session is created, THE Session_Manager SHALL clear the chat buffer and reset statistics
4. WHEN a new session is created, THE Chat_Session SHALL retain current provider and mode settings

### Requirement 5: Session Deletion

**User Story:** As a developer, I want to delete old chat sessions, so that I can manage storage and remove irrelevant conversations.

#### Acceptance Criteria

1. WHEN a user executes `/delete` command with no argument, THE Session_Picker SHALL display sessions for deletion selection
2. WHEN a user confirms deletion, THE Session_Manager SHALL remove the session file from storage
3. IF the deleted session is the Current_Session, THEN THE Session_Manager SHALL switch to the most recent remaining session
4. IF no sessions remain after deletion, THEN THE Session_Manager SHALL create a new empty session

### Requirement 6: Session Indicator

**User Story:** As a developer, I want to see which session is currently active, so that I know which conversation context I'm working in.

#### Acceptance Criteria

1. WHILE a session is active, THE Chat_Session SHALL display the session name in the window title or status area
2. WHEN the session name is too long, THE Chat_Session SHALL truncate it with ellipsis
3. WHEN hovering or focusing the session indicator, THE Chat_Session SHALL show full session details

### Requirement 7: Configuration Options

**User Story:** As a developer, I want to configure session restore behavior, so that I can customize the experience to my workflow.

#### Acceptance Criteria

1. THE Chat_Session SHALL provide an `auto_restore` config option (default: true)
2. THE Chat_Session SHALL provide a `max_sessions` config option to limit stored sessions per Workspace
3. WHEN `max_sessions` is exceeded, THE Session_Manager SHALL delete the oldest sessions automatically
4. THE Chat_Session SHALL provide a `session_save_on_close` config option (default: true)

### Requirement 8: Session Persistence

**User Story:** As a developer, I want my sessions to be saved automatically, so that I don't lose conversation progress.

#### Acceptance Criteria

1. WHEN a chat message is sent or received, THE Session_Manager SHALL save the session to disk
2. WHEN the chat window closes AND `session_save_on_close` is enabled, THE Session_Manager SHALL save the current session
3. WHEN Neovim exits, THE Session_Manager SHALL save the current session
4. IF a save operation fails, THEN THE Session_Manager SHALL notify the user with an error message
