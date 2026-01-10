# Plan: Proposals Diff Popup (Unified + Side-by-Side)

> **Status:** Ready for implementation  
> **Created:** 2026-01-10  
> **Scope:** TUI diff viewer for `propose_change` tool output with session-scoped proposal storage

---

## Executive Summary

Implement a native TUI diff viewer popup for reviewing `propose_change` tool output in Plan mode. The feature includes:

- **Proposal capture**: Persist every `propose_change` invocation as a structured record
- **Diff popup**: Unified and side-by-side view modes with synced scrolling
- **Apply/Reject workflows**: Apply changes in Build mode with drift detection
- **Session-scoped storage**: Proposals stored under `.tark/sessions/<session_id>/proposals/`

**Key architectural decisions:**
- No `git difftool` (interactive, terminal-hostile)
- Capture from existing `AgentEvent::ToolCallStarted/Completed` (no tool plumbing changes)
- Popup overlay pattern (like `PlanPicker`, `HelpPopup`)
- Direct `std::fs::write` for Apply (like plan export)

---

## Data Model

### Storage Location

```
.tark/sessions/<session_id>/proposals/
├── prop_<uuid1>.json
├── prop_<uuid2>.json
└── ...
```

### Proposal Record Schema (v1)

**File:** `<proposal_id>.json`

```json
{
  "version": 1,
  "id": "prop_550e8400-e29b-41d4-a716-446655440000",
  "created_at": "2026-01-10T12:34:56.789Z",
  "session_id": "session_550e8400-e29b-41d4-a716-446655440001",
  "file_path": "src/tools/file_ops.rs",
  "description": "Add new diff generation function",
  "base_hash": 12345678901234567890,
  "status": "pending",
  "new_content": "use super::{Tool, ToolResult};\n..."
}
```

**Field specifications:**

| Field | Type | Description |
|-------|------|-------------|
| `version` | `u8` | Schema version, always `1` for now |
| `id` | `String` | Format: `prop_<uuid>` |
| `created_at` | `DateTime<Utc>` | ISO 8601 timestamp |
| `session_id` | `String` | Parent session ID |
| `file_path` | `String` | Relative path from workspace root |
| `description` | `Option<String>` | From tool args, may be null |
| `base_hash` | `u64` | xxHash64 of original file content; `0` = new file |
| `status` | `ProposalStatus` | Current lifecycle state |
| `new_content` | `String` | Proposed file content |

### ProposalStatus Enum

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposalStatus {
    /// Created on ToolCallStarted, not yet complete
    PendingExecution,
    /// Tool completed successfully, ready for review
    Pending,
    /// User applied the change
    Applied,
    /// User rejected the change
    Rejected,
    /// Replaced by newer proposal for same file
    Superseded,
    /// Tool execution failed
    Failed,
}
```

---

## Files to Create

### 1. `src/storage/proposals.rs` (NEW)

```rust
//! Proposal storage for change proposals
//!
//! Manages session-scoped proposal records for the `propose_change` tool.
//! Proposals are stored in `.tark/sessions/<session_id>/proposals/`.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Current schema version
pub const PROPOSAL_VERSION: u8 = 1;

/// Proposal status in its lifecycle
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProposalStatus {
    /// Created on ToolCallStarted, not yet complete
    #[default]
    PendingExecution,
    /// Tool completed successfully, ready for review
    Pending,
    /// User applied the change
    Applied,
    /// User rejected the change
    Rejected,
    /// Replaced by newer proposal for same file
    Superseded,
    /// Tool execution failed
    Failed,
}

impl ProposalStatus {
    /// Check if proposal is actionable (can be applied/rejected)
    pub fn is_actionable(&self) -> bool {
        matches!(self, Self::Pending)
    }
    
    /// Check if proposal is in a terminal state
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Applied | Self::Rejected | Self::Superseded | Self::Failed)
    }
}

/// A change proposal record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proposal {
    /// Schema version
    pub version: u8,
    /// Unique proposal ID (format: prop_<uuid>)
    pub id: String,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Parent session ID
    pub session_id: String,
    /// Target file path (relative to workspace)
    pub file_path: String,
    /// Optional description from tool args
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// xxHash64 of original file content (0 = new file)
    pub base_hash: u64,
    /// Current status
    pub status: ProposalStatus,
    /// Proposed new content
    pub new_content: String,
}

impl Proposal {
    /// Create a new proposal
    pub fn new(
        session_id: impl Into<String>,
        file_path: impl Into<String>,
        new_content: impl Into<String>,
        base_hash: u64,
        description: Option<String>,
    ) -> Self {
        Self {
            version: PROPOSAL_VERSION,
            id: format!("prop_{}", uuid::Uuid::new_v4()),
            created_at: Utc::now(),
            session_id: session_id.into(),
            file_path: file_path.into(),
            description,
            base_hash,
            status: ProposalStatus::PendingExecution,
            new_content: new_content.into(),
        }
    }
    
    /// Check if this is a new file proposal
    pub fn is_new_file(&self) -> bool {
        self.base_hash == 0
    }
}

/// Lightweight proposal metadata for listing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposalMeta {
    pub id: String,
    pub file_path: String,
    pub status: ProposalStatus,
    pub created_at: DateTime<Utc>,
    pub description: Option<String>,
}

impl From<&Proposal> for ProposalMeta {
    fn from(p: &Proposal) -> Self {
        Self {
            id: p.id.clone(),
            file_path: p.file_path.clone(),
            status: p.status,
            created_at: p.created_at,
            description: p.description.clone(),
        }
    }
}

/// Compute xxHash64 of content
pub fn compute_content_hash(content: &str) -> u64 {
    use xxhash_rust::xxh64::xxh64;
    xxh64(content.as_bytes(), 0)
}
```

### 2. `src/tui/widgets/diff_popup.rs` (NEW)

```rust
//! Diff popup widget for viewing change proposals
//!
//! Provides unified and side-by-side diff views for proposals
//! created by the `propose_change` tool.

#![allow(dead_code)]

use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget},
};

use crate::storage::proposals::{Proposal, ProposalStatus};

/// Minimum terminal width for side-by-side mode
pub const SIDE_BY_SIDE_MIN_WIDTH: u16 = 120;

/// Diff view mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DiffViewMode {
    #[default]
    Unified,
    SideBySide,
}

/// Type of diff line
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffLineType {
    /// Unchanged context line
    Context,
    /// Added line (green)
    Addition,
    /// Deleted line (red)
    Deletion,
}

/// A single line in the diff view
#[derive(Debug, Clone)]
pub struct DiffLine {
    pub line_type: DiffLineType,
    pub old_line_no: Option<usize>,
    pub new_line_no: Option<usize>,
    pub content: String,
}

/// State for the diff popup
#[derive(Debug, Default)]
pub struct DiffPopupState {
    /// Whether popup is visible
    pub visible: bool,
    /// Current proposal being viewed
    pub proposal: Option<Proposal>,
    /// Original file content (empty string for new files)
    pub original_content: String,
    /// Current view mode
    pub mode: DiffViewMode,
    /// Vertical scroll offset
    pub scroll_offset: usize,
    /// Computed diff lines
    pub diff_lines: Vec<DiffLine>,
    /// Total lines (for scroll bounds)
    pub total_lines: usize,
    /// Visible height (set during render)
    pub visible_height: u16,
    /// Whether in Build mode (affects Apply availability)
    pub build_mode: bool,
    /// Pending confirmation message
    pub confirm_message: Option<String>,
}

impl DiffPopupState {
    /// Create new state
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Open popup with a proposal
    pub fn open(&mut self, proposal: Proposal, original_content: String, build_mode: bool) {
        self.diff_lines = Self::compute_diff(&original_content, &proposal.new_content);
        self.total_lines = self.diff_lines.len();
        self.proposal = Some(proposal);
        self.original_content = original_content;
        self.mode = DiffViewMode::Unified;
        self.scroll_offset = 0;
        self.build_mode = build_mode;
        self.confirm_message = None;
        self.visible = true;
    }
    
    /// Close popup
    pub fn close(&mut self) {
        self.visible = false;
        self.proposal = None;
        self.diff_lines.clear();
        self.confirm_message = None;
    }
    
    /// Check if visible
    pub fn is_visible(&self) -> bool {
        self.visible
    }
    
    /// Toggle view mode (respects width constraint)
    pub fn toggle_mode(&mut self, width: u16) {
        self.mode = match self.mode {
            DiffViewMode::Unified => {
                if width >= SIDE_BY_SIDE_MIN_WIDTH {
                    DiffViewMode::SideBySide
                } else {
                    DiffViewMode::Unified // Can't switch, too narrow
                }
            }
            DiffViewMode::SideBySide => DiffViewMode::Unified,
        };
    }
    
    /// Set to unified mode
    pub fn set_unified(&mut self) {
        self.mode = DiffViewMode::Unified;
    }
    
    /// Set to side-by-side mode (if width allows)
    pub fn set_side_by_side(&mut self, width: u16) {
        if width >= SIDE_BY_SIDE_MIN_WIDTH {
            self.mode = DiffViewMode::SideBySide;
        }
    }
    
    /// Scroll down
    pub fn scroll_down(&mut self, amount: usize) {
        let max_scroll = self.total_lines.saturating_sub(self.visible_height as usize);
        self.scroll_offset = (self.scroll_offset + amount).min(max_scroll);
    }
    
    /// Scroll up
    pub fn scroll_up(&mut self, amount: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(amount);
    }
    
    /// Compute diff lines between old and new content
    fn compute_diff(old: &str, new: &str) -> Vec<DiffLine> {
        use similar::{ChangeTag, TextDiff};
        
        let diff = TextDiff::from_lines(old, new);
        let mut lines = Vec::new();
        let mut old_line_no = 1usize;
        let mut new_line_no = 1usize;
        
        for change in diff.iter_all_changes() {
            let (line_type, old_ln, new_ln) = match change.tag() {
                ChangeTag::Equal => {
                    let result = (DiffLineType::Context, Some(old_line_no), Some(new_line_no));
                    old_line_no += 1;
                    new_line_no += 1;
                    result
                }
                ChangeTag::Delete => {
                    let result = (DiffLineType::Deletion, Some(old_line_no), None);
                    old_line_no += 1;
                    result
                }
                ChangeTag::Insert => {
                    let result = (DiffLineType::Addition, None, Some(new_line_no));
                    new_line_no += 1;
                    result
                }
            };
            
            lines.push(DiffLine {
                line_type,
                old_line_no: old_ln,
                new_line_no: new_ln,
                content: change.value().trim_end_matches('\n').to_string(),
            });
        }
        
        lines
    }
    
    /// Get file path for display
    pub fn file_path(&self) -> &str {
        self.proposal.as_ref().map(|p| p.file_path.as_str()).unwrap_or("")
    }
    
    /// Get proposal ID
    pub fn proposal_id(&self) -> Option<&str> {
        self.proposal.as_ref().map(|p| p.id.as_str())
    }
}

/// Diff popup widget
pub struct DiffPopupWidget<'a> {
    state: &'a DiffPopupState,
}

impl<'a> DiffPopupWidget<'a> {
    pub fn new(state: &'a DiffPopupState) -> Self {
        Self { state }
    }
    
    fn render_unified(&self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(format!(" Diff: {} ", self.state.file_path()))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));
        
        let inner = block.inner(area);
        block.render(area, buf);
        
        let mut lines: Vec<Line> = Vec::new();
        
        for diff_line in self.state.diff_lines.iter().skip(self.state.scroll_offset) {
            if lines.len() >= inner.height as usize {
                break;
            }
            
            let (prefix, style) = match diff_line.line_type {
                DiffLineType::Addition => ("+", Style::default().fg(Color::Green)),
                DiffLineType::Deletion => ("-", Style::default().fg(Color::Red)),
                DiffLineType::Context => (" ", Style::default().fg(Color::Gray)),
            };
            
            let line_no = diff_line.new_line_no
                .or(diff_line.old_line_no)
                .map(|n| format!("{:4} ", n))
                .unwrap_or_else(|| "     ".to_string());
            
            lines.push(Line::from(vec![
                Span::styled(line_no, Style::default().fg(Color::DarkGray)),
                Span::styled("│ ", Style::default().fg(Color::DarkGray)),
                Span::styled(prefix, style.add_modifier(Modifier::BOLD)),
                Span::styled(" ", Style::default()),
                Span::styled(diff_line.content.clone(), style),
            ]));
        }
        
        let para = Paragraph::new(lines);
        para.render(inner, buf);
    }
    
    fn render_side_by_side(&self, area: Rect, buf: &mut Buffer) {
        // Split horizontally
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(area);
        
        // Left pane: Original
        let left_block = Block::default()
            .title(" Original ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray));
        
        // Right pane: Proposed
        let right_block = Block::default()
            .title(" Proposed ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Green));
        
        let left_inner = left_block.inner(chunks[0]);
        let right_inner = right_block.inner(chunks[1]);
        
        left_block.render(chunks[0], buf);
        right_block.render(chunks[1], buf);
        
        // Render left pane (deletions + context)
        let mut left_lines: Vec<Line> = Vec::new();
        let mut right_lines: Vec<Line> = Vec::new();
        
        for diff_line in self.state.diff_lines.iter().skip(self.state.scroll_offset) {
            if left_lines.len() >= left_inner.height as usize {
                break;
            }
            
            match diff_line.line_type {
                DiffLineType::Context => {
                    let line_no_left = diff_line.old_line_no
                        .map(|n| format!("{:4} │ ", n))
                        .unwrap_or_else(|| "     │ ".to_string());
                    let line_no_right = diff_line.new_line_no
                        .map(|n| format!("{:4} │ ", n))
                        .unwrap_or_else(|| "     │ ".to_string());
                    
                    left_lines.push(Line::from(vec![
                        Span::styled(line_no_left, Style::default().fg(Color::DarkGray)),
                        Span::styled(diff_line.content.clone(), Style::default().fg(Color::Gray)),
                    ]));
                    right_lines.push(Line::from(vec![
                        Span::styled(line_no_right, Style::default().fg(Color::DarkGray)),
                        Span::styled(diff_line.content.clone(), Style::default().fg(Color::Gray)),
                    ]));
                }
                DiffLineType::Deletion => {
                    let line_no = diff_line.old_line_no
                        .map(|n| format!("{:4} │ ", n))
                        .unwrap_or_else(|| "     │ ".to_string());
                    
                    left_lines.push(Line::from(vec![
                        Span::styled(line_no, Style::default().fg(Color::DarkGray)),
                        Span::styled(
                            diff_line.content.clone(),
                            Style::default().fg(Color::Red).bg(Color::Rgb(40, 0, 0)),
                        ),
                    ]));
                    // Placeholder on right
                    right_lines.push(Line::from(Span::styled(
                        "     │ ".to_string() + &"░".repeat(diff_line.content.len().min(40)),
                        Style::default().fg(Color::Rgb(40, 40, 40)),
                    )));
                }
                DiffLineType::Addition => {
                    let line_no = diff_line.new_line_no
                        .map(|n| format!("{:4} │ ", n))
                        .unwrap_or_else(|| "     │ ".to_string());
                    
                    // Placeholder on left
                    left_lines.push(Line::from(Span::styled(
                        "     │ ".to_string() + &"░".repeat(diff_line.content.len().min(40)),
                        Style::default().fg(Color::Rgb(40, 40, 40)),
                    )));
                    right_lines.push(Line::from(vec![
                        Span::styled(line_no, Style::default().fg(Color::DarkGray)),
                        Span::styled(
                            diff_line.content.clone(),
                            Style::default().fg(Color::Green).bg(Color::Rgb(0, 40, 0)),
                        ),
                    ]));
                }
            }
        }
        
        Paragraph::new(left_lines).render(left_inner, buf);
        Paragraph::new(right_lines).render(right_inner, buf);
    }
    
    fn render_help_bar(&self, area: Rect, buf: &mut Buffer) {
        let help_text = if self.state.confirm_message.is_some() {
            " [y] Confirm  [n] Cancel "
        } else {
            match self.state.mode {
                DiffViewMode::Unified => {
                    if self.state.build_mode {
                        " [a]pply [r]eject [s]ide-by-side [j/k]scroll │ Esc:close "
                    } else {
                        " [r]eject [s]ide-by-side [j/k]scroll │ (Build mode to apply) │ Esc:close "
                    }
                }
                DiffViewMode::SideBySide => {
                    if self.state.build_mode {
                        " [a]pply [r]eject [u]nified [j/k]scroll │ Esc:close "
                    } else {
                        " [r]eject [u]nified [j/k]scroll │ (Build mode to apply) │ Esc:close "
                    }
                }
            }
        };
        
        let help_y = area.y + area.height.saturating_sub(1);
        buf.set_string(
            area.x,
            help_y,
            help_text,
            Style::default().fg(Color::DarkGray).bg(Color::Rgb(30, 30, 30)),
        );
    }
}

impl Widget for DiffPopupWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Calculate popup area (centered, 80% width, 80% height)
        let popup_width = (area.width as f32 * 0.8) as u16;
        let popup_height = (area.height as f32 * 0.8) as u16;
        let popup_x = (area.width - popup_width) / 2;
        let popup_y = (area.height - popup_height) / 2;
        
        let popup_area = Rect::new(popup_x, popup_y, popup_width, popup_height);
        
        // Clear the area
        Clear.render(popup_area, buf);
        
        // Determine effective mode (force unified if too narrow)
        let effective_mode = if popup_area.width < SIDE_BY_SIDE_MIN_WIDTH {
            DiffViewMode::Unified
        } else {
            self.state.mode
        };
        
        // Reserve last line for help bar
        let content_area = Rect::new(
            popup_area.x,
            popup_area.y,
            popup_area.width,
            popup_area.height.saturating_sub(1),
        );
        
        // Render content based on mode
        match effective_mode {
            DiffViewMode::Unified => self.render_unified(content_area, buf),
            DiffViewMode::SideBySide => self.render_side_by_side(content_area, buf),
        }
        
        // Render help bar
        self.render_help_bar(popup_area, buf);
        
        // Render confirmation overlay if present
        if let Some(ref msg) = self.state.confirm_message {
            let msg_width = msg.len() as u16 + 4;
            let msg_x = popup_area.x + (popup_area.width - msg_width) / 2;
            let msg_y = popup_area.y + popup_area.height / 2;
            
            buf.set_string(
                msg_x,
                msg_y,
                format!(" {} ", msg),
                Style::default()
                    .fg(Color::Yellow)
                    .bg(Color::Rgb(60, 60, 0))
                    .add_modifier(Modifier::BOLD),
            );
        }
    }
}
```

---

## Files to Modify

### 1. `Cargo.toml`

**Add dependencies:**

```toml
# After existing dependencies, add:
xxhash-rust = { version = "0.8", features = ["xxh64"] }
similar = "2"
```

### 2. `src/storage/mod.rs`

**Add module export and helper:**

```rust
// Near top, after existing module declarations:
pub mod proposals;

// In TarkStorage impl, add:

/// Get proposals directory for a session
pub fn proposal_dir(&self, session_id: &str) -> PathBuf {
    self.session_dir(session_id).join("proposals")
}

/// Save a proposal
pub fn save_proposal(&self, session_id: &str, proposal: &proposals::Proposal) -> Result<()> {
    let dir = self.proposal_dir(session_id);
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{}.json", proposal.id));
    let content = serde_json::to_string_pretty(proposal)?;
    std::fs::write(path, content)?;
    Ok(())
}

/// Load a proposal by ID
pub fn load_proposal(&self, session_id: &str, proposal_id: &str) -> Result<proposals::Proposal> {
    let path = self.proposal_dir(session_id).join(format!("{}.json", proposal_id));
    let content = std::fs::read_to_string(&path)
        .context(format!("Failed to read proposal: {}", proposal_id))?;
    serde_json::from_str(&content).context("Failed to parse proposal")
}

/// List all proposals for a session
pub fn list_proposals(&self, session_id: &str) -> Result<Vec<proposals::ProposalMeta>> {
    let dir = self.proposal_dir(session_id);
    if !dir.exists() {
        return Ok(Vec::new());
    }
    
    let mut proposals = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().map(|e| e == "json").unwrap_or(false) {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(proposal) = serde_json::from_str::<proposals::Proposal>(&content) {
                    proposals.push(proposals::ProposalMeta::from(&proposal));
                }
            }
        }
    }
    
    // Sort by created_at descending (most recent first)
    proposals.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(proposals)
}

/// Update proposal status
pub fn update_proposal_status(
    &self,
    session_id: &str,
    proposal_id: &str,
    status: proposals::ProposalStatus,
) -> Result<()> {
    let mut proposal = self.load_proposal(session_id, proposal_id)?;
    proposal.status = status;
    self.save_proposal(session_id, &proposal)
}

/// Mark older pending proposals for same file as superseded
pub fn supersede_proposals_for_path(&self, session_id: &str, file_path: &str) -> Result<()> {
    let proposals = self.list_proposals(session_id)?;
    for meta in proposals {
        if meta.file_path == file_path && meta.status == proposals::ProposalStatus::Pending {
            self.update_proposal_status(session_id, &meta.id, proposals::ProposalStatus::Superseded)?;
        }
    }
    Ok(())
}
```

### 3. `src/tui/widgets/collapsible.rs`

**Add `proposal_id` field to `ToolCallInfo`:**

```rust
// In struct ToolCallInfo, add after block_id:
/// Proposal ID for propose_change tools (enables diff popup)
pub proposal_id: Option<String>,

// Update the constructors:
impl ToolCallInfo {
    pub fn new(
        tool: impl Into<String>,
        args: serde_json::Value,
        result_preview: impl Into<String>,
    ) -> Self {
        Self {
            tool: tool.into(),
            args,
            result_preview: result_preview.into(),
            error: None,
            block_id: Uuid::new_v4().to_string(),
            proposal_id: None,  // NEW
        }
    }
    
    pub fn with_error(
        tool: impl Into<String>,
        args: serde_json::Value,
        error: impl Into<String>,
    ) -> Self {
        Self {
            tool: tool.into(),
            args,
            result_preview: String::new(),
            error: Some(error.into()),
            block_id: Uuid::new_v4().to_string(),
            proposal_id: None,  // NEW
        }
    }
    
    /// Set proposal ID for propose_change tools
    pub fn with_proposal_id(mut self, id: impl Into<String>) -> Self {
        self.proposal_id = Some(id.into());
        self
    }
}
```

### 4. `src/tui/widgets/mod.rs`

**Add export:**

```rust
// After existing mod declarations:
mod diff_popup;

// After existing pub use statements:
pub use diff_popup::{DiffPopupState, DiffPopupWidget, DiffViewMode, SIDE_BY_SIDE_MIN_WIDTH};
```

### 5. `src/tui/app.rs`

**Add state and handling (key locations):**

```rust
// In AppState struct, add:
/// Diff popup state
pub diff_popup: super::widgets::DiffPopupState,

// In AppState::new(), add:
diff_popup: super::widgets::DiffPopupState::new(),

// In handle_key_event(), add BEFORE other popup checks:
// Handle diff popup input if visible
if self.state.diff_popup.is_visible() {
    return self.handle_diff_popup_key(key);
}

// Add new method:
fn handle_diff_popup_key(&mut self, key: KeyEvent) -> anyhow::Result<bool> {
    use crossterm::event::KeyCode;
    
    // Handle confirmation dialog first
    if self.state.diff_popup.confirm_message.is_some() {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                // Confirmed - apply the change
                self.apply_proposal_confirmed()?;
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                self.state.diff_popup.confirm_message = None;
            }
            _ => {}
        }
        return Ok(true);
    }
    
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => {
            self.state.diff_popup.close();
        }
        KeyCode::Char('j') | KeyCode::Down => {
            self.state.diff_popup.scroll_down(1);
        }
        KeyCode::Char('k') | KeyCode::Up => {
            self.state.diff_popup.scroll_up(1);
        }
        KeyCode::Char('s') => {
            let width = self.state.terminal_size.0;
            self.state.diff_popup.set_side_by_side(width);
        }
        KeyCode::Char('u') => {
            self.state.diff_popup.set_unified();
        }
        KeyCode::Char('a') => {
            self.try_apply_proposal()?;
        }
        KeyCode::Char('r') => {
            self.reject_proposal()?;
        }
        _ => {}
    }
    Ok(true)
}

fn try_apply_proposal(&mut self) -> anyhow::Result<()> {
    // Check Build mode
    if self.state.mode != AgentMode::Build {
        self.state.status_message = Some("Switch to Build mode to apply changes".to_string());
        return Ok(());
    }
    
    let Some(ref proposal) = self.state.diff_popup.proposal else {
        return Ok(());
    };
    
    // Check for drift
    let file_path = self.working_dir().join(&proposal.file_path);
    let current_hash = if file_path.exists() {
        let content = std::fs::read_to_string(&file_path)?;
        crate::storage::proposals::compute_content_hash(&content)
    } else {
        0
    };
    
    if proposal.base_hash != 0 && current_hash != proposal.base_hash {
        // File changed since proposal - require confirmation
        self.state.diff_popup.confirm_message = Some(
            "File changed since proposal. Apply anyway? (y/n)".to_string()
        );
        return Ok(());
    }
    
    // No drift or new file - apply directly
    self.apply_proposal_confirmed()
}

fn apply_proposal_confirmed(&mut self) -> anyhow::Result<()> {
    let Some(ref proposal) = self.state.diff_popup.proposal else {
        return Ok(());
    };
    
    let file_path = self.working_dir().join(&proposal.file_path);
    
    // Create parent directories if needed
    if let Some(parent) = file_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    
    // Write the file
    std::fs::write(&file_path, &proposal.new_content)?;
    
    // Update proposal status
    if let Some(ref bridge) = self.agent_bridge {
        let session_id = bridge.session_id();
        let _ = bridge.storage().update_proposal_status(
            session_id,
            &proposal.id,
            crate::storage::proposals::ProposalStatus::Applied,
        );
    }
    
    self.state.status_message = Some(format!("Applied: {}", proposal.file_path));
    self.state.diff_popup.close();
    Ok(())
}

fn reject_proposal(&mut self) -> anyhow::Result<()> {
    let Some(ref proposal) = self.state.diff_popup.proposal else {
        return Ok(());
    };
    
    // Update proposal status
    if let Some(ref bridge) = self.agent_bridge {
        let session_id = bridge.session_id();
        let _ = bridge.storage().update_proposal_status(
            session_id,
            &proposal.id,
            crate::storage::proposals::ProposalStatus::Rejected,
        );
    }
    
    self.state.status_message = Some(format!("Rejected: {}", proposal.file_path));
    self.state.diff_popup.close();
    Ok(())
}
```

**Proposal capture in event loop (in handle of AgentEvent):**

```rust
// Add tracking map to TuiApp struct:
/// Pending proposal IDs keyed by tool block_id
pending_proposals: std::collections::HashMap<String, String>,

// In AgentEvent::ToolCallStarted handling, add after existing code:
if tool == "propose_change" {
    if let Some(ref bridge) = self.agent_bridge {
        let session_id = bridge.session_id().to_string();
        let storage = bridge.storage();
        
        // Extract args
        let file_path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let new_content = args.get("new_content").and_then(|v| v.as_str()).unwrap_or("");
        let description = args.get("description").and_then(|v| v.as_str()).map(String::from);
        
        // Compute base hash
        let full_path = self.working_dir().join(file_path);
        let base_hash = if full_path.exists() {
            match std::fs::read_to_string(&full_path) {
                Ok(content) => crate::storage::proposals::compute_content_hash(&content),
                Err(_) => 0,
            }
        } else {
            0
        };
        
        // Supersede old proposals for same file
        let _ = storage.supersede_proposals_for_path(&session_id, file_path);
        
        // Create proposal
        let proposal = crate::storage::proposals::Proposal::new(
            &session_id,
            file_path,
            new_content,
            base_hash,
            description,
        );
        
        // Save and track
        if let Ok(()) = storage.save_proposal(&session_id, &proposal) {
            // Store mapping from block_id to proposal_id
            if let Some(last_msg) = self.state.message_list.messages().last() {
                if let Some(tool_info) = last_msg.tool_call_info.last() {
                    self.pending_proposals.insert(tool_info.block_id.clone(), proposal.id.clone());
                }
            }
        }
    }
}

// In AgentEvent::ToolCallCompleted handling, add:
if tool == "propose_change" {
    if let Some(ref bridge) = self.agent_bridge {
        let session_id = bridge.session_id();
        
        // Find the proposal ID for this tool call
        if let Some(last_msg) = self.state.message_list.messages_mut().last_mut() {
            for tool_info in last_msg.tool_call_info.iter_mut().rev() {
                if tool_info.tool == "propose_change" && tool_info.proposal_id.is_none() {
                    if let Some(proposal_id) = self.pending_proposals.remove(&tool_info.block_id) {
                        // Update proposal status to pending
                        let _ = bridge.storage().update_proposal_status(
                            session_id,
                            &proposal_id,
                            crate::storage::proposals::ProposalStatus::Pending,
                        );
                        // Set proposal_id on tool info
                        tool_info.proposal_id = Some(proposal_id);
                        self.state.status_message = Some(format!("Proposal saved: {}", tool_info.args.get("path").and_then(|v| v.as_str()).unwrap_or("file")));
                        break;
                    }
                }
            }
        }
    }
}

// In AgentEvent::ToolCallFailed handling, add:
if tool == "propose_change" {
    if let Some(ref bridge) = self.agent_bridge {
        let session_id = bridge.session_id();
        
        // Find and update the proposal
        if let Some(last_msg) = self.state.message_list.messages().last() {
            for tool_info in last_msg.tool_call_info.iter().rev() {
                if tool_info.tool == "propose_change" {
                    if let Some(proposal_id) = self.pending_proposals.remove(&tool_info.block_id) {
                        let _ = bridge.storage().update_proposal_status(
                            session_id,
                            &proposal_id,
                            crate::storage::proposals::ProposalStatus::Failed,
                        );
                        break;
                    }
                }
            }
        }
    }
}
```

**Open diff popup from tool block (in key handling for Messages focus):**

```rust
// When 'd' is pressed on a focused tool block:
KeyCode::Char('d') => {
    // Check if focused block is a propose_change tool with proposal_id
    if let Some(ref focused_id) = self.state.message_list.block_state().focused_block() {
        for msg in self.state.message_list.messages() {
            for tool_info in &msg.tool_call_info {
                if &tool_info.block_id == focused_id && tool_info.tool == "propose_change" {
                    if let Some(ref proposal_id) = tool_info.proposal_id {
                        self.open_diff_popup(proposal_id)?;
                        return Ok(true);
                    }
                }
            }
        }
    }
}

// Add method:
fn open_diff_popup(&mut self, proposal_id: &str) -> anyhow::Result<()> {
    let Some(ref bridge) = self.agent_bridge else {
        return Ok(());
    };
    
    let session_id = bridge.session_id();
    let proposal = bridge.storage().load_proposal(session_id, proposal_id)?;
    
    // Load original content
    let file_path = self.working_dir().join(&proposal.file_path);
    let original_content = if file_path.exists() {
        std::fs::read_to_string(&file_path).unwrap_or_default()
    } else {
        String::new()
    };
    
    let build_mode = self.state.mode == AgentMode::Build;
    self.state.diff_popup.open(proposal, original_content, build_mode);
    
    Ok(())
}
```

**Render diff popup (in render method, after other popups):**

```rust
// After rendering other popups, add:
if self.state.diff_popup.is_visible() {
    // Update visible height for scroll calculations
    self.state.diff_popup.visible_height = (area.height as f32 * 0.8 * 0.9) as u16;
    
    let popup = super::widgets::DiffPopupWidget::new(&self.state.diff_popup);
    frame.render_widget(popup, area);
}
```

---

## Testing Requirements

### Unit Tests for `src/storage/proposals.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_proposal_new() {
        let p = Proposal::new("session_1", "src/main.rs", "fn main() {}", 12345, None);
        assert!(p.id.starts_with("prop_"));
        assert_eq!(p.status, ProposalStatus::PendingExecution);
        assert_eq!(p.base_hash, 12345);
    }
    
    #[test]
    fn test_proposal_is_new_file() {
        let p = Proposal::new("session_1", "new.rs", "content", 0, None);
        assert!(p.is_new_file());
        
        let p2 = Proposal::new("session_1", "existing.rs", "content", 123, None);
        assert!(!p2.is_new_file());
    }
    
    #[test]
    fn test_status_is_actionable() {
        assert!(ProposalStatus::Pending.is_actionable());
        assert!(!ProposalStatus::PendingExecution.is_actionable());
        assert!(!ProposalStatus::Applied.is_actionable());
    }
    
    #[test]
    fn test_compute_content_hash() {
        let hash1 = compute_content_hash("hello");
        let hash2 = compute_content_hash("hello");
        let hash3 = compute_content_hash("world");
        
        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
    }
}
```

### Integration Test

```rust
#[test]
fn test_proposal_roundtrip() {
    let temp = tempfile::TempDir::new().unwrap();
    let storage = TarkStorage::new(temp.path()).unwrap();
    
    let session_id = "test_session";
    let proposal = Proposal::new(session_id, "test.rs", "fn test() {}", 12345, Some("desc".into()));
    let proposal_id = proposal.id.clone();
    
    storage.save_proposal(session_id, &proposal).unwrap();
    let loaded = storage.load_proposal(session_id, &proposal_id).unwrap();
    
    assert_eq!(loaded.id, proposal.id);
    assert_eq!(loaded.file_path, "test.rs");
    assert_eq!(loaded.new_content, "fn test() {}");
}
```

### Property Tests

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn prop_hash_deterministic(content in ".*") {
        let h1 = compute_content_hash(&content);
        let h2 = compute_content_hash(&content);
        prop_assert_eq!(h1, h2);
    }
    
    #[test]
    fn prop_proposal_serialization_roundtrip(
        file_path in "[a-z/]+\\.rs",
        content in ".*",
    ) {
        let p = Proposal::new("session", file_path, content, 0, None);
        let json = serde_json::to_string(&p).unwrap();
        let restored: Proposal = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(p.id, restored.id);
        prop_assert_eq!(p.file_path, restored.file_path);
    }
}
```

---

## Acceptance Checklist

- [ ] `propose_change` creates proposal record at `.tark/sessions/<id>/proposals/<proposal_id>.json`
- [ ] Proposal status transitions: `pending_execution` -> `pending` on completion
- [ ] Pressing `d` on propose_change tool block opens diff popup
- [ ] Diff popup shows unified view with colored +/- lines
- [ ] `s` switches to side-by-side view (if terminal >= 120 cols)
- [ ] `u` switches back to unified view
- [ ] `j`/`k` scrolls the diff
- [ ] `a` applies change in Build mode with drift detection
- [ ] `r` rejects proposal and updates status
- [ ] `Esc`/`q` closes popup
- [ ] Status message confirms proposal saved/applied/rejected
- [ ] Older pending proposals for same file are marked superseded
- [ ] New file proposals work (base_hash = 0)
- [ ] All tests pass: `cargo test --all-features`
- [ ] Linting passes: `cargo clippy --all-targets --all-features -- -D warnings`
- [ ] Formatting passes: `cargo fmt --all -- --check`

---

## Execution Order

1. Add dependencies to `Cargo.toml`
2. Create `src/storage/proposals.rs`
3. Update `src/storage/mod.rs` with exports and methods
4. Add `proposal_id` to `ToolCallInfo` in `src/tui/widgets/collapsible.rs`
5. Create `src/tui/widgets/diff_popup.rs`
6. Update `src/tui/widgets/mod.rs` with exports
7. Update `src/tui/app.rs` with state, capture logic, key handling, rendering
8. Add tests
9. Run full validation: `cargo build --release && cargo fmt --all && cargo clippy -- -D warnings && cargo test --all-features`
