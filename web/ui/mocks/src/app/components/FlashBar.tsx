/**
 * ============================================================================
 * FLASH BAR COMPONENT
 * ============================================================================
 * 
 * A status indicator bar positioned between messages and input area.
 * Shows animated dots when working, static dot when idle, and colored
 * backgrounds for errors/warnings/rate limits.
 * 
 * @ratatui-widget: Custom widget with animated rendering
 * @ratatui-layout: Constraint::Length(1) - Single line height
 */

import React, { useState, useEffect } from 'react';

export type FlashBarState = 'idle' | 'working' | 'rate-limit' | 'error' | 'warning';

interface FlashBarProps {
  state: FlashBarState;
  message?: string;
  /** Animation frame (0-8) for working state - controlled externally for sync */
  animationFrame?: number;
}

/**
 * Dot sizes for the progressive gradient effect
 * Center is largest, edges are smallest
 */
const DOT_SIZES = [
  { size: 4, opacity: 0.3 },   // Outer
  { size: 5, opacity: 0.4 },
  { size: 6, opacity: 0.5 },
  { size: 8, opacity: 0.7 },
  { size: 12, opacity: 1.0 },  // Center (always visible)
  { size: 8, opacity: 0.7 },
  { size: 6, opacity: 0.5 },
  { size: 5, opacity: 0.4 },
  { size: 4, opacity: 0.3 },   // Outer
];

const CENTER_INDEX = 4;

export function FlashBar({ state, message, animationFrame = 0 }: FlashBarProps) {
  // Internal animation state if not controlled externally
  const [internalFrame, setInternalFrame] = useState(0);
  const [expanding, setExpanding] = useState(true);
  
  // Use internal animation when working
  useEffect(() => {
    if (state !== 'working') {
      setInternalFrame(0);
      setExpanding(true);
      return;
    }
    
    const interval = setInterval(() => {
      setInternalFrame(prev => {
        if (expanding) {
          if (prev >= CENTER_INDEX) {
            setExpanding(false);
            return prev - 1;
          }
          return prev + 1;
        } else {
          if (prev <= 0) {
            setExpanding(true);
            return prev + 1;
          }
          return prev - 1;
        }
      });
    }, 150);
    
    return () => clearInterval(interval);
  }, [state, expanding]);
  
  const frame = animationFrame || internalFrame;
  
  // Render based on state
  if (state === 'idle') {
    return (
      <div className="h-8 flex items-center justify-center bg-[var(--terminal-header-bg)]">
        <div 
          className="rounded-full"
          style={{
            width: DOT_SIZES[CENTER_INDEX].size,
            height: DOT_SIZES[CENTER_INDEX].size,
            backgroundColor: 'var(--foreground)',
            opacity: 0.3,
          }}
        />
      </div>
    );
  }
  
  if (state === 'working') {
    // Calculate which dots are visible based on animation frame
    const visibleRange = frame; // 0 = only center, 4 = all dots
    
    return (
      <div className="h-8 flex items-center justify-center gap-1 bg-[var(--terminal-header-bg)]">
        {DOT_SIZES.map((dot, index) => {
          const distanceFromCenter = Math.abs(index - CENTER_INDEX);
          const isVisible = distanceFromCenter <= visibleRange;
          const isCenter = index === CENTER_INDEX;
          
          return (
            <div
              key={index}
              className="rounded-full transition-all duration-150"
              style={{
                width: dot.size,
                height: dot.size,
                backgroundColor: 'var(--msg-system)', // Cyan from theme
                opacity: isCenter ? 1 : (isVisible ? dot.opacity : 0),
                transform: isVisible ? 'scale(1)' : 'scale(0)',
              }}
            />
          );
        })}
      </div>
    );
  }
  
  // Message states (rate-limit, error, warning)
  // Using bordered style with dot indicator as shown in design
  const stateStyles: Record<string, { border: string; dot: string; text: string; bg: string }> = {
    'rate-limit': {
      border: '#fab387', // Peach/amber border
      dot: '#fab387',    // Amber dot
      text: '#fab387',   // Amber text
      bg: 'rgba(250, 179, 135, 0.1)', // Subtle amber bg
    },
    'error': {
      border: '#f38ba8', // Red border
      dot: '#f38ba8',    // Red dot
      text: '#f38ba8',   // Red text  
      bg: 'rgba(243, 139, 168, 0.1)', // Subtle red bg
    },
    'warning': {
      border: '#fab387', // Peach border
      dot: '#fab387',    // Peach dot
      text: '#fab387',   // Peach text
      bg: 'rgba(250, 179, 135, 0.1)', // Subtle peach bg
    },
  };
  
  const style = stateStyles[state] || stateStyles['error'];
  
  return (
    <div 
      className="h-10 mx-4 my-2 flex items-center justify-center gap-3 px-4 rounded-lg text-sm font-medium"
      style={{
        backgroundColor: style.bg,
        border: `1px solid ${style.border}`,
        color: style.text,
      }}
    >
      {/* Animated dot indicator */}
      <div className="flex items-center gap-1">
        <span 
          className="inline-block w-2.5 h-2.5 rounded-full animate-pulse"
          style={{ backgroundColor: style.dot }}
        />
        <span 
          className="inline-block w-1.5 h-1.5 rounded-full opacity-60"
          style={{ backgroundColor: style.dot }}
        />
      </div>
      <span>{message || getDefaultMessage(state)}</span>
    </div>
  );
}

function getDefaultMessage(state: FlashBarState): string {
  switch (state) {
    case 'rate-limit':
      return 'RATE LIMITED: Trying again in 1s.';
    case 'error':
      return 'CRITICAL ERROR: Connection failed.';
    case 'warning':
      return 'Request timeout, retrying...';
    default:
      return '';
  }
}

/**
 * Demo component showing all Flash Bar states
 */
export function FlashBarDemo() {
  const [demoState, setDemoState] = useState<FlashBarState>('working');
  
  return (
    <div className="space-y-6 p-6 bg-[var(--background)] min-h-screen">
      <h1 className="text-xl font-bold text-[var(--foreground)]">Flash Bar States</h1>
      
      {/* State selector */}
      <div className="flex gap-2">
        {(['idle', 'working', 'rate-limit', 'error', 'warning'] as FlashBarState[]).map(s => (
          <button
            key={s}
            onClick={() => setDemoState(s)}
            className={`px-3 py-1 rounded text-sm ${
              demoState === s 
                ? 'bg-[var(--msg-system)] text-[var(--background)]' 
                : 'bg-[var(--terminal-border)] text-[var(--foreground)]'
            }`}
          >
            {s}
          </button>
        ))}
      </div>
      
      {/* Current state demo */}
      <div className="border border-[var(--terminal-border)] rounded-lg overflow-hidden">
        <div className="p-4 bg-[var(--terminal-bg)] text-[var(--foreground)] text-sm">
          Messages area...
        </div>
        <FlashBar state={demoState} />
        <div className="p-4 bg-[var(--terminal-header-bg)] text-[var(--foreground)] text-sm">
          Input area...
        </div>
      </div>
      
      {/* All states preview */}
      <div className="space-y-4">
        <h2 className="text-lg font-semibold text-[var(--foreground)]">All States</h2>
        
        {(['idle', 'working', 'rate-limit', 'error', 'warning'] as FlashBarState[]).map(s => (
          <div key={s} className="border border-[var(--terminal-border)] rounded overflow-hidden">
            <div className="px-3 py-1 bg-[var(--terminal-header-bg)] text-xs text-[var(--foreground)]/50">
              {s}
            </div>
            <FlashBar state={s} />
          </div>
        ))}
      </div>
    </div>
  );
}

export default FlashBar;
