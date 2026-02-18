import { useState } from "react";
import { Terminal } from "@/app/components/Terminal";
import { Sidebar } from "@/app/components/Sidebar";
import { defaultAppConfig } from "@/app/config";

/**
 * ============================================================================
 * RATATUI TUI MAPPING - ROOT APPLICATION COMPONENT
 * ============================================================================
 * 
 * This is the root component that orchestrates the entire application layout.
 * In Ratatui, this would be the main application struct and event loop.
 * 
 * @ratatui-equivalent: main.rs with App struct
 * @ratatui-pattern: Event-driven architecture with terminal backend
 * 
 * RUST STRUCTURE EQUIVALENT:
 * ```rust
 * struct App {
 *     sidebar_collapsed: bool,
 *     terminal_output: Vec<TerminalLine>,
 *     input: String,
 *     // Additional state fields from child components would be hoisted here
 * }
 * ```
 */
function App() {
  /**
   * @ratatui-state: sidebar_collapsed: bool
   * @ratatui-behavior: Controls the sidebar width in layout calculation
   * @ratatui-layout: When true, sidebar gets minimal width (e.g., 3 columns)
   *                  When false, sidebar gets fixed width (e.g., 30 columns)
   */
  const [isSidebarCollapsed, setIsSidebarCollapsed] = useState(false);
  
  /**
   * @ratatui-state: terminal_output: Vec<TerminalLine>
   * @ratatui-behavior: Stores all conversation messages and tool outputs
   * @ratatui-type-definition:
   * ```rust
   * #[derive(Clone, Debug)]
   * enum LineType {
   *     System,
   *     Command,
   *     Output,
   *     Input,
   *     Tool,
   * }
   * 
   * #[derive(Clone, Debug)]
   * struct TerminalLine {
   *     line_type: LineType,
   *     content: String,
   *     meta: Option<String>,
   *     details: Option<String>, // For expandable tool details
   * }
   * ```
   * @ratatui-rendering: Each line type gets different Style configurations
   * @ratatui-scroll: Use ScrollbarState to track position, auto-scroll on new items
   */
  // NOTE: System messages use configurable agent name from AppConfig
  const [terminalOutput, setTerminalOutput] = useState([
    { type: "system", content: `${defaultAppConfig.agentNameShort} Core v${defaultAppConfig.version} initialized` },
    { type: "system", content: "Connected to Engine. Ready for Build task." },
    
    // APPROVAL MODAL - High visibility test
    {
      type: "approval",
      content: "Command requires approval",
      command: "rm -rf node_modules && npm install --force",
      riskLevel: "high",
      meta: "This command will delete the node_modules directory and reinstall dependencies with --force flag.",
      detectedPattern: "rm -rf node_modules*"
    },
    
    { type: "input", content: "Help me refactor the authentication module and add JWT support" },
    
    /**
     * @ratatui-message-type: thinking
     * @ratatui-behavior: Shows agent's internal reasoning process
     * @ratatui-style: Muted gray, italic, dashed border, brain icon
     * @ratatui-collapsible: Can be collapsed/expanded like tool details
     */
    { 
      type: "thinking", 
      content: "The user wants to refactor authentication and add JWT. Let me break this down:\n\n1. First, I need to scan the existing auth files to understand the current implementation\n2. Identify what patterns are being used (session-based? token-based?)\n3. Plan the migration path to JWT\n4. Consider security implications: token expiration, refresh tokens, CSRF\n\nI should start by scanning the auth directory to see what we're working with."
    },
    
    { type: "output", content: "I'll analyze your authentication module, identify refactoring opportunities, and integrate JWT support. Let me start by examining the current structure." },
    
    { 
      type: "tool", 
      content: "SCAN: src/auth/ - Analyzing authentication patterns",
      details: "✓ Scanned 3 auth files\n✓ Found legacy session patterns\n✓ Identified refactor opportunities\n✓ Security recommendations:\n  - Add JWT token expiration\n  - Implement refresh token rotation\n  - Add CSRF protection"
    },
    { type: "output", content: "Found 3 auth modules:\n  • src/auth/session.ts (Legacy session-based auth)\n  • src/auth/middleware.ts (Route protection)\n  • src/auth/utils.ts (Helper functions)\n\nKey findings:\n  - No type safety (any types used)\n  - Session tokens stored in memory\n  - Missing CSRF protection" },
    
    { 
      type: "tool", 
      content: "INSTALL: npm install jsonwebtoken@^9.0.0 zod@^3.22.0 --save",
      details: "Resolving dependencies...\nadded 8 packages:\n  • jsonwebtoken@9.0.2\n  • zod@3.22.4\n  • semver@7.5.4\n  • uuid@9.0.1\n\n✓ All dependencies verified\n✓ No vulnerabilities found"
    },
    { type: "output", content: "added 8 packages in 2.3s\n✓ jsonwebtoken installed\n✓ zod installed (runtime type validation)" },
    
    { 
      type: "tool", 
      content: "WRITE: src/auth/jwt.ts - JWT token management",
      details: "Created JWT handler with:\n\n✓ generateToken(userId, expiresIn)\n✓ verifyToken(token)\n✓ refreshToken(refreshToken)\n✓ revokeToken(token)\n\nFeatures:\n- Async/await support\n- Error handling\n- Type-safe with TypeScript\n- Follows JWT best practices"
    },
    { type: "output", content: "export const generateToken = async (userId: string, expiresIn = '24h') => {\n  return jwt.sign({ userId }, process.env.JWT_SECRET!, { expiresIn });\n};\n\nexport const verifyToken = async (token: string) => {\n  return jwt.verify(token, process.env.JWT_SECRET!);\n};" },
    
    { type: "input", content: "Great! Now add rate limiting and ensure the middleware validates tokens properly" },
    
    /**
     * @ratatui-message-type: thinking
     * @ratatui-example: Shows shorter thinking block for quick decisions
     */
    { 
      type: "thinking", 
      content: "Rate limiting + JWT validation requested. I'll need:\n• express-rate-limit package for rate limiting\n• Update middleware to use verifyToken()\n• Add proper error handling for expired/invalid tokens"
    },
    
    { type: "output", content: "Perfect! I'll implement express-rate-limit and update the auth middleware to validate JWT tokens." },
    
    { 
      type: "tool", 
      content: "INSTALL: npm install express-rate-limit@^7.0.0",
      details: "Installing rate limiting middleware...\nadded 3 packages:\n  • express-rate-limit@7.0.0\n  • redis@4.6.10\n  • ioredis@5.3.2\n\n✓ Rate limiting ready\n✓ Redis adapter included"
    },
    { type: "output", content: "added 3 packages in 1.8s\n✓ express-rate-limit ready" },
    
    { type: "system", content: "Migration task completed successfully with JWT & rate limiting implemented" },
    
    /**
     * ============================================================================
     * QUESTION EXAMPLES - FOR AGENT ILLUSTRATION
     * ============================================================================
     * These examples demonstrate different question types the agent can ask users.
     * Each question type has different interaction patterns.
     */
    
    /**
     * @ratatui-question-type: single (Radio buttons - select ONE option)
     * @ratatui-behavior: Only one option can be selected at a time
     * @ratatui-keyboard: Arrow keys to navigate, Space/Enter to select, letter shortcuts
     * @ratatui-icons: ○ (unselected) → ◉ (selected)
     */
    { 
      type: "question", 
      content: "Which database would you like to use for this project?",
      questionType: "single",
      options: [
        { id: "postgres", label: "PostgreSQL - Recommended for complex queries" },
        { id: "mysql", label: "MySQL - Good for web applications" },
        { id: "sqlite", label: "SQLite - Lightweight, file-based" },
        { id: "mongodb", label: "MongoDB - NoSQL document store" },
      ],
      answered: false,
    },
    
    /**
     * @ratatui-question-type: multi (Checkboxes - select MULTIPLE options)
     * @ratatui-behavior: Multiple options can be toggled on/off
     * @ratatui-keyboard: Arrow keys to navigate, Space to toggle, letter shortcuts
     * @ratatui-icons: ☐ (unchecked) → ☑ (checked)
     */
    { 
      type: "question", 
      content: "Select all the features you want to include:",
      questionType: "multi",
      options: [
        { id: "auth", label: "User Authentication (JWT + Sessions)" },
        { id: "api", label: "REST API endpoints" },
        { id: "graphql", label: "GraphQL API" },
        { id: "websocket", label: "WebSocket real-time updates" },
        { id: "caching", label: "Redis caching layer" },
        { id: "logging", label: "Structured logging" },
      ],
      answered: false,
    },
    
    /**
     * @ratatui-question-type: freetext (Text input - open response)
     * @ratatui-behavior: User types free-form text response
     * @ratatui-keyboard: Regular text input, Enter to submit
     * @ratatui-cursor: Show blinking cursor at input position
     */
    { 
      type: "question", 
      content: "What should the main API endpoint prefix be?",
      questionType: "freetext",
      placeholder: "e.g., /api/v1",
      answered: false,
    },
    
    /**
     * Example of an answered question (shows completed state)
     */
    { 
      type: "question", 
      content: "Which package manager do you prefer?",
      questionType: "single",
      options: [
        { id: "npm", label: "npm" },
        { id: "yarn", label: "yarn" },
        { id: "pnpm", label: "pnpm" },
      ],
      answered: true,
      answer: "pnpm",
    },
    
  ]);

  /**
   * ============================================================================
   * SYSTEM MODAL DATA - Provider/Model/File Pickers (NOT terminal output)
   * ============================================================================
   * These are SYSTEM UI MODALS, not questions from the agent.
   * They appear as popup overlays triggered by user actions:
   * 
   * @ratatui-invocation:
   *   - Provider Picker: Click "Claude 3.5 Sonnet" in status bar OR type "/model"
   *   - Model Picker: Shown after provider selection
   *   - File Picker: Type "@" in message input area
   * 
   * @ratatui-pattern:
   * ```rust
   * enum SystemModal {
   *     None,
   *     ProviderPicker,
   *     ModelPicker,  
   *     FilePicker,
   * }
   * 
   * struct AppState {
   *     active_modal: SystemModal,
   *     modal_filter: String,
   *     modal_selected_idx: usize,
   *     // Modal data
   *     providers: Vec<ProviderOption>,
   *     models: Vec<ModelOption>,
   *     files: Vec<FileOption>,
   * }
   * ```
   */
  
  // Provider data for the Provider Picker modal
  const availableProviders = [
    { id: "openai", name: "OpenAI", description: "GPT-4, GPT-4o, and other OpenAI models", icon: "🤖", status: "active" as const },
    { id: "anthropic", name: "Claude", description: "Anthropic's Claude models (Sonnet, Opus, Haiku)", icon: "🎭", status: "warning" as const },
    { id: "github", name: "GitHub Copilot", description: "GPT-4o via GitHub Copilot (Device Flow)", icon: "🐙", status: "warning" as const },
    { id: "google", name: "Google Gemini", description: "Gemini 2.0 models with long context", icon: "💎", status: "warning" as const },
    { id: "openrouter", name: "OpenRouter", description: "Access to 200+ models from various providers", icon: "🌐", status: "warning" as const },
    { id: "ollama", name: "Ollama", description: "Local models via Ollama (CodeLlama, Mistral)", icon: "🦙", status: "warning" as const },
    { id: "gemini-oauth", name: "Gemini (OAuth)", description: "Gemini via Cloud Code Assist API", icon: "🔐", status: "unknown" as const },
  ];

  // Model data for the Model Picker modal
  const availableModels = [
    { id: "codex-mini", name: "Codex Mini", capabilities: ["tools", "reasoning"], isLatest: true },
    { id: "gpt-35-turbo", name: "GPT-3.5-turbo", capabilities: ["text"] },
    { id: "gpt-4", name: "GPT-4", capabilities: ["tools"] },
    { id: "gpt-4-turbo", name: "GPT-4 Turbo", capabilities: ["tools", "vision"] },
    { id: "gpt-41", name: "GPT-4.1", capabilities: ["tools", "vision", "structured"] },
    { id: "gpt-41-mini", name: "GPT-4.1 mini", capabilities: ["tools", "vision", "structured"] },
    { id: "gpt-4o", name: "GPT-4o", capabilities: ["tools", "vision", "structured"] },
    { id: "gpt-5", name: "GPT-5", capabilities: ["tools", "reasoning", "vision", "structured"] },
    { id: "gpt-5-chat", name: "GPT-5 Chat (latest)", capabilities: ["reasoning", "vision", "structured"], isLatest: true },
    { id: "o1", name: "o1", capabilities: ["tools", "reasoning", "vision", "structured"] },
    { id: "o1-mini", name: "o1-mini", capabilities: ["reasoning", "structured"] },
  ];

  // File data for the File Picker modal (@ mentions)
  const availableFiles = [
    { path: "lua/tark/init.lua", name: "lua/tark/init.lua", isFolder: false, indentLevel: 0 },
    { path: "lua/tark/ghost.lua", name: "lua/tark/ghost.lua", isFolder: false, indentLevel: 1 },
    { path: "lua/tark/tui.lua", name: "lua/tark/tui.lua", isFolder: false, indentLevel: 1 },
    { path: "lua/tark/binary.lua", name: "lua/tark/binary.lua", isFolder: false, indentLevel: 1 },
    { path: "lua/tark/health.lua", name: "lua/tark/health.lua", isFolder: false, indentLevel: 1 },
    { path: "lua/tark/statusline.lua", name: "lua/tark/statusline.lua", isFolder: false, indentLevel: 1 },
    { path: "lua/tark/lsp.lua", name: "lua/tark/lsp.lua", isFolder: false, indentLevel: 1 },
    { path: "run_tui.sh", name: "run_tui.sh", isFolder: false, indentLevel: 0 },
  ];

  /**
   * @ratatui-state: input: String
   * @ratatui-behavior: Current input buffer for user typing
   * @ratatui-rendering: Shown in input area with cursor position tracked
   * @ratatui-events: Modified by KeyEvent::Char, KeyEvent::Backspace, etc.
   */
  const [input, setInput] = useState("");

  /**
   * @ratatui-event-handler: on_submit / handle_enter_key
   * @ratatui-trigger: KeyCode::Enter when input is focused
   * @ratatui-behavior: 
   *   1. Validates input is not empty (trim whitespace)
   *   2. Appends new TerminalLine with type=Input to terminal_output Vec
   *   3. Clears the input buffer
   *   4. Auto-scrolls to bottom (scrollbar_state.last())
   * @ratatui-pattern:
   * ```rust
   * fn handle_submit(&mut self) {
   *     if !self.input.trim().is_empty() {
   *         self.terminal_output.push(TerminalLine {
   *             line_type: LineType::Input,
   *             content: self.input.clone(),
   *             meta: None,
   *             details: None,
   *         });
   *         self.input.clear();
   *         self.scroll_state.last(); // Auto-scroll to bottom
   *     }
   * }
   * ```
   */
  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (input.trim()) {
      setTerminalOutput([
        ...terminalOutput,
        { type: "input", content: input },
      ]);
      setInput("");
    }
  };

  /**
   * ============================================================================
   * ROOT LAYOUT STRUCTURE
   * ============================================================================
   * 
   * @ratatui-layout: Horizontal split using Layout::horizontal()
   * @ratatui-constraints: [Constraint::Percentage(70), Constraint::Percentage(30)]
   *                       When sidebar collapsed: [Constraint::Min(0), Constraint::Length(3)]
   * @ratatui-style: Background color: Color::Rgb(13, 17, 23) [#0d1117]
   *                 Foreground: Color::Rgb(156, 163, 175) [text-gray-100]
   * 
   * LAYOUT STRUCTURE:
   * ```
   * ┌─────────────────────────────────────────────────────────────┐
   * │                                                             │
   * │  Terminal Component          │  Sidebar Component           │
   * │  (70% width)                 │  (30% width or collapsed)    │
   * │                                                             │
   * └─────────────────────────────────────────────────────────────┘
   * ```
   * 
   * @ratatui-rendering-code:
   * ```rust
   * fn render(&mut self, frame: &mut Frame) {
   *     let sidebar_width = if self.sidebar_collapsed {
   *         Constraint::Length(3)
   *     } else {
   *         Constraint::Percentage(30)
   *     };
   *     
   *     let chunks = Layout::horizontal([
   *         Constraint::Min(0),      // Terminal takes remaining space
   *         sidebar_width,           // Sidebar fixed or collapsed
   *     ]).split(frame.size());
   *     
   *     self.render_terminal(frame, chunks[0]);
   *     self.render_sidebar(frame, chunks[1]);
   * }
   * ```
   * 
   * @ratatui-component-props:
   * - Props are passed via the App struct fields (no prop drilling in Rust)
   * - Callbacks become methods on the App struct
   * - State is managed centrally in the App struct
   */
  return (
    <div className="flex h-screen bg-[#0d1117] text-gray-100 font-mono overflow-hidden">
      {/* 
        @ratatui-widget: Terminal (Custom widget or composite of Paragraph/List/Block)
        @ratatui-props: Access via &self in App struct
        @ratatui-data-flow: terminal_output, input, sidebar_collapsed
      */}
      <Terminal
        output={terminalOutput}
        input={input}
        onInputChange={setInput}
        onSubmit={handleSubmit}
        isSidebarCollapsed={isSidebarCollapsed}
      />
      {/* 
        @ratatui-widget: Sidebar (Custom widget with nested Lists and Paragraphs)
        @ratatui-props: Access via &self in App struct
        @ratatui-callback: setIsSidebarCollapsed becomes App::toggle_sidebar_collapsed()
      */}
      <Sidebar 
        isCollapsed={isSidebarCollapsed}
        onCollapsedChange={setIsSidebarCollapsed}
      />
    </div>
  );
}

/**
 * ============================================================================
 * RATATUI EVENT LOOP PATTERN
 * ============================================================================
 * 
 * The React component lifecycle and state updates map to an event loop in Ratatui:
 * 
 * ```rust
 * fn main() -> Result<()> {
 *     let mut terminal = setup_terminal()?;
 *     let mut app = App::new();
 *     
 *     loop {
 *         terminal.draw(|frame| app.render(frame))?;
 *         
 *         if let Event::Key(key) = event::read()? {
 *             match key.code {
 *                 KeyCode::Char('q') => break,
 *                 KeyCode::Enter => app.handle_submit(),
 *                 KeyCode::Char(c) => app.input.push(c),
 *                 KeyCode::Backspace => { app.input.pop(); },
 *                 KeyCode::Tab => app.cycle_focus(),
 *                 _ => {}
 *             }
 *         }
 *     }
 *     
 *     restore_terminal(terminal)?;
 *     Ok(())
 * }
 * ```
 * 
 * @ratatui-key-concepts:
 * 1. Single event loop replaces React's event system
 * 2. State mutations are immediate (no async state updates)
 * 3. Re-render happens explicitly via terminal.draw()
 * 4. Focus management must be manually tracked
 */


export default App;
