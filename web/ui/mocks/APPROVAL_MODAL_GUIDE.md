# Approval Modal Location Guide

## Access URLs
- **Network Access**: http://172.19.0.2:5173/
- **Localhost** (if port forwarded): http://localhost:5173/

## Where to Find the Approval Modal

### Step-by-Step:

1. Open the URL in your browser
2. Look at the terminal conversation area (dark background with messages)
3. **Scroll all the way down** to the bottom
4. You will see in order:
   - System messages (green)
   - User/Agent conversation bubbles
   - Tool executions
   - **Question 1**: "Which package manager do you prefer?" (purple/cyan theme)
   - **Question 2**: "Which features should be included?" (purple/cyan theme)
   - **Question 3**: "What should be the project name?" (purple/cyan theme)
   - **🛡️ APPROVAL MODAL** ← HERE! (yellow/amber theme)

## What the Approval Modal Looks Like

```
🛡️ Approval Required
┌─────────────────────────────────────────────────────────┐
│ [HIGH RISK]                                              │
│                                                          │
│ Command:                                                 │
│ ┌─────────────────────────────────────────────────────┐ │
│ │ rm -rf node_modules && npm install --force          │ │
│ └─────────────────────────────────────────────────────┘ │
│                                                          │
│ This command will delete the node_modules directory...  │
│                                                          │
│ Choose an action:                                        │
│                                                          │
│ [▶ Run Once]              Execute one time only         │
│ [🛡️ Always Allow]         Add exact command to list    │
│ [✱ Pattern Match]         Allow with wildcards          │
│ [🚫 Skip]                 Don't run this command        │
└─────────────────────────────────────────────────────────┘
```

## Visual Characteristics

- **Color**: Yellow/Amber theme (different from purple questions)
- **Icon**: 🛡️ Shield icon in the left margin
- **Risk Badge**: Red "HIGH RISK" badge at top
- **4 Action Buttons**: Each with distinct icons and colors
- **Command Display**: Shown in a code block with monospace font

## If You Still Don't See It

1. Make sure you scrolled **all the way to the bottom**
2. The approval modal is AFTER the 3 questions
3. Try refreshing the page (Ctrl+R or Cmd+R)
4. Check browser console for any errors (F12)
5. The modal has a distinct amber/yellow color scheme

## Screenshots Reference

Look in the `screenshots/` folder for visual examples of the UI layout.
