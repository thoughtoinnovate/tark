/**
 * Theme Presets for Terminal UI
 * 
 * Each theme defines CSS custom properties that control the appearance
 * of the terminal, modals, messages, and other UI elements.
 * 
 * @ratatui-note: In Ratatui, these would map to Color::Rgb values
 * stored in a ThemeConfig struct.
 */

export interface ThemePreset {
  id: string;
  name: string;
  description: string;
  isDark: boolean;
  colors: {
    // Base
    background: string;
    foreground: string;
    
    // Terminal
    terminalBg: string;
    terminalBorder: string;
    terminalHeaderBg: string;
    
    // Status Bar
    statusBarBg: string;
    statusBarBorder: string;
    
    // Thinking Mode
    thinkingActive: string;
    thinkingInactive: string;
    thinkingBg: string;
    thinkingBorder: string;
    
    // LLM Status
    llmConnected: string;
    llmError: string;
    
    // Mode Selectors
    modeActiveBg: string;
    modeActiveBorder: string;
    modeHoverBg: string;
    
    // Context Files
    contextBg: string;
    contextBorder: string;
    contextText: string;
    contextBadgeBg: string;
    contextBadgeBorder: string;
    
    // Message Types
    msgSystem: string;
    msgUser: string;
    msgAgent: string;
    msgTool: string;
    msgCommand: string;
    msgThinking: string;
    msgQuestion: string;
    
    // Modals
    modalBg: string;
    modalBorder: string;
    modalOverlay: string;
    
    // File Picker
    pickerSelectedBg: string;
    pickerSelectedBorder: string;
    pickerHoverBg: string;
    pickerFolder: string;
    pickerFile: string;
    
    // User Message Bubble
    userBubbleBg: string;
    userBubbleBorder: string;
    userBubbleText: string;
    userIconBg: string;
    userIconBorder: string;
    userIconColor: string;
    userLabelColor: string;
    
    // Agent Message Bubble
    agentBubbleBg: string;
    agentBubbleBorder: string;
    agentBubbleText: string;
    agentIconBg: string;
    agentIconBorder: string;
    agentIconColor: string;
    agentLabelColor: string;
  };
}

// ============================================
// CATPPUCCIN MOCHA (Dark)
// ============================================
export const catppuccinMocha: ThemePreset = {
  id: 'catppuccin-mocha',
  name: 'Catppuccin Mocha',
  description: 'Soothing pastel theme for the high-spirited (dark)',
  isDark: true,
  colors: {
    background: '#1e1e2e',
    foreground: '#cdd6f4',
    
    terminalBg: '#1e1e2e',
    terminalBorder: '#313244',
    terminalHeaderBg: '#181825',
    
    statusBarBg: 'rgba(24, 24, 37, 0.95)',
    statusBarBorder: '#313244',
    
    thinkingActive: '#f9e2af',
    thinkingInactive: '#6c7086',
    thinkingBg: 'rgba(249, 226, 175, 0.1)',
    thinkingBorder: 'rgba(249, 226, 175, 0.3)',
    
    llmConnected: '#a6e3a1',
    llmError: '#fab387',
    
    modeActiveBg: '#313244',
    modeActiveBorder: '#45475a',
    modeHoverBg: 'rgba(69, 71, 90, 0.5)',
    
    contextBg: 'rgba(137, 180, 250, 0.1)',
    contextBorder: 'rgba(137, 180, 250, 0.2)',
    contextText: '#89b4fa',
    contextBadgeBg: 'rgba(137, 180, 250, 0.15)',
    contextBadgeBorder: 'rgba(137, 180, 250, 0.25)',
    
    msgSystem: '#94e2d5',
    msgUser: '#bac2de',
    msgAgent: '#cdd6f4',
    msgTool: '#a6adc8',
    msgCommand: '#a6e3a1',
    msgThinking: '#9399b2',
    msgQuestion: '#89dceb',
    
    modalBg: '#181825',
    modalBorder: '#313244',
    modalOverlay: 'rgba(17, 17, 27, 0.7)',
    
    pickerSelectedBg: 'rgba(137, 180, 250, 0.12)',
    pickerSelectedBorder: '#89b4fa',
    pickerHoverBg: '#313244',
    pickerFolder: '#94e2d5',
    pickerFile: '#9399b2',
    
    // User - Sapphire blue
    userBubbleBg: 'rgba(116, 199, 236, 0.08)',
    userBubbleBorder: 'rgba(116, 199, 236, 0.2)',
    userBubbleText: '#cdd6f4',
    userIconBg: 'rgba(116, 199, 236, 0.15)',
    userIconBorder: 'rgba(116, 199, 236, 0.3)',
    userIconColor: '#74c7ec',
    userLabelColor: '#74c7ec',
    
    // Agent - Green
    agentBubbleBg: 'rgba(166, 227, 161, 0.08)',
    agentBubbleBorder: 'rgba(166, 227, 161, 0.2)',
    agentBubbleText: '#cdd6f4',
    agentIconBg: 'rgba(166, 227, 161, 0.15)',
    agentIconBorder: 'rgba(166, 227, 161, 0.3)',
    agentIconColor: '#a6e3a1',
    agentLabelColor: '#a6e3a1',
  },
};

// ============================================
// CATPPUCCIN LATTE (Light)
// ============================================
export const catppuccinLatte: ThemePreset = {
  id: 'catppuccin-latte',
  name: 'Catppuccin Latte',
  description: 'Soothing pastel theme for the high-spirited (light)',
  isDark: false,
  colors: {
    background: '#eff1f5',
    foreground: '#4c4f69',
    
    terminalBg: '#e6e9ef',
    terminalBorder: '#ccd0da',
    terminalHeaderBg: '#dce0e8',
    
    statusBarBg: 'rgba(220, 224, 232, 0.95)',
    statusBarBorder: '#ccd0da',
    
    thinkingActive: '#df8e1d',
    thinkingInactive: '#8c8fa1',
    thinkingBg: 'rgba(223, 142, 29, 0.1)',
    thinkingBorder: 'rgba(223, 142, 29, 0.3)',
    
    llmConnected: '#40a02b',
    llmError: '#fe640b',
    
    modeActiveBg: '#ccd0da',
    modeActiveBorder: '#bcc0cc',
    modeHoverBg: 'rgba(172, 176, 190, 0.5)',
    
    contextBg: 'rgba(30, 102, 245, 0.1)',
    contextBorder: 'rgba(30, 102, 245, 0.2)',
    contextText: '#1e66f5',
    contextBadgeBg: 'rgba(30, 102, 245, 0.15)',
    contextBadgeBorder: 'rgba(30, 102, 245, 0.25)',
    
    msgSystem: '#179299',
    msgUser: '#5c5f77',
    msgAgent: '#4c4f69',
    msgTool: '#6c6f85',
    msgCommand: '#40a02b',
    msgThinking: '#8c8fa1',
    msgQuestion: '#04a5e5',
    
    modalBg: '#dce0e8',
    modalBorder: '#ccd0da',
    modalOverlay: 'rgba(76, 79, 105, 0.4)',
    
    pickerSelectedBg: 'rgba(30, 102, 245, 0.12)',
    pickerSelectedBorder: '#1e66f5',
    pickerHoverBg: '#ccd0da',
    pickerFolder: '#179299',
    pickerFile: '#6c6f85',
    
    // User - Blue
    userBubbleBg: 'rgba(30, 102, 245, 0.08)',
    userBubbleBorder: 'rgba(30, 102, 245, 0.2)',
    userBubbleText: '#4c4f69',
    userIconBg: 'rgba(30, 102, 245, 0.12)',
    userIconBorder: 'rgba(30, 102, 245, 0.25)',
    userIconColor: '#1e66f5',
    userLabelColor: '#1e66f5',
    
    // Agent - Green
    agentBubbleBg: 'rgba(64, 160, 43, 0.08)',
    agentBubbleBorder: 'rgba(64, 160, 43, 0.2)',
    agentBubbleText: '#4c4f69',
    agentIconBg: 'rgba(64, 160, 43, 0.12)',
    agentIconBorder: 'rgba(64, 160, 43, 0.25)',
    agentIconColor: '#40a02b',
    agentLabelColor: '#40a02b',
  },
};

// ============================================
// GITHUB DARK
// ============================================
export const githubDark: ThemePreset = {
  id: 'github-dark',
  name: 'GitHub Dark',
  description: 'GitHub\'s dark theme',
  isDark: true,
  colors: {
    background: '#0d1117',
    foreground: '#e6edf3',
    
    terminalBg: '#0d1117',
    terminalBorder: '#30363d',
    terminalHeaderBg: '#161b22',
    
    statusBarBg: 'rgba(22, 27, 34, 0.95)',
    statusBarBorder: '#30363d',
    
    thinkingActive: '#f59e0b',
    thinkingInactive: '#6b7280',
    thinkingBg: 'rgba(245, 158, 11, 0.1)',
    thinkingBorder: 'rgba(245, 158, 11, 0.3)',
    
    llmConnected: '#3fb950',
    llmError: '#f85149',
    
    modeActiveBg: '#21262d',
    modeActiveBorder: '#30363d',
    modeHoverBg: 'rgba(48, 54, 61, 0.5)',
    
    contextBg: 'rgba(56, 139, 253, 0.1)',
    contextBorder: 'rgba(56, 139, 253, 0.2)',
    contextText: '#58a6ff',
    contextBadgeBg: 'rgba(56, 139, 253, 0.15)',
    contextBadgeBorder: 'rgba(56, 139, 253, 0.25)',
    
    msgSystem: '#58a6ff',
    msgUser: '#e6edf3',
    msgAgent: '#c9d1d9',
    msgTool: '#8b949e',
    msgCommand: '#3fb950',
    msgThinking: '#8b949e',
    msgQuestion: '#a371f7',
    
    modalBg: '#161b22',
    modalBorder: '#30363d',
    modalOverlay: 'rgba(0, 0, 0, 0.6)',
    
    pickerSelectedBg: 'rgba(56, 139, 253, 0.12)',
    pickerSelectedBorder: '#58a6ff',
    pickerHoverBg: '#21262d',
    pickerFolder: '#58a6ff',
    pickerFile: '#8b949e',
    
    // User - Blue
    userBubbleBg: 'rgba(56, 139, 253, 0.08)',
    userBubbleBorder: 'rgba(56, 139, 253, 0.2)',
    userBubbleText: '#e6edf3',
    userIconBg: 'rgba(56, 139, 253, 0.15)',
    userIconBorder: 'rgba(56, 139, 253, 0.3)',
    userIconColor: '#58a6ff',
    userLabelColor: '#58a6ff',
    
    // Agent - Green
    agentBubbleBg: 'rgba(63, 185, 80, 0.08)',
    agentBubbleBorder: 'rgba(63, 185, 80, 0.2)',
    agentBubbleText: '#e6edf3',
    agentIconBg: 'rgba(63, 185, 80, 0.15)',
    agentIconBorder: 'rgba(63, 185, 80, 0.3)',
    agentIconColor: '#3fb950',
    agentLabelColor: '#3fb950',
  },
};

// ============================================
// NORD
// ============================================
export const nord: ThemePreset = {
  id: 'nord',
  name: 'Nord',
  description: 'Arctic, north-bluish color palette',
  isDark: true,
  colors: {
    background: '#2e3440',
    foreground: '#eceff4',
    
    terminalBg: '#2e3440',
    terminalBorder: '#3b4252',
    terminalHeaderBg: '#272c36',
    
    statusBarBg: 'rgba(39, 44, 54, 0.95)',
    statusBarBorder: '#3b4252',
    
    thinkingActive: '#ebcb8b',
    thinkingInactive: '#4c566a',
    thinkingBg: 'rgba(235, 203, 139, 0.1)',
    thinkingBorder: 'rgba(235, 203, 139, 0.3)',
    
    llmConnected: '#a3be8c',
    llmError: '#bf616a',
    
    modeActiveBg: '#3b4252',
    modeActiveBorder: '#434c5e',
    modeHoverBg: 'rgba(67, 76, 94, 0.5)',
    
    contextBg: 'rgba(136, 192, 208, 0.1)',
    contextBorder: 'rgba(136, 192, 208, 0.2)',
    contextText: '#88c0d0',
    contextBadgeBg: 'rgba(136, 192, 208, 0.15)',
    contextBadgeBorder: 'rgba(136, 192, 208, 0.25)',
    
    msgSystem: '#88c0d0',
    msgUser: '#eceff4',
    msgAgent: '#e5e9f0',
    msgTool: '#d8dee9',
    msgCommand: '#a3be8c',
    msgThinking: '#4c566a',
    msgQuestion: '#b48ead',
    
    modalBg: '#272c36',
    modalBorder: '#3b4252',
    modalOverlay: 'rgba(0, 0, 0, 0.6)',
    
    pickerSelectedBg: 'rgba(136, 192, 208, 0.12)',
    pickerSelectedBorder: '#88c0d0',
    pickerHoverBg: '#3b4252',
    pickerFolder: '#88c0d0',
    pickerFile: '#4c566a',
    
    // User - Frost blue
    userBubbleBg: 'rgba(136, 192, 208, 0.08)',
    userBubbleBorder: 'rgba(136, 192, 208, 0.2)',
    userBubbleText: '#eceff4',
    userIconBg: 'rgba(136, 192, 208, 0.15)',
    userIconBorder: 'rgba(136, 192, 208, 0.3)',
    userIconColor: '#88c0d0',
    userLabelColor: '#88c0d0',
    
    // Agent - Green
    agentBubbleBg: 'rgba(163, 190, 140, 0.08)',
    agentBubbleBorder: 'rgba(163, 190, 140, 0.2)',
    agentBubbleText: '#eceff4',
    agentIconBg: 'rgba(163, 190, 140, 0.15)',
    agentIconBorder: 'rgba(163, 190, 140, 0.3)',
    agentIconColor: '#a3be8c',
    agentLabelColor: '#a3be8c',
  },
};

// ============================================
// ONE DARK PRO
// ============================================
export const oneDarkPro: ThemePreset = {
  id: 'one-dark-pro',
  name: 'One Dark Pro',
  description: 'Atom\'s iconic One Dark theme',
  isDark: true,
  colors: {
    background: '#282c34',
    foreground: '#abb2bf',
    
    terminalBg: '#282c34',
    terminalBorder: '#3e4451',
    terminalHeaderBg: '#21252b',
    
    statusBarBg: 'rgba(33, 37, 43, 0.95)',
    statusBarBorder: '#3e4451',
    
    thinkingActive: '#e5c07b',
    thinkingInactive: '#5c6370',
    thinkingBg: 'rgba(229, 192, 123, 0.1)',
    thinkingBorder: 'rgba(229, 192, 123, 0.3)',
    
    llmConnected: '#98c379',
    llmError: '#e06c75',
    
    modeActiveBg: '#3e4451',
    modeActiveBorder: '#4b5263',
    modeHoverBg: 'rgba(62, 68, 81, 0.5)',
    
    contextBg: 'rgba(97, 175, 239, 0.1)',
    contextBorder: 'rgba(97, 175, 239, 0.2)',
    contextText: '#61afef',
    contextBadgeBg: 'rgba(97, 175, 239, 0.15)',
    contextBadgeBorder: 'rgba(97, 175, 239, 0.25)',
    
    msgSystem: '#56b6c2',
    msgUser: '#abb2bf',
    msgAgent: '#abb2bf',
    msgTool: '#5c6370',
    msgCommand: '#98c379',
    msgThinking: '#5c6370',
    msgQuestion: '#c678dd',
    
    modalBg: '#21252b',
    modalBorder: '#3e4451',
    modalOverlay: 'rgba(0, 0, 0, 0.6)',
    
    pickerSelectedBg: 'rgba(97, 175, 239, 0.12)',
    pickerSelectedBorder: '#61afef',
    pickerHoverBg: '#3e4451',
    pickerFolder: '#61afef',
    pickerFile: '#5c6370',
    
    // User - Blue
    userBubbleBg: 'rgba(97, 175, 239, 0.08)',
    userBubbleBorder: 'rgba(97, 175, 239, 0.2)',
    userBubbleText: '#abb2bf',
    userIconBg: 'rgba(97, 175, 239, 0.15)',
    userIconBorder: 'rgba(97, 175, 239, 0.3)',
    userIconColor: '#61afef',
    userLabelColor: '#61afef',
    
    // Agent - Green
    agentBubbleBg: 'rgba(152, 195, 121, 0.08)',
    agentBubbleBorder: 'rgba(152, 195, 121, 0.2)',
    agentBubbleText: '#abb2bf',
    agentIconBg: 'rgba(152, 195, 121, 0.15)',
    agentIconBorder: 'rgba(152, 195, 121, 0.3)',
    agentIconColor: '#98c379',
    agentLabelColor: '#98c379',
  },
};

// ============================================
// TOKYO NIGHT
// ============================================
export const tokyoNight: ThemePreset = {
  id: 'tokyo-night',
  name: 'Tokyo Night',
  description: 'A clean, dark theme inspired by Tokyo city lights',
  isDark: true,
  colors: {
    background: '#1a1b26',
    foreground: '#c0caf5',
    
    terminalBg: '#1a1b26',
    terminalBorder: '#292e42',
    terminalHeaderBg: '#16161e',
    
    statusBarBg: 'rgba(22, 22, 30, 0.95)',
    statusBarBorder: '#292e42',
    
    thinkingActive: '#e0af68',
    thinkingInactive: '#565f89',
    thinkingBg: 'rgba(224, 175, 104, 0.1)',
    thinkingBorder: 'rgba(224, 175, 104, 0.3)',
    
    llmConnected: '#9ece6a',
    llmError: '#f7768e',
    
    modeActiveBg: '#292e42',
    modeActiveBorder: '#3b4261',
    modeHoverBg: 'rgba(59, 66, 97, 0.5)',
    
    contextBg: 'rgba(122, 162, 247, 0.1)',
    contextBorder: 'rgba(122, 162, 247, 0.2)',
    contextText: '#7aa2f7',
    contextBadgeBg: 'rgba(122, 162, 247, 0.15)',
    contextBadgeBorder: 'rgba(122, 162, 247, 0.25)',
    
    msgSystem: '#7dcfff',
    msgUser: '#c0caf5',
    msgAgent: '#c0caf5',
    msgTool: '#565f89',
    msgCommand: '#9ece6a',
    msgThinking: '#565f89',
    msgQuestion: '#bb9af7',
    
    modalBg: '#16161e',
    modalBorder: '#292e42',
    modalOverlay: 'rgba(0, 0, 0, 0.6)',
    
    pickerSelectedBg: 'rgba(122, 162, 247, 0.12)',
    pickerSelectedBorder: '#7aa2f7',
    pickerHoverBg: '#292e42',
    pickerFolder: '#7dcfff',
    pickerFile: '#565f89',
    
    // User - Blue
    userBubbleBg: 'rgba(122, 162, 247, 0.08)',
    userBubbleBorder: 'rgba(122, 162, 247, 0.2)',
    userBubbleText: '#c0caf5',
    userIconBg: 'rgba(122, 162, 247, 0.15)',
    userIconBorder: 'rgba(122, 162, 247, 0.3)',
    userIconColor: '#7aa2f7',
    userLabelColor: '#7aa2f7',
    
    // Agent - Green
    agentBubbleBg: 'rgba(158, 206, 106, 0.08)',
    agentBubbleBorder: 'rgba(158, 206, 106, 0.2)',
    agentBubbleText: '#c0caf5',
    agentIconBg: 'rgba(158, 206, 106, 0.15)',
    agentIconBorder: 'rgba(158, 206, 106, 0.3)',
    agentIconColor: '#9ece6a',
    agentLabelColor: '#9ece6a',
  },
};

// ============================================
// GRUVBOX DARK
// ============================================
export const gruvboxDark: ThemePreset = {
  id: 'gruvbox-dark',
  name: 'Gruvbox Dark',
  description: 'Retro groove color scheme',
  isDark: true,
  colors: {
    background: '#282828',
    foreground: '#ebdbb2',
    
    terminalBg: '#282828',
    terminalBorder: '#3c3836',
    terminalHeaderBg: '#1d2021',
    
    statusBarBg: 'rgba(29, 32, 33, 0.95)',
    statusBarBorder: '#3c3836',
    
    thinkingActive: '#fabd2f',
    thinkingInactive: '#665c54',
    thinkingBg: 'rgba(250, 189, 47, 0.1)',
    thinkingBorder: 'rgba(250, 189, 47, 0.3)',
    
    llmConnected: '#b8bb26',
    llmError: '#fb4934',
    
    modeActiveBg: '#3c3836',
    modeActiveBorder: '#504945',
    modeHoverBg: 'rgba(80, 73, 69, 0.5)',
    
    contextBg: 'rgba(131, 165, 152, 0.1)',
    contextBorder: 'rgba(131, 165, 152, 0.2)',
    contextText: '#83a598',
    contextBadgeBg: 'rgba(131, 165, 152, 0.15)',
    contextBadgeBorder: 'rgba(131, 165, 152, 0.25)',
    
    msgSystem: '#8ec07c',
    msgUser: '#ebdbb2',
    msgAgent: '#ebdbb2',
    msgTool: '#a89984',
    msgCommand: '#b8bb26',
    msgThinking: '#665c54',
    msgQuestion: '#d3869b',
    
    modalBg: '#1d2021',
    modalBorder: '#3c3836',
    modalOverlay: 'rgba(0, 0, 0, 0.6)',
    
    pickerSelectedBg: 'rgba(131, 165, 152, 0.12)',
    pickerSelectedBorder: '#83a598',
    pickerHoverBg: '#3c3836',
    pickerFolder: '#8ec07c',
    pickerFile: '#665c54',
    
    // User - Aqua/Blue
    userBubbleBg: 'rgba(131, 165, 152, 0.08)',
    userBubbleBorder: 'rgba(131, 165, 152, 0.2)',
    userBubbleText: '#ebdbb2',
    userIconBg: 'rgba(131, 165, 152, 0.15)',
    userIconBorder: 'rgba(131, 165, 152, 0.3)',
    userIconColor: '#83a598',
    userLabelColor: '#83a598',
    
    // Agent - Green
    agentBubbleBg: 'rgba(184, 187, 38, 0.08)',
    agentBubbleBorder: 'rgba(184, 187, 38, 0.2)',
    agentBubbleText: '#ebdbb2',
    agentIconBg: 'rgba(184, 187, 38, 0.15)',
    agentIconBorder: 'rgba(184, 187, 38, 0.3)',
    agentIconColor: '#b8bb26',
    agentLabelColor: '#b8bb26',
  },
};

// ============================================
// ALL PRESETS
// ============================================
export const themePresets: ThemePreset[] = [
  catppuccinMocha,
  catppuccinLatte,
  githubDark,
  nord,
  oneDarkPro,
  tokyoNight,
  gruvboxDark,
];

export const defaultTheme = catppuccinMocha;

/**
 * Apply a theme preset to the document
 */
export function applyTheme(theme: ThemePreset): void {
  const root = document.documentElement;
  
  // Set dark mode class
  if (theme.isDark) {
    root.classList.add('dark');
  } else {
    root.classList.remove('dark');
  }
  
  // Apply all CSS custom properties
  const { colors } = theme;
  
  root.style.setProperty('--background', colors.background);
  root.style.setProperty('--foreground', colors.foreground);
  
  root.style.setProperty('--terminal-bg', colors.terminalBg);
  root.style.setProperty('--terminal-border', colors.terminalBorder);
  root.style.setProperty('--terminal-header-bg', colors.terminalHeaderBg);
  
  root.style.setProperty('--status-bar-bg', colors.statusBarBg);
  root.style.setProperty('--status-bar-border', colors.statusBarBorder);
  
  root.style.setProperty('--thinking-active', colors.thinkingActive);
  root.style.setProperty('--thinking-inactive', colors.thinkingInactive);
  root.style.setProperty('--thinking-bg', colors.thinkingBg);
  root.style.setProperty('--thinking-border', colors.thinkingBorder);
  
  root.style.setProperty('--llm-connected', colors.llmConnected);
  root.style.setProperty('--llm-error', colors.llmError);
  
  root.style.setProperty('--mode-active-bg', colors.modeActiveBg);
  root.style.setProperty('--mode-active-border', colors.modeActiveBorder);
  root.style.setProperty('--mode-hover-bg', colors.modeHoverBg);
  
  root.style.setProperty('--context-bg', colors.contextBg);
  root.style.setProperty('--context-border', colors.contextBorder);
  root.style.setProperty('--context-text', colors.contextText);
  root.style.setProperty('--context-badge-bg', colors.contextBadgeBg);
  root.style.setProperty('--context-badge-border', colors.contextBadgeBorder);
  
  root.style.setProperty('--msg-system', colors.msgSystem);
  root.style.setProperty('--msg-user', colors.msgUser);
  root.style.setProperty('--msg-agent', colors.msgAgent);
  root.style.setProperty('--msg-tool', colors.msgTool);
  root.style.setProperty('--msg-command', colors.msgCommand);
  root.style.setProperty('--msg-thinking', colors.msgThinking);
  root.style.setProperty('--msg-question', colors.msgQuestion);
  
  root.style.setProperty('--modal-bg', colors.modalBg);
  root.style.setProperty('--modal-border', colors.modalBorder);
  root.style.setProperty('--modal-overlay', colors.modalOverlay);
  
  root.style.setProperty('--picker-selected-bg', colors.pickerSelectedBg);
  root.style.setProperty('--picker-selected-border', colors.pickerSelectedBorder);
  root.style.setProperty('--picker-hover-bg', colors.pickerHoverBg);
  root.style.setProperty('--picker-folder', colors.pickerFolder);
  root.style.setProperty('--picker-file', colors.pickerFile);
  
  // User Message Bubble
  root.style.setProperty('--user-bubble-bg', colors.userBubbleBg);
  root.style.setProperty('--user-bubble-border', colors.userBubbleBorder);
  root.style.setProperty('--user-bubble-text', colors.userBubbleText);
  root.style.setProperty('--user-icon-bg', colors.userIconBg);
  root.style.setProperty('--user-icon-border', colors.userIconBorder);
  root.style.setProperty('--user-icon-color', colors.userIconColor);
  root.style.setProperty('--user-label-color', colors.userLabelColor);
  
  // Agent Message Bubble
  root.style.setProperty('--agent-bubble-bg', colors.agentBubbleBg);
  root.style.setProperty('--agent-bubble-border', colors.agentBubbleBorder);
  root.style.setProperty('--agent-bubble-text', colors.agentBubbleText);
  root.style.setProperty('--agent-icon-bg', colors.agentIconBg);
  root.style.setProperty('--agent-icon-border', colors.agentIconBorder);
  root.style.setProperty('--agent-icon-color', colors.agentIconColor);
  root.style.setProperty('--agent-label-color', colors.agentLabelColor);
  
  // Save to localStorage
  localStorage.setItem('terminal-theme', theme.id);
}

/**
 * Load theme from localStorage or return default
 */
export function loadSavedTheme(): ThemePreset {
  const savedId = localStorage.getItem('terminal-theme');
  if (savedId) {
    const found = themePresets.find(t => t.id === savedId);
    if (found) return found;
  }
  return defaultTheme;
}

/**
 * Get theme by ID
 */
export function getThemeById(id: string): ThemePreset | undefined {
  return themePresets.find(t => t.id === id);
}
