# 🎨 InnoDrupe Terminal UI - Visual Feature Guide

## Overview
This document provides a visual walkthrough of all implemented features in the InnoDrupe Terminal UI mockup.

---

## 1️⃣ Terminal Header & Status Bar

```
┌────────────────────────────────────────────────────────────────────┐
│ 🖥️ INNODRUPE TERMINAL                    ~/innodrupe/core/engine    │
├────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  [Build▼ Agent]         ● Claude 3.5 Sonnet (Anthropic)            │
│                                                                      │
└────────────────────────────────────────────────────────────────────┘
```

**Features:**
- Agent mode selector (Build, Plan, Ask)
- LLM connection status indicator
- Current path display
- Professional branding with InnoDrupe name

---

## 2️⃣ Conversation Messages

### System Message
```
┌─────────────────────────────────────────────────┐
│ ●  Innodrupe Core v2.1.0 initialized            │
└─────────────────────────────────────────────────┘
```
**Style**: Cyan background, system notification indicator

### User Message
```
┌─────────────────────────────────────────────────┐
│ 👤 You                                          │
│ Help me refactor the authentication module and  │
│ add JWT support                                 │
└─────────────────────────────────────────────────┘
```
**Style**: Blue theme with user icon, right-aligned

### Bot Message
```
┌─────────────────────────────────────────────────┐
│ 🤖 InnoDrupe                                    │
│ I'll analyze your authentication module...      │
│ [Copy]                                          │
└─────────────────────────────────────────────────┘
```
**Style**: Indigo theme with bot icon, copy button on hover

---

## 3️⃣ Expandable Tool Invocation

### Collapsed State
```
┌─────────────────────────────────────────────────┐
│ 🔧 TOOL INVOCATION          [Show details ▼]   │
│ ┌─────────────────────────────────────────────┐ │
│ │ SCAN: src/auth/ - Analyzing patterns       │ │
│ │                                      [Copy]  │ │
│ └─────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────┘
```

### Expanded State
```
┌─────────────────────────────────────────────────┐
│ 🔧 TOOL INVOCATION          [Hide details ▲]   │
│ ┌─────────────────────────────────────────────┐ │
│ │ SCAN: src/auth/ - Analyzing patterns       │ │
│ │                                      [Copy]  │ │
│ └─────────────────────────────────────────────┘ │
│                                                  │
│ Detailed Output:                                │
│ ┌─────────────────────────────────────────────┐ │
│ │ ✓ Scanned 3 auth files                     │ │
│ │ ✓ Found legacy session patterns            │ │
│ │ ✓ Identified refactor opportunities       │ │
│ │ ✓ Security recommendations:                │ │
│ │   - Add JWT token expiration               │ │
│ │   - Implement refresh token rotation       │ │
│ │                                      [Copy]  │ │
│ └─────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────┘
```

**Features:**
- Chevron icon indicates expand/collapse state
- Separate copy buttons for main and detailed output
- Smooth animation on expand/collapse
- Formatted code blocks

---

## 4️⃣ Build Mode Selector

Only visible when **Build Agent** mode is selected.

### Visual Representation
```
┌──────────────────────────────────────────────────┐
│ Build Mode: [🛡️ Careful] [⚡ Manual] [⚖️ Balanced]  │
└──────────────────────────────────────────────────┘
```

### Mode Details

| Mode | Icon | Color | Purpose |
|------|------|-------|---------|
| **Careful** | 🛡️ | Red | Safe & Validated approach |
| **Manual** | ⚡ | Amber | User Control with transparency |
| **Balanced** | ⚖️ | Emerald | Optimized for most scenarios |

### Active Mode Example
```
┌──────────────────────────────────────────────────┐
│ Build Mode: [ Careful ] [⚡ Manual] [⚖️ Balanced]  │
│            ▲ Selected with ring effect            │
└──────────────────────────────────────────────────┘
```

**Visual Feedback:**
- Selected mode: Highlighted background + ring effect
- Hover effect on unselected modes
- Icon and label visible
- Smooth color transitions

---

## 5️⃣ Context File Management

### Empty State
```
┌─────────────────────────────────────────────────┐
│ ➕ [INSERT] Send a message to Build agent...    │
└─────────────────────────────────────────────────┘
```

### With Files Added
```
┌─────────────────────────────────────────────────┐
│ Context: [📄 auth.ts] [📄 config.json] [📄 .env] │
│          Hovering over reveals ✕ button         │
├─────────────────────────────────────────────────┤
│ ➕ [INSERT] Send a message to Build agent...    │
└─────────────────────────────────────────────────┘
```

### File Picker Interaction
```
Click plus button
      ↓
File picker opens
      ↓
Select one or more files
      ↓
Files appear in context box with 📄 icon
      ↓
Hover over file to see ✕ button
      ↓
Click ✕ to remove file
```

### Alternative: Paste Support
```
Press Cmd+V or Ctrl+V
      ↓
Paste files from clipboard
      ↓
Files automatically added
      ↓
Duplicates prevented
```

**Features:**
- Multi-file selection support
- Paste from clipboard (Cmd+V / Ctrl+V)
- File icons with names
- Hover-to-reveal remove buttons
- Deduplication
- Visual grouping in blue box

---

## 6️⃣ Sidebar Panel

### Expanded State
```
┌──────────────────────────────────┐
│ Panel  [↞] [⇅]                   │
├──────────────────────────────────┤
│ ▼ Session                        │
│  • main                          │
│  • feature/sidebar-update        │
│  • gemini-1.5-pro-preview        │
│  • $0.015 (3 models)             │
├──────────────────────────────────┤
│ ▼ Context 1.0k                   │
│  1,833 / 1,000,000 tokens        │
│  ▶ Loaded Files (8)              │
├──────────────────────────────────┤
│ ▼ Tasks 8                        │
│  🟢 Understanding the codebase   │
│  Queued (7 more...)              │
├──────────────────────────────────┤
│ ▼ Git Changes 12                 │
│  7 Mod | 3 New | 2 Del           │
└──────────────────────────────────┘
```

### Collapsed State
```
┌──────────────────────────────────┐
│ [⇅]                              │
│                                  │
│ • main                           │
│ • feature/sidebar-update         │
│ • gemini-1.5-pro-preview         │
│ • $0.015 (3 models)              │
│ ...                              │
│                                  │
└──────────────────────────────────┘
```

**Features:**
- Collapsible/expandable sections
- "Collapse all" toggle button
- Entire sidebar can be hidden
- Individual sections expand independently
- Shows Git status, tasks, context info
- Session information display

---

## 7️⃣ Copy to Clipboard Feature

### Before Copy
```
┌─────────────────────────────────────────────────┐
│ 🤖 InnoDrupe                                    │
│ Here's the implementation...                    │
│ (hover to show copy button)                     │
└─────────────────────────────────────────────────┘
```

### During Hover
```
┌─────────────────────────────────────────────────┐
│ 🤖 InnoDrupe                                    │
│ Here's the implementation...     [📋]           │
│                                  Copy button     │
└─────────────────────────────────────────────────┘
```

### After Click
```
┌─────────────────────────────────────────────────┐
│ 🤖 InnoDrupe                                    │
│ Here's the implementation...     [✓]            │
│                                  Success!       │
│                        (resets after 2 seconds)  │
└─────────────────────────────────────────────────┘
```

**Features:**
- Hover-based copy button
- Works for all message types
- Visual feedback with checkmark
- Auto-resets to copy icon
- Smooth animation

---

## 8️⃣ Color Palette

### Message Types
```
System:      ●━━━━━━━━━ Cyan     (#06b6d4)
User:        ○━━━━━━━━━ Blue     (#3b82f6)
Bot:         ○━━━━━━━━━ Indigo   (#6366f1)
Tool:        ●━━━━━━━━━ Indigo   (#6366f1)
```

### Build Modes
```
Careful:     ●━━━━━━━━━ Red      (#ef4444)
Manual:      ●━━━━━━━━━ Amber    (#f59e0b)
Balanced:    ●━━━━━━━━━ Emerald  (#10b981)
```

### Context Files
```
Context:     ●━━━━━━━━━ Blue     (#3b82f6)
```

---

## 9️⃣ Interactive Workflow

### Complete User Journey

```
1. LAUNCH
   ↓
2. SELECT AGENT MODE (Build/Plan/Ask)
   ↓
3. IF BUILD MODE SELECTED:
   ├─ SELECT BUILD STRATEGY (Careful/Manual/Balanced)
   └─ ADD CONTEXT FILES (Click + or Paste)
   ↓
4. TYPE MESSAGE
   ↓
5. AGENT PROCESSES & DISPLAYS:
   ├─ System messages
   ├─ Bot responses
   ├─ Tool invocations (expandable)
   └─ Detailed output
   ↓
6. USER CAN:
   ├─ Copy any message
   ├─ Expand tool details
   ├─ Switch modes
   └─ Toggle sidebar
```

---

## 🔟 Responsive Behavior

### Desktop (Wide Screen)
```
┌─────────────────────────────────┬──────────────┐
│                                 │              │
│      Terminal                   │   Sidebar    │
│                                 │   (visible)  │
│                                 │              │
└─────────────────────────────────┴──────────────┘
```

### Tablet (Medium Screen)
```
┌──────────────────────────────────────────────┐
│                                              │
│  Terminal (sidebar can be toggled)           │
│                                              │
└──────────────────────────────────────────────┘
```

### Mobile (Narrow Screen)
```
┌──────────────────┐
│                  │
│ Terminal only    │
│ (sidebar hidden) │
│                  │
└──────────────────┘
```

---

## ✨ Key Visual Effects

- **Fade-in animations**: Messages appear smoothly
- **Smooth transitions**: Color changes and hover effects
- **Hover feedback**: Interactive elements respond to cursor
- **Copy feedback**: Visual confirmation of clipboard action
- **Section collapse/expand**: Smooth height transitions
- **Ring effects**: Selected items highlighted with rings
- **Shadow effects**: Depth and layering

---

## 📊 Feature Checklist

- ✅ Multi-mode agent selector
- ✅ Agent name: InnoDrupe
- ✅ Rich message types (5 types)
- ✅ Message bubbles with avatars
- ✅ Expandable tool invocation details
- ✅ Copy to clipboard (all messages)
- ✅ Build mode selector (3 modes)
- ✅ Context file management
- ✅ File picker integration
- ✅ Paste support (Cmd+V / Ctrl+V)
- ✅ Collapsible sidebar
- ✅ Collapse all sections toggle
- ✅ Color-coded UI elements
- ✅ Responsive design
- ✅ Smooth animations

---

**Visual Design**: Modern dark theme with accent colors
**User Experience**: Intuitive and interactive
**Accessibility**: Semantic HTML and clear labels
**Performance**: Optimized rendering and smooth interactions

