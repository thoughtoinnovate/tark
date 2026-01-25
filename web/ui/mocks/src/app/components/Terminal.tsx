/**
 * ============================================================================
 * RATATUI TUI MAPPING - TERMINAL COMPONENT
 * ============================================================================
 * 
 * This is the main terminal interface component showing conversation history,
 * status bar, mode selectors, and input area. In Ratatui, this would be a
 * composite widget combining multiple sub-widgets.
 * 
 * @ratatui-structure: Custom Terminal widget using Layout to split areas
 * @ratatui-crates-needed:
 *   - ratatui (core TUI framework)
 *   - crossterm (terminal control and events)
 *   - arboard or clipboard (for copy functionality)
 *   - unicode-width (for proper text width calculations)
 */
import React, { useState, useEffect, useRef } from 'react';
import { defaultAppConfig } from '../config';
import { 
  Terminal as TerminalIcon, 
  Circle, 
  Plus, 
  ChevronUp,
  ChevronDown,
  ChevronRight,
  Wrench,
  Copy,
  Check,
  User,
  Bot,
  Shield,
  Zap,
  Gauge,
  X,
  FileCode,
  Brain,  // Simple brain icon for thinking blocks
  CircleDot,     // For selected radio button
  Square,        // For unchecked checkbox
  CheckSquare,   // For checked checkbox
  MessageCircleQuestion,  // For question icon
  Send,          // For submit button
  AlertTriangle, // For warning status
  HelpCircle,    // For unknown status
  Folder,        // For folder icon
  File,          // For file icon
  Search,        // For filter input
  Server,        // For provider icon
  Cpu,           // For model icon
  FolderOpen,    // For open folder
  Palette,       // For theme picker
  Moon,          // For dark theme indicator
  ListTodo,      // For task queue indicator
  Sun,           // For light theme indicator
  ShieldCheck,   // For approval modal icon
  Play,          // For "run once" option
  ShieldPlus,    // For "always allow" option
  Asterisk,      // For "pattern match" option
  Ban,           // For "skip" option
  Clock          // For "run once" alternative icon
} from 'lucide-react';
import { themePresets, applyTheme, loadSavedTheme, type ThemePreset } from '../themes/presets';
import { FlashBar, type FlashBarState } from './FlashBar';

/**
 * ============================================================================
 * TYPE DEFINITIONS - RATATUI RUST EQUIVALENTS
 * ============================================================================
 */

/**
 * @ratatui-enum: LineType
 * ```rust
 * #[derive(Clone, Copy, Debug, PartialEq, Eq)]
 * enum LineType {
 *     System,
 *     Command,
 *     Output,
 *     Input,
 *     Tool,
 *     Thinking,       // Agent's internal reasoning process
 *     Question,       // Agent asking user a question (various types)
 *     Approval,       // Agent requesting approval for a command
 * }
 * ```
 */
type LineType = 'system' | 'command' | 'output' | 'input' | 'tool' | 'thinking' | 'question' | 'approval';

/**
 * @ratatui-enum: QuestionType
 * @ratatui-display: Agent questions render as POPUP/MODAL overlays (centered floating windows)
 * @ratatui-behavior: Different input modes for agent questions ONLY
 * @ratatui-note: Provider/Model/File pickers are SYSTEM MODALS, not questions!
 * ```rust
 * // AGENT QUESTIONS - Asked by the agent during conversation
 * #[derive(Clone, Copy, Debug, PartialEq, Eq)]
 * enum QuestionType {
 *     FreeText,       // Open text input
 *     SingleChoice,   // Radio buttons - select one
 *     MultiChoice,    // Checkboxes - select multiple
 * }
 * ```
 */
type QuestionType = 'freetext' | 'single' | 'multi';

/**
 * ============================================================================
 * SYSTEM MODALS - NOT QUESTIONS, triggered by user actions
 * ============================================================================
 * 
 * @ratatui-enum: SystemModal
 * @ratatui-behavior: UI overlays triggered by user interaction, NOT agent questions
 * @ratatui-pattern:
 * ```rust
 * // SYSTEM MODALS - Invoked by user actions
 * #[derive(Clone, Copy, Debug, PartialEq, Eq)]
 * enum SystemModal {
 *     None,
 *     ProviderPicker,  // Triggered: Click model selector OR type "/model"
 *     ModelPicker,     // Triggered: After provider OR click model name
 *     FilePicker,      // Triggered: Type "@" in input area
 * }
 * 
 * struct AppState {
 *     active_modal: SystemModal,
 *     // ... other state
 * }
 * ```
 * 
 * INVOCATION:
 * - Provider Picker: Click "Claude 3.5 Sonnet ANTHROPIC" in status bar OR type "/model"
 * - Model Picker: Shown after provider selection OR when clicking model name
 * - File Picker: Type "@" character in message input area
 * - Theme Picker: Type "/theme" command in input area
 * - Help/Shortcuts: Click "?" button in status bar OR type "/help"
 */
type SystemModalType = 'provider' | 'model' | 'file' | 'theme' | 'help' | null;

/**
 * @ratatui-enum: ProviderStatus
 * @ratatui-icons: 
 *   - active: ● (green) - fully working
 *   - warning: ⚠ (yellow) - needs attention
 *   - error: ✖ (red) - not working
 *   - unknown: ? (gray) - status unknown
 */
type ProviderStatus = 'active' | 'warning' | 'error' | 'unknown';

/**
 * @ratatui-struct: ProviderOption
 * @ratatui-invocation: Triggered from status bar LLM selector (right side)
 * @ratatui-purpose: Select which LLM provider to use for the agent
 * @ratatui-trigger: Clicking on current provider name in status bar
 * ```rust
 * struct ProviderOption {
 *     id: String,
 *     name: String,
 *     description: String,
 *     icon: String,      // Provider icon/emoji
 *     status: ProviderStatus,
 * }
 * ```
 */
interface ProviderOption {
  id: string;
  name: string;
  description: string;
  icon?: string;
  status?: ProviderStatus;
}

/**
 * @ratatui-struct: ModelOption
 * @ratatui-invocation: Triggered from status bar after provider is selected
 * @ratatui-purpose: Select which model to use from the chosen provider
 * @ratatui-trigger: Clicking on model name or after provider selection
 * ```rust
 * struct ModelOption {
 *     id: String,
 *     name: String,
 *     capabilities: Vec<String>,  // tools, reasoning, vision, structured
 *     is_latest: bool,
 * }
 * ```
 */
interface ModelOption {
  id: string;
  name: string;
  capabilities?: string[];
  isLatest?: boolean;
}

/**
 * @ratatui-struct: FileOption
 * @ratatui-invocation: Triggered when user types "@" in input area
 * @ratatui-purpose: Provides context files to the agent (not for editing)
 * @ratatui-pattern: Like Cursor's @file mention feature
 * ```rust
 * struct FileOption {
 *     path: String,
 *     name: String,
 *     is_folder: bool,
 *     indent_level: usize,
 * }
 * ```
 */
interface FileOption {
  path: string;
  name: string;
  isFolder?: boolean;
  indentLevel?: number;
}

/**
 * @ratatui-struct: QuestionOption
 * ```rust
 * #[derive(Clone, Debug)]
 * struct QuestionOption {
 *     id: String,
 *     label: String,
 *     selected: bool,
 * }
 * ```
 */
interface QuestionOption {
  id: string;
  label: string;
  selected?: boolean;
}

/**
 * @ratatui-enum: AgentMode
 * @ratatui-rendering: Different colors for each mode (amber/blue/purple)
 * @ratatui-ui-pattern: Radio button selector shown in status bar
 * ```rust
 * #[derive(Clone, Copy, Debug, PartialEq, Eq)]
 * enum AgentMode {
 *     Build,  // Amber color
 *     Plan,   // Blue color
 *     Ask,    // Purple color
 * }
 * 
 * impl AgentMode {
 *     fn color(&self) -> Color {
 *         match self {
 *             Self::Build => Color::Rgb(251, 191, 36),  // text-amber-400
 *             Self::Plan => Color::Rgb(96, 165, 250),   // text-blue-400
 *             Self::Ask => Color::Rgb(192, 132, 252),   // text-purple-400
 *         }
 *     }
 * }
 * ```
 */
type AgentMode = 'Build' | 'Plan' | 'Ask';

/**
 * @ratatui-enum: BuildMode
 * @ratatui-rendering: Only shown when AgentMode is Build
 * @ratatui-keyboard-shortcuts: Cmd+1/2/3 to switch modes
 * @ratatui-ui-pattern: Dropdown menu with colored badges
 * ```rust
 * #[derive(Clone, Copy, Debug, PartialEq, Eq)]
 * enum BuildMode {
 *     Careful,   // Red theme - Shield icon
 *     Manual,    // Amber theme - Zap icon
 *     Balanced,  // Emerald theme - Gauge icon
 * }
 * 
 * impl BuildMode {
 *     fn color(&self) -> Color {
 *         match self {
 *             Self::Careful => Color::Rgb(248, 113, 113),  // text-red-400
 *             Self::Manual => Color::Rgb(251, 191, 36),    // text-amber-400
 *             Self::Balanced => Color::Rgb(52, 211, 153), // text-emerald-400
 *         }
 *     }
 *     
 *     fn icon(&self) -> &'static str {
 *         match self {
 *             Self::Careful => "🛡",
 *             Self::Manual => "⚡",
 *             Self::Balanced => "⚖",
 *         }
 *     }
 * }
 * ```
 */
type BuildMode = 'Careful' | 'Manual' | 'Balanced';

/**
 * @ratatui-struct: ContextFile
 * @ratatui-rendering: Displayed as removable pills/badges above input area
 * ```rust
 * #[derive(Clone, Debug)]
 * struct ContextFile {
 *     name: String,
 *     path: Option<String>,
 * }
 * ```
 */
interface ContextFile {
  name: string;
  path?: string;
}

/**
 * @ratatui-struct: TerminalLine
 * @ratatui-rendering: Each line renders differently based on type
 * @ratatui-storage: Stored in Vec<TerminalLine> in main App state
 * @ratatui-behavior: 
 *   - system: Cyan background with dot icon
 *   - input: User message with user icon
 *   - output: Bot message with bot icon
 *   - tool: Expandable with details field
 *   - thinking: Agent's internal reasoning
 *   - question: Interactive question with options
 *   - command: Shell command history style
 * ```rust
 * #[derive(Clone, Debug)]
 * struct TerminalLine {
 *     line_type: LineType,
 *     content: String,
 *     meta: Option<String>,
 *     details: Option<String>,
 *     // Question-specific fields
 *     question_type: Option<QuestionType>,
 *     options: Option<Vec<QuestionOption>>,
 *     placeholder: Option<String>,
 *     answered: bool,
 *     answer: Option<String>,
 * }
 * ```
 */
interface TerminalLine {
  type: LineType;
  content: string;
  meta?: string;
  details?: string; // For tool invocations with expandable details
  // Question-specific fields (for AGENT questions only)
  questionType?: QuestionType;
  options?: QuestionOption[];
  placeholder?: string;
  answered?: boolean;
  answer?: string;
  // Approval-specific fields
  command?: string;         // The command being requested for approval
  riskLevel?: 'low' | 'medium' | 'high';  // Risk level of the command
  approvalResponse?: 'run_once' | 'always_allow' | 'pattern' | 'skip';  // User's decision
  detectedPattern?: string; // Detected wildcard pattern for pattern matching
}

/**
 * @ratatui-struct: SystemModalData
 * @ratatui-behavior: Data for system modals (Provider/Model/File pickers)
 * @ratatui-note: These are NOT part of terminal output - they are overlay modals
 */
interface SystemModalData {
  providers: ProviderOption[];
  models: ModelOption[];
  files: FileOption[];
}

/**
 * @ratatui-note: Props in React become fields in the App struct in Ratatui
 * @ratatui-pattern: No prop passing - all state managed in single App struct
 * 
 * In Ratatui, these "props" would be accessed as:
 * ```rust
 * struct App {
 *     terminal_output: Vec<TerminalLine>,
 *     input: String,
 *     llm_model: String,
 *     llm_provider: String,
 *     connection_status: ConnectionStatus,
 *     sidebar_collapsed: bool,
 *     // ... other state fields
 * }
 * ```
 */
interface TerminalProps {
  output: TerminalLine[];
  input: string;
  onInputChange: (value: string) => void;
  onSubmit: (e: React.FormEvent) => void;
  onQuestionAnswer?: (questionIndex: number, answer: string, selectedOptions?: string[]) => void;
  llmModel?: string;
  llmProvider?: string;
  connectionStatus?: 'active' | 'error';
  isSidebarCollapsed?: boolean;
}

/**
 * ============================================================================
 * TERMINAL COMPONENT - MAIN RENDER LOGIC
 * ============================================================================
 * 
 * @ratatui-layout: Vertical split into 4 main sections:
 *   1. Header (fixed height ~3 lines)
 *   2. Output area (flexible, scrollable)
 *   3. Status bar (fixed height ~2 lines)
 *   4. Input area (fixed height ~3-5 lines depending on context files)
 * 
 * @ratatui-structure:
 * ```rust
 * fn render_terminal(&mut self, frame: &mut Frame, area: Rect) {
 *     let chunks = Layout::vertical([
 *         Constraint::Length(3),      // Header
 *         Constraint::Min(0),         // Output (takes remaining space)
 *         Constraint::Length(2),      // Status bar
 *         Constraint::Length(if self.context_files.is_empty() { 3 } else { 5 }),
 *     ]).split(area);
 *     
 *     self.render_header(frame, chunks[0]);
 *     self.render_output(frame, chunks[1]);
 *     self.render_status_bar(frame, chunks[2]);
 *     self.render_input(frame, chunks[3]);
 * }
 * ```
 */
export function Terminal({ 
  output, 
  input, 
  onInputChange, 
  onSubmit,
  onQuestionAnswer,
  llmModel = "Claude 3.5 Sonnet",
  llmProvider = "Anthropic",
  connectionStatus = 'active',
  isSidebarCollapsed = false
}: TerminalProps) {
  
  /**
   * ============================================================================
   * COMPONENT STATE - RATATUI APP STRUCT FIELDS
   * ============================================================================
   */
  
  /**
   * @ratatui-state: mode: AgentMode
   * @ratatui-default: AgentMode::Build
   * @ratatui-ui: Dropdown selector in status bar
   * @ratatui-keyboard: Tab or Ctrl+M to cycle through modes
   */
  const [mode, setMode] = useState<AgentMode>('Build');
  
  /**
   * @ratatui-state: mode_selector_open: bool
   * @ratatui-behavior: Controls popup menu visibility for agent mode selection
   * @ratatui-rendering: When true, render popup List widget over status bar
   * @ratatui-keyboard: Esc to close, Arrow keys to navigate, Enter to select
   */
  const [isModeSelectorOpen, setIsModeSelectorOpen] = useState(false);
  
  /**
   * @ratatui-state: build_mode_selector_open: bool
   * @ratatui-behavior: Controls build mode dropdown visibility
   * @ratatui-condition: Only rendered when mode == AgentMode::Build
   * @ratatui-keyboard: Cmd+1/2/3 shortcuts to directly select modes
   */
  const [isBuildModeSelectorOpen, setIsBuildModeSelectorOpen] = useState(false);
  
  /**
   * @ratatui-state: current_model: String
   * @ratatui-behavior: Currently selected LLM model name
   * @ratatui-ui: Displayed in status bar, changed via model picker modal
   */
  const [currentModel, setCurrentModel] = useState(llmModel);
  
  /**
   * @ratatui-state: current_provider: String
   * @ratatui-behavior: Currently selected LLM provider name
   * @ratatui-ui: Displayed in status bar, changed via provider picker modal
   */
  const [currentProvider, setCurrentProvider] = useState(llmProvider);
  
  /**
   * @ratatui-state: copied_index: Option<String>
   * @ratatui-behavior: Tracks which message was recently copied (for visual feedback)
   * @ratatui-timer: Auto-clears after 2 seconds
   * @ratatui-alternative: In TUI, show temporary "Copied!" message or status bar notification
   * @ratatui-note: Hover-based UI doesn't translate; use key binding (e.g., 'c' to copy focused item)
   */
  const [copiedIndex, setCopiedIndex] = useState<string | null>(null);
  
  /**
   * @ratatui-state: expanded_tool_index: Option<usize>
   * @ratatui-behavior: Tracks which tool invocation has details expanded
   * @ratatui-rendering: When Some(index), render details for that tool line
   * @ratatui-keyboard: Enter or Space on tool line to toggle
   */
  const [expandedToolIndex, setExpandedToolIndex] = useState<number | null>(null);
  
  /**
   * @ratatui-state: build_mode: BuildMode
   * @ratatui-default: BuildMode::Balanced
   * @ratatui-rendering: Shown as colored badge in status bar
   * @ratatui-keyboard: Cmd+1=Careful, Cmd+2=Manual, Cmd+3=Balanced
   */
  const [buildMode, setBuildMode] = useState<BuildMode>('Balanced');
  
  /**
   * @ratatui-state: context_files: Vec<ContextFile>
   * @ratatui-rendering: Horizontal list of removable pills above input
   * @ratatui-keyboard: 'x' or Delete to remove focused file
   * @ratatui-behavior: Files can be added via file picker (TUI: text input of paths)
   */
  const [addedContextFiles, setAddedContextFiles] = useState<ContextFile[]>([]);
  
  /**
   * @ratatui-state: is_focused: bool
   * @ratatui-behavior: Tracks if terminal area has focus (vs sidebar)
   * @ratatui-rendering: Changes scrollbar visibility and input cursor
   * @ratatui-keyboard: Tab to switch focus between terminal and sidebar
   */
  const [isFocused, setIsFocused] = useState(false);
  
  /**
   * @ratatui-state: thinking_enabled: bool
   * @ratatui-rendering: Toggle button in status bar with Sparkles icon
   * @ratatui-behavior: Feature flag for AI thinking mode
   * @ratatui-keyboard: Ctrl+T to toggle
   */
  const [thinkingEnabled, setThinkingEnabled] = useState(true);
  
  /**
   * @ratatui-state: task_queue_count: usize
   * @ratatui-behavior: Shows number of tasks waiting in queue
   * @ratatui-ui: Queue icon with count badge next to brain icon
   */
  const [taskQueueCount, setTaskQueueCount] = useState(7); // Demo: 7 tasks in queue
  
  /**
   * @ratatui-state: is_agent_working: bool
   * @ratatui-behavior: Shows blinking green dot when agent is processing a request
   * @ratatui-ui: Blinking green dot in center of status bar
   */
  const [isAgentWorking, setIsAgentWorking] = useState(true); // Demo: set to true to show indicator
  
  /**
   * @ratatui-state: flash_bar_state: FlashBarState
   * @ratatui-behavior: Controls the Flash Bar display state
   * @ratatui-states: idle, working, rate-limit, error, warning
   */
  const [flashBarState, setFlashBarState] = useState<FlashBarState>('working');
  const [flashBarMessage, setFlashBarMessage] = useState<string | undefined>(undefined);
  
  /**
   * @web-only: Demo control visibility - NOT part of TUI implementation
   * This is only for web mockup testing purposes
   */
  const [showFlashBarDemo, setShowFlashBarDemo] = useState(true);
  
  /**
   * @ratatui-state: question_responses: HashMap<usize, QuestionResponse>
   * @ratatui-behavior: Tracks user responses to agent questions
   * @ratatui-pattern:
   * ```rust
   * struct QuestionResponse {
   *     text_input: String,           // For freetext
   *     selected_options: Vec<String>, // For single/multi choice
   *     filter_text: String,          // For picker filter
   * }
   * ```
   */
  const [questionResponses, setQuestionResponses] = useState<{[key: number]: {
    textInput: string;
    selectedOptions: string[];
    filterText?: string;
  }}>({});
  
  // Local state to track answered questions (for demo purposes)
  const [answeredQuestions, setAnsweredQuestions] = useState<{[key: number]: {
    answer: string;
    selectedLabels?: string[];
  }}>({});
  
  /**
   * @ratatui-handler: Handle question submission
   * @ratatui-pattern: In Ratatui, this would send the answer to the agent
   * and update the UI to show the answered state
   */
  const handleQuestionSubmit = (index: number, line: TerminalLine) => {
    const response = questionResponses[index];
    if (!response) return;
    
    let answerText = '';
    let selectedLabels: string[] = [];
    
    if (line.questionType === 'freetext') {
      answerText = response.textInput || '';
    } else if (line.questionType === 'single' && line.options) {
      const selectedOption = line.options.find(o => response.selectedOptions?.includes(o.id));
      answerText = selectedOption?.label || '';
      selectedLabels = [answerText];
    } else if (line.questionType === 'multi' && line.options) {
      selectedLabels = line.options
        .filter(o => response.selectedOptions?.includes(o.id))
        .map(o => o.label);
      answerText = selectedLabels.join(', ');
    }
    
    if (!answerText) return;
    
    setAnsweredQuestions(prev => ({
      ...prev,
      [index]: { answer: answerText, selectedLabels }
    }));
    
    // Call parent callback if provided
    if (onQuestionAnswer) {
      onQuestionAnswer(index, answerText, response.selectedOptions);
    }
  };

  /**
   * ============================================================================
   * SYSTEM MODAL STATE - For Provider/Model/File Pickers
   * ============================================================================
   * @ratatui-state: active_system_modal: Option<SystemModal>
   * @ratatui-behavior: Controls which system modal overlay is displayed
   * @ratatui-invocation:
   *   - Provider: Click LLM button in status bar OR type "/model" in input
   *   - Model: After provider selection OR direct click on model name
   *   - File: Type "@" character in message input
   * @ratatui-pattern:
   * ```rust
   * struct AppState {
   *     active_system_modal: Option<SystemModal>,
   *     modal_filter_text: String,
   *     modal_selected_index: usize,
   * }
   * 
   * // Handle in main event loop:
   * fn handle_input(&mut self, input: &str) {
   *     if input.starts_with('@') {
   *         self.active_system_modal = Some(SystemModal::FilePicker);
   *     } else if input == "/model" {
   *         self.active_system_modal = Some(SystemModal::ProviderPicker);
   *     }
   * }
   * ```
   */
  const [activeSystemModal, setActiveSystemModal] = useState<SystemModalType>(null);
  const [modalFilterText, setModalFilterText] = useState('');
  const [modalSelectedIndex, setModalSelectedIndex] = useState(0);
  const [pendingProvider, setPendingProvider] = useState<{id: string; name: string} | null>(null);
  
  /**
   * @ratatui-state: currentTheme -> ThemePreset
   * @ratatui-pattern: Load saved theme from config on startup
   * @ratatui-trigger: Type "/theme" command or use theme picker modal
   */
  const [currentTheme, setCurrentTheme] = useState<ThemePreset>(() => loadSavedTheme());
  
  /**
   * @ratatui-ref: scrollRef -> ScrollbarState
   * @ratatui-pattern: Use ratatui::widgets::ScrollbarState to track position
   * @ratatui-behavior: Auto-scrolls to bottom on new messages
   * ```rust
   * struct TerminalState {
   *     scroll_state: ScrollbarState,
   *     scroll_offset: usize,
   * }
   * ```
   */
  const scrollRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  /**
   * @ratatui-effect: Auto-scroll on new output
   * @ratatui-pattern: In event loop, after adding new message:
   * ```rust
   * fn add_terminal_line(&mut self, line: TerminalLine) {
   *     self.terminal_output.push(line);
   *     // Auto-scroll to bottom
   *     self.scroll_state = self.scroll_state.content_length(self.terminal_output.len());
   *     self.scroll_state.last(); // Jump to last item
   * }
   * ```
   */
  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [output]);

  /**
   * @ratatui-handler: handle_copy
   * @ratatui-crate: Use `arboard` or `clipboard` crate
   * @ratatui-keyboard: 'c' key when line is focused/selected
   * @ratatui-behavior:
   *   1. Copy content to system clipboard
   *   2. Show visual feedback (temporary status message or highlight)
   *   3. Auto-clear feedback after 2 seconds
   * ```rust
   * use arboard::Clipboard;
   * 
   * fn handle_copy(&mut self, text: &str, key: String) {
   *     if let Ok(mut clipboard) = Clipboard::new() {
   *         let _ = clipboard.set_text(text);
   *         self.copied_index = Some(key);
   *         // In real impl, use tokio::time::sleep or timer to clear after 2s
   *     }
   * }
   * ```
   */
  const handleCopy = (text: string, key: string) => {
    navigator.clipboard.writeText(text);
    setCopiedIndex(key);
    setTimeout(() => setCopiedIndex(null), 2000);
  };

  /**
   * @ratatui-handler: handle_add_context
   * @ratatui-ui-alternative: In TUI, show text input prompt for file path
   * @ratatui-pattern: Use fuzzy file picker or path input dialog
   * @ratatui-keyboard: '+' key or Ctrl+O to open file selector
   * ```rust
   * fn handle_add_context(&mut self) {
   *     // In TUI: Could use tui-input for path entry
   *     // Or integrate with external file picker like `skim` or `fzf`
   *     self.show_file_picker = true;
   * }
   * ```
   */
  /**
   * @ratatui-handler: handle_add_context
   * @ratatui-behavior: Opens file picker modal (same as typing "@")
   * @ratatui-trigger: Click "+" button OR Ctrl+O keyboard shortcut
   */
  const handleAddContext = () => {
    // In React demo: click hidden file input
    // In Ratatui TUI: set active_system_modal = SystemModal::FilePicker
    setActiveSystemModal('file');
    fileInputRef.current?.click(); // Keep for demo functionality
  };

  /**
   * @ratatui-handler: handle_file_select
   * @ratatui-behavior: Add files to context_files Vec, deduplicating by path
   * @ratatui-pattern: 
   * ```rust
   * fn add_context_files(&mut self, paths: Vec<String>) {
   *     for path in paths {
   *         let file = ContextFile {
   *             name: path.split('/').last().unwrap_or(&path).to_string(),
   *             path: Some(path.clone()),
   *         };
   *         // Deduplicate
   *         if !self.context_files.iter().any(|f| f.path == file.path) {
   *             self.context_files.push(file);
   *         }
   *     }
   * }
   * ```
   */
  const handleFileSelect = (e: React.ChangeEvent<HTMLInputElement>) => {
    const files = e.currentTarget.files;
    if (files) {
      const incoming = Array.from(files).map(f => ({
        name: f.name,
        path: f.webkitRelativePath || f.name,
      }));
      setAddedContextFiles(prev => {
        const map = new Map(prev.map(f => [(f.path ?? f.name), f]));
        incoming.forEach(f => map.set(f.path ?? f.name, f));
        return Array.from(map.values());
      });
    }
    // Reset input so same file can be selected again
    if (fileInputRef.current) {
      fileInputRef.current.value = '';
    }
  };

  /**
   * @ratatui-handler: handle_remove_context
   * @ratatui-keyboard: 'x' or Delete key on focused context file
   * @ratatui-behavior: Remove file from Vec by index
   * ```rust
   * fn remove_context_file(&mut self, index: usize) {
   *     if index < self.context_files.len() {
   *         self.context_files.remove(index);
   *     }
   * }
   * ```
   */
  const handleRemoveContext = (index: number) => {
    setAddedContextFiles(addedContextFiles.filter((_, i) => i !== index));
  };

  /**
   * @ratatui-effect: Handle paste events
   * @ratatui-note: Paste support in TUI is limited
   * @ratatui-alternative: Use bracketed paste mode in terminal
   * @ratatui-pattern: Detect pasted file paths and add to context
   * ```rust
   * // In crossterm, enable bracketed paste:
   * crossterm::execute!(io::stdout(), EnableBracketedPaste)?;
   * 
   * // In event loop:
   * if let Event::Paste(data) = event::read()? {
   *     // Parse data for file paths
   *     let paths: Vec<String> = data.lines()
   *         .filter(|line| std::path::Path::new(line).exists())
   *         .map(String::from)
   *         .collect();
   *     self.add_context_files(paths);
   * }
   * ```
   */
  useEffect(() => {
    const handlePaste = (e: ClipboardEvent) => {
      const items = e.clipboardData?.items;
      if (items) {
        for (let i = 0; i < items.length; i++) {
          if (items[i].kind === 'file') {
            const file = items[i].getAsFile();
            if (file) {
              const incoming: ContextFile = {
                name: file.name,
                path: file.name,
              };
              setAddedContextFiles(prev => {
                const map = new Map(prev.map(f => [(f.path ?? f.name), f]));
                map.set(incoming.path ?? incoming.name, incoming);
                return Array.from(map.values());
              });
            }
          }
        }
      }
    };

    window.addEventListener('paste', handlePaste);
    return () => window.removeEventListener('paste', handlePaste);
  }, []);

  /**
   * @ratatui-effect: Sync context files with @mentions in input
   * @ratatui-behavior: When @filename is removed from input text, remove file from context
   * @ratatui-pattern:
   * ```rust
   * fn sync_context_with_input(&mut self) {
   *     // Parse all @mentions from input
   *     let mentions: HashSet<String> = self.input
   *         .split_whitespace()
   *         .filter(|w| w.starts_with('@'))
   *         .map(|w| w.trim_start_matches('@').to_string())
   *         .collect();
   *     
   *     // Remove context files not mentioned in input
   *     self.context_files.retain(|f| mentions.contains(&f.name));
   * }
   * ```
   */
  useEffect(() => {
    // Extract all @mentions from input
    const mentionRegex = /@(\S+)/g;
    const mentions = new Set<string>();
    let match;
    while ((match = mentionRegex.exec(input)) !== null) {
      mentions.add(match[1]);
    }
    
    // Remove context files that are no longer mentioned in input
    setAddedContextFiles(prev => {
      const filtered = prev.filter(file => mentions.has(file.name));
      // Only update if there's a change (prevent infinite loop)
      if (filtered.length !== prev.length) {
        return filtered;
      }
      return prev;
    });
  }, [input]);

  /**
   * @ratatui-effect: Keyboard shortcuts for build modes
   * @ratatui-keyboard-bindings:
   *   - Cmd+1 or Ctrl+1: Set BuildMode::Careful
   *   - Cmd+2 or Ctrl+2: Set BuildMode::Manual
   *   - Cmd+3 or Ctrl+3: Set BuildMode::Balanced
   * @ratatui-condition: Only active when mode == AgentMode::Build
   * @ratatui-pattern:
   * ```rust
   * fn handle_key_event(&mut self, key: KeyEvent) {
   *     if self.mode != AgentMode::Build {
   *         return;
   *     }
   *     
   *     if key.modifiers.contains(KeyModifiers::CONTROL) {
   *         match key.code {
   *             KeyCode::Char('1') => self.build_mode = BuildMode::Careful,
   *             KeyCode::Char('2') => self.build_mode = BuildMode::Manual,
   *             KeyCode::Char('3') => self.build_mode = BuildMode::Balanced,
   *             _ => {}
   *         }
   *     }
   * }
   * ```
   */
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (mode !== 'Build') return;
      
      if ((e.ctrlKey || e.metaKey) && !e.shiftKey && !e.altKey) {
        switch(e.key) {
          case '1':
            e.preventDefault();
            setBuildMode('Careful');
            break;
          case '2':
            e.preventDefault();
            setBuildMode('Manual');
            break;
          case '3':
            e.preventDefault();
            setBuildMode('Balanced');
            break;
        }
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [mode]);

  /**
   * @ratatui-data: Mode configuration with colors
   * @ratatui-pattern: Use match expression to get color for current mode
   * ```rust
   * const MODES: &[(AgentMode, Color)] = &[
   *     (AgentMode::Build, Color::Rgb(251, 191, 36)),  // amber-400
   *     (AgentMode::Plan, Color::Rgb(96, 165, 250)),   // blue-400
   *     (AgentMode::Ask, Color::Rgb(192, 132, 252)),   // purple-400
   * ];
   * ```
   */
  const modes: { label: AgentMode; color: string }[] = [
    { label: 'Build', color: 'text-amber-400' },
    { label: 'Plan', color: 'text-blue-400' },
    { label: 'Ask', color: 'text-purple-400' },
  ];

  /**
   * ============================================================================
   * TERMINAL RENDERING - ROOT CONTAINER
   * ============================================================================
   * 
   * @ratatui-widget: Custom Terminal widget (combination of Block + Layout + nested widgets)
   * @ratatui-layout: Vertical layout with 4 sections
   * @ratatui-style: 
   *   - Background: Color::Rgb(13, 17, 23) [#0d1117]
   *   - Border: Color::Rgb(31, 35, 40) [border-gray-800]
   * @ratatui-focus: Track focus state to change border color or show cursor
   * 
   * ```rust
   * fn render_terminal(&self, frame: &mut Frame, area: Rect) {
   *     let border_color = if self.is_focused {
   *         Color::Rgb(59, 130, 246)  // Blue highlight
   *     } else {
   *         Color::Rgb(31, 35, 40)    // Gray
   *     };
   *     
   *     let block = Block::default()
   *         .borders(Borders::RIGHT)
   *         .border_style(Style::default().fg(border_color))
   *         .style(Style::default().bg(Color::Rgb(13, 17, 23)));
   *     
   *     let inner = block.inner(area);
   *     frame.render_widget(block, area);
   *     
   *     // Split inner area into sections
   *     let chunks = Layout::vertical([
   *         Constraint::Length(3),  // Header
   *         Constraint::Min(0),     // Output
   *         Constraint::Length(2),  // Status
   *         Constraint::Length(3),  // Input
   *     ]).split(inner);
   *     
   *     self.render_header(frame, chunks[0]);
   *     self.render_output(frame, chunks[1]);
   *     self.render_status_bar(frame, chunks[2]);
   *     self.render_input(frame, chunks[3]);
   * }
   * ```
   */
  return (
    <div 
      ref={containerRef}
      tabIndex={0}
      onFocus={() => setIsFocused(true)}
      onBlur={(e) => {
        // Only blur if focus moves outside the container
        if (!containerRef.current?.contains(e.relatedTarget as Node)) {
          setIsFocused(false);
        }
      }}
      className="flex-1 flex flex-col border-r border-terminal-border bg-terminal-bg h-full font-mono shadow-2xl overflow-hidden transition-all duration-300 outline-none"
    >
      
      {/* ===================================================================
          HEADER SECTION
          ===================================================================
          @ratatui-widget: Paragraph inside Block
          @ratatui-layout: Constraint::Length(3) - Fixed height
          @ratatui-style:
            - Background: Color::Rgb(11, 16, 21) [#0b1015]
            - Border bottom: Color::Rgb(31, 35, 40)
          
          @ratatui-content: Two parts - left (title) and right (path)
          @ratatui-pattern:
          ```rust
          fn render_header(&self, frame: &mut Frame, area: Rect) {
              let block = Block::default()
                  .borders(Borders::BOTTOM)
                  .border_style(Style::default().fg(Color::Rgb(31, 35, 40)))
                  .style(Style::default().bg(Color::Rgb(11, 16, 21)));
              
              let inner = block.inner(area);
              frame.render_widget(block, area);
              
              // Split horizontally for title and path
              let chunks = Layout::horizontal([
                  Constraint::Percentage(50),
                  Constraint::Percentage(50),
              ]).split(inner);
              
              // Left: Terminal icon (Unicode) + title
              // NOTE: Use config.agent_name from AppConfig - make it CONFIGURABLE
              let title = Paragraph::new(format!("{} {}", config.header_icon, config.agent_name.to_uppercase()))
                  .style(Style::default()
                      .fg(Color::Rgb(156, 163, 175))
                      .add_modifier(Modifier::BOLD));
              frame.render_widget(title, chunks[0]);
              
              // Right: Current path (right-aligned)
              // NOTE: Use config.default_path from AppConfig - make it CONFIGURABLE
              let path = Paragraph::new(&config.default_path)
                  .style(Style::default().fg(Color::Rgb(107, 114, 128)))
                  .alignment(Alignment::Right);
              frame.render_widget(path, chunks[1]);
          }
          ```
          
          @ratatui-icons: Terminal icon "🖥" or use Unicode box-drawing
       */}
      <div className="flex items-center justify-between px-4 py-2.5 bg-[#0b1015] border-b border-gray-800 select-none">
        <div className="flex items-center gap-2">
          {/* @ratatui-icon: Terminal icon - use Unicode "🖥" or "▶" */}
          <TerminalIcon className="w-4 h-4 text-[var(--llm-connected)]" />
          <span className="text-xs font-medium tracking-widest uppercase text-gray-400">
            {/* CONFIGURABLE: Use defaultAppConfig.agentName */}
            {defaultAppConfig.agentName}
          </span>
        </div>
        <span className="text-[11px] text-gray-500 font-medium">
          {/* CONFIGURABLE: Use defaultAppConfig.defaultPath */}
          {defaultAppConfig.defaultPath}
        </span>
      </div>

      {/* ===================================================================
          OUTPUT AREA - SCROLLABLE MESSAGE LIST
          ===================================================================
          @ratatui-widget: List or Paragraph with ScrollbarState
          @ratatui-layout: Constraint::Min(0) - Takes remaining space
          @ratatui-scroll: Vertical scrollbar, auto-scrolls to bottom
          
          @ratatui-pattern:
          ```rust
          fn render_output(&mut self, frame: &mut Frame, area: Rect) {
              // Calculate total content height
              let content_height = self.terminal_output.len();
              
              // Create scrollbar state
              self.scroll_state = self.scroll_state
                  .content_length(content_height)
                  .viewport_content_length(area.height as usize);
              
              // Render messages as List or multiple Paragraphs
              let messages: Vec<ListItem> = self.terminal_output
                  .iter()
                  .enumerate()
                  .skip(self.scroll_offset)
                  .take(area.height as usize)
                  .map(|(idx, line)| self.render_terminal_line(line, idx))
                  .collect();
              
              let list = List::new(messages)
                  .block(Block::default()
                      .style(Style::default().bg(Color::Rgb(13, 17, 23))));
              
              frame.render_widget(list, area);
              
              // Render scrollbar
              let scrollbar = Scrollbar::default()
                  .orientation(ScrollbarOrientation::VerticalRight)
                  .begin_symbol(Some("↑"))
                  .end_symbol(Some("↓"));
              frame.render_stateful_widget(
                  scrollbar,
                  area,
                  &mut self.scroll_state
              );
          }
          ```
          
          @ratatui-gradient: Gradients not directly supported; use solid bg color
          @ratatui-scrollbar: Use ratatui::widgets::Scrollbar widget
       */}
      <div className="relative flex-1">
        <div 
          ref={scrollRef}
          className={`absolute inset-0 p-6 overflow-y-auto bg-gradient-to-b from-[#0d1117] to-[#0b0f15] transition-all ${
            isFocused 
              ? 'scrollbar-thin scrollbar-thumb-gray-600 scrollbar-track-gray-900/50' 
              : 'scrollbar-none hover:scrollbar-thin hover:scrollbar-thumb-gray-700 hover:scrollbar-track-transparent'
          }`}
        >
        
        <div className="space-y-4 text-sm font-mono max-w-4xl mx-auto">
          {/* @ratatui-iteration: Loop through terminal_output Vec */}
          {output.map((line, index) => (
            <div key={index} className="leading-relaxed animate-in fade-in duration-300 group">
              
              {/* =========================================================
                  SYSTEM MESSAGE TYPE
                  =========================================================
                  @ratatui-style:
                    - Icon: Filled circle "●" in cyan
                    - Background: Cyan transparent bg (use Rgb with darker variant)
                    - Text: Cyan Color::Rgb(103, 232, 249)
                  
                  @ratatui-pattern:
                  ```rust
                  fn render_system_message(&self, content: &str) -> Paragraph {
                      let text = vec![
                          Span::styled("● ", Style::default()
                              .fg(Color::Rgb(103, 232, 249))),
                          Span::styled(content, Style::default()
                              .fg(Color::Rgb(165, 243, 252))),
                      ];
                      Paragraph::new(Line::from(text))
                          .style(Style::default()
                              .bg(Color::Rgb(22, 78, 99))) // Dark cyan bg
                  }
                  ```
               */}
              {line.type === 'system' && (
                <div className="flex gap-3 my-2">
                  <div className="flex-shrink-0 w-6 h-6 rounded-lg bg-cyan-500/10 border border-cyan-500/20 flex items-center justify-center">
                    {/* @ratatui-icon: "●" Unicode character */}
                    <Circle className="w-2 h-2 fill-[var(--msg-system)] text-[var(--msg-system)]" />
                  </div>
                  <div className="flex-1 flex items-center">
                    <span className="text-[var(--msg-system)]/80 font-medium text-xs">{line.content}</span>
                  </div>
                </div>
              )}

              {/* =========================================================
                  TOOL INVOCATION MESSAGE TYPE
                  =========================================================
                  @ratatui-style:
                    - Icon: Wrench "🔧" or "⚙"
                    - Code block background: Gray Rgb(31, 41, 55)
                    - Border: Gray with hover effect (selection highlight)
                  
                  @ratatui-expandable: Has optional details field
                  @ratatui-keyboard: Enter/Space to toggle expansion
                  @ratatui-state: expanded_tool_index tracks which is expanded
                  
                  @ratatui-pattern:
                  ```rust
                  fn render_tool_message(
                      &self,
                      line: &TerminalLine,
                      index: usize,
                      is_selected: bool
                  ) -> Vec<Line> {
                      let mut lines = vec![
                          Line::from(vec![
                              Span::styled("🔧 ", Style::default()
                                  .fg(Color::Rgb(156, 163, 175))),
                              Span::styled("Tool", Style::default()
                                  .fg(Color::Rgb(156, 163, 175))),
                          ]),
                      ];
                      
                      // Main content in code block style
                      let content_style = Style::default()
                          .bg(Color::Rgb(31, 41, 55))
                          .fg(Color::Rgb(229, 231, 235));
                      
                      for content_line in line.content.lines() {
                          lines.push(Line::styled(
                              format!("  {}", content_line),
                              content_style
                          ));
                      }
                      
                      // Show expand indicator if has details
                      if line.details.is_some() {
                          let indicator = if self.expanded_tool_index == Some(index) {
                              "▲ Hide details"
                          } else {
                              "▼ Show details"
                          };
                          lines.push(Line::styled(
                              indicator,
                              Style::default().fg(Color::Rgb(107, 114, 128))
                          ));
                      }
                      
                      // Render details if expanded
                      if self.expanded_tool_index == Some(index) {
                          if let Some(details) = &line.details {
                              lines.push(Line::from(""));
                              for detail_line in details.lines() {
                                  lines.push(Line::styled(
                                      format!("    {}", detail_line),
                                      Style::default()
                                          .fg(Color::Rgb(209, 213, 219))
                                  ));
                              }
                          }
                      }
                      
                      lines
                  }
                  ```
                  
                  @ratatui-copy: Bind 'c' key on selected tool to copy content
               */}
              {line.type === 'tool' && (
                <div className="flex gap-3 my-3">
                  <div className="flex-shrink-0 w-6 h-6 rounded-lg bg-gray-700/30 border border-gray-600/30 flex items-center justify-center">
                    {/* @ratatui-icon: "🔧" or "⚙" Unicode */}
                    <Wrench className="w-3.5 h-3.5 text-gray-400" />
                  </div>
                  <div className="flex-1">
                    <div className="flex items-center justify-between mb-1">
                      <span className="text-[11px] text-gray-400 font-medium">Tool</span>
                      {/* @ratatui-toggle: Show/hide details on Enter/Space key */}
                      {line.details && (
                        <button
                          onClick={() => setExpandedToolIndex(expandedToolIndex === index ? null : index)}
                          className="p-1 hover:bg-gray-700/50 rounded transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gray-500/60"
                          title={expandedToolIndex === index ? "Hide details" : "Show details"}
                          aria-expanded={expandedToolIndex === index}
                          aria-controls={`tool-details-${index}`}
                          aria-label={expandedToolIndex === index ? "Hide tool details" : "Show tool details"}
                        >
                          {expandedToolIndex === index ? (
                            <ChevronUp className="w-3.5 h-3.5 text-gray-400" />
                          ) : (
                            <ChevronDown className="w-3.5 h-3.5 text-gray-400" />
                          )}
                        </button>
                      )}
                    </div>
                    <div className="relative group/tool">
                      {/* @ratatui-widget: Code block as Paragraph with specific bg/fg */}
                      <pre className="text-gray-200 text-xs font-mono bg-gray-800/50 p-3 rounded-lg border border-gray-700/50 overflow-x-auto hover:border-gray-600/50 transition-colors">
                        {line.content}
                      </pre>
                      {/* @ratatui-copy: In TUI, bind 'c' key when this line is selected */}
                      <button
                        onClick={() => handleCopy(line.content, `tool-${index}`)}
                        className="absolute top-2 right-2 opacity-0 group-hover/tool:opacity-100 group-focus-within/tool:opacity-100 focus-visible:opacity-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gray-500/60 transition-opacity p-1 bg-gray-700 hover:bg-gray-600 rounded"
                        title="Copy to clipboard"
                        aria-label="Copy tool output"
                      >
                        {copiedIndex === `tool-${index}` ? <Check className="w-3 h-3 text-green-400" /> : <Copy className="w-3 h-3 text-gray-400" />}
                      </button>
                    </div>
                    
                    {/* =====================================================
                        EXPANDED TOOL DETAILS
                        =====================================================
                        @ratatui-condition: Rendered when expanded_tool_index == Some(index)
                        @ratatui-style: Darker background, bordered
                        @ratatui-pattern: Additional lines appended below main content
                        @ratatui-animation: Fade-in not available; instant render
                     */}
                    {expandedToolIndex === index && line.details && (
                      <div 
                        id={`tool-details-${index}`}
                        className="mt-2 pt-2 border-t border-gray-700/50 animate-in fade-in duration-200"
                      >
                        {/* @ratatui-widget: Paragraph with word-wrapping */}
                        <pre className="text-gray-300 text-xs font-mono bg-gray-800/30 p-3 rounded-lg border border-gray-700/30 overflow-x-auto whitespace-pre-wrap break-words">
                          {line.details}
                        </pre>
                        <div className="mt-2 flex gap-2">
                          {/* @ratatui-keyboard: 'c' to copy when details are focused */}
                          <button
                            onClick={() => handleCopy(line.details || '', `details-${index}`)}
                            className="flex items-center gap-1 px-2 py-1 text-xs bg-gray-700/50 hover:bg-gray-700/70 rounded transition-colors text-gray-300 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gray-500/60"
                            title="Copy details"
                            aria-label="Copy tool details"
                          >
                            {copiedIndex === `details-${index}` ? (
                              <>
                                <Check className="w-3 h-3" />
                                Copied
                              </>
                            ) : (
                              <>
                                <Copy className="w-3 h-3" />
                                Copy
                              </>
                            )}
                          </button>
                        </div>
                      </div>
                    )}
                  </div>
                </div>
              )}

              {/* =========================================================
                  USER INPUT MESSAGE TYPE
                  =========================================================
                  @ratatui-style:
                    - Icon: User "👤" or ">" (config.user_icon) in BLUE
                    - Icon Background: Blue Rgb(30, 58, 138) at 15% opacity
                    - Icon Border: Blue Rgb(59, 130, 246) at 30% opacity
                    - Icon Color: Blue Rgb(96, 165, 250)
                    - Label: config.user_name in Blue (auto-detected from $USER/$USERNAME)
                    - Message Background: Blue Rgb(59, 130, 246) at 5% opacity
                    - Message Border: Blue Rgb(59, 130, 246) at 20% opacity
                    - Text: Light gray Rgb(243, 244, 246)
                    - NOTE: User name is CONFIGURABLE and auto-detected in Ratatui
                    - NOTE: BLUE color scheme distinguishes user from agent (green)
                  
                  @ratatui-pattern:
                  ```rust
                  fn render_input_message(&self, content: &str) -> Vec<Line> {
                      vec![
                          Line::from(vec![
                              // Blue icon for user identification
                              Span::styled(format!("{} ", self.config.user_icon), Style::default()
                                  .fg(Color::Rgb(96, 165, 250))),  // Blue-400
                              Span::styled(&self.config.user_name, Style::default()
                                  .fg(Color::Rgb(96, 165, 250))),  // Blue-400
                          ]),
                          Line::from(""),
                          Line::styled(
                              content,
                              Style::default()
                                  .bg(Color::Rgb(31, 41, 55))
                                  .fg(Color::Rgb(243, 244, 246))
                          ),
                      ]
                  }
                  ```
               */}
              {line.type === 'input' && (
                <div className="flex gap-3 my-3">
                  {/* 
                    @ratatui-user-icon: Distinctive themed icon to identify user messages
                    @ratatui-colors: Uses theme variables for consistent theming
                      - Icon BG: var(--user-icon-bg)
                      - Icon Border: var(--user-icon-border)
                      - Icon Color: var(--user-icon-color)
                      - Bubble BG: var(--user-bubble-bg)
                      - Bubble Border: var(--user-bubble-border)
                  */}
                  <div className="flex-shrink-0 w-6 h-6 rounded-lg bg-[var(--user-icon-bg)] border border-[var(--user-icon-border)] flex items-center justify-center">
                    {/* @ratatui-icon: config.user_icon - "👤" or ">" themed */}
                    <User className="w-3.5 h-3.5 text-[var(--user-icon-color)]" />
                  </div>
                  <div className="flex-1">
                    {/* @ratatui-label: config.user_name - auto-detected from $USER/$USERNAME */}
                    <div className="text-[11px] text-[var(--user-label-color)] mb-1 font-medium">{defaultAppConfig.userName}</div>
                    <div className="bg-[var(--user-bubble-bg)] border border-[var(--user-bubble-border)] rounded-lg px-3 py-2.5 text-[var(--user-bubble-text)] text-sm break-words hover:border-[var(--user-icon-color)]/30 transition-colors">
                      {line.content}
                    </div>
                  </div>
                </div>
              )}

              {/* =========================================================
                  AGENT OUTPUT MESSAGE TYPE
                  =========================================================
                  @ratatui-style:
                    - Icon: Bot "🤖" or "◆"
                    - Background: Emerald transparent bg
                    - Text: Light gray Rgb(229, 231, 235)
                    - Label: config.agent_name_short in emerald Rgb(52, 211, 153)
                    - NOTE: Agent name is CONFIGURABLE via AppConfig
                  
                  @ratatui-pattern:
                  ```rust
                  fn render_output_message(&self, content: &str, config: &AppConfig) -> Vec<Line> {
                      vec![
                          Line::from(vec![
                              Span::styled(format!("{} ", config.agent_icon), Style::default()
                                  .fg(Color::Rgb(52, 211, 153))),
                              Span::styled(&config.agent_name_short, Style::default()
                                  .fg(Color::Rgb(52, 211, 153))),
                          ]),
                          Line::from(""),
                          // Render content (may span multiple lines)
                      ]
                      .into_iter()
                      .chain(
                          content.lines().map(|line| {
                              Line::styled(
                                  line,
                                  Style::default()
                                      .bg(Color::Rgb(31, 41, 55))
                                      .fg(Color::Rgb(229, 231, 235))
                              )
                          })
                      )
                      .collect()
                  }
                  ```
                  
                  @ratatui-copy: Bind 'c' when message is selected/focused
               */}
              {line.type === 'output' && (
                <div className="flex gap-3 my-3">
                  {/* 
                    @ratatui-agent-icon: Themed icon to identify agent messages
                    @ratatui-colors: Uses theme variables for consistent theming
                      - Icon BG: var(--agent-icon-bg)
                      - Icon Border: var(--agent-icon-border)
                      - Icon Color: var(--agent-icon-color)
                      - Bubble BG: var(--agent-bubble-bg)
                      - Bubble Border: var(--agent-bubble-border)
                  */}
                  <div className="flex-shrink-0 w-6 h-6 rounded-lg bg-[var(--agent-icon-bg)] border border-[var(--agent-icon-border)] flex items-center justify-center">
                    {/* @ratatui-icon: "🤖" or "◆" - use config.agent_icon */}
                    <Bot className="w-3.5 h-3.5 text-[var(--agent-icon-color)]" />
                  </div>
                  <div className="flex-1">
                    {/* CONFIGURABLE: Use defaultAppConfig.agentNameShort */}
                    <div className="text-[11px] text-[var(--agent-label-color)] mb-1 font-medium">{defaultAppConfig.agentNameShort}</div>
                    <div className="relative group/msg">
                      {/* @ratatui-widget: Paragraph with word wrap */}
                      <pre className="bg-[var(--agent-bubble-bg)] border border-[var(--agent-bubble-border)] rounded-lg px-3 py-2.5 text-[var(--agent-bubble-text)] text-xs leading-relaxed whitespace-pre-wrap break-words overflow-x-auto hover:border-[var(--agent-icon-color)]/30 transition-colors">
                        {line.content}
                      </pre>
                      {/* @ratatui-copy: 'c' key on focused output message */}
                      <button
                        onClick={() => handleCopy(line.content, `output-${index}`)}
                        className="absolute top-2 right-2 opacity-0 group-hover/msg:opacity-100 group-focus-within/msg:opacity-100 focus-visible:opacity-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gray-500/60 transition-opacity p-1 bg-gray-700 hover:bg-gray-600 rounded"
                        title="Copy to clipboard"
                        aria-label="Copy agent response"
                      >
                        {copiedIndex === `output-${index}` ? <Check className="w-3 h-3 text-green-400" /> : <Copy className="w-3 h-3 text-gray-400" />}
                      </button>
                    </div>
                  </div>
                </div>
              )}

              {/* =========================================================
                  THINKING MESSAGE TYPE
                  =========================================================
                  @ratatui-style:
                    - Icon: Brain "🧠" in muted gray
                    - Background: Darker gray with dashed/dotted border
                    - Text: Muted gray, italic-like appearance
                    - Label: "Thinking..." in muted gray
                  
                  @ratatui-pattern:
                  ```rust
                  fn render_thinking_message(&self, content: &str) -> Vec<Line> {
                      vec![
                          Line::from(vec![
                              Span::styled("🧠 ", Style::default()
                                  .fg(Color::Rgb(107, 114, 128))),
                              Span::styled("Thinking...", Style::default()
                                  .fg(Color::Rgb(107, 114, 128))
                                  .add_modifier(Modifier::ITALIC)),
                          ]),
                          Line::from(""),
                      ]
                      .into_iter()
                      .chain(
                          content.lines().map(|line| {
                              Line::styled(
                                  format!("  {}", line),
                                  Style::default()
                                      .fg(Color::Rgb(107, 114, 128))
                              )
                          })
                      )
                      .collect()
                  }
                  ```
                  
                  @ratatui-behavior: 
                    - Collapsible like tool details
                    - Shows agent's internal reasoning process
                    - Styled distinctly from regular output to indicate meta-content
               */}
              {line.type === 'thinking' && (
                <div className="flex gap-3 my-3">
                  <div className="flex-shrink-0 w-6 h-6 rounded-lg bg-gray-800/50 border border-dashed border-gray-600/50 flex items-center justify-center">
                    {/* @ratatui-icon: "🧠" Simple brain icon */}
                    <Brain className="w-3.5 h-3.5 text-gray-500" />
                  </div>
                  <div className="flex-1">
                    <div className="text-[11px] text-gray-500 mb-1 font-medium italic">Thinking...</div>
                    <div className="relative group/thinking">
                      {/* @ratatui-widget: Paragraph with muted styling and dashed border */}
                      <pre className="text-gray-400 text-xs font-mono bg-gray-900/50 p-3 rounded-lg border border-dashed border-gray-700/50 overflow-x-auto whitespace-pre-wrap break-words italic">
                        {line.content}
                      </pre>
                      {/* @ratatui-copy: 'c' key when thinking block is focused */}
                      <button
                        onClick={() => handleCopy(line.content, `thinking-${index}`)}
                        className="absolute top-2 right-2 opacity-0 group-hover/thinking:opacity-100 group-focus-within/thinking:opacity-100 focus-visible:opacity-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gray-500/60 transition-opacity p-1 bg-gray-700 hover:bg-gray-600 rounded"
                        title="Copy to clipboard"
                        aria-label="Copy thinking content"
                      >
                        {copiedIndex === `thinking-${index}` ? <Check className="w-3 h-3 text-green-400" /> : <Copy className="w-3 h-3 text-gray-400" />}
                      </button>
                    </div>
                  </div>
                </div>
              )}

              {/* =========================================================
                  QUESTION MESSAGE TYPE
                  =========================================================
                  @ratatui-style:
                    - Icon: Question mark "❓" or MessageCircleQuestion
                    - Background: Purple/violet tint for distinction
                    - Border: Solid purple border
                    - Interactive elements: Radio buttons, checkboxes, text input
                  
                  @ratatui-behavior:
                    - User can interact with options
                    - Single choice: Only one option can be selected
                    - Multi choice: Multiple options can be selected
                    - Free text: Text input field
                    - Submit button to confirm answer
                  
                  @ratatui-keyboard:
                    - Arrow keys: Navigate options
                    - Space/Enter: Toggle selection
                    - Tab: Move to submit button
                    - Letters (a-z): Quick select option by letter prefix
                  
                  @ratatui-pattern:
                  ```rust
                  fn render_question(&mut self, line: &TerminalLine, index: usize) -> Vec<Line> {
                      let mut lines = vec![
                          Line::from(vec![
                              Span::styled("❓ ", Style::default().fg(Color::Rgb(192, 132, 252))),
                              Span::styled(&line.content, Style::default()
                                  .fg(Color::Rgb(216, 180, 254))
                                  .add_modifier(Modifier::BOLD)),
                          ]),
                      ];
                      
                      match line.question_type {
                          Some(QuestionType::SingleChoice) => {
                              for (i, opt) in line.options.iter().enumerate() {
                                  let prefix = if opt.selected { "◉" } else { "○" };
                                  let letter = (b'a' + i as u8) as char;
                                  lines.push(Line::from(format!(
                                      "  {} {}) {}", prefix, letter, opt.label
                                  )));
                              }
                          }
                          Some(QuestionType::MultiChoice) => {
                              for (i, opt) in line.options.iter().enumerate() {
                                  let prefix = if opt.selected { "☑" } else { "☐" };
                                  let letter = (b'a' + i as u8) as char;
                                  lines.push(Line::from(format!(
                                      "  {} {}) {}", prefix, letter, opt.label
                                  )));
                              }
                          }
                          Some(QuestionType::FreeText) => {
                              lines.push(Line::from(format!(
                                  "  > {}_", self.question_input.get(&index).unwrap_or(&String::new())
                              )));
                          }
                          _ => {}
                      }
                      
                      lines
                  }
                  ```
               */}
              {line.type === 'question' && (
                <div className="flex gap-3 my-3">
                  <div className="flex-shrink-0 w-6 h-6 rounded-lg bg-[var(--msg-question)]/15 border border-[var(--msg-question)]/30 flex items-center justify-center">
                    {/* @ratatui-icon: "❓" or "?" for questions */}
                    <MessageCircleQuestion className="w-3.5 h-3.5 text-[var(--msg-question)]" />
                  </div>
                  <div className="flex-1">
                    <div className="text-[11px] text-[var(--msg-question)]/80 mb-1 font-medium">Question</div>
                    
                    {/* Question Content */}
                    <div className="bg-[var(--msg-question)]/5 border border-[var(--msg-question)]/20 rounded-lg p-3 space-y-3">
                      {/* Question Text */}
                      <p className="text-gray-100 text-sm font-medium">{line.content}</p>
                      
                      {/* Free Text Input */}
                      {line.questionType === 'freetext' && !line.answered && !answeredQuestions[index] && (
                        <div className="space-y-2">
                          {/* @ratatui-widget: Text input with cursor */}
                          <input
                            type="text"
                            placeholder={line.placeholder || "Type your answer..."}
                            value={questionResponses[index]?.textInput || ''}
                            onChange={(e) => setQuestionResponses(prev => ({
                              ...prev,
                              [index]: { ...prev[index], textInput: e.target.value, selectedOptions: [] }
                            }))}
                            className="w-full bg-[var(--terminal-bg)] border border-[var(--msg-question)]/30 rounded px-3 py-2 text-sm text-[var(--foreground)] placeholder:text-[var(--foreground)]/50 focus:outline-none focus:border-[var(--msg-question)]/50"
                          />
                          <button 
                            onClick={() => handleQuestionSubmit(index, line)}
                            className="flex items-center gap-1.5 px-3 py-1.5 bg-[var(--msg-question)]/20 hover:bg-[var(--msg-question)]/30 border border-[var(--msg-question)]/30 rounded text-[var(--msg-question)] text-xs font-medium transition-colors"
                          >
                            <Send className="w-3 h-3" />
                            Submit
                          </button>
                        </div>
                      )}
                      
                      {/* Single Choice (Radio Buttons) */}
                      {line.questionType === 'single' && !line.answered && !answeredQuestions[index] && line.options && (
                        <div className="space-y-2">
                          {/* @ratatui-widget: List with radio button styling */}
                          {line.options.map((option, optIndex) => {
                            const isSelected = questionResponses[index]?.selectedOptions?.includes(option.id);
                            const letter = String.fromCharCode(97 + optIndex); // a, b, c, d...
                            return (
                              <button
                                key={option.id}
                                onClick={() => setQuestionResponses(prev => ({
                                  ...prev,
                                  [index]: { textInput: '', selectedOptions: [option.id] }
                                }))}
                                className={`w-full flex items-center gap-3 px-3 py-2 rounded border text-left transition-all ${
                                  isSelected 
                                    ? 'bg-[var(--msg-question)]/20 border-[var(--msg-question)]/50 text-[var(--msg-question)]' 
                                    : 'bg-[var(--terminal-bg)]/50 border-[var(--terminal-border)] text-[var(--foreground)]/80 hover:bg-[var(--terminal-bg)] hover:border-[var(--terminal-border)]'
                                }`}
                              >
                                {/* @ratatui-icon: "○" empty, "◉" selected */}
                                {isSelected ? (
                                  <CircleDot className="w-4 h-4 text-[var(--msg-question)]" />
                                ) : (
                                  <Circle className="w-4 h-4 text-gray-500" />
                                )}
                                <span className="text-xs text-gray-500 font-mono">{letter})</span>
                                <span className="text-sm">{option.label}</span>
                              </button>
                            );
                          })}
                          <button 
                            onClick={() => handleQuestionSubmit(index, line)}
                            className="flex items-center gap-1.5 px-3 py-1.5 bg-[var(--msg-question)]/20 hover:bg-[var(--msg-question)]/30 border border-[var(--msg-question)]/30 rounded text-[var(--msg-question)] text-xs font-medium transition-colors mt-2"
                          >
                            <Send className="w-3 h-3" />
                            Confirm Selection
                          </button>
                        </div>
                      )}
                      
                      {/* Multi Choice (Checkboxes) */}
                      {line.questionType === 'multi' && !line.answered && !answeredQuestions[index] && line.options && (
                        <div className="space-y-2">
                          {/* @ratatui-widget: List with checkbox styling */}
                          {line.options.map((option, optIndex) => {
                            const isSelected = questionResponses[index]?.selectedOptions?.includes(option.id);
                            const letter = String.fromCharCode(97 + optIndex); // a, b, c, d...
                            return (
                              <button
                                key={option.id}
                                onClick={() => setQuestionResponses(prev => {
                                  const current = prev[index]?.selectedOptions || [];
                                  const newSelected = isSelected 
                                    ? current.filter(id => id !== option.id)
                                    : [...current, option.id];
                                  return {
                                    ...prev,
                                    [index]: { textInput: '', selectedOptions: newSelected, filterText: '' }
                                  };
                                })}
                                className={`w-full flex items-center gap-3 px-3 py-2 rounded border text-left transition-all ${
                                  isSelected 
                                    ? 'bg-[var(--msg-question)]/20 border-[var(--msg-question)]/50 text-[var(--msg-question)]' 
                                    : 'bg-[var(--terminal-bg)]/50 border-[var(--terminal-border)] text-[var(--foreground)]/80 hover:bg-[var(--terminal-bg)] hover:border-[var(--terminal-border)]'
                                }`}
                              >
                                {/* @ratatui-icon: "☐" unchecked, "☑" checked */}
                                {isSelected ? (
                                  <CheckSquare className="w-4 h-4 text-[var(--msg-question)]" />
                                ) : (
                                  <Square className="w-4 h-4 text-gray-500" />
                                )}
                                <span className="text-xs text-gray-500 font-mono">{letter})</span>
                                <span className="text-sm">{option.label}</span>
                              </button>
                            );
                          })}
                          <div className="flex items-center gap-2 mt-2">
                            <span className="text-xs text-[var(--foreground)]/50">
                              {questionResponses[index]?.selectedOptions?.length || 0} selected
                            </span>
                            <button 
                              onClick={() => handleQuestionSubmit(index, line)}
                              className="flex items-center gap-1.5 px-3 py-1.5 bg-[var(--msg-question)]/20 hover:bg-[var(--msg-question)]/30 border border-[var(--msg-question)]/30 rounded text-[var(--msg-question)] text-xs font-medium transition-colors"
                            >
                              <Send className="w-3 h-3" />
                              Confirm Selection
                            </button>
                          </div>
                        </div>
                      )}

                      {/* =========================================================
                          PROVIDER PICKER - VISUAL DEMO ONLY
                          =========================================================
                          ⚠️ NOTE: This is shown INLINE here for demonstration purposes.
                          In the actual Ratatui TUI, this should be a MODAL OVERLAY.
                          
                          @ratatui-type: SYSTEM MODAL (not a question!)
                          @ratatui-display: POPUP/MODAL - Centered floating window overlay
                          @ratatui-invocation: 
                            - Click on LLM name in status bar (e.g., "Claude 3.5 Sonnet")
                            - OR type "/model" command in input
                          @ratatui-widget: Block with bordered List and filter input
                          @ratatui-layout: 
                            - Centered rect over main UI (e.g., 60x15)
                            - Constraint::Length(1) for filter input
                            - Constraint::Min(0) for scrollable list
                          @ratatui-style:
                            - Background: Dimmed overlay behind modal
                            - Border: Color::Rgb(52, 58, 64)
                            - Title: "Select Provider" in border
                          @ratatui-state: 
                            - active_system_modal: SystemModal::ProviderPicker
                            - modal_selected_index: usize
                            - modal_filter_text: String
                          @ratatui-events:
                            - Escape: Close modal without selection
                            - Up/Down: Navigate list
                            - Enter: Select provider and close
                            - Typing: Filter list
                          @ratatui-pattern:
                          ```rust
                          fn render_provider_picker(&mut self, frame: &mut Frame, area: Rect, providers: &[ProviderOption]) {
                              let block = Block::bordered()
                                  .title(" Select Provider ")
                                  .title_style(Style::default().fg(Color::Rgb(156, 163, 175)))
                                  .border_style(Style::default().fg(Color::Rgb(52, 58, 64)));
                              
                              let inner = block.inner(area);
                              frame.render_widget(block, area);
                              
                              let chunks = Layout::vertical([
                                  Constraint::Length(1),  // Filter input
                                  Constraint::Min(0),     // Provider list
                              ]).split(inner);
                              
                              // Filter input with ">" prompt
                              let filter_line = Line::from(vec![
                                  Span::styled("> ", Style::default().fg(Color::Rgb(156, 163, 175))),
                                  Span::styled(&self.filter_text, Style::default().fg(Color::White)),
                                  Span::styled("_", Style::default().fg(Color::White).add_modifier(Modifier::SLOW_BLINK)),
                              ]);
                              frame.render_widget(Paragraph::new(filter_line), chunks[0]);
                              
                              // Build list items with status icons
                              let items: Vec<ListItem> = providers.iter()
                                  .filter(|p| p.name.to_lowercase().contains(&self.filter_text.to_lowercase()))
                                  .map(|p| {
                                      let status_icon = match p.status {
                                          ProviderStatus::Active => Span::styled("● ", Style::default().fg(Color::Green)),
                                          ProviderStatus::Warning => Span::styled("⚠ ", Style::default().fg(Color::Yellow)),
                                          ProviderStatus::Error => Span::styled("✖ ", Style::default().fg(Color::Red)),
                                          ProviderStatus::Unknown => Span::styled("? ", Style::default().fg(Color::Gray)),
                                      };
                                      ListItem::new(Line::from(vec![
                                          status_icon,
                                          Span::styled(&p.icon, Style::default()),
                                          Span::raw(" "),
                                          Span::styled(&p.name, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
                                          Span::styled(" - ", Style::default().fg(Color::Rgb(107, 114, 128))),
                                          Span::styled(&p.description, Style::default().fg(Color::Rgb(107, 114, 128)).add_modifier(Modifier::ITALIC)),
                                      ]))
                                  })
                                  .collect();
                              
                              let list = List::new(items)
                                  .highlight_style(Style::default().bg(Color::Rgb(31, 41, 55)))
                                  .highlight_symbol("▶ ");
                              
                              frame.render_stateful_widget(list, chunks[1], &mut self.provider_list_state);
                          }
                          ```
                       */}
                      {line.questionType === 'provider' && !line.answered && line.providers && (
                        <div className="space-y-0 border border-gray-700 rounded-lg overflow-hidden">
                          {/* Title bar */}
                          <div className="border-b border-gray-700 px-3 py-1.5 bg-gray-900/50">
                            <span className="text-gray-400 text-xs font-medium">Select Provider</span>
                          </div>
                          
                          {/* Filter input */}
                          <div className="flex items-center gap-2 px-3 py-2 border-b border-gray-800 bg-gray-900/30">
                            <span className="text-gray-500">{'>'}</span>
                            <input
                              type="text"
                              placeholder="Type to filter..."
                              value={questionResponses[index]?.filterText || ''}
                              onChange={(e) => setQuestionResponses(prev => ({
                                ...prev,
                                [index]: { ...prev[index], textInput: '', selectedOptions: prev[index]?.selectedOptions || [], filterText: e.target.value }
                              }))}
                              className="flex-1 bg-transparent text-gray-200 text-sm placeholder:text-gray-600 focus:outline-none"
                            />
                          </div>
                          
                          {/* Provider list */}
                          <div className="max-h-64 overflow-y-auto">
                            {line.providers
                              .filter(p => !questionResponses[index]?.filterText || 
                                p.name.toLowerCase().includes(questionResponses[index].filterText.toLowerCase()) ||
                                p.description.toLowerCase().includes(questionResponses[index].filterText.toLowerCase()))
                              .map((provider, provIndex) => {
                                const isSelected = questionResponses[index]?.selectedOptions?.includes(provider.id);
                                return (
                                  <button
                                    key={provider.id}
                                    onClick={() => setQuestionResponses(prev => ({
                                      ...prev,
                                      [index]: { textInput: '', selectedOptions: [provider.id], filterText: prev[index]?.filterText || '' }
                                    }))}
                                    className={`w-full flex items-center gap-2 px-3 py-2 text-left transition-colors ${
                                      isSelected 
                                        ? 'bg-gray-800' 
                                        : 'hover:bg-gray-800/50'
                                    }`}
                                  >
                                    {/* Selection indicator */}
                                    <span className={`text-sm ${isSelected ? 'text-[var(--picker-folder)]' : 'text-transparent'}`}>▶</span>
                                    
                                    {/* Status icon */}
                                    {/* @ratatui-icon: Status indicators - ● ⚠ ✖ ? */}
                                    {provider.status === 'active' && <span className="text-green-400 text-sm">●</span>}
                                    {provider.status === 'warning' && <AlertTriangle className="w-3.5 h-3.5 text-yellow-400" />}
                                    {provider.status === 'error' && <X className="w-3.5 h-3.5 text-red-400" />}
                                    {provider.status === 'unknown' && <HelpCircle className="w-3.5 h-3.5 text-gray-500" />}
                                    
                                    {/* Provider icon */}
                                    <span className="text-sm">{provider.icon || '🔌'}</span>
                                    
                                    {/* Provider name and description */}
                                    <span className="font-semibold text-gray-100 text-sm">{provider.name}</span>
                                    <span className="text-gray-500 text-sm">-</span>
                                    <span className="text-gray-500 text-sm italic truncate">{provider.description}</span>
                                  </button>
                                );
                              })}
                          </div>
                        </div>
                      )}

                      {/* =========================================================
                          MODEL PICKER - VISUAL DEMO ONLY
                          =========================================================
                          ⚠️ NOTE: This is shown INLINE here for demonstration purposes.
                          In the actual Ratatui TUI, this should be a MODAL OVERLAY.
                          
                          @ratatui-type: SYSTEM MODAL (not a question!)
                          @ratatui-display: POPUP/MODAL - Centered floating window overlay
                          @ratatui-invocation: 
                            - Shown after provider picker selection
                            - OR click directly on model name in status bar
                          @ratatui-widget: Block with bordered List and filter input
                          @ratatui-layout: Centered rect (e.g., 70x25), same structure as provider
                          @ratatui-style:
                            - Background: Dimmed overlay behind modal
                            - Model name: Bold white
                            - Capabilities: Dimmed tags
                            - Latest badge: Highlighted in yellow
                          @ratatui-state: active_system_modal: SystemModal::ModelPicker
                          @ratatui-events: Escape to close, Up/Down navigate, Enter select, Type to filter
                          @ratatui-pattern:
                          ```rust
                          fn render_model_picker(&mut self, frame: &mut Frame, area: Rect, models: &[ModelOption]) {
                              let block = Block::bordered()
                                  .title(" Select Model ")
                                  .title_style(Style::default().fg(Color::Rgb(156, 163, 175)))
                                  .border_style(Style::default().fg(Color::Rgb(52, 58, 64)));
                              
                              let inner = block.inner(area);
                              frame.render_widget(block, area);
                              
                              let chunks = Layout::vertical([
                                  Constraint::Length(1),
                                  Constraint::Min(0),
                              ]).split(inner);
                              
                              // Filter input
                              let filter = Paragraph::new(Line::from(vec![
                                  Span::styled("> ", Style::default().fg(Color::Rgb(156, 163, 175))),
                                  Span::styled(&self.filter_text, Style::default().fg(Color::White)),
                              ]));
                              frame.render_widget(filter, chunks[0]);
                              
                              // Model list items
                              let items: Vec<ListItem> = models.iter()
                                  .filter(|m| m.name.to_lowercase().contains(&self.filter_text.to_lowercase()))
                                  .map(|m| {
                                      let mut spans = vec![
                                          Span::styled(&m.name, Style::default()
                                              .fg(if m.is_latest { Color::Yellow } else { Color::White })
                                              .add_modifier(Modifier::BOLD)),
                                      ];
                                      if !m.capabilities.is_empty() {
                                          spans.push(Span::styled(" - ", Style::default().fg(Color::Rgb(75, 85, 99))));
                                          spans.push(Span::styled(
                                              m.capabilities.join(", "),
                                              Style::default().fg(Color::Rgb(107, 114, 128))
                                          ));
                                      }
                                      ListItem::new(Line::from(spans))
                                  })
                                  .collect();
                              
                              let list = List::new(items)
                                  .highlight_style(Style::default().bg(Color::Rgb(31, 41, 55)))
                                  .highlight_symbol("● ");
                              
                              frame.render_stateful_widget(list, chunks[1], &mut self.model_list_state);
                          }
                          ```
                       */}
                      {line.questionType === 'model' && !line.answered && line.models && (
                        <div className="space-y-0 border border-gray-700 rounded-lg overflow-hidden">
                          {/* Title bar */}
                          <div className="border-b border-gray-700 px-3 py-1.5 bg-gray-900/50">
                            <span className="text-gray-400 text-xs font-medium">Select Model</span>
                          </div>
                          
                          {/* Filter input */}
                          <div className="flex items-center gap-2 px-3 py-2 border-b border-gray-800 bg-gray-900/30">
                            <span className="text-gray-500">{'>'}</span>
                            <input
                              type="text"
                              placeholder="Type to filter..."
                              value={questionResponses[index]?.filterText || ''}
                              onChange={(e) => setQuestionResponses(prev => ({
                                ...prev,
                                [index]: { ...prev[index], textInput: '', selectedOptions: prev[index]?.selectedOptions || [], filterText: e.target.value }
                              }))}
                              className="flex-1 bg-transparent text-gray-200 text-sm placeholder:text-gray-600 focus:outline-none"
                            />
                          </div>
                          
                          {/* Model list */}
                          <div className="max-h-80 overflow-y-auto">
                            {line.models
                              .filter(m => !questionResponses[index]?.filterText || 
                                m.name.toLowerCase().includes(questionResponses[index].filterText.toLowerCase()) ||
                                m.capabilities?.some(c => c.toLowerCase().includes(questionResponses[index].filterText.toLowerCase())))
                              .map((model) => {
                                const isSelected = questionResponses[index]?.selectedOptions?.includes(model.id);
                                return (
                                  <button
                                    key={model.id}
                                    onClick={() => setQuestionResponses(prev => ({
                                      ...prev,
                                      [index]: { textInput: '', selectedOptions: [model.id], filterText: prev[index]?.filterText || '' }
                                    }))}
                                    className={`w-full flex items-start gap-2 px-3 py-1.5 text-left transition-colors ${
                                      isSelected 
                                        ? 'bg-gray-800' 
                                        : 'hover:bg-gray-800/50'
                                    }`}
                                  >
                                    {/* Selection indicator */}
                                    {/* @ratatui-icon: ● for selected, space for unselected */}
                                    <span className={`text-sm mt-0.5 ${isSelected ? 'text-yellow-400' : 'text-gray-700'}`}>●</span>
                                    
                                    {/* Model info */}
                                    <div className="flex-1 min-w-0">
                                      <span className={`font-semibold text-sm ${model.isLatest ? 'text-yellow-300' : 'text-gray-100'}`}>
                                        {model.name}
                                        {model.isLatest && <span className="text-yellow-400/70 text-xs ml-2">(latest)</span>}
                                      </span>
                                      {model.capabilities && model.capabilities.length > 0 && (
                                        <span className="text-gray-500 text-sm ml-2">
                                          - {model.capabilities.join(', ')}
                                        </span>
                                      )}
                                    </div>
                                  </button>
                                );
                              })}
                          </div>
                        </div>
                      )}

                      {/* =========================================================
                          FILE PICKER - VISUAL DEMO ONLY (@ Mentions)
                          =========================================================
                          ⚠️ NOTE: This is shown INLINE here for demonstration purposes.
                          In the actual Ratatui TUI, this should be a MODAL OVERLAY.
                          
                          @ratatui-type: SYSTEM MODAL (not a question!)
                          @ratatui-display: POPUP/MODAL - Centered floating window overlay
                          @ratatui-invocation: Triggered when user types "@" in input area
                          @ratatui-purpose: Add context files to agent (NOT for editing)
                          @ratatui-widget: Block with TreeView or indented List
                          @ratatui-layout: Centered rect (e.g., 50x20)
                          @ratatui-style:
                            - Background: Dimmed overlay behind modal
                            - Folders: Bold cyan with folder icon 📁
                            - Files: Normal white with file icon 📄
                            - Indentation: 2 spaces per level
                          @ratatui-state:
                            - active_system_modal: SystemModal::FilePicker
                            - modal_selected_index: usize
                            - expanded_folders: HashSet<String>
                          @ratatui-events:
                            - Escape: Close modal without selection
                            - Up/Down: Navigate
                            - Enter: Select file (adds to context tags in input)
                            - Left/Right: Collapse/Expand folder
                          @ratatui-pattern:
                          ```rust
                          fn render_file_picker(&mut self, frame: &mut Frame, area: Rect, files: &[FileOption]) {
                              let block = Block::bordered()
                                  .title(" Select File ")
                                  .title_style(Style::default().fg(Color::Rgb(156, 163, 175)))
                                  .border_style(Style::default().fg(Color::Rgb(52, 58, 64)));
                              
                              let inner = block.inner(area);
                              frame.render_widget(block, area);
                              
                              let items: Vec<ListItem> = files.iter()
                                  .map(|f| {
                                      let indent = "  ".repeat(f.indent_level);
                                      let icon = if f.is_folder { "📁" } else { "📄" };
                                      let style = if f.is_folder {
                                          Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
                                      } else {
                                          Style::default().fg(Color::White)
                                      };
                                      ListItem::new(Line::from(vec![
                                          Span::raw(indent),
                                          Span::styled(icon, style),
                                          Span::raw(" "),
                                          Span::styled(&f.name, style),
                                      ]))
                                  })
                                  .collect();
                              
                              let list = List::new(items)
                                  .highlight_style(Style::default().bg(Color::Rgb(31, 41, 55)))
                                  .highlight_symbol("▶ ");
                              
                              frame.render_stateful_widget(list, inner, &mut self.file_list_state);
                          }
                          ```
                       */}
                      {line.questionType === 'file' && !line.answered && line.files && (
                        <div className="space-y-0 border border-gray-700 rounded-lg overflow-hidden">
                          {/* Title bar */}
                          <div className="border-b border-gray-700 px-3 py-1.5 bg-gray-900/50">
                            <span className="text-gray-400 text-xs font-medium">Select File</span>
                          </div>
                          
                          {/* File list */}
                          <div className="max-h-64 overflow-y-auto">
                            {line.files.map((file) => {
                              const isSelected = questionResponses[index]?.selectedOptions?.includes(file.path);
                              const indent = (file.indentLevel || 0) * 16;
                              return (
                                <button
                                  key={file.path}
                                  onClick={() => setQuestionResponses(prev => ({
                                    ...prev,
                                    [index]: { textInput: '', selectedOptions: [file.path], filterText: '' }
                                  }))}
                                  className={`w-full flex items-center gap-2 px-3 py-1.5 text-left transition-colors ${
                                    isSelected 
                                      ? 'bg-gray-800' 
                                      : 'hover:bg-gray-800/50'
                                  }`}
                                  style={{ paddingLeft: `${12 + indent}px` }}
                                >
                                  {/* Selection indicator */}
                                  <span className={`text-sm ${isSelected ? 'text-cyan-400' : 'text-transparent'}`}>▶</span>
                                  
                                  {/* File/Folder icon */}
                                  {/* @ratatui-icon: 📁 for folder, 📄 for file */}
                                  {file.isFolder ? (
                                    <Folder className="w-4 h-4 text-[var(--picker-folder)]" />
                                  ) : (
                                    <File className="w-4 h-4 text-gray-400" />
                                  )}
                                  
                                  {/* File name */}
                                  <span className={`text-sm ${file.isFolder ? 'text-[var(--picker-folder)] font-medium' : 'text-[var(--foreground)]'}`}>
                                    {file.name}
                                  </span>
                                </button>
                              );
                            })}
                          </div>
                          
                          {/* Input mode indicator */}
                          <div className="border-t border-gray-800 px-3 py-2 bg-gray-900/30">
                            <div className="flex items-center gap-2 text-gray-500 text-xs">
                              <span className="px-1.5 py-0.5 bg-gray-800 rounded text-[10px]">[INSERT]</span>
                              <span className="flex items-center gap-1">
                                <span className="text-gray-400">@</span>
                                <span className="w-1.5 h-4 bg-gray-400 animate-pulse" />
                              </span>
                            </div>
                          </div>
                        </div>
                      )}
                      
                      {/* Answered State - from props or local state */}
                      {(line.answered && line.answer) && (
                        <div className="flex items-center gap-2 px-3 py-2 bg-[var(--llm-connected)]/10 border border-[var(--llm-connected)]/20 rounded">
                          <Check className="w-4 h-4 text-[var(--llm-connected)]" />
                          <span className="text-sm text-[var(--llm-connected)]">Answered: {line.answer}</span>
                        </div>
                      )}
                      
                      {/* Answered State - from local state (user just answered) */}
                      {answeredQuestions[index] && (
                        <div className="space-y-2">
                          <div className="flex items-start gap-2 px-3 py-2 bg-[var(--llm-connected)]/10 border border-[var(--llm-connected)]/20 rounded">
                            <Check className="w-4 h-4 text-[var(--llm-connected)] mt-0.5 flex-shrink-0" />
                            <div className="flex-1">
                              <span className="text-sm text-[var(--llm-connected)] font-medium">Answered</span>
                              <div className="text-sm text-[var(--foreground)] mt-1">
                                {answeredQuestions[index].selectedLabels && answeredQuestions[index].selectedLabels!.length > 0 ? (
                                  <div className="flex flex-wrap gap-1.5">
                                    {answeredQuestions[index].selectedLabels!.map((label, i) => (
                                      <span 
                                        key={i}
                                        className="inline-flex items-center gap-1 px-2 py-0.5 bg-[var(--llm-connected)]/15 border border-[var(--llm-connected)]/30 rounded text-xs text-[var(--llm-connected)]"
                                      >
                                        <Check className="w-3 h-3" />
                                        {label}
                                      </span>
                                    ))}
                                  </div>
                                ) : (
                                  <span className="text-[var(--foreground)]/80">{answeredQuestions[index].answer}</span>
                                )}
                              </div>
                            </div>
                          </div>
                        </div>
                      )}
                    </div>
                  </div>
                </div>
              )}

              {/* =========================================================
                  APPROVAL REQUEST
                  =========================================================
                  @ratatui-display: Inline approval request modal
                  @ratatui-behavior: Agent requesting approval for risky command
                  @ratatui-note: Similar styling to questions but approval-specific
                  
                  @ratatui-pattern:
                  ```rust
                  fn render_approval(&self, line: &TerminalLine) -> Vec<Line> {
                      let risk_color = match line.risk_level {
                          RiskLevel::High => Color::Rgb(251, 73, 52),    // Red
                          RiskLevel::Medium => Color::Rgb(251, 191, 36), // Amber
                          RiskLevel::Low => Color::Rgb(74, 222, 128),    // Green
                      };
                      
                      vec![
                          Line::from(vec![
                              Span::styled("🛡️ ", Style::default().fg(risk_color)),
                              Span::styled("Approval Required", Style::default()
                                  .fg(risk_color)
                                  .add_modifier(Modifier::BOLD)),
                          ]),
                          Line::from(format!("Command: {}", line.command)),
                          // ... options ...
                      ]
                  }
                  ```
               */}
              {line.type === 'approval' && (
                <div className="flex gap-3 my-3">
                  <div className="flex-shrink-0 w-6 h-6 rounded-lg bg-[var(--msg-approval)]/15 border border-[var(--msg-approval)]/30 flex items-center justify-center">
                    <ShieldCheck className="w-3.5 h-3.5 text-[var(--msg-approval)]" />
                  </div>
                  <div className="flex-1">
                    <div className="text-[11px] text-[var(--msg-approval)]/80 mb-1 font-medium">Approval Required</div>
                    
                    {/* Approval Content */}
                    <div className="bg-[var(--msg-approval)]/5 border border-[var(--msg-approval)]/20 rounded-lg p-3 space-y-3">
                      {/* Risk Level Badge */}
                      <div className="flex items-center gap-2">
                        <span className={`px-2 py-0.5 rounded text-xs font-medium ${
                          line.riskLevel === 'high' 
                            ? 'bg-red-500/15 border border-red-500/30 text-red-400'
                            : line.riskLevel === 'medium'
                            ? 'bg-amber-500/15 border border-amber-500/30 text-amber-400'
                            : 'bg-emerald-500/15 border border-emerald-500/30 text-emerald-400'
                        }`}>
                          {line.riskLevel?.toUpperCase()} RISK
                        </span>
                      </div>

                      {/* Command Display */}
                      <div className="space-y-1.5">
                        <div className="text-xs text-[var(--foreground)]/50 font-medium">Command:</div>
                        <div className="bg-[var(--terminal-bg)] border border-[var(--msg-approval)]/30 rounded px-3 py-2">
                          <code className="text-sm text-[var(--msg-approval)] font-mono">{line.command}</code>
                        </div>
                      </div>

                      {/* Description */}
                      {line.meta && (
                        <p className="text-xs text-[var(--foreground)]/60">{line.meta}</p>
                      )}

                      {/* Not Answered Yet - Show Options */}
                      {!line.answered && !answeredQuestions[index] && (
                        <div className="space-y-2">
                          <div className="text-xs text-[var(--foreground)]/70 font-medium mb-2">Choose an action:</div>
                          
                          {/* Run Once */}
                          <button
                            onClick={() => {
                              setAnsweredQuestions(prev => ({
                                ...prev,
                                [index]: { answer: 'Run once', selectedLabels: [] }
                              }));
                            }}
                            className="w-full flex items-center gap-3 px-3 py-2.5 rounded border bg-[var(--terminal-bg)]/50 border-[var(--terminal-border)] text-[var(--foreground)]/80 hover:bg-[var(--msg-approval)]/10 hover:border-[var(--msg-approval)]/30 transition-all text-left group"
                          >
                            <Play className="w-4 h-4 text-[var(--msg-approval)] flex-shrink-0" />
                            <div className="flex-1">
                              <div className="text-sm font-medium text-[var(--foreground)]">Run Once</div>
                              <div className="text-xs text-[var(--foreground)]/50 mt-0.5">Execute this command one time only</div>
                            </div>
                          </button>

                          {/* Always Allow */}
                          <button
                            onClick={() => {
                              setAnsweredQuestions(prev => ({
                                ...prev,
                                [index]: { answer: 'Always allow this exact command', selectedLabels: [] }
                              }));
                            }}
                            className="w-full flex items-center gap-3 px-3 py-2.5 rounded border bg-[var(--terminal-bg)]/50 border-[var(--terminal-border)] text-[var(--foreground)]/80 hover:bg-emerald-500/10 hover:border-emerald-500/30 transition-all text-left group"
                          >
                            <ShieldPlus className="w-4 h-4 text-emerald-400 flex-shrink-0" />
                            <div className="flex-1">
                              <div className="text-sm font-medium text-[var(--foreground)]">Always Allow</div>
                              <div className="text-xs text-[var(--foreground)]/50 mt-0.5">Add exact command to approval list</div>
                            </div>
                          </button>

                          {/* Pattern Match */}
                          <button
                            onClick={() => {
                              const pattern = line.detectedPattern || 'rm -rf * && npm install *';
                              setAnsweredQuestions(prev => ({
                                ...prev,
                                [index]: { answer: `Allow pattern: ${pattern}`, selectedLabels: [] }
                              }));
                            }}
                            className="w-full flex items-center gap-3 px-3 py-2.5 rounded border bg-[var(--terminal-bg)]/50 border-[var(--terminal-border)] text-[var(--foreground)]/80 hover:bg-blue-500/10 hover:border-blue-500/30 transition-all text-left group"
                          >
                            <Asterisk className="w-4 h-4 text-blue-400 flex-shrink-0" />
                            <div className="flex-1">
                              <div className="text-sm font-medium text-[var(--foreground)]">Pattern Match</div>
                              <div className="text-xs text-[var(--foreground)]/50 mt-0.5">
                                Allow similar commands using wildcards
                                {line.detectedPattern && (
                                  <div className="mt-1 font-mono text-blue-400">Pattern: {line.detectedPattern}</div>
                                )}
                              </div>
                            </div>
                          </button>

                          {/* Skip */}
                          <button
                            onClick={() => {
                              setAnsweredQuestions(prev => ({
                                ...prev,
                                [index]: { answer: 'Skipped execution', selectedLabels: [] }
                              }));
                            }}
                            className="w-full flex items-center gap-3 px-3 py-2.5 rounded border bg-[var(--terminal-bg)]/50 border-[var(--terminal-border)] text-[var(--foreground)]/80 hover:bg-red-500/10 hover:border-red-500/30 transition-all text-left group"
                          >
                            <Ban className="w-4 h-4 text-red-400 flex-shrink-0" />
                            <div className="flex-1">
                              <div className="text-sm font-medium text-[var(--foreground)]">Skip</div>
                              <div className="text-xs text-[var(--foreground)]/50 mt-0.5">Don't run this command</div>
                            </div>
                          </button>
                        </div>
                      )}

                      {/* Answered State */}
                      {answeredQuestions[index] && (
                        <div className="flex items-start gap-2 px-3 py-2 bg-[var(--llm-connected)]/10 border border-[var(--llm-connected)]/20 rounded">
                          <Check className="w-4 h-4 text-[var(--llm-connected)] mt-0.5 flex-shrink-0" />
                          <div className="flex-1">
                            <span className="text-sm text-[var(--llm-connected)] font-medium">Decision Made</span>
                            <div className="text-sm text-[var(--foreground)] mt-1">
                              {answeredQuestions[index].answer}
                            </div>
                          </div>
                        </div>
                      )}
                    </div>
                  </div>
                </div>
              )}

              {/* =========================================================
                  COMMAND MESSAGE TYPE
                  =========================================================
                  @ratatui-style:
                    - Shell prompt: "$" in emerald green
                    - Command text: Light gray
                    - Opacity reduced (darker colors)
                  
                  @ratatui-pattern:
                  ```rust
                  fn render_command(&self, content: &str) -> Line {
                      Line::from(vec![
                          Span::styled("$ ", Style::default()
                              .fg(Color::Rgb(52, 211, 153))
                              .add_modifier(Modifier::BOLD)),
                          Span::styled(content, Style::default()
                              .fg(Color::Rgb(229, 231, 235))),
                      ])
                  }
                  ```
               */}
              {line.type === 'command' && (
                <div className="flex items-start gap-2 mt-4 opacity-75 pl-2">
                  {/* @ratatui-icon: "$" shell prompt character */}
                  <span className="text-emerald-400 mt-0.5 font-semibold">$</span>
                  <span className="text-gray-200 text-sm">{line.content}</span>
                </div>
              )}
            </div>
          ))}
        </div>
        </div>
        {/* @ratatui-gradient: Not available in TUI; omit or use solid fade */}
        <div className="absolute bottom-0 left-0 right-0 h-8 bg-gradient-to-t from-[#0b0f15] to-transparent pointer-events-none" />
      </div>

      {/* ===================================================================
          FLASH BAR - STATUS INDICATOR
          ===================================================================
          @ratatui-widget: Custom FlashBar widget
          @ratatui-layout: Constraint::Length(1) - Single line height
          @ratatui-states: idle (single dot), working (animated dots), 
                          rate-limit (yellow bg), error (red bg), warning (peach bg)
       */}
      <FlashBar state={flashBarState} message={flashBarMessage} />
      
      {/* Demo controls for FlashBar states - remove in production */}
      {/* ============================================================
          @web-only: DEMO CONTROLS - DO NOT IMPLEMENT IN TUI
          ============================================================
          These buttons are for testing the FlashBar states in the web mockup.
          They should NOT be part of the Ratatui TUI implementation.
          The FlashBar state in TUI will be controlled by the agent/backend.
       */}
      {showFlashBarDemo && (
        <div className="flex items-center justify-center gap-2 py-1 bg-[var(--terminal-header-bg)] border-t border-[var(--terminal-border)]">
          <span className="text-[10px] text-gray-500 mr-2">Flash Bar:</span>
          {(['idle', 'working', 'rate-limit', 'error', 'warning'] as const).map((s) => (
            <button
              key={s}
              onClick={() => {
                setFlashBarState(s);
                setFlashBarMessage(undefined);
              }}
              className={`px-2 py-0.5 text-[10px] rounded transition-colors ${
                flashBarState === s 
                  ? 'bg-[var(--msg-system)] text-[var(--background)]' 
                  : 'bg-[var(--terminal-border)] text-[var(--foreground)] hover:bg-[var(--mode-hover-bg)]'
              }`}
            >
              {s}
            </button>
          ))}
          <button
            onClick={() => setShowFlashBarDemo(false)}
            className="ml-2 px-2 py-0.5 text-[10px] rounded bg-red-500/20 text-red-400 hover:bg-red-500/30 transition-colors"
          >
            Hide
          </button>
        </div>
      )}

      {/* ===================================================================
          STATUS BAR - MODE SELECTORS AND LLM INFO
          ===================================================================
          @ratatui-widget: Paragraph or horizontal layout of Spans
          @ratatui-layout: Constraint::Length(2) - Fixed height, 2 rows
          @ratatui-split: Horizontal split - left (mode selectors), right (LLM info)
          
          @ratatui-pattern:
          ```rust
          fn render_status_bar(&mut self, frame: &mut Frame, area: Rect) {
              let chunks = Layout::horizontal([
                  Constraint::Percentage(70),  // Left: Mode selectors
                  Constraint::Percentage(30),  // Right: LLM status
              ]).split(area);
              
              self.render_mode_selector(frame, chunks[0]);
              self.render_llm_status(frame, chunks[1]);
          }
          ```
          
          @ratatui-popups: Mode dropdowns render as popup overlays
       */}
      
      {/* ===================================================================
          INPUT AREA - CONTEXT FILES + INPUT FIELD
          ===================================================================
          @ratatui-widget: Vertical layout with conditional context row
          @ratatui-layout: 
            - If context_files empty: Single row for input (height 3)
            - If context_files present: Two rows - context (2) + input (3)
          
          @ratatui-pattern:
          ```rust
          fn render_input(&mut self, frame: &mut Frame, area: Rect) {
              let chunks = if self.context_files.is_empty() {
                  vec![area]  // Just input area
              } else {
                  Layout::vertical([
                      Constraint::Length(2),  // Context files
                      Constraint::Length(3),  // Input field
                  ]).split(area).to_vec()
              };
              
              if !self.context_files.is_empty() {
                  self.render_context_files(frame, chunks[0]);
                  self.render_input_field(frame, chunks[1]);
              } else {
                  self.render_input_field(frame, chunks[0]);
              }
          }
          ```
       */}
      <div className="px-4 py-3 bg-[#05070a] border-t border-amber-500/20">
        <div className="space-y-2 max-w-4xl mx-auto">
        {/* =====================================================
            CONTEXT FILES DISPLAY
            =====================================================
            @ratatui-widget: Horizontal Spans wrapped in Paragraph
            @ratatui-condition: Only rendered if context_files.len() > 0
            @ratatui-style: Blue theme with badges
            @ratatui-interactive: Navigate with Tab, remove with 'x' or Delete
            
            @ratatui-pattern:
            ```rust
            fn render_context_files(&self, frame: &mut Frame, area: Rect) {
                let mut spans = vec![
                    Span::styled(
                        "Context: ",
                        Style::default().fg(Color::Rgb(147, 197, 253))
                    ),
                ];
                
                for (idx, file) in self.context_files.iter().enumerate() {
                    spans.push(Span::raw(" "));
                    spans.push(Span::styled(
                        format!("📄 {} [x]", file.name),
                        Style::default()
                            .bg(Color::Rgb(29, 78, 216))
                            .fg(Color::Rgb(191, 219, 254))
                    ));
                }
                
                let paragraph = Paragraph::new(Line::from(spans))
                    .style(Style::default()
                        .bg(Color::Rgb(30, 58, 138))
                        .fg(Color::Rgb(147, 197, 253)));
                
                frame.render_widget(paragraph, area);
            }
            ```
            
            @ratatui-removal: When file is focused/selected, 'x' or Delete removes it
         */}
        {addedContextFiles.length > 0 && (
          <div className="flex items-center gap-2 px-3 py-2 rounded bg-blue-500/10 border border-blue-500/20">
            <span className="text-xs text-blue-300 font-medium uppercase tracking-wider">Context:</span>
            <div className="flex flex-wrap gap-1.5">
              {addedContextFiles.map((file, idx) => (
                <div
                  key={idx}
                  title={file.path ?? file.name}
                  className="flex items-center gap-1.5 px-2 py-0.5 bg-blue-500/20 border border-blue-500/30 rounded text-xs text-blue-200 group/file"
                >
                  {/* @ratatui-icon: "📄" file icon */}
                  <FileCode className="w-3 h-3" />
                  <span className="max-w-[150px] truncate">{file.name}</span>
                  {/* @ratatui-removal: In TUI, use 'x' when focused */}
                  <button
                    onClick={() => handleRemoveContext(idx)}
                    className="ml-1 opacity-0 group-hover/file:opacity-100 group-focus-within/file:opacity-100 focus-visible:opacity-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-red-400/60 rounded hover:text-red-400 transition-all"
                    title="Remove context file"
                    aria-label={`Remove ${file.name}`}
                  >
                    <X className="w-3 h-3" />
                  </button>
                </div>
              ))}
            </div>
          </div>
        )}
        
        {/* =====================================================
            INPUT FIELD WITH ADD CONTEXT BUTTON
            =====================================================
            @ratatui-widget: Paragraph for input with cursor position
            @ratatui-state: input: String, cursor_position: usize
            
            FILE PICKER TRIGGERS (System Modal):
            - Type "@" → IMMEDIATELY show file picker modal
            - Click "+" button → Show file picker modal
            - Ctrl+O → Show file picker modal
            
            @ratatui-keyboard:
              - Char keys: Append to input
              - Backspace: Remove character
              - Left/Right: Move cursor
              - Enter: Submit
              - "@" key: IMMEDIATELY trigger file picker modal
              - Ctrl+O or click '+': Open file picker modal
            
            @ratatui-file-picker-trigger:
            ```rust
            fn handle_char(&mut self, c: char) {
                self.input.insert(self.cursor_position, c);
                self.cursor_position += 1;
                
                // "@" IMMEDIATELY triggers file picker
                if c == '@' {
                    self.active_system_modal = SystemModal::FilePicker;
                }
            }
            
            fn handle_plus_click(&mut self) {
                self.active_system_modal = SystemModal::FilePicker;
            }
            ```
            
            @ratatui-pattern:
            ```rust
            fn render_input_field(&mut self, frame: &mut Frame, area: Rect) {
                let chunks = Layout::horizontal([
                    Constraint::Length(3),   // Plus button
                    Constraint::Min(0),      // Input field
                ]).split(area);
                
                // Render plus button
                let plus_btn = Paragraph::new("+")
                    .style(Style::default()
                        .fg(Color::Rgb(107, 114, 128))
                        .bg(Color::Rgb(31, 41, 55)));
                frame.render_widget(plus_btn, chunks[0]);
                
                // Render input with cursor
                let input_display = if self.is_focused {
                    format!("▶ {}█", self.input)  // With cursor
                } else {
                    format!("▶ {}", self.input)
                };
                
                let input_widget = Paragraph::new(input_display)
                    .style(Style::default()
                        .fg(Color::Rgb(229, 231, 235))
                        .bg(Color::Rgb(5, 7, 10)));
                
                frame.render_widget(input_widget, chunks[1]);
            }
            
            // In event loop:
            fn handle_input_key(&mut self, key: KeyEvent) {
                match key.code {
                    KeyCode::Char(c) => {
                        self.input.insert(self.cursor_position, c);
                        self.cursor_position += 1;
                    }
                    KeyCode::Backspace => {
                        if self.cursor_position > 0 {
                            self.input.remove(self.cursor_position - 1);
                            self.cursor_position -= 1;
                        }
                    }
                    KeyCode::Left => {
                        self.cursor_position = self.cursor_position.saturating_sub(1);
                    }
                    KeyCode::Right => {
                        self.cursor_position = (self.cursor_position + 1).min(self.input.len());
                    }
                    KeyCode::Enter => {
                        self.handle_submit();
                    }
                    _ => {}
                }
            }
            ```
         */}
        <form onSubmit={onSubmit} className="relative flex items-center gap-3">
          {/* @ratatui-button: "+" for add context (Ctrl+O binding) */}
          <button 
            type="button"
            onClick={handleAddContext}
            className="p-1.5 text-gray-500 hover:text-gray-200 hover:bg-gray-800 rounded transition-all focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-400/60"
            title="Add Context / Files (Click or Cmd+V)"
            aria-label="Add context files"
            aria-keyshortcuts="Meta+V Control+V"
          >
            {/* @ratatui-icon: "+" or "📎" */}
            <Plus className="w-4 h-4" />
          </button>

          <div className="flex-1 flex items-start gap-2 text-sm">
            {/* @ratatui-prompt: "▶" or ">" prompt character */}
            <ChevronRight className="w-4 h-4 text-amber-500/60 mt-2" />
            {/* @ratatui-input: Main input buffer with cursor
                @ratatui-cursor: Track position in string, render as "█" or "_"
                @ratatui-placeholder: Show when input.is_empty() */}
            <textarea
              ref={inputRef}
              value={input}
              onChange={(e) => {
                const newValue = e.target.value;
                onInputChange(newValue);
                
                // "@" typed → IMMEDIATELY show file picker
                // @ratatui-trigger: if newValue.ends_with('@') && !input.ends_with('@')
                if (newValue.endsWith('@') && !input.endsWith('@')) {
                  setActiveSystemModal('file');
                }
                
                // "/model" command → show provider picker
                // @ratatui-trigger: if newValue.trim() == "/model"
                if (newValue.trim() === '/model') {
                  setActiveSystemModal('provider');
                  onInputChange(''); // Clear input after command
                }
                
                // "/theme" command → show theme picker
                // @ratatui-trigger: if newValue.trim() == "/theme"
                if (newValue.trim() === '/theme') {
                  setActiveSystemModal('theme');
                  onInputChange(''); // Clear input after command
                }
                
                // "/help" command → show help/shortcuts modal
                // @ratatui-trigger: if newValue.trim() == "/help"
                if (newValue.trim() === '/help') {
                  setActiveSystemModal('help');
                  onInputChange(''); // Clear input after command
                }
              }}
              onKeyDown={(e) => {
                // Submit on Enter (without Shift)
                if (e.key === 'Enter' && !e.shiftKey) {
                  e.preventDefault();
                  // Form submission will be handled by the form's onSubmit
                  const form = e.currentTarget.form;
                  if (form) form.requestSubmit();
                }
                
                /**
                 * @ratatui-behavior: Backspace removes entire @mention word at once
                 * @ratatui-pattern:
                 * ```rust
                 * fn handle_backspace(&mut self) {
                 *     if let Some(pos) = self.cursor_position {
                 *         // Check if cursor is right after an @mention
                 *         let before_cursor = &self.input[..pos];
                 *         if let Some(at_idx) = before_cursor.rfind('@') {
                 *             // Check if there's no space between @ and cursor
                 *             let mention = &before_cursor[at_idx..];
                 *             if !mention.contains(' ') || mention.ends_with(' ') {
                 *                 // Remove entire @mention
                 *                 self.input = format!("{}{}", &self.input[..at_idx], &self.input[pos..]);
                 *                 return;
                 *             }
                 *         }
                 *     }
                 *     // Default: remove single character
                 * }
                 * ```
                 */
                if (e.key === 'Backspace') {
                  const cursorPos = e.currentTarget.selectionStart || 0;
                  const beforeCursor = input.slice(0, cursorPos);
                  
                  // Find the last @ before cursor
                  const lastAtIndex = beforeCursor.lastIndexOf('@');
                  
                  if (lastAtIndex !== -1) {
                    const mention = beforeCursor.slice(lastAtIndex);
                    // Check if cursor is at the end of an @mention (no space in the mention, or at end after space)
                    // Pattern: @filename or @filename followed by cursor
                    if (!mention.includes(' ') || mention.trim() === mention) {
                      // Check if this looks like a complete @mention (has characters after @)
                      if (mention.length > 1 || cursorPos === lastAtIndex + 1) {
                        e.preventDefault();
                        // Remove entire @mention word (find the end - next space or end of string)
                        const afterAt = input.slice(lastAtIndex);
                        const spaceAfter = afterAt.indexOf(' ', 1); // Find space after @
                        const mentionEnd = spaceAfter === -1 ? input.length : lastAtIndex + spaceAfter + 1;
                        
                        const newInput = input.slice(0, lastAtIndex) + input.slice(mentionEnd);
                        onInputChange(newInput);
                      }
                    }
                  }
                }
              }}
              className="flex-1 bg-transparent outline-none text-gray-200 placeholder:text-gray-700 font-medium resize-none min-h-[44px] max-h-[200px] py-2 leading-relaxed"
              placeholder={`Message ${mode.toLowerCase()}...`}
              rows={2}
              autoFocus
              style={{ 
                overflowY: 'auto',
                wordWrap: 'break-word',
                whiteSpace: 'pre-wrap'
              }}
            />
          </div>
        </form>
        </div>
      </div>

      {/* @ratatui-note: Hidden file input is web-specific
          In TUI, use text input for paths or integrate with terminal file picker */}
      <input
        ref={fileInputRef}
        type="file"
        multiple
        onChange={handleFileSelect}
        className="hidden"
        accept="*"
      />

      <div className="px-4 py-2 bg-[#0b1015] border-t border-gray-800 text-xs select-none relative z-20">
        <div className="flex items-center justify-between max-w-4xl mx-auto">
        
        {/* =========================================================
            LEFT SIDE: AGENT MODE SELECTOR + BUILD MODE
            =========================================================
            @ratatui-interactive: Keyboard-navigable dropdowns
            @ratatui-keyboard: 
              - Tab/Shift+Tab: Cycle between dropdowns
              - Arrow keys: Navigate options
              - Enter: Select option
              - Esc: Close dropdown
         */}
        <div className="flex items-center gap-1.5">
          <span className="text-[9px] text-gray-600 uppercase tracking-wider">agent</span>
          {/* =====================================================
              AGENT MODE DROPDOWN BUTTON
              =====================================================
              @ratatui-widget: Spans with colored circle indicator
              @ratatui-state: mode (Build/Plan/Ask)
              @ratatui-popup: When open, show List widget as overlay
              
              @ratatui-pattern:
              ```rust
              fn render_mode_selector(&self, frame: &mut Frame, area: Rect) {
                  let mode_text = vec![
                      Span::styled("● ", Style::default().fg(self.mode.color())),
                      Span::styled(
                          format!("{:?}", self.mode),
                          Style::default().fg(Color::Rgb(229, 231, 235))
                      ),
                      Span::raw(" "),
                      Span::styled(
                          if self.mode_selector_open { "▲" } else { "▼" },
                          Style::default().fg(Color::Rgb(107, 114, 128))
                      ),
                  ];
                  
                  let paragraph = Paragraph::new(Line::from(mode_text))
                      .style(Style::default().bg(Color::Rgb(31, 41, 55)));
                  frame.render_widget(paragraph, area);
                  
                  // If dropdown open, render popup List
                  if self.mode_selector_open {
                      self.render_mode_popup(frame, area);
                  }
              }
              ```
           */}
          <div className="relative">
            <button 
              onClick={() => setIsModeSelectorOpen(!isModeSelectorOpen)}
              className="flex items-center gap-1 px-1.5 py-0.5 bg-gray-800/50 hover:bg-gray-800 text-gray-200 rounded transition-colors border border-gray-700/50"
            >
              <div className={`flex items-center gap-1 ${modes.find(m => m.label === mode)?.color}`}>
                {/* @ratatui-icon: "●" filled circle in mode color */}
                <Circle className="w-1.5 h-1.5 fill-current" />
                <span className="text-[11px] font-medium">{mode}</span>
              </div>
              {/* @ratatui-icon: "▼" or "▲" based on open state */}
              <ChevronUp className={`w-2.5 h-2.5 text-gray-500 transition-transform ${isModeSelectorOpen ? 'rotate-180' : ''}`} />
            </button>

            {/* =====================================================
                MODE DROPDOWN POPUP MENU
                =====================================================
                @ratatui-widget: Popup with List widget
                @ratatui-state: ListState to track selected item
                @ratatui-keyboard: Arrow Up/Down to navigate, Enter to select
                
                @ratatui-pattern:
                ```rust
                fn render_mode_popup(&mut self, frame: &mut Frame, anchor: Rect) {
                    // Calculate popup area above the button
                    let popup_area = Rect {
                        x: anchor.x,
                        y: anchor.y.saturating_sub(5), // Above button
                        width: 20,
                        height: 5, // 3 options + borders
                    };
                    
                    let items: Vec<ListItem> = vec![
                        ListItem::new("● Build").style(
                            Style::default().fg(Color::Rgb(251, 191, 36))
                        ),
                        ListItem::new("● Plan").style(
                            Style::default().fg(Color::Rgb(96, 165, 250))
                        ),
                        ListItem::new("● Ask").style(
                            Style::default().fg(Color::Rgb(192, 132, 252))
                        ),
                    ];
                    
                    let list = List::new(items)
                        .block(Block::default()
                            .borders(Borders::ALL)
                            .border_style(Style::default().fg(Color::Rgb(55, 65, 81))))
                        .highlight_style(Style::default()
                            .bg(Color::Rgb(31, 41, 55)));
                    
                    frame.render_stateful_widget(
                        list,
                        popup_area,
                        &mut self.mode_selector_state
                    );
                }
                ```
             */}
            {isModeSelectorOpen && (
              <div className="absolute bottom-full left-0 mb-2 w-28 bg-terminal-header-bg border border-terminal-border rounded shadow-xl overflow-hidden animate-in zoom-in-95 duration-100">
                {modes.map((m) => (
                  <button
                    key={m.label}
                    onClick={() => {
                      setMode(m.label);
                      setIsModeSelectorOpen(false);
                    }}
                    className="w-full text-left px-3 py-2 hover:bg-gray-800 text-gray-300 hover:text-white flex items-center gap-2"
                  >
                     {/* @ratatui-item: Colored circle + label */}
                     <Circle className={`w-1.5 h-1.5 fill-current ${m.color}`} />
                     {m.label}
                  </button>
                ))}
              </div>
            )}
          </div>

          {/* =====================================================
              BUILD MODE DROPDOWN - Only in Build Mode
              =====================================================
              @ratatui-condition: Rendered only when mode == AgentMode::Build
              @ratatui-widget: Colored badge button with icon
              @ratatui-keyboard: Cmd+1/2/3 shortcuts bypass dropdown
              
              @ratatui-colors:
                - Careful: Red Rgb(252, 165, 165) bg Rgb(127, 29, 29)
                - Manual: Amber Rgb(253, 224, 71) bg Rgb(120, 53, 15)
                - Balanced: Emerald Rgb(52, 211, 153) bg Rgb(6, 95, 70)
              
              @ratatui-pattern:
              ```rust
              fn render_build_mode(&self, frame: &mut Frame, area: Rect) {
                  if self.mode != AgentMode::Build {
                      return;
                  }
                  
                  let (icon, fg, bg) = match self.build_mode {
                      BuildMode::Careful => ("🛡", Color::Rgb(252, 165, 165), Color::Rgb(127, 29, 29)),
                      BuildMode::Manual => ("⚡", Color::Rgb(253, 224, 71), Color::Rgb(120, 53, 15)),
                      BuildMode::Balanced => ("⚖", Color::Rgb(52, 211, 153), Color::Rgb(6, 95, 70)),
                  };
                  
                  let text = vec![
                      Span::raw(icon),
                      Span::raw(" "),
                      Span::styled(
                          format!("{:?}", self.build_mode),
                          Style::default().fg(fg)
                      ),
                      Span::raw(" "),
                      Span::styled(
                          if self.build_mode_selector_open { "▲" } else { "▼" },
                          Style::default().fg(fg)
                      ),
                  ];
                  
                  let paragraph = Paragraph::new(Line::from(text))
                      .style(Style::default().bg(bg));
                  frame.render_widget(paragraph, area);
              }
              ```
           */}
          {mode === 'Build' && (
            <div className="relative border-l border-gray-700/50 pl-1.5">
              <button
                onClick={() => setIsBuildModeSelectorOpen(!isBuildModeSelectorOpen)}
                className={`flex items-center gap-1 px-1.5 py-0.5 rounded transition-colors border border-gray-700/50 ${
                  buildMode === 'Careful' ? 'bg-red-500/10 text-red-300 hover:bg-red-500/20' :
                  buildMode === 'Manual' ? 'bg-amber-500/10 text-amber-300 hover:bg-amber-500/20' :
                  'bg-emerald-500/10 text-emerald-300 hover:bg-emerald-500/20'
                }`}
              >
                {/* @ratatui-icons: "🛡" "⚡" "⚖" Unicode symbols */}
                {buildMode === 'Careful' && <Shield className="w-2.5 h-2.5" />}
                {buildMode === 'Manual' && <Zap className="w-2.5 h-2.5" />}
                {buildMode === 'Balanced' && <Gauge className="w-2.5 h-2.5" />}
                <span className="text-[10px] font-medium">{buildMode}</span>
                <ChevronUp className={`w-2.5 h-2.5 opacity-60 transition-transform ${isBuildModeSelectorOpen ? 'rotate-180' : ''}`} />
              </button>

              {/* =====================================================
                  BUILD MODE DROPDOWN POPUP
                  =====================================================
                  @ratatui-widget: Popup List with descriptions
                  @ratatui-items: 3 options with icon, name, desc, shortcut
                  @ratatui-keyboard: Cmd+1/2/3 shown as hints
                  
                  @ratatui-pattern:
                  ```rust
                  fn render_build_mode_popup(&mut self, frame: &mut Frame, anchor: Rect) {
                      let items = vec![
                          ("🛡 Careful", "Safe & Validated", "⌘1"),
                          ("⚡ Manual", "User Control", "⌘2"),
                          ("⚖ Balanced", "Optimized", "⌘3"),
                      ];
                      
                      let list_items: Vec<ListItem> = items
                          .iter()
                          .enumerate()
                          .map(|(idx, (name, desc, key))| {
                              let is_selected = match (idx, &self.build_mode) {
                                  (0, BuildMode::Careful) => true,
                                  (1, BuildMode::Manual) => true,
                                  (2, BuildMode::Balanced) => true,
                                  _ => false,
                              };
                              
                              let lines = vec![
                                  Line::from(vec![
                                      Span::raw(*name),
                                      Span::raw(" "),
                                      Span::styled(*key, Style::default()
                                          .fg(Color::Rgb(107, 114, 128))),
                                  ]),
                                  Line::styled(*desc, Style::default()
                                      .fg(Color::Rgb(107, 114, 128))),
                              ];
                              
                              ListItem::new(lines)
                          })
                          .collect();
                      
                      // Render popup...
                  }
                  ```
               */}
              {isBuildModeSelectorOpen && (
                <div className="absolute bottom-full left-0 mb-2 w-36 bg-terminal-header-bg border border-terminal-border rounded shadow-xl overflow-hidden animate-in zoom-in-95 duration-100 z-50">
                  {([
                    { mode: 'Careful' as BuildMode, icon: Shield, color: 'text-red-400', desc: 'Safe & Validated', key: '1' },
                    { mode: 'Manual' as BuildMode, icon: Zap, color: 'text-amber-400', desc: 'User Control', key: '2' },
                    { mode: 'Balanced' as BuildMode, icon: Gauge, color: 'text-emerald-400', desc: 'Optimized', key: '3' },
                  ]).map((item) => {
                    const Icon = item.icon;
                    const isSelected = buildMode === item.mode;
                    return (
                      <button
                        key={item.mode}
                        onClick={() => {
                          setBuildMode(item.mode);
                          setIsBuildModeSelectorOpen(false);
                        }}
                        className={`w-full text-left px-3 py-2 hover:bg-gray-800 flex items-center gap-2 ${
                          isSelected ? 'bg-gray-800/50' : ''
                        }`}
                      >
                        <Icon className={`w-3 h-3 ${item.color}`} />
                        <div className="flex-1">
                          <div className={`text-xs font-medium ${isSelected ? item.color : 'text-gray-300'}`}>
                            {item.mode}
                          </div>
                          <div className="text-[10px] text-gray-500">{item.desc}</div>
                        </div>
                        <span className="text-[10px] text-gray-600">⌘{item.key}</span>
                      </button>
                    );
                  })}
                </div>
              )}
            </div>
          )}

          {/* =====================================================
              THINKING MODE TOGGLE
              =====================================================
              @ratatui-widget: Toggle indicator (colored Span)
              @ratatui-keyboard: Ctrl+T to toggle
              @ratatui-state: thinking_enabled: bool
              
              @ratatui-pattern:
              ```rust
              fn render_thinking_toggle(&self) -> Span {
                  let (icon, color) = if self.thinking_enabled {
                      ("🧠", Color::Rgb(209, 213, 219))
                  } else {
                      ("🧠", Color::Rgb(75, 85, 99))
                  };
                  Span::styled(icon, Style::default().fg(color))
              }
              ```
           */}
          <button
            onClick={() => setThinkingEnabled(!thinkingEnabled)}
            className="flex items-center p-1 rounded transition-all hover:bg-mode-hover-bg"
            title={thinkingEnabled ? "Thinking enabled" : "Thinking disabled"}
            aria-label={thinkingEnabled ? "Disable thinking mode" : "Enable thinking mode"}
            aria-pressed={thinkingEnabled}
          >
            {/* @ratatui-icon: "🧠" Simple brain icon - themed color when active */}
            <Brain className={`w-4 h-4 transition-all ${thinkingEnabled ? 'text-thinking-active' : 'text-thinking-inactive opacity-50'}`} />
          </button>
          
          {/* =====================================================
              TASK QUEUE INDICATOR
              =====================================================
              @ratatui-widget: Queue icon with count badge
              @ratatui-state: task_queue_count: usize
              @ratatui-style: Muted when queue empty, highlighted when tasks waiting
              @ratatui-icon: "📋" or list icon with count
              
              @ratatui-pattern:
              ```rust
              fn render_queue_indicator(&self, frame: &mut Frame, area: Rect) {
                  let count = self.task_queue_count;
                  let style = if count > 0 {
                      Style::default().fg(Color::Rgb(148, 226, 213)) // Teal when active
                  } else {
                      Style::default().fg(Color::Rgb(107, 114, 128)) // Muted when empty
                  };
                  let text = format!("📋 {}", count);
                  let para = Paragraph::new(text).style(style);
                  frame.render_widget(para, area);
              }
              ```
           */}
          <div 
            className={`flex items-center gap-1 px-2 py-1 rounded transition-all ${
              taskQueueCount > 0 
                ? 'text-teal-400' 
                : 'text-gray-500 opacity-50'
            }`}
            title={`${taskQueueCount} tasks in queue`}
          >
            <ListTodo className="w-4 h-4" />
            <span className="text-[11px] font-medium tabular-nums">{taskQueueCount}</span>
          </div>
        </div>

        {/* =====================================================
            RIGHT SIDE: LLM CONNECTION STATUS (CLICKABLE!)
            =====================================================
            @ratatui-widget: Clickable Paragraph with status indicator
            @ratatui-alignment: Right-aligned in status bar
            
            *** IMPORTANT: CLICKING THIS TRIGGERS PROVIDER PICKER ***
            - Click this button → Show Provider Picker modal
            - Same as typing "/model" command
            - After provider selected → Model Picker automatically shown
            
            @ratatui-events:
              - Mouse click → active_system_modal = SystemModal::ProviderPicker
              - Keyboard: Tab to focus, Enter to activate
            
            @ratatui-pattern:
            ```rust
            fn render_llm_status(&self, frame: &mut Frame, area: Rect) {
                let status_icon = match self.connection_status {
                    ConnectionStatus::Active => "●",
                    ConnectionStatus::Error => "●",
                };
                
                let status_color = match self.connection_status {
                    ConnectionStatus::Active => Color::Rgb(16, 185, 129), // Emerald
                    ConnectionStatus::Error => Color::Rgb(245, 158, 11),  // Amber
                };
                
                let text = vec![
                    Span::styled(status_icon, Style::default()
                        .fg(status_color)),
                    Span::raw(" "),
                    Span::styled(&self.llm_model, Style::default()
                        .fg(Color::Rgb(229, 231, 235))
                        .add_modifier(Modifier::BOLD)),
                    Span::raw(" "),
                    Span::styled(
                        &self.llm_provider.to_uppercase(),
                        Style::default().fg(Color::Rgb(107, 114, 128))
                    ),
                ];
                
                let paragraph = Paragraph::new(Line::from(text))
                    .alignment(Alignment::Right)
                    .style(Style::default()
                        .bg(Color::Rgb(17, 24, 39)));
                
                frame.render_widget(paragraph, area);
            }
            
            // Handle click on LLM status area
            fn handle_mouse(&mut self, mouse: MouseEvent, llm_status_area: Rect) {
                if llm_status_area.contains(mouse.position) && mouse.kind == MouseEventKind::Down(MouseButton::Left) {
                    self.active_system_modal = SystemModal::ProviderPicker;
                }
            }
            ```
         */}
        <div className="flex items-center gap-2">
          {/* This entire div is CLICKABLE to trigger provider picker */}
          <button 
            type="button"
            onClick={() => setActiveSystemModal('provider')}
            className="flex items-center gap-1.5 px-1.5 py-0.5 rounded bg-gray-900 border border-gray-800 hover:bg-gray-800 hover:border-gray-700 transition-colors cursor-pointer"
            title="Click to change model/provider (or type /model)"
          >
             {/* @ratatui-status: "●" colored circle indicator */}
             {connectionStatus === 'active' ? (
                <div className="w-1.5 h-1.5 rounded-full bg-llm-connected shadow-[0_0_4px_rgba(16,185,129,0.5)]" />
             ) : (
                <div className="w-1.5 h-1.5 rounded-full bg-amber-500 shadow-[0_0_4px_rgba(245,158,11,0.5)]" />
             )}
             
             <div className="flex items-baseline gap-1">
               <span className="text-[11px] text-gray-200 font-medium">{currentModel}</span>
               <span className="text-[9px] text-gray-500 uppercase tracking-wide">{currentProvider}</span>
             </div>
          </button>
          
          {/* =====================================================
              HELP BUTTON - Opens shortcuts/help modal
              =====================================================
              @ratatui-widget: Clickable "?" icon button
              @ratatui-trigger: Click or type "/help" command
              @ratatui-modal: Opens help/shortcuts picker modal
              
              @ratatui-pattern:
              ```rust
              fn render_help_button(&self, frame: &mut Frame, area: Rect) {
                  let help_btn = Span::styled(
                      " ? ",
                      Style::default()
                          .fg(Color::Rgb(156, 163, 175))
                          .add_modifier(Modifier::BOLD)
                  );
                  frame.render_widget(Paragraph::new(help_btn), area);
              }
              ```
           */}
          <button
            type="button"
            onClick={() => setActiveSystemModal('help')}
            className="flex items-center justify-center p-1 rounded transition-colors hover:bg-[var(--mode-hover-bg)]"
            title="Help & Shortcuts (/help)"
          >
            <HelpCircle className="w-4 h-4 text-[var(--foreground)]/50 hover:text-[var(--foreground)]/80 transition-colors" />
          </button>
        </div>
        </div>
      </div>

      {/* =====================================================
          SYSTEM MODAL OVERLAYS (Provider/Model/File Picker)
          =====================================================
          @ratatui-type: SYSTEM MODAL - NOT questions, user-triggered
          @ratatui-display: Centered floating modal with dimmed background
          @ratatui-pattern:
          ```rust
          // Render modal on top of main UI
          if self.active_system_modal != SystemModal::None {
              // 1. Render dimmed background
              let dim = Block::default().style(Style::default().bg(Color::Rgb(0, 0, 0)));
              frame.render_widget(Clear, frame.size());
              // 2. Render centered modal
              let modal_area = centered_rect(60, 20, frame.size());
              self.render_modal_content(frame, modal_area);
          }
          ```
       */}
      {activeSystemModal === 'provider' && (
        <div className="absolute inset-0 bg-modal-overlay flex items-center justify-center z-50">
          <div className="bg-modal-bg border border-modal-border rounded-lg w-96 max-h-96 overflow-hidden">
            <div className="border-b border-gray-700 px-4 py-2 flex justify-between items-center">
              <span className="text-gray-300 font-medium">Select Provider</span>
              <button onClick={() => setActiveSystemModal(null)} className="text-gray-500 hover:text-gray-300">✕</button>
            </div>
            <div className="p-2 border-b border-gray-800">
              <input 
                type="text" 
                placeholder="> Type to filter..." 
                value={modalFilterText}
                onChange={(e) => setModalFilterText(e.target.value)}
                className="w-full bg-gray-800 text-gray-200 px-3 py-2 rounded text-sm focus:outline-none focus:ring-1 focus:ring-blue-500"
                autoFocus
              />
            </div>
            <div className="max-h-64 overflow-y-auto">
              {[
                { id: "openai", name: "OpenAI", icon: "🤖", status: "active" },
                { id: "anthropic", name: "Claude", icon: "🎭", status: "warning" },
                { id: "github", name: "GitHub Copilot", icon: "🐙", status: "warning" },
                { id: "google", name: "Google Gemini", icon: "💎", status: "warning" },
                { id: "openrouter", name: "OpenRouter", icon: "🌐", status: "warning" },
                { id: "ollama", name: "Ollama", icon: "🦙", status: "warning" },
              ].filter(p => !modalFilterText || p.name.toLowerCase().includes(modalFilterText.toLowerCase()))
               .map((provider, idx) => (
                <button
                  key={provider.id}
                  onClick={() => {
                    // Track selected provider and show model picker
                    setPendingProvider({ id: provider.id, name: provider.name });
                    setActiveSystemModal('model');
                    setModalFilterText('');
                  }}
                  className={`w-full flex items-center gap-3 px-4 py-2.5 text-left hover:bg-gray-800 transition-colors ${idx === modalSelectedIndex ? 'bg-gray-800' : ''}`}
                >
                  {provider.status === 'active' && <span className="text-green-400">●</span>}
                  {provider.status === 'warning' && <span className="text-yellow-400">⚠</span>}
                  <span>{provider.icon}</span>
                  <span className="text-gray-200 font-medium">{provider.name}</span>
                </button>
              ))}
            </div>
          </div>
        </div>
      )}

      {activeSystemModal === 'model' && (
        <div className="absolute inset-0 bg-modal-overlay flex items-center justify-center z-50">
          <div className="bg-modal-bg border border-modal-border rounded-lg w-96 max-h-[28rem] overflow-hidden">
            <div className="border-b border-gray-700 px-4 py-2 flex justify-between items-center">
              <span className="text-gray-300 font-medium">Select Model</span>
              <button onClick={() => setActiveSystemModal(null)} className="text-gray-500 hover:text-gray-300">✕</button>
            </div>
            <div className="p-2 border-b border-gray-800">
              <input 
                type="text" 
                placeholder="> Type to filter..." 
                value={modalFilterText}
                onChange={(e) => setModalFilterText(e.target.value)}
                className="w-full bg-gray-800 text-gray-200 px-3 py-2 rounded text-sm focus:outline-none focus:ring-1 focus:ring-blue-500"
                autoFocus
              />
            </div>
            <div className="max-h-80 overflow-y-auto">
              {[
                { id: "gpt-4o", name: "GPT-4o", capabilities: ["tools", "vision"] },
                { id: "gpt-4-turbo", name: "GPT-4 Turbo", capabilities: ["tools", "vision"] },
                { id: "claude-3-opus", name: "Claude 3 Opus", capabilities: ["tools", "vision"], isLatest: true },
                { id: "claude-3-sonnet", name: "Claude 3.5 Sonnet", capabilities: ["tools", "vision"] },
                { id: "gemini-pro", name: "Gemini Pro", capabilities: ["tools"] },
                { id: "o1-preview", name: "o1-preview", capabilities: ["reasoning"], isLatest: true },
              ].filter(m => !modalFilterText || m.name.toLowerCase().includes(modalFilterText.toLowerCase()))
               .map((model, idx) => (
                <button
                  key={model.id}
                  onClick={() => {
                    // Update the selected model and provider
                    setCurrentModel(model.name);
                    if (pendingProvider) {
                      setCurrentProvider(pendingProvider.name);
                    }
                    setPendingProvider(null);
                    setActiveSystemModal(null);
                    setModalFilterText('');
                  }}
                  className={`w-full flex items-center gap-3 px-4 py-2.5 text-left hover:bg-gray-800 transition-colors ${idx === modalSelectedIndex ? 'bg-gray-800' : ''}`}
                >
                  <span className={`${model.isLatest ? 'text-yellow-400' : 'text-gray-600'}`}>●</span>
                  <span className={`font-medium ${model.isLatest ? 'text-yellow-300' : 'text-gray-200'}`}>{model.name}</span>
                  <span className="text-gray-500 text-xs">{model.capabilities.join(', ')}</span>
                </button>
              ))}
            </div>
          </div>
        </div>
      )}

      {activeSystemModal === 'file' && (
        <div className="absolute inset-0 bg-modal-overlay flex items-center justify-center z-50">
          <div className="bg-modal-bg border border-modal-border rounded-lg w-80 max-h-96 overflow-hidden">
            <div className="border-b border-gray-700 px-4 py-2 flex justify-between items-center">
              <span className="text-gray-300 font-medium">Select Files (@mentions)</span>
              <button onClick={() => setActiveSystemModal(null)} className="text-gray-500 hover:text-gray-300">✕</button>
            </div>
            <div className="p-2 border-b border-gray-800">
              <div className="flex items-center gap-2 bg-gray-800 px-3 py-2 rounded text-sm">
                <span className="text-[var(--context-text)]">@</span>
                <input 
                  type="text" 
                  placeholder="Type to filter files..." 
                  value={modalFilterText}
                  onChange={(e) => setModalFilterText(e.target.value)}
                  className="flex-1 bg-transparent text-gray-200 focus:outline-none"
                  autoFocus
                />
              </div>
            </div>
            <div className="max-h-48 overflow-y-auto">
              {[
                { path: "src/app/App.tsx", isFolder: false },
                { path: "src/app/components/Terminal.tsx", isFolder: false },
                { path: "src/app/components/Sidebar.tsx", isFolder: false },
                { path: "src/styles/", isFolder: true },
                { path: "package.json", isFolder: false },
                { path: "README.md", isFolder: false },
              ].filter(f => !modalFilterText || f.path.toLowerCase().includes(modalFilterText.toLowerCase()))
               .map((file, idx) => {
                // Check if file is already in context (by checking input for @filename)
                const fileName = file.path.split('/').pop() || file.path;
                const isSelected = input.includes(`@${fileName}`) || addedContextFiles.some(f => f.path === file.path);
                
                return (
                  <button
                    key={file.path}
                    onClick={() => {
                      if (!file.isFolder) {
                        const fileName = file.path.split('/').pop() || file.path;
                        
                        if (isSelected) {
                          // Remove from context and input
                          setAddedContextFiles(prev => prev.filter(f => f.path !== file.path));
                          // Remove @filename from input (handle trailing @ that triggered picker)
                          const cleanedInput = input
                            .replace(new RegExp(`@${fileName}\\s*`, 'g'), '')
                            .replace(/@$/, ''); // Remove trailing @ if it triggered the picker
                          onInputChange(cleanedInput);
                        } else {
                          // Add to context
                          const newFile: ContextFile = {
                            name: fileName,
                            path: file.path,
                          };
                          setAddedContextFiles(prev => [...prev, newFile]);
                          
                          // Add @filename to input text (replace trailing @ if present)
                          const newInput = input.endsWith('@') 
                            ? input.slice(0, -1) + `@${fileName} `
                            : input + `@${fileName} `;
                          onInputChange(newInput);
                        }
                      }
                    }}
                    className={`w-full flex items-center gap-2 px-4 py-2 text-left hover:bg-gray-800 transition-colors ${isSelected ? 'bg-blue-900/30 border-l-2 border-blue-500' : ''}`}
                  >
                    {file.isFolder ? (
                      <span className="text-[var(--picker-folder)]">📁</span>
                    ) : isSelected ? (
                      <span className="text-blue-400">✓</span>
                    ) : (
                      <span className="text-gray-400">📄</span>
                    )}
                    <span className={`text-sm ${file.isFolder ? 'text-[var(--picker-folder)]' : isSelected ? 'text-[var(--picker-selected-border)]' : 'text-[var(--foreground)]'}`}>{file.path}</span>
                  </button>
                );
              })}
            </div>
            {/* Done button to close picker */}
            <div className="border-t border-gray-700 p-2">
              <button
                onClick={() => {
                  setActiveSystemModal(null);
                  setModalFilterText('');
                }}
                className="w-full py-2 bg-blue-600 hover:bg-blue-700 text-white rounded text-sm font-medium transition-colors"
              >
                Done ({addedContextFiles.length} files selected)
              </button>
            </div>
          </div>
        </div>
      )}
      
      {/* =========================================================
          THEME PICKER MODAL
          =========================================================
          @ratatui-type: SYSTEM MODAL
          @ratatui-trigger: Type "/theme" command in input area
          @ratatui-widget: Centered floating popup with theme list
          @ratatui-layout: 
            - Centered rect over main UI (e.g., 50x20)
            - Constraint::Length(1) for filter input
            - Constraint::Min(0) for scrollable list
          @ratatui-style:
            - Background: modal-bg with semi-transparent overlay
            - Border: modal-border
            - Title: "Select Theme" in border
          @ratatui-state:
            - active_system_modal: SystemModal::ThemePicker
            - modal_selected_index: usize
            - modal_filter_text: String
          @ratatui-events:
            - Escape: Close modal without selection
            - Up/Down: Navigate list
            - Enter: Select theme and apply
            - Typing: Filter list
      */}
      {activeSystemModal === 'theme' && (
        <div className="fixed inset-0 bg-black/60 flex items-center justify-center z-50">
          <div className="w-[450px] bg-[var(--modal-bg)] border border-[var(--modal-border)] rounded-lg shadow-2xl overflow-hidden">
            {/* Header */}
            <div className="flex items-center justify-between p-4 border-b border-[var(--modal-border)]">
              <div className="flex items-center gap-2">
                <Palette className="w-5 h-5 text-[var(--context-text)]" />
                <span className="text-[var(--foreground)] font-medium">Select Theme</span>
              </div>
              <button onClick={() => setActiveSystemModal(null)} className="text-[var(--foreground)]/50 hover:text-[var(--foreground)]">✕</button>
            </div>
            
            {/* Filter input */}
            <div className="p-3 border-b border-[var(--modal-border)]">
              <div className="flex items-center gap-2 bg-[var(--terminal-bg)] rounded px-3 py-2">
                <Search className="w-4 h-4 text-[var(--foreground)]/50" />
                <input
                  type="text"
                  value={modalFilterText}
                  onChange={(e) => {
                    setModalFilterText(e.target.value);
                    setModalSelectedIndex(0);
                  }}
                  placeholder="Search themes..."
                  className="flex-1 bg-transparent text-[var(--foreground)] text-sm outline-none placeholder:text-[var(--foreground)]/40"
                  autoFocus
                />
              </div>
            </div>
            
            {/* Theme list */}
            <div className="max-h-[350px] overflow-y-auto">
              {themePresets
                .filter(theme => 
                  theme.name.toLowerCase().includes(modalFilterText.toLowerCase()) ||
                  theme.description.toLowerCase().includes(modalFilterText.toLowerCase())
                )
                .map((theme, index) => {
                  const isSelected = theme.id === currentTheme.id;
                  const isHovered = index === modalSelectedIndex;
                  return (
                    <button
                      key={theme.id}
                      onClick={() => {
                        applyTheme(theme);
                        setCurrentTheme(theme);
                        setActiveSystemModal(null);
                        setModalFilterText('');
                      }}
                      className={`w-full flex items-center gap-3 px-4 py-3 text-left transition-colors ${
                        isHovered ? 'bg-[var(--picker-hover-bg)]' : ''
                      } ${isSelected ? 'bg-[var(--picker-selected-bg)] border-l-2 border-[var(--picker-selected-border)]' : ''}`}
                    >
                      {/* Theme indicator (dark/light) */}
                      <div className={`w-8 h-8 rounded-lg flex items-center justify-center ${
                        theme.isDark ? 'bg-gray-800' : 'bg-gray-200'
                      }`}>
                        {theme.isDark ? (
                          <Moon className="w-4 h-4 text-blue-400" />
                        ) : (
                          <Sun className="w-4 h-4 text-amber-500" />
                        )}
                      </div>
                      
                      {/* Theme info */}
                      <div className="flex-1 min-w-0">
                        <div className="flex items-center gap-2">
                          <span className="text-sm font-medium text-[var(--foreground)]">{theme.name}</span>
                          {isSelected && (
                            <Check className="w-4 h-4 text-[var(--llm-connected)]" />
                          )}
                        </div>
                        <span className="text-xs text-[var(--foreground)]/60">{theme.description}</span>
                      </div>
                      
                      {/* Color preview swatches */}
                      <div className="flex gap-1">
                        <div className="w-3 h-3 rounded-full" style={{ backgroundColor: theme.colors.background }} title="Background" />
                        <div className="w-3 h-3 rounded-full" style={{ backgroundColor: theme.colors.llmConnected }} title="Accent" />
                        <div className="w-3 h-3 rounded-full" style={{ backgroundColor: theme.colors.msgQuestion }} title="Question" />
                        <div className="w-3 h-3 rounded-full" style={{ backgroundColor: theme.colors.contextText }} title="Context" />
                      </div>
                    </button>
                  );
                })}
            </div>
            
            {/* Current theme indicator */}
            <div className="border-t border-[var(--modal-border)] p-3 flex items-center justify-between">
              <span className="text-xs text-[var(--foreground)]/50">
                Current: <span className="text-[var(--context-text)]">{currentTheme.name}</span>
              </span>
              <span className="text-xs text-[var(--foreground)]/40">
                Press Enter to select, Esc to close
              </span>
            </div>
          </div>
        </div>
      )}

      {/* =====================================================
          HELP & SHORTCUTS MODAL
          =====================================================
          @ratatui-modal: Help/shortcuts popup
          @ratatui-trigger: Click "?" button or type "/help"
          @ratatui-keyboard: Esc to close
          
          @ratatui-pattern:
          ```rust
          fn render_help_modal(&self, frame: &mut Frame) {
              let help_items = vec![
                  ("General", vec![
                      ("Enter", "Send message"),
                      ("Ctrl+C", "Cancel/Interrupt"),
                      ("Esc", "Close modal"),
                  ]),
                  ("Commands", vec![
                      ("/model", "Change model/provider"),
                      ("/theme", "Change theme"),
                      ("/help", "Show this help"),
                      ("@", "Add file to context"),
                  ]),
              ];
              // Render as Block with List inside...
          }
          ```
       */}
      {activeSystemModal === 'help' && (
        <div className="fixed inset-0 bg-black/60 flex items-center justify-center z-50">
          <div className="w-[500px] bg-[var(--modal-bg)] border border-[var(--modal-border)] rounded-lg shadow-2xl overflow-hidden">
            {/* Header */}
            <div className="flex items-center justify-between p-4 border-b border-[var(--modal-border)]">
              <div className="flex items-center gap-2">
                <HelpCircle className="w-5 h-5 text-[var(--context-text)]" />
                <span className="text-lg font-semibold text-[var(--foreground)]">Help & Shortcuts</span>
              </div>
              <button
                onClick={() => setActiveSystemModal(null)}
                className="text-[var(--foreground)]/50 hover:text-[var(--foreground)] transition-colors"
              >
                <X className="w-5 h-5" />
              </button>
            </div>
            
            {/* Content */}
            <div className="p-4 max-h-[60vh] overflow-y-auto space-y-6">
              {/* General Shortcuts */}
              <div>
                <h3 className="text-xs uppercase tracking-wider text-[var(--foreground)]/50 font-semibold mb-3">General</h3>
                <div className="space-y-2">
                  {[
                    { key: 'Enter', desc: 'Send message' },
                    { key: 'Shift+Enter', desc: 'New line in input' },
                    { key: 'Ctrl+C', desc: 'Cancel / Interrupt agent' },
                    { key: 'Esc', desc: 'Close modal / Cancel' },
                    { key: '↑ / ↓', desc: 'Navigate history' },
                  ].map((item) => (
                    <div key={item.key} className="flex items-center justify-between">
                      <span className="text-sm text-[var(--foreground)]/80">{item.desc}</span>
                      <kbd className="px-2 py-1 text-xs font-mono bg-[var(--terminal-bg)] border border-[var(--terminal-border)] rounded text-[var(--foreground)]/70">
                        {item.key}
                      </kbd>
                    </div>
                  ))}
                </div>
              </div>
              
              {/* Commands */}
              <div>
                <h3 className="text-xs uppercase tracking-wider text-[var(--foreground)]/50 font-semibold mb-3">Commands</h3>
                <div className="space-y-2">
                  {[
                    { cmd: '/model', desc: 'Change model & provider' },
                    { cmd: '/theme', desc: 'Change color theme' },
                    { cmd: '/help', desc: 'Show this help dialog' },
                    { cmd: '/clear', desc: 'Clear terminal output' },
                  ].map((item) => (
                    <div key={item.cmd} className="flex items-center justify-between">
                      <span className="text-sm text-[var(--foreground)]/80">{item.desc}</span>
                      <code className="px-2 py-1 text-xs font-mono bg-[var(--context-bg)] border border-[var(--context-border)] rounded text-[var(--context-text)]">
                        {item.cmd}
                      </code>
                    </div>
                  ))}
                </div>
              </div>
              
              {/* Context & Files */}
              <div>
                <h3 className="text-xs uppercase tracking-wider text-[var(--foreground)]/50 font-semibold mb-3">Context & Files</h3>
                <div className="space-y-2">
                  {[
                    { key: '@', desc: 'Open file picker to add context' },
                    { key: '+ button', desc: 'Add file to context' },
                    { key: 'Backspace', desc: 'Remove @mention from input' },
                  ].map((item) => (
                    <div key={item.key} className="flex items-center justify-between">
                      <span className="text-sm text-[var(--foreground)]/80">{item.desc}</span>
                      <kbd className="px-2 py-1 text-xs font-mono bg-[var(--terminal-bg)] border border-[var(--terminal-border)] rounded text-[var(--foreground)]/70">
                        {item.key}
                      </kbd>
                    </div>
                  ))}
                </div>
              </div>
              
              {/* Mode Shortcuts */}
              <div>
                <h3 className="text-xs uppercase tracking-wider text-[var(--foreground)]/50 font-semibold mb-3">Mode Shortcuts</h3>
                <div className="space-y-2">
                  {[
                    { key: '⌘1', desc: 'Careful mode (Safe & Validated)' },
                    { key: '⌘2', desc: 'Manual mode (User Control)' },
                    { key: '⌘3', desc: 'Balanced mode (Optimized)' },
                    { key: 'Ctrl+T', desc: 'Toggle thinking mode' },
                  ].map((item) => (
                    <div key={item.key} className="flex items-center justify-between">
                      <span className="text-sm text-[var(--foreground)]/80">{item.desc}</span>
                      <kbd className="px-2 py-1 text-xs font-mono bg-[var(--terminal-bg)] border border-[var(--terminal-border)] rounded text-[var(--foreground)]/70">
                        {item.key}
                      </kbd>
                    </div>
                  ))}
                </div>
              </div>
            </div>
            
            {/* Footer */}
            <div className="border-t border-[var(--modal-border)] p-3 flex items-center justify-between bg-[var(--terminal-bg)]/50">
              <span className="text-xs text-[var(--foreground)]/40">
                Press <kbd className="px-1.5 py-0.5 text-[10px] bg-[var(--terminal-bg)] border border-[var(--terminal-border)] rounded">Esc</kbd> to close
              </span>
              <span className="text-xs text-[var(--foreground)]/40">
                v2.1.0
              </span>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

/**
 * ============================================================================
 * TERMINAL COMPONENT SUMMARY FOR RATATUI
 * ============================================================================
 * 
 * This Terminal component maps to a complex Ratatui widget structure:
 * 
 * 1. **Layout**: 4 vertical sections (Header, Output, Status, Input)
 * 2. **State Management**: All useState hooks become fields in App struct
 * 3. **Event Handling**: onClick/onChange become KeyEvent/MouseEvent handlers
 * 4. **Rendering**: JSX conditionals become if statements in render functions
 * 5. **Animations**: Fade-ins and transitions not directly available; instant rendering
 * 6. **Hover Effects**: Replace with selection/focus states
 * 7. **Copy to Clipboard**: Use arboard or clipboard crate with key binding
 * 8. **Scrolling**: Use ScrollbarState and Scrollbar widget
 * 9. **Popups**: Render List widgets as overlays with conditional positioning
 * 10. **Icons**: Use Unicode characters or nerd fonts
 * 
 * Key Crates Needed:
 * - ratatui: Core TUI framework
 * - crossterm: Terminal control and events
 * - arboard or clipboard: Clipboard operations
 * - unicode-width: Proper text width calculations
 * - textwrap: Word wrapping for long messages
 */

// --- Preview / Implementation Wrapper ---
// NOTE: Component name uses configurable agent name
export default function AgentTerminalPreview() {
  const [inputValue, setInputValue] = useState('');
  
  // Meaningful conversation data representing a backend setup
  // NOTE: System messages use configurable agent name and version from AppConfig
  const [lines, setLines] = useState<TerminalLine[]>([
    { type: 'system', content: `${defaultAppConfig.agentNameShort} Core v${defaultAppConfig.version} initialized` },
    { type: 'system', content: 'Connected to Engine. Ready for Build task.' },
    
    // Demo Approval Request - MOVED TO TOP FOR VISIBILITY
    {
      type: 'approval' as const,
      content: 'Command requires approval',
      command: 'rm -rf node_modules && npm install --force',
      riskLevel: 'high' as const,
      meta: 'This command will delete the node_modules directory and reinstall dependencies with --force flag.'
    } as TerminalLine,
    
    // User Request
    { type: 'input', content: 'Create a simple Express server with a health check endpoint and install dotenv.' },
    
    // Agent Planning Output
    { type: 'output', content: 'I will initialize a new project, install necessary dependencies, and scaffold the server file.' },
    
    // Agent using Tool (Shell)
    { type: 'tool', content: 'EXEC: mkdir my-server && cd my-server && npm init -y' },
    
    // Tool Output
    { type: 'output', content: 'Wrote to /home/user/my-server/package.json:\n{\n  "name": "my-server",\n  "version": "1.0.0",\n  "main": "index.js",\n  "scripts": {\n    "test": "echo \\"Error: no test specified\\" && exit 1"\n  },\n  "keywords": [],\n  "author": "",\n  "license": "ISC"\n}' },
    
    // Agent using Tool (Shell)
    { type: 'tool', content: 'EXEC: npm install express dotenv cors' },
    
    // Tool Output
    { type: 'output', content: 'added 68 packages, and audited 69 packages in 2s\nfound 0 vulnerabilities' },
    
    // Agent using Tool (File Write)
    { type: 'tool', content: 'WRITE: src/server.js' },
    
    // Final Code Output
    { type: 'output', content: `const express = require('express');\nconst cors = require('cors');\nrequire('dotenv').config();\n\nconst app = express();\nconst PORT = process.env.PORT || 3000;\n\napp.use(cors());\napp.use(express.json());\n\n// Health Check\napp.get('/health', (req, res) => {\n  res.status(200).json({ \n    status: 'ok', \n    uptime: process.uptime(),\n    timestamp: new Date().toISOString()\n  });\n});\n\napp.listen(PORT, () => {\n  console.log(\`Server running on port \${PORT}\`);\n});` },
    
    { type: 'system', content: 'Build task completed successfully.' },
    
    // Demo Questions for testing
    { 
      type: 'question', 
      content: 'Which package manager do you prefer?',
      questionType: 'single',
      options: [
        { id: 'npm', label: 'npm' },
        { id: 'yarn', label: 'yarn' },
        { id: 'pnpm', label: 'pnpm' },
        { id: 'bun', label: 'bun' }
      ]
    },
    { 
      type: 'question', 
      content: 'Which features should be included?',
      questionType: 'multi',
      options: [
        { id: 'auth', label: 'Authentication (JWT)' },
        { id: 'db', label: 'Database (PostgreSQL)' },
        { id: 'cache', label: 'Caching (Redis)' },
        { id: 'queue', label: 'Job Queue (Bull)' },
        { id: 'ws', label: 'WebSockets' }
      ]
    },
    { 
      type: 'question', 
      content: 'What should be the project name?',
      questionType: 'freetext',
      placeholder: 'Enter project name...'
    }
  ]);

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!inputValue.trim()) return;
    
    // Add user input
    const newLines = [...lines, { type: 'input', content: inputValue } as TerminalLine];
    setLines(newLines);
    setInputValue('');

    // Simulate agent thinking/replying (optional for demo)
    setTimeout(() => {
      setLines(prev => [...prev, { type: 'output', content: 'Processing request...' } as TerminalLine]);
    }, 600);
  };

  return (
    <div className="w-full min-h-screen bg-black flex items-center justify-center p-8">
      <Terminal 
        output={lines} 
        input={inputValue}
        onInputChange={setInputValue}
        onSubmit={handleSubmit}
        llmModel="Claude 3.5 Sonnet"
        llmProvider="Anthropic"
      />
    </div>
  );
}