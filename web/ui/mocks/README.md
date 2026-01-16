# 🚀 InnoDrupe Terminal UI - Modern TUI Mockup

A beautiful, feature-rich Terminal User Interface (TUI) mockup built with React, TypeScript, and Tailwind CSS. This project showcases a professional AI agent interface with advanced UI components and interactions.

## ✨ Features

### 🤖 Agent System
- **Multi-Mode Agent**: Switch between Build, Plan, and Ask modes
- **Agent Branding**: Renamed from "Tank" to "InnoDrupe"
- **LLM Integration Display**: Shows connected Claude 3.5 Sonnet from Anthropic
- **Connection Status**: Real-time indicator of agent connectivity

### 💬 Enhanced Conversation
- **Rich Message Types**: System, Input, Output, Tool, Command messages
- **Visual Message Bubbles**: Distinct styling for user (blue) and bot (indigo) messages
- **User Avatars**: User icon and InnoDrupe bot icon for easy identification
- **Copy to Clipboard**: One-click copy for any message with visual feedback

### 🛠️ Tool Invocation System
- **Expandable Details**: Tool invocations have collapsible detailed output sections
- **Show/Hide Toggle**: Chevron icons for intuitive expand/collapse
- **Separate Copy Buttons**: Copy main output or detailed results independently
- **Formatted Display**: Code blocks with proper syntax highlighting

### 🎯 Build Mode Selector
Three strategic build modes with meaningful visual distinction:
- **Careful** 🛡️ (Red theme): Safe & Validated approach
- **Manual** ⚡ (Amber theme): User Control with full transparency
- **Balanced** ⚖️ (Emerald theme): Optimized for most scenarios

Each mode has unique color coding and icon for quick visual recognition.

### 📁 Context File Management
- **File Picker Integration**: Click plus icon to add files from system
- **Paste Support**: Cmd+V or Ctrl+V to paste files directly
- **File Display**: Shows all added context files in blue-themed box
- **Remove Option**: Hover to reveal X button for removing files
- **Deduplication**: Prevents adding the same file twice
- **Visual Feedback**: Shows file names with file code icons

### 📊 Sidebar Panel
- **Collapsible Sections**: Session, Context, Tasks, Git Changes
- **Global Toggle**: "Collapse/Expand All" button for all sections
- **Sidebar Hide**: Minimize entire panel to maximize terminal view
- **Git Integration**: Shows file changes with diff statistics
- **Context Display**: Token usage tracking and loaded files listing

### 🎨 Modern UI/UX
- **Dark Theme**: Eye-friendly dark interface with accent colors
- **Smooth Animations**: Fade-in effects and smooth transitions
- **Responsive Design**: Adapts to different screen sizes
- **Hover Effects**: Interactive feedback on clickable elements
- **Color Coded**: Status indicators and mode selectors use meaningful colors

## 🏗️ Technical Stack

- **Frontend**: React 18+ with TypeScript
- **Styling**: Tailwind CSS with custom dark theme
- **Icons**: Lucide React for beautiful SVG icons
- **Build Tool**: Vite for fast development and building
- **State Management**: React Hooks (useState, useEffect, useRef)

## 📦 Project Structure

```
src/
├── app/
│   ├── App.tsx                    # Main app component
│   ├── layout.tsx                 # Layout wrapper
│   └── components/
│       ├── Terminal.tsx           # Main terminal UI component
│       └── Sidebar.tsx            # Right panel component
├── styles/
│   └── globals.css               # Global styles and Tailwind imports
└── index.tsx                      # React entry point
```

## 🚀 Getting Started

### Prerequisites
- Node.js 16+
- npm or yarn

### Installation

```bash
# Install dependencies
npm install

# Start development server
npm run dev

# Build for production
npm run build
```

The application will be available at `http://localhost:5173/`

## 🎮 Usage Guide

### Agent Modes
1. Click the **"Build/Plan/Ask Agent"** button to switch modes
2. Different modes enable different features in the UI
3. Build mode reveals the Build Mode Selector

### Build Modes (Build mode only)
1. Three buttons appear: Careful, Manual, Balanced
2. Click any button to select your build strategy
3. Selected mode shows highlighted background and ring effect

### Adding Context Files
1. Click the **plus icon** next to the input field
2. Select files from your system using the file picker
3. Selected files appear in the context box above the input
4. Hover over files to reveal the remove button (X)
5. **Alternative**: Paste files directly with **Cmd+V** or **Ctrl+V**

### Viewing Tool Details
1. Tool invocations display with a "Show details" button
2. Click the button to expand and see detailed output
3. Click again or use the X to collapse
4. Copy buttons appear on hover for both main and detailed output

### Using the Sidebar
1. Click section headers to expand/collapse individual sections
2. Use the **collapse all icon** to toggle all sections at once
3. Click the **chevrons icon** to hide/show the entire sidebar
4. Sidebar can be collapsed to maximize the terminal view

### Copying Content
1. Hover over any message or output
2. A copy button appears in the top right
3. Click to copy to clipboard
4. Button shows checkmark when copied
5. Auto-reverts to copy icon after 2 seconds

## 📝 Key Code Examples

### File Selection Handler
```typescript
const handleFileSelect = (e: React.ChangeEvent<HTMLInputElement>) => {
  const files = e.currentTarget.files;
  if (files) {
    const newFiles = Array.from(files).map(f => f.name);
    setAddedContextFiles(prev => [...new Set([...prev, ...newFiles])]);
  }
};
```

### Build Mode Toggle
```typescript
{(['Careful', 'Manual', 'Balanced'] as BuildMode[]).map((bMode) => {
  const isSelected = buildMode === bMode;
  return (
    <button
      onClick={() => setBuildMode(bMode)}
      className={`flex items-center gap-1.5 px-2.5 py-1.5 rounded border text-xs font-medium transition-all`}
    >
      {/* Button content */}
    </button>
  );
})}
```

### Expandable Tool Details
```typescript
{line.type === 'tool' && (
  <div className="flex flex-col gap-2 my-3 border-l-2 border-indigo-500/40">
    {/* Main tool output */}
    {expandedToolIndex === index && line.details && (
      <div className="mt-2 pt-2 border-t border-indigo-500/30">
        {/* Detailed output */}
      </div>
    )}
  </div>
)}
```

## 🎨 Color Scheme

### Message Types
- **System**: Cyan (#06b6d4) - System notifications
- **User Input**: Blue (#3b82f6) - User messages
- **Bot Output**: Indigo (#6366f1) - InnoDrupe responses
- **Tool Invocation**: Indigo (#6366f1) - Tool execution details
- **Copy Feedback**: Green (#10b981) - Success indicators

### Build Modes
- **Careful**: Red (#ef4444) - Conservative/Safe
- **Manual**: Amber (#f59e0b) - Manual/Control
- **Balanced**: Emerald (#10b981) - Balanced/Optimal

## 📊 Git Commit History

```
f22b7bca Revert to file picker for context file selection
f6497e1e Add comprehensive implementation summary document
e5d96f57 Add TUI-compatible context file addition with prompt and paste support
87d175f4 feat: add build mode selector with three options (Careful, Manual, Balanced)
a5194310 feat: improve TUI conversation UI with enhanced messaging, expandable tool details, and panel controls
```

## 🔄 State Management

### Terminal Component State
- `mode`: Current agent mode (Build/Plan/Ask)
- `buildMode`: Selected build strategy (Careful/Manual/Balanced)
- `expandedToolIndex`: Tracks expanded tool details
- `addedContextFiles`: Array of file paths added by user
- `copiedIndex`: Tracks which item was copied
- `isModeSelectorOpen`: Dropdown menu visibility

### Sidebar Component State
- `isSidebarCollapsed`: Panel visibility
- `expandedSections`: Tracks which panel sections are open

## 🚀 Performance Optimizations

- **Auto-scroll**: Efficient ref-based scrolling to new messages
- **Memoization**: Prevent unnecessary re-renders of message lists
- **Event Listeners**: Proper cleanup of paste event listeners
- **CSS-in-JS**: Tailwind classes for optimal CSS bundling

## 📱 Responsive Design

- **Desktop**: Full layout with sidebar
- **Tablet**: Collapsible sidebar for better screen real estate
- **Mobile**: Sidebar automatically hidden, focus on terminal
- **Custom Scrollbar**: Styled scrollbar matches theme

## 🔮 Future Enhancements

- [ ] Multi-session support
- [ ] Message search and filtering
- [ ] Export conversation as Markdown/JSON
- [ ] Keyboard shortcuts for common actions
- [ ] Light/Dark theme toggle
- [ ] Settings panel
- [ ] Message history persistence
- [ ] Custom agent configurations

## 📄 License

This project is part of the InnoDrupe platform redesign initiative.

## 👨‍💻 Development

### Available Scripts

```bash
npm run dev      # Start development server
npm run build    # Build for production
npm run preview  # Preview production build locally
npm run lint     # Run ESLint
```

### Code Style

This project uses:
- TypeScript for type safety
- Tailwind CSS for styling
- React Hooks for state management
- ESLint for code quality

## 🤝 Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

---

**Made with ❤️ for the InnoDrupe Project**
