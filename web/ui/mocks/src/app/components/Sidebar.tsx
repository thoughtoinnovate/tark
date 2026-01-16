/**
 * ============================================================================
 * RATATUI TUI MAPPING - SIDEBAR COMPONENT
 * ============================================================================
 * 
 * This is the sidebar panel showing session info, context, tasks, and git
 * changes. In Ratatui, this is a composite widget with collapsible sections.
 * 
 * @ratatui-structure: Custom Sidebar widget with accordion-style sections
 * @ratatui-layout: Fixed or dynamic width based on collapsed state
 * @ratatui-interaction: Keyboard navigation through sections and items
 */
import { useState } from "react";
import {
  ChevronDown,
  ChevronRight,
  Sparkles,
  Cloud,
  Database,
  CheckCircle2,
  FileText,
  Clock,
  DollarSign,
  Gauge,
  Loader2,
  LucideIcon,
  Circle,
  ChevronsLeft,
  ChevronsRight,
  FilePlus,
  FileMinus,
  FilePenLine,
  GitGraph,
  X,
  Pencil,
  Trash2,
  ArrowUp,
  ArrowDown,
  GitBranch,
  Folder,
  FileCode,
  AppWindow,
  ChevronsUpDown,
  Palette,
  Check,
} from "lucide-react";
import { useTheme } from "../themes";

/**
 * ============================================================================
 * THEME CONFIGURATION - RATATUI COLOR PALETTE
 * ============================================================================
 * 
 * @ratatui-pattern: Define colors as constants using Color::Rgb()
 * 
 * ```rust
 * pub mod theme {
 *     use ratatui::style::Color;
 *     
 *     pub const MAIN_BG: Color = Color::Rgb(22, 27, 34);       // #161b22
 *     pub const BORDER: Color = Color::Rgb(48, 54, 61);        // border-gray-800
 *     pub const TEXT_PRIMARY: Color = Color::Rgb(201, 209, 217);   // text-gray-200
 *     pub const TEXT_SECONDARY: Color = Color::Rgb(139, 148, 158); // text-gray-400
 *     pub const TEXT_MUTED: Color = Color::Rgb(107, 114, 128);     // text-gray-500
 *     pub const HOVER_BG: Color = Color::Rgb(48, 54, 61);      // Same as border
 *     pub const ACTIVE_ITEM_BG: Color = Color::Rgb(37, 99, 235);  // blue-500/10
 *     
 *     // Specific Action Colors
 *     pub const ACCENT: Color = Color::Rgb(96, 165, 250);      // blue-400
 *     pub const MUSTARD: Color = Color::Rgb(251, 191, 36);     // amber-400
 *     pub const SUCCESS: Color = Color::Rgb(52, 211, 153);     // emerald-400
 *     pub const DANGER: Color = Color::Rgb(248, 113, 113);     // red-400
 *     pub const GIT_MODIFIED: Color = Color::Rgb(234, 179, 8); // yellow-500
 *     pub const GIT_NEW: Color = Color::Rgb(34, 197, 94);      // emerald-500
 *     pub const GIT_DELETED: Color = Color::Rgb(239, 68, 68);  // red-500
 * }
 * ```
 * 
 * @ratatui-usage: Apply colors via Style::default().fg(theme::ACCENT)
 */
/**
 * @ratatui-pattern: Theme colors now come from CSS variables
 * These classes reference --var() CSS custom properties defined in theme.css
 */
const sidebarTheme = {
  // CSS variable-based classes for dynamic theming
  mainBg: "bg-[var(--terminal-bg)]",
  border: "border-[var(--terminal-border)]",
  textPrimary: "text-[var(--foreground)]",
  textSecondary: "text-[var(--foreground)]/70",
  textMuted: "text-[var(--foreground)]/50",
  hoverBg: "hover:bg-[var(--mode-hover-bg)]",
  activeItemBg: "bg-[var(--context-bg)]",
  itemHover: "hover:bg-[var(--mode-hover-bg)]",
  
  // Specific Action Colors from theme
  accent: "text-[var(--context-text)]",
  mustard: "text-[var(--thinking-active)]", // Color for Active Tasks panel
  success: "text-[var(--llm-connected)]",
  danger: "text-[var(--llm-error)]",
  gitModified: "text-[var(--thinking-active)]",
  gitNew: "text-[var(--llm-connected)]",
  gitDeleted: "text-[var(--llm-error)]",
};

/**
 * @ratatui-scrollbar: Use ratatui::widgets::Scrollbar
 * @ratatui-note: Custom scrollbar styling is handled by Scrollbar widget config
 * 
 * ```rust
 * use ratatui::widgets::{Scrollbar, ScrollbarOrientation};
 * 
 * let scrollbar = Scrollbar::default()
 *     .orientation(ScrollbarOrientation::VerticalRight)
 *     .begin_symbol(Some("↑"))
 *     .end_symbol(Some("↓"))
 *     .track_symbol(Some("│"))
 *     .thumb_symbol("█");
 * ```
 */
const scrollbarStyles = `
  .custom-scrollbar::-webkit-scrollbar {
    width: 6px;
  }
  .custom-scrollbar::-webkit-scrollbar-track {
    background: transparent; 
  }
  .custom-scrollbar::-webkit-scrollbar-thumb {
    background: #4b5563; 
    border-radius: 10px;
  }
  .custom-scrollbar::-webkit-scrollbar-thumb:hover {
    background: #6b7280; 
  }
`;

/**
 * ============================================================================
 * TYPE DEFINITIONS - RATATUI RUST EQUIVALENTS
 * ============================================================================
 */

/**
 * @ratatui-enum: GitStatusType
 * ```rust
 * #[derive(Clone, Copy, Debug, PartialEq, Eq)]
 * enum GitStatusType {
 *     Modified,
 *     New,
 *     Deleted,
 * }
 * 
 * impl GitStatusType {
 *     fn icon(&self) -> &'static str {
 *         match self {
 *             Self::Modified => "M",  // or "✎" "📝"
 *             Self::New => "A",       // or "+" "✚"
 *             Self::Deleted => "D",   // or "−" "✖"
 *         }
 *     }
 *     
 *     fn color(&self) -> Color {
 *         match self {
 *             Self::Modified => Color::Rgb(234, 179, 8),   // yellow
 *             Self::New => Color::Rgb(34, 197, 94),        // emerald
 *             Self::Deleted => Color::Rgb(239, 68, 68),    // red
 *         }
 *     }
 * }
 * ```
 */
type GitStatusType = "modified" | "new" | "deleted";

/**
 * @ratatui-enum: ContextItemType
 * ```rust
 * #[derive(Clone, Copy, Debug, PartialEq, Eq)]
 * enum ContextItemType {
 *     File,
 *     Folder,
 * }
 * 
 * impl ContextItemType {
 *     fn icon(&self) -> &'static str {
 *         match self {
 *             Self::File => "📄",
 *             Self::Folder => "📁",
 *         }
 *     }
 * }
 * ```
 */
type ContextItemType = "file" | "folder";

/**
 * @ratatui-struct: GitFileItem
 * ```rust
 * #[derive(Clone, Debug)]
 * struct GitFileItem {
 *     name: String,
 *     status_type: GitStatusType,
 *     additions: Option<usize>,
 *     deletions: Option<usize>,
 * }
 * ```
 */
interface GitFileItem {
  name: string;
  type: GitStatusType;
  additions?: number;
  deletions?: number;
}

/**
 * @ratatui-struct: ContextFileItem
 * ```rust
 * #[derive(Clone, Debug)]
 * struct ContextFileItem {
 *     name: String,
 *     item_type: ContextItemType,
 * }
 * ```
 */
interface ContextFileItem {
  name: string;
  type: ContextItemType;
}

/**
 * @ratatui-struct: CostItem
 * ```rust
 * #[derive(Clone, Debug)]
 * struct CostItem {
 *     model: String,
 *     cost: String,  // Or use f64 and format on display
 * }
 * ```
 */
interface CostItem {
  model: string;
  cost: string;
}

/**
 * @ratatui-struct: PanelItem
 * @ratatui-note: Icon represented as Unicode string, not React component
 * ```rust
 * #[derive(Clone, Debug)]
 * struct PanelItem {
 *     name: String,
 *     icon: Option<&'static str>,  // Unicode icon
 *     color: Option<Color>,
 *     id: Option<String>,
 *     is_cost_item: bool,
 * }
 * ```
 */
interface PanelItem {
  name: string;
  icon?: LucideIcon;
  color?: string;
  id?: string;
  isCostItem?: boolean;
}

/**
 * @ratatui-struct: TaskItem
 * ```rust
 * #[derive(Clone, Debug)]
 * struct TaskItem {
 *     id: String,
 *     name: String,
 *     icon: Option<&'static str>,
 * }
 * ```
 */
interface TaskItem {
  id: string;
  name: string;
  icon?: LucideIcon;
}

/**
 * @ratatui-struct: PanelData
 * @ratatui-note: Complete data structure for each sidebar panel section
 * ```rust
 * #[derive(Clone, Debug)]
 * struct PanelData {
 *     id: String,
 *     title: String,
 *     icon: &'static str,
 *     badge: Option<String>,
 *     items: Vec<PanelItem>,
 *     loaded_files: Vec<ContextFileItem>,
 *     cost_breakdown: Vec<CostItem>,
 *     git_summary: Option<GitSummary>,
 *     git_files: Vec<GitFileItem>,
 * }
 * 
 * #[derive(Clone, Debug)]
 * struct GitSummary {
 *     modified: usize,
 *     new: usize,
 *     deleted: usize,
 * }
 * ```
 */
interface PanelData {
  id: string;
  title: string;
  icon: LucideIcon;
  badge?: string;
  items?: PanelItem[];
  loadedFiles?: ContextFileItem[];
  costBreakdown?: CostItem[];
  git?: {
    summary: { modified: number; new: number; deleted: number };
    files: GitFileItem[];
  };
}

/**
 * @ratatui-props: These become fields in App struct
 * @ratatui-pattern: Sidebar is part of main App state, not separate props
 * ```rust
 * struct App {
 *     sidebar_collapsed: bool,
 *     // Callback becomes method: pub fn toggle_sidebar(&mut self)
 * }
 * ```
 */
interface SidebarProps {
  isCollapsed?: boolean;
  onCollapsedChange?: (collapsed: boolean) => void;
}

/**
 * ============================================================================
 * SIDEBAR COMPONENT - MAIN RENDER LOGIC
 * ============================================================================
 * 
 * @ratatui-layout: Fixed or dynamic width based on collapsed state
 *   - Collapsed: 3-5 columns wide (shows only icons)
 *   - Expanded: 30-40 columns wide (shows full content)
 * 
 * @ratatui-structure: Vertical list of accordion sections
 * @ratatui-scroll: Scrollable content area with ScrollbarState
 * 
 * @ratatui-pattern:
 * ```rust
 * fn render_sidebar(&mut self, frame: &mut Frame, area: Rect) {
 *     let width = if self.sidebar_collapsed {
 *         Constraint::Length(3)
 *     } else {
 *         Constraint::Length(35)
 *     };
 *     
 *     let chunks = Layout::vertical([
 *         Constraint::Length(3),  // Header
 *         Constraint::Min(0),     // Scrollable panels
 *     ]).split(area);
 *     
 *     self.render_sidebar_header(frame, chunks[0]);
 *     self.render_sidebar_panels(frame, chunks[1]);
 * }
 * ```
 */
export function Sidebar({ isCollapsed, onCollapsedChange }: SidebarProps) {
  /**
   * ============================================================================
   * SIDEBAR STATE - RATATUI APP STRUCT FIELDS
   * ============================================================================
   */
  
  /**
   * @ratatui-state: Theme management via useTheme hook
   * @ratatui-behavior: Allows switching between theme presets
   */
  const { currentTheme, themes, switchTheme } = useTheme();
  const [isThemePickerOpen, setIsThemePickerOpen] = useState(false);
  
  /**
   * @ratatui-state: expanded_sections: HashSet<String>
   * @ratatui-default: {"session", "context", "tasks", "git"}
   * @ratatui-behavior: Tracks which accordion sections are open
   * @ratatui-keyboard: Enter/Space on section header to toggle
   * ```rust
   * use std::collections::HashSet;
   * 
   * struct SidebarState {
   *     expanded_sections: HashSet<String>,
   * }
   * 
   * impl SidebarState {
   *     fn toggle_section(&mut self, section: &str) {
   *         if !self.expanded_sections.remove(section) {
   *             self.expanded_sections.insert(section.to_string());
   *         }
   *     }
   * }
   * ```
   */
  const [expandedSections, setExpandedSections] = useState<Set<string>>(
    new Set(["session", "context", "tasks", "git"])
  );
  
  // Use controlled state if props provided, otherwise use internal state
  const [internalCollapsed, setInternalCollapsed] = useState(false);
  const isSidebarCollapsed = isCollapsed ?? internalCollapsed;
  const setIsSidebarCollapsed = onCollapsedChange ?? setInternalCollapsed;
  
  /**
   * @ratatui-state: cost_expanded: bool
   * @ratatui-behavior: Nested expansion within Session panel
   * @ratatui-keyboard: Enter on cost item to toggle
   */
  const [isCostExpanded, setIsCostExpanded] = useState(false);
  
  /**
   * @ratatui-state: loaded_files_expanded: bool
   * @ratatui-behavior: Nested expansion within Context panel
   * @ratatui-default: true (expanded by default)
   */
  const [isLoadedFilesExpanded, setIsLoadedFilesExpanded] = useState(true);

  /**
   * ============================================================================
   * TASKS STATE MANAGEMENT
   * ============================================================================
   * 
   * @ratatui-pattern: Task queue management with CRUD operations
   * @ratatui-state: 
   *   - active_task: Option<TaskItem>
   *   - queued_tasks: Vec<TaskItem>
   * 
   * ```rust
   * #[derive(Clone, Debug)]
   * struct TasksState {
   *     active_task: Option<TaskItem>,
   *     queued_tasks: Vec<TaskItem>,
   * }
   * 
   * impl TasksState {
   *     fn cancel_active(&mut self) {
   *         self.active_task = None;
   *     }
   *     
   *     fn delete_queued(&mut self, id: &str) {
   *         self.queued_tasks.retain(|t| t.id != id);
   *     }
   *     
   *     fn move_task(&mut self, index: usize, direction: MoveDirection) {
   *         if direction == MoveDirection::Up && index > 0 {
   *             self.queued_tasks.swap(index, index - 1);
   *         } else if direction == MoveDirection::Down 
   *             && index < self.queued_tasks.len() - 1 {
   *             self.queued_tasks.swap(index, index + 1);
   *         }
   *     }
   * }
   * ```
   */
  const [activeTask, setActiveTask] = useState<TaskItem | null>({
    id: "active-1",
    name: "Understanding the codebase architecture",
    icon: Loader2,
  });

  const [queuedTasks, setQueuedTasks] = useState<TaskItem[]>([
    { id: "q-1", name: "Which is the most complex component?" },
    { id: "q-2", name: "Refactor the gaming class structure" },
    { id: "q-3", name: "Optimize database queries" },
    { id: "q-4", name: "Fix authentication bug" },
    { id: "q-5", name: "Update documentation" },
    { id: "q-6", name: "Review pull requests" },
    { id: "q-7", name: "Implement dark mode toggle" },
  ]);

  /**
   * ============================================================================
   * EVENT HANDLERS - RATATUI KEY EVENT PROCESSING
   * ============================================================================
   */
  
  /**
   * @ratatui-handler: cancel_active_task
   * @ratatui-keyboard: 'x' or Delete on active task
   * @ratatui-confirmation: Show confirmation dialog (use popup)
   * ```rust
   * fn handle_cancel_active(&mut self) -> bool {
   *     // In TUI, show confirmation popup
   *     self.show_confirmation_dialog = true;
   *     self.confirmation_action = ConfirmAction::CancelTask;
   *     true
   * }
   * 
   * fn on_confirmation_yes(&mut self) {
   *     match self.confirmation_action {
   *         ConfirmAction::CancelTask => {
   *             self.active_task = None;
   *         }
   *         // ... other actions
   *     }
   *     self.show_confirmation_dialog = false;
   * }
   * ```
   */
  const handleCancelActive = () => {
    if (confirm("Cancel current task?")) setActiveTask(null);
  };

  /**
   * @ratatui-handler: delete_queued_task
   * @ratatui-keyboard: 'd' or Delete on focused queued task
   * @ratatui-behavior: Remove from Vec by filtering
   * ```rust
   * fn delete_queued_task(&mut self, id: &str) {
   *     self.queued_tasks.retain(|task| task.id != id);
   * }
   * ```
   */
  const handleDeleteQueue = (id: string) => {
    setQueuedTasks((prev) => prev.filter((t) => t.id !== id));
  };

  /**
   * @ratatui-handler: edit_queued_task
   * @ratatui-keyboard: 'e' or Enter on focused queued task
   * @ratatui-ui: Show text input popup for editing
   * ```rust
   * fn start_edit_task(&mut self, id: String, current_name: String) {
   *     self.edit_mode = Some(EditMode::TaskName {
   *         task_id: id,
   *         buffer: current_name,
   *         cursor_pos: current_name.len(),
   *     });
   * }
   * 
   * fn finish_edit_task(&mut self) {
   *     if let Some(EditMode::TaskName { task_id, buffer, .. }) = &self.edit_mode {
   *         if !buffer.trim().is_empty() {
   *             if let Some(task) = self.queued_tasks.iter_mut()
   *                 .find(|t| t.id == *task_id) {
   *                 task.name = buffer.clone();
   *             }
   *         }
   *     }
   *     self.edit_mode = None;
   * }
   * ```
   */
  const handleEditQueue = (id: string, currentName: string) => {
    const newName = window.prompt("Edit task name:", currentName);
    if (newName && newName.trim() !== "") {
      setQueuedTasks((prev) =>
        prev.map((t) => (t.id === id ? { ...t, name: newName } : t))
      );
    }
  };

  /**
   * @ratatui-handler: move_task
   * @ratatui-keyboard: Ctrl+Up / Ctrl+Down on focused task
   * @ratatui-behavior: Swap positions in Vec
   * ```rust
   * enum MoveDirection { Up, Down }
   * 
   * fn move_task(&mut self, index: usize, direction: MoveDirection) {
   *     let target_index = match direction {
   *         MoveDirection::Up if index > 0 => index - 1,
   *         MoveDirection::Down if index < self.queued_tasks.len() - 1 => index + 1,
   *         _ => return,  // Can't move
   *     };
   *     self.queued_tasks.swap(index, target_index);
   * }
   * ```
   */
  const handleMoveTask = (index: number, direction: "up" | "down") => {
    if (
      (direction === "up" && index === 0) ||
      (direction === "down" && index === queuedTasks.length - 1)
    ) return;
    
    const newQueue = [...queuedTasks];
    const targetIndex = direction === "up" ? index - 1 : index + 1;
    [newQueue[index], newQueue[targetIndex]] = [newQueue[targetIndex], newQueue[index]];
    setQueuedTasks(newQueue);
  };

  /**
   * @ratatui-handler: toggle_section
   * @ratatui-keyboard: Enter/Space on section header
   * @ratatui-behavior: Add/remove section from expanded_sections HashSet
   * @ratatui-special: If sidebar collapsed, first expand sidebar then expand section
   * ```rust
   * fn toggle_section(&mut self, section: &str) {
   *     if self.sidebar_collapsed {
   *         self.sidebar_collapsed = false;
   *         self.expanded_sections.insert(section.to_string());
   *         return;
   *     }
   *     
   *     if !self.expanded_sections.remove(section) {
   *         self.expanded_sections.insert(section.to_string());
   *     }
   * }
   * ```
   */
  const toggleSection = (section: string) => {
    if (isSidebarCollapsed) {
      setIsSidebarCollapsed(false);
      const newExpanded = new Set(expandedSections);
      newExpanded.add(section);
      setExpandedSections(newExpanded);
      return;
    }
    const newExpanded = new Set(expandedSections);
    if (newExpanded.has(section)) newExpanded.delete(section);
    else newExpanded.add(section);
    setExpandedSections(newExpanded);
  };

  /**
   * @ratatui-handler: toggle_all_sections
   * @ratatui-keyboard: 'a' or Ctrl+A in sidebar
   * @ratatui-behavior: If all expanded, collapse all; otherwise expand all
   * ```rust
   * fn toggle_all_sections(&mut self) {
   *     if self.expanded_sections.len() == self.panels.len() {
   *         self.expanded_sections.clear();
   *     } else {
   *         self.expanded_sections = self.panels.iter()
   *             .map(|p| p.id.clone())
   *             .collect();
   *     }
   * }
   * ```
   */
  const toggleAllSections = () => {
    if (expandedSections.size === panels.length) {
      // Collapse all
      setExpandedSections(new Set());
    } else {
      // Expand all
      setExpandedSections(new Set(panels.map(p => p.id)));
    }
  };

  /**
   * ============================================================================
   * HELPER FUNCTIONS
   * ============================================================================
   */
  
  /**
   * @ratatui-helper: render_git_icon
   * @ratatui-return: Unicode character with color
   * ```rust
   * fn render_git_icon(status_type: GitStatusType) -> Span<'static> {
   *     let (icon, color) = match status_type {
   *         GitStatusType::Modified => ("M", Color::Rgb(234, 179, 8)),   // Yellow
   *         GitStatusType::New => ("A", Color::Rgb(34, 197, 94)),        // Emerald
   *         GitStatusType::Deleted => ("D", Color::Rgb(239, 68, 68)),    // Red
   *     };
   *     Span::styled(icon, Style::default().fg(color))
   * }
   * 
   * // Alternative with Unicode symbols:
   * // Modified: "✎" "📝" "M"
   * // New: "✚" "+" "A"
   * // Deleted: "✖" "−" "D"
   * ```
   */
  const renderGitIcon = (type: GitStatusType) => {
    switch (type) {
      case "modified": return <FilePenLine className={`w-3.5 h-3.5 ${sidebarTheme.gitModified} opacity-80`} />;
      case "new": return <FilePlus className={`w-3.5 h-3.5 ${sidebarTheme.gitNew} opacity-80`} />;
      case "deleted": return <FileMinus className={`w-3.5 h-3.5 ${sidebarTheme.gitDeleted} opacity-80`} />;
    }
  };

  /**
   * ============================================================================
   * PANELS DATA CONFIGURATION
   * ============================================================================
   * 
   * @ratatui-data: Static panel definitions with dynamic data
   * @ratatui-pattern: Each panel is rendered as an accordion section
   * 
   * ```rust
   * fn get_panels(&self) -> Vec<PanelData> {
   *     vec![
   *         self.get_session_panel(),
   *         self.get_context_panel(),
   *         self.get_tasks_panel(),
   *         self.get_git_panel(),
   *     ]
   * }
   * ```
   * 
   * @ratatui-icons: All icons should be Unicode characters:
   *   - Database: "💾" "📊" "DB"
   *   - AppWindow: "◫" "⊞" "⬚"
   *   - GitBranch: "⎇" "⑂"
   *   - Sparkles: "✨" "⭐"
   *   - Cloud: "☁" "☁️"
   *   - DollarSign: "$" "💰"
   */
  const panels: PanelData[] = [
    /**
     * SESSION PANEL
     * @ratatui-content: Shows active session, branches, models, and costs
     * @ratatui-expandable: Cost breakdown is nested expansion
     */
    {
      id: "session",
      title: "Session",
      icon: Database,
      items: [
        // 1. UPDATED ICON: Using AppWindow for "main"
        { name: "main", icon: AppWindow }, 
        { name: "feature/sidebar-update", icon: GitBranch, color: "text-[var(--picker-folder)]" },
        { name: "gemini-1.5-pro-preview", icon: Sparkles },
        { name: "gemini-oauth", icon: Cloud },
        { 
          name: "$0.015 (3 models)", 
          icon: DollarSign, 
          color: sidebarTheme.success,
          isCostItem: true 
        },
      ],
      costBreakdown: [
        { model: "gemini-1.5-pro", cost: "$0.012" },
        { model: "gemini-1.0-pro", cost: "$0.002" },
        { model: "gpt-4-turbo", cost: "$0.001" },
        { model: "claude-3-opus", cost: "$0.000" },
      ]
    },
    /**
     * CONTEXT PANEL
     * @ratatui-content: Shows token usage and loaded files list
     * @ratatui-expandable: Loaded files is nested expansion
     * @ratatui-scroll: Files list is scrollable if many files
     */
    {
      id: "context",
      title: "Context",
      icon: FileText,
      badge: "1.0k",
      items: [
        { name: "1,833 / 1,000,000 tokens", icon: Gauge, color: sidebarTheme.accent },
      ],
      loadedFiles: [
        { name: "src/components/Sidebar.tsx", type: "file" },
        { name: "src/styles/", type: "folder" },
        { name: "package.json", type: "file" },
        { name: "src/utils/helpers.ts", type: "file" },
        { name: "src/app/layout.tsx", type: "file" },
        { name: "src/hooks/", type: "folder" },
        { name: "tailwind.config.ts", type: "file" },
        { name: "next.config.js", type: "file" },
      ],
    },
    /**
     * TASKS PANEL
     * @ratatui-content: Shows active task (if any) and queued tasks
     * @ratatui-badge: Dynamic count of total tasks
     * @ratatui-interactive: Tasks can be edited, deleted, reordered
     * @ratatui-keyboard:
     *   - 'x'/Delete: Cancel active or delete queued
     *   - 'e'/Enter: Edit task name
     *   - Ctrl+Up/Down: Reorder queued tasks
     */
    {
      id: "tasks",
      title: "Tasks",
      icon: CheckCircle2,
      badge: `${(activeTask ? 1 : 0) + queuedTasks.length}`,
    },
    /**
     * GIT CHANGES PANEL
     * @ratatui-content: Shows git status summary and file changes
     * @ratatui-summary: Counts of modified, new, deleted files
     * @ratatui-files: List of changed files with diff stats
     * @ratatui-colors: Different colors for each status type
     * @ratatui-scroll: Files list is scrollable
     */
    {
      id: "git",
      title: "Git Changes",
      icon: GitGraph,
      badge: "12",
      git: {
        summary: { modified: 7, new: 3, deleted: 2 },
        files: [
          { name: "src/components/Sidebar.tsx", type: "modified", additions: 45, deletions: 12 },
          { name: "src/utils/helpers.ts", type: "new" },
          { name: "public/legacy-logo.svg", type: "deleted" },
          { name: "src/styles/globals.css", type: "modified", additions: 10, deletions: 5 },
          { name: "README.md", type: "modified", additions: 2, deletions: 1 },
        ],
      },
    },
  ];

  /**
   * ============================================================================
   * SIDEBAR RENDERING - ROOT CONTAINER
   * ============================================================================
   * 
   * @ratatui-widget: Block with borders on left side
   * @ratatui-layout: Vertical split - header (fixed) + panels (scrollable)
   * @ratatui-width: Dynamic based on sidebar_collapsed state
   *   - Collapsed: 14 chars (~3-5 columns)
   *   - Expanded: 80 chars (~30-40 columns)
   * 
   * @ratatui-pattern:
   * ```rust
   * fn render_sidebar(&mut self, frame: &mut Frame, area: Rect) {
   *     let sidebar_width = if self.sidebar_collapsed { 5 } else { 35 };
   *     let sidebar_area = Rect {
   *         x: area.width.saturating_sub(sidebar_width),
   *         y: area.y,
   *         width: sidebar_width.min(area.width),
   *         height: area.height,
   *     };
   *     
   *     let block = Block::default()
   *         .borders(Borders::LEFT)
   *         .border_style(Style::default().fg(theme::BORDER))
   *         .style(Style::default().bg(theme::MAIN_BG));
   *     
   *     let inner = block.inner(sidebar_area);
   *     frame.render_widget(block, sidebar_area);
   *     
   *     let chunks = Layout::vertical([
   *         Constraint::Length(3),  // Header
   *         Constraint::Min(0),     // Panels (scrollable)
   *     ]).split(inner);
   *     
   *     self.render_sidebar_header(frame, chunks[0]);
   *     self.render_sidebar_panels(frame, chunks[1]);
   * }
   * ```
   */
  return (
    <div
      className={`${sidebarTheme.mainBg} ${sidebarTheme.border} border-l flex flex-col h-screen transition-all duration-300 ease-in-out ${
        isSidebarCollapsed ? "w-14" : "w-80"
      }`}
    >
      <style>{scrollbarStyles}</style>

      {/* ===================================================================
          SIDEBAR HEADER
          ===================================================================
          @ratatui-widget: Paragraph with title and buttons
          @ratatui-layout: Horizontal split - title (left) + buttons (right)
          @ratatui-buttons:
            - Toggle all: Expand/collapse all sections
            - Collapse sidebar: Show/hide entire sidebar
          
          @ratatui-pattern:
          ```rust
          fn render_sidebar_header(&self, frame: &mut Frame, area: Rect) {
              if self.sidebar_collapsed {
                  // Only show collapse/expand button
                  let button = Paragraph::new("»")
                      .alignment(Alignment::Center);
                  frame.render_widget(button, area);
              } else {
                  let chunks = Layout::horizontal([
                      Constraint::Min(0),      // Title
                      Constraint::Length(10),  // Buttons
                  ]).split(area);
                  
                  let title = Paragraph::new("Panel")
                      .style(Style::default()
                          .fg(theme::TEXT_PRIMARY)
                          .add_modifier(Modifier::BOLD));
                  frame.render_widget(title, chunks[0]);
                  
                  // Render buttons in chunks[1]
                  // "⇅" toggle all, "«" collapse
              }
          }
          ```
       */}
      <div className={`px-3 py-3 ${sidebarTheme.border} border-b flex items-center justify-between shrink-0`}>
        {!isSidebarCollapsed && (
          <h2 className={`text-sm font-semibold ${sidebarTheme.textPrimary} fade-in pl-1`}>
            Panel
          </h2>
        )}
        <div className="flex items-center gap-1">
          {!isSidebarCollapsed && (
            <>
              <button
                onClick={toggleAllSections}
                title={expandedSections.size === panels.length ? "Collapse all sections" : "Expand all sections"}
                className={`p-1 rounded-md ${sidebarTheme.textSecondary} hover:${sidebarTheme.textPrimary} hover:bg-[var(--mode-hover-bg)] transition-colors`}
              >
                {/* @ratatui-icon: "⇅" or "↕" for toggle all */}
                <ChevronsUpDown className="w-4 h-4" />
              </button>
              
              {/* Theme Picker Button */}
              <div className="relative">
                <button
                  onClick={() => setIsThemePickerOpen(!isThemePickerOpen)}
                  title={`Theme: ${currentTheme.name}`}
                  className={`p-1 rounded-md ${sidebarTheme.textSecondary} hover:${sidebarTheme.textPrimary} hover:bg-[var(--mode-hover-bg)] transition-colors`}
                >
                  <Palette className="w-4 h-4" />
                </button>
                
                {/* Theme Picker Dropdown */}
                {isThemePickerOpen && (
                  <div className="absolute right-0 top-full mt-2 w-56 bg-modal-bg border border-modal-border rounded-lg shadow-xl z-50 overflow-hidden">
                    <div className="px-3 py-2 border-b border-modal-border">
                      <span className="text-sm font-medium text-[var(--foreground)]">Select Theme</span>
                    </div>
                    <div className="max-h-64 overflow-y-auto">
                      {themes.map((t) => (
                        <button
                          key={t.id}
                          onClick={() => {
                            switchTheme(t);
                            setIsThemePickerOpen(false);
                          }}
                          className={`w-full flex items-center gap-3 px-3 py-2 text-left hover:bg-picker-hover-bg transition-colors ${
                            currentTheme.id === t.id ? 'bg-picker-selected-bg' : ''
                          }`}
                        >
                          <div 
                            className="w-4 h-4 rounded-full border border-[var(--terminal-border)]"
                            style={{ backgroundColor: t.colors.terminalBg }}
                          />
                          <div className="flex-1">
                            <div className="text-sm text-[var(--foreground)]">{t.name}</div>
                            <div className="text-xs text-[var(--foreground)]/50">{t.isDark ? 'Dark' : 'Light'}</div>
                          </div>
                          {currentTheme.id === t.id && (
                            <Check className="w-4 h-4 text-llm-connected" />
                          )}
                        </button>
                      ))}
                    </div>
                  </div>
                )}
              </div>
            </>
          )}
          <button
            onClick={() => setIsSidebarCollapsed(!isSidebarCollapsed)}
            className={`p-1 rounded-md ${sidebarTheme.textSecondary} hover:${sidebarTheme.textPrimary} hover:bg-gray-800 transition-colors ${
              isSidebarCollapsed ? "mx-auto" : ""
            }`}
          >
            {/* @ratatui-icons: "»" (collapsed) or "«" (expanded) */}
            {isSidebarCollapsed ? <ChevronsRight className="w-4 h-4" /> : <ChevronsLeft className="w-4 h-4" />}
          </button>
        </div>
      </div>

      {/* ===================================================================
          SCROLLABLE PANELS AREA
          ===================================================================
          @ratatui-widget: Scrollable List of accordion sections
          @ratatui-scroll: Use ScrollbarState to track position
          @ratatui-layout: Each panel is rendered conditionally based on expanded state
          
          @ratatui-pattern:
          ```rust
          fn render_sidebar_panels(&mut self, frame: &mut Frame, area: Rect) {
              let mut lines: Vec<Line> = vec![];
              
              for panel in &self.panels {
                  lines.extend(self.render_panel_header(panel));
                  
                  if self.expanded_sections.contains(&panel.id) {
                      lines.extend(self.render_panel_content(panel));
                  }
              }
              
              let paragraph = Paragraph::new(lines)
                  .scroll((self.panel_scroll_offset as u16, 0));
              frame.render_widget(paragraph, area);
              
              // Render scrollbar
              let scrollbar = Scrollbar::default()
                  .orientation(ScrollbarOrientation::VerticalRight);
              frame.render_stateful_widget(
                  scrollbar,
                  area,
                  &mut self.panel_scroll_state
              );
          }
          ```
       */}
      <div className="flex-1 overflow-y-auto custom-scrollbar">
        <div className="p-2">
          {panels.map((panel) => {
            const Icon = panel.icon;
            const isExpanded = expandedSections.has(panel.id);
            const isTasksPanel = panel.id === "tasks";
            
            /**
             * @ratatui-conditional-color: Tasks panel changes color when active task exists
             * @ratatui-color: Mustard/amber when active task present
             */
            const isActiveTasksPanel = isTasksPanel && activeTask !== null;
            const headerColorClass = isActiveTasksPanel ? sidebarTheme.mustard : sidebarTheme.textSecondary;
            const titleColorClass = isActiveTasksPanel ? sidebarTheme.mustard : sidebarTheme.textSecondary;
            const hoverTitleClass = isActiveTasksPanel ? "group-hover:text-amber-300" : `group-hover:${sidebarTheme.textPrimary}`;

            return (
              <div key={panel.id} className="mb-2">
                {/* Panel Header Button */}
                <button
                  onClick={() => toggleSection(panel.id)}
                  className={`w-full flex items-center rounded-lg ${sidebarTheme.hoverBg} transition-colors group ${
                    isSidebarCollapsed ? "justify-center py-2" : "justify-between px-3 py-2"
                  }`}
                >
                  <div className="flex items-center gap-2">
                    {!isSidebarCollapsed && (
                      <span className="shrink-0">
                        {isExpanded ? (
                          <ChevronDown className={`w-4 h-4 ${sidebarTheme.textSecondary}`} />
                        ) : (
                          <ChevronRight className={`w-4 h-4 ${sidebarTheme.textSecondary}`} />
                        )}
                      </span>
                    )}
                    
                    {/* Icon with Dynamic Color */}
                    <Icon
                      className={`${headerColorClass} group-hover:${sidebarTheme.textPrimary} ${
                        isSidebarCollapsed ? "w-5 h-5" : "w-4 h-4"
                      }`}
                    />
                    
                    {!isSidebarCollapsed && (
                      <span className={`text-sm ${titleColorClass} ${hoverTitleClass}`}>
                        {panel.title}
                      </span>
                    )}
                  </div>
                  {!isSidebarCollapsed && panel.badge && (
                    <span className={`text-xs ${sidebarTheme.textMuted} px-1.5 py-0.5 bg-[var(--mode-hover-bg)] rounded`}>
                      {panel.badge}
                    </span>
                  )}
                </button>

                {/* EXPANDED CONTENT */}
                {isExpanded && !isSidebarCollapsed && (
                  <div className="ml-6 mt-1 space-y-1">
                    
                    {/* Items Logic */}
                    {panel.items &&
                      panel.items.map((item, idx) => {
                        const ItemIcon = item.icon;
                        
                        if (item.isCostItem) {
                          return (
                            <div key={idx}>
                              <div
                                onClick={() => setIsCostExpanded(!isCostExpanded)}
                                className={`flex items-center gap-2 px-3 py-1.5 rounded-lg ${sidebarTheme.itemHover} transition-colors cursor-pointer justify-between group/cost`}
                              >
                                <div className="flex items-center gap-2">
                                  {ItemIcon && (
                                    <ItemIcon className={`w-3.5 h-3.5 ${item.color || sidebarTheme.textMuted}`} />
                                  )}
                                  <span className={`text-xs ${item.color || sidebarTheme.textSecondary}`}>
                                    {item.name}
                                  </span>
                                </div>
                                <ChevronDown className={`w-3 h-3 ${sidebarTheme.textMuted} transition-transform ${isCostExpanded ? "rotate-180" : ""}`} />
                              </div>
                              
                              {isCostExpanded && panel.costBreakdown && (
                                <div className={`ml-3 ${sidebarTheme.border} border-l pl-2 mt-1 mb-2 max-h-24 overflow-y-auto custom-scrollbar`}>
                                  {panel.costBreakdown.map((cost, cIdx) => (
                                    <div key={cIdx} className="flex justify-between items-center py-1 pr-2 text-[11px]">
                                      <span className={`${sidebarTheme.textSecondary} truncate max-w-[120px]`}>{cost.model}</span>
                                      <span className={`${sidebarTheme.success} font-mono`}>{cost.cost}</span>
                                    </div>
                                  ))}
                                </div>
                              )}
                            </div>
                          );
                        }

                        return (
                          <div
                            key={idx}
                            className={`flex items-center gap-2 px-3 py-1.5 rounded-lg ${sidebarTheme.itemHover} transition-colors cursor-pointer`}
                          >
                            {ItemIcon && (
                              <ItemIcon className={`w-3.5 h-3.5 ${item.color || sidebarTheme.textMuted}`} />
                            )}
                            <span className={`text-xs ${item.color || sidebarTheme.textSecondary}`}>
                              {item.name}
                            </span>
                          </div>
                        );
                      })}

                    {/* Context Logic */}
                    {panel.id === "context" && panel.loadedFiles && (
                      <div className="mt-2">
                         <button 
                          onClick={() => setIsLoadedFilesExpanded(!isLoadedFilesExpanded)}
                          className={`w-full flex items-center justify-between px-3 py-1 text-[10px] ${sidebarTheme.textMuted} uppercase tracking-wider hover:${sidebarTheme.textPrimary} transition-colors`}
                        >
                          <span>Loaded Files ({panel.loadedFiles.length})</span>
                          <ChevronDown className={`w-3 h-3 transition-transform ${isLoadedFilesExpanded ? "rotate-180" : ""}`} />
                        </button>
                        
                        {isLoadedFilesExpanded && (
                          <div className="max-h-24 overflow-y-auto pr-1 custom-scrollbar space-y-0.5 mt-1">
                            {panel.loadedFiles.map((file, idx) => (
                              <div key={idx} className={`flex items-center gap-2 px-3 py-1.5 rounded-lg ${sidebarTheme.itemHover} transition-colors cursor-pointer group/file`}>
                                {file.type === 'folder' ? (
                                  <Folder className={`w-3.5 h-3.5 ${sidebarTheme.accent} opacity-70`} />
                                ) : (
                                  <FileCode className={`w-3.5 h-3.5 ${sidebarTheme.textMuted}`} />
                                )}
                                <span className={`text-[11px] ${sidebarTheme.textSecondary} group-hover/file:${sidebarTheme.textPrimary} leading-relaxed truncate`}>
                                  {file.name}
                                </span>
                              </div>
                            ))}
                          </div>
                        )}
                      </div>
                    )}

                    {/* Tasks Logic */}
                    {isTasksPanel && (
                      <>
                        {activeTask && (
                          <div className={`flex items-start gap-2 px-3 py-2 rounded-lg ${sidebarTheme.activeItemBg} border ${sidebarTheme.border} group/active relative`}>
                            <Loader2 className={`w-3.5 h-3.5 ${sidebarTheme.accent} animate-spin mt-0.5 shrink-0`} />
                            <div className="flex-1 min-w-0">
                              <span className={`text-xs ${sidebarTheme.textPrimary} leading-relaxed block pr-4`}>
                                {activeTask.name}
                              </span>
                              <div className={`text-[10px] ${sidebarTheme.accent} mt-0.5`}>Active</div>
                            </div>
                            <button onClick={handleCancelActive} className={`absolute right-2 top-2 opacity-0 group-hover/active:opacity-100 p-0.5 hover:bg-[var(--mode-hover-bg)] rounded transition-all`}>
                              <X className={`w-3.5 h-3.5 ${sidebarTheme.textSecondary} hover:${sidebarTheme.danger}`} />
                            </button>
                          </div>
                        )}

                        {queuedTasks.length > 0 && activeTask && (
                          <div className={`px-3 py-1 text-[10px] ${sidebarTheme.textMuted} uppercase tracking-wider mt-1 mb-1`}>
                            Queued
                          </div>
                        )}

                        <div className="max-h-32 overflow-y-auto pr-1 custom-scrollbar space-y-0.5">
                          {queuedTasks.map((task, idx) => (
                            <div key={task.id} className={`flex items-center gap-2.5 px-3 py-1.5 rounded-lg ${sidebarTheme.itemHover} transition-colors group/item relative`}>
                              <Circle className="w-3 h-3 text-[var(--foreground)]/30 group-hover/item:text-[var(--foreground)]/50 shrink-0" />
                              <span className={`text-[11px] ${sidebarTheme.textMuted} group-hover/item:${sidebarTheme.textSecondary} leading-relaxed truncate flex-1`}>
                                {task.name}
                              </span>
                              <div className="hidden group-hover/item:flex items-center gap-1 absolute right-2 bg-[var(--terminal-bg)] shadow-sm pl-2">
                                {idx > 0 && (
                                  <button onClick={() => handleMoveTask(idx, "up")} className="p-1 hover:bg-[var(--mode-hover-bg)] rounded text-[var(--foreground)]/50 hover:text-[var(--foreground)]">
                                    <ArrowUp className="w-3 h-3" />
                                  </button>
                                )}
                                {idx < queuedTasks.length - 1 && (
                                  <button onClick={() => handleMoveTask(idx, "down")} className="p-1 hover:bg-[var(--mode-hover-bg)] rounded text-[var(--foreground)]/50 hover:text-[var(--foreground)]">
                                    <ArrowDown className="w-3 h-3" />
                                  </button>
                                )}
                                <button onClick={() => handleEditQueue(task.id, task.name)} className={`p-1 hover:bg-[var(--mode-hover-bg)] rounded ${sidebarTheme.textMuted} hover:${sidebarTheme.accent}`}>
                                  <Pencil className="w-3 h-3" />
                                </button>
                                <button onClick={() => handleDeleteQueue(task.id)} className={`p-1 hover:bg-[var(--mode-hover-bg)] rounded ${sidebarTheme.textMuted} hover:${sidebarTheme.danger}`}>
                                  <Trash2 className="w-3 h-3" />
                                </button>
                              </div>
                            </div>
                          ))}
                        </div>
                      </>
                    )}

                    {/* Git Logic */}
                    {panel.git && (
                      <div className="mt-2 mb-1">
                        <div className={`flex items-center gap-2 px-3 pb-2 text-[10px] ${sidebarTheme.border} border-b mb-2`}>
                          <div className="flex items-center gap-1">
                            <span className={`${sidebarTheme.gitModified} font-medium`}>{panel.git.summary.modified}</span>
                            <span className={sidebarTheme.textMuted}>Mod</span>
                          </div>
                          <div className="w-px h-3 bg-[var(--terminal-border)]"></div>
                          <div className="flex items-center gap-1">
                            <span className={`${sidebarTheme.gitNew} font-medium`}>{panel.git.summary.new}</span>
                            <span className={sidebarTheme.textMuted}>New</span>
                          </div>
                          <div className="w-px h-3 bg-[var(--terminal-border)]"></div>
                          <div className="flex items-center gap-1">
                            <span className={`${sidebarTheme.gitDeleted} font-medium`}>{panel.git.summary.deleted}</span>
                            <span className={sidebarTheme.textMuted}>Del</span>
                          </div>
                        </div>
                        <div className="max-h-40 overflow-y-auto pr-1 custom-scrollbar space-y-0.5">
                          {panel.git.files.map((file, idx) => (
                            <div key={idx} className={`flex items-center justify-between px-3 py-1.5 rounded-lg ${sidebarTheme.itemHover} transition-colors cursor-pointer group/file`}>
                              <div className="flex items-center gap-2 min-w-0 flex-1">
                                {renderGitIcon(file.type)}
                                <span className={`text-[11px] truncate leading-relaxed ${file.type === 'deleted' ? `${sidebarTheme.textMuted} line-through decoration-[var(--terminal-border)]` : `${sidebarTheme.textSecondary} group-hover/file:${sidebarTheme.textPrimary}`}`}>
                                  {file.name}
                                </span>
                              </div>
                              {file.type === "modified" && (
                                <div className="flex items-center gap-1.5 pl-2 text-[9px] font-mono shrink-0 opacity-60">
                                  <span className={sidebarTheme.gitNew}>+{file.additions}</span>
                                  <span className={sidebarTheme.gitDeleted}>-{file.deletions}</span>
                                </div>
                              )}
                              {file.type === "new" && <span className={`text-[9px] ${sidebarTheme.gitNew} opacity-70 font-mono pl-2 shrink-0`}>NEW</span>}
                              {file.type === "deleted" && <span className={`text-[9px] ${sidebarTheme.gitDeleted} opacity-70 font-mono pl-2 shrink-0`}>DEL</span>}
                            </div>
                          ))}
                        </div>
                      </div>
                    )}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}

/**
 * ============================================================================
 * SIDEBAR COMPONENT SUMMARY FOR RATATUI
 * ============================================================================
 * 
 * This Sidebar component maps to a complex accordion-style widget in Ratatui:
 * 
 * ## Key Concepts:
 * 
 * 1. **Collapsible Sidebar**: Width changes between 3-5 and 30-40 columns
 * 2. **Accordion Sections**: 4 panels (Session, Context, Tasks, Git)
 * 3. **Nested Expansions**: Cost breakdown and loaded files within panels
 * 4. **Task Management**: CRUD operations with reordering
 * 5. **Git Visualization**: Diff stats with colored status indicators
 * 
 * ## Ratatui Structure:
 * 
 * ```rust
 * struct SidebarState {
 *     collapsed: bool,
 *     expanded_sections: HashSet<String>,
 *     cost_expanded: bool,
 *     loaded_files_expanded: bool,
 *     active_task: Option<TaskItem>,
 *     queued_tasks: Vec<TaskItem>,
 *     panel_scroll_state: ScrollbarState,
 *     panel_scroll_offset: usize,
 * }
 * 
 * impl SidebarState {
 *     fn render(&mut self, frame: &mut Frame, area: Rect) {
 *         // Main render function
 *     }
 *     
 *     fn render_panel_header(&self, panel: &PanelData) -> Vec<Line> {
 *         // Render expandable panel header with icon, title, badge
 *     }
 *     
 *     fn render_session_panel(&self) -> Vec<Line> {
 *         // Render session info with cost breakdown
 *     }
 *     
 *     fn render_context_panel(&self) -> Vec<Line> {
 *         // Render token usage and loaded files
 *     }
 *     
 *     fn render_tasks_panel(&self) -> Vec<Line> {
 *         // Render active task and queued tasks with actions
 *     }
 *     
 *     fn render_git_panel(&self) -> Vec<Line> {
 *         // Render git changes summary and file list
 *     }
 * }
 * ```
 * 
 * ## Keyboard Navigation:
 * 
 * | Key | Action |
 * |-----|--------|
 * | Tab | Focus next section/item |
 * | Shift+Tab | Focus previous section/item |
 * | Enter/Space | Toggle section expansion |
 * | a / Ctrl+A | Toggle all sections |
 * | h / « | Collapse sidebar |
 * | l / » | Expand sidebar |
 * | e | Edit focused task |
 * | x / Delete | Delete focused task / Cancel active task |
 * | Ctrl+Up | Move task up in queue |
 * | Ctrl+Down | Move task down in queue |
 * | j / Down | Scroll down |
 * | k / Up | Scroll up |
 * 
 * ## Visual Elements Mapping:
 * 
 * | React Element | Ratatui Equivalent |
 * |---------------|-------------------|
 * | Lucide Icons | Unicode characters or nerd fonts |
 * | Hover effects | Selection/focus highlighting |
 * | Animations | Instant rendering (no animations) |
 * | Badges | Styled Span with background color |
 * | Scrollbar | ratatui::widgets::Scrollbar |
 * | Dropdown chevrons | "▼" "▶" Unicode arrows |
 * | Loading spinner | Rotating chars: "⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏" |
 * 
 * ## Color Coding:
 * 
 * - **Session Panel**: Default gray
 * - **Context Panel**: Default gray  
 * - **Tasks Panel**: Mustard/amber when active task exists
 * - **Git Panel**: Status-specific colors (yellow/emerald/red)
 * 
 * ## State Management:
 * 
 * All useState hooks become fields in the main App struct. Callbacks become
 * methods on the App struct that mutate the state directly.
 * 
 * ## Key Crates:
 * 
 * - `ratatui`: Core TUI framework
 * - `crossterm`: Terminal events and control
 * - `unicode-width`: Proper text width calculations
 * - `textwrap`: Text wrapping for long task names
 * 
 * ## Implementation Notes:
 * 
 * 1. **Hover Actions**: Replace with always-visible or keyboard-triggered actions
 * 2. **Confirm Dialogs**: Implement as popup overlays with Yes/No options
 * 3. **Inline Editing**: Use popup text input instead of window.prompt
 * 4. **Smooth Transitions**: Not available; all changes are instant
 * 5. **Opacity/Fade**: Use dimmer colors instead of opacity
 */