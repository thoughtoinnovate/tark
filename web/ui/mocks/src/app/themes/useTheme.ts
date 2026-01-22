import { useState, useEffect, useCallback } from 'react';
import { 
  ThemePreset, 
  themePresets, 
  applyTheme, 
  loadSavedTheme, 
  defaultTheme 
} from './presets';

/**
 * Hook for managing terminal theme
 * 
 * @ratatui-note: In Ratatui, theme would be stored in App state
 * and colors accessed via theme.colors.terminal_bg etc.
 */
export function useTheme() {
  const [currentTheme, setCurrentTheme] = useState<ThemePreset>(defaultTheme);
  const [isLoaded, setIsLoaded] = useState(false);

  // Load saved theme on mount
  useEffect(() => {
    const saved = loadSavedTheme();
    setCurrentTheme(saved);
    applyTheme(saved);
    setIsLoaded(true);
  }, []);

  // Switch to a new theme
  const switchTheme = useCallback((theme: ThemePreset) => {
    setCurrentTheme(theme);
    applyTheme(theme);
  }, []);

  // Switch theme by ID
  const switchThemeById = useCallback((id: string) => {
    const theme = themePresets.find(t => t.id === id);
    if (theme) {
      switchTheme(theme);
    }
  }, [switchTheme]);

  // Cycle to next theme
  const cycleTheme = useCallback(() => {
    const currentIndex = themePresets.findIndex(t => t.id === currentTheme.id);
    const nextIndex = (currentIndex + 1) % themePresets.length;
    switchTheme(themePresets[nextIndex]);
  }, [currentTheme.id, switchTheme]);

  return {
    currentTheme,
    themes: themePresets,
    switchTheme,
    switchThemeById,
    cycleTheme,
    isLoaded,
  };
}

export type UseThemeReturn = ReturnType<typeof useTheme>;
