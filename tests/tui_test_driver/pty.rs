//! PTY wrapper for E2E testing
//!
//! Spawns real process in a pseudo-terminal

use anyhow::{anyhow, Result};
use portable_pty::{CommandBuilder, PtyPair, PtySize, native_pty_system};
use std::io::{Read, Write};
use std::time::{Duration, Instant};

/// PTY driver for E2E tests
/// Spawns the real binary in a pseudo-terminal
pub struct PtyDriver {
    _pair: PtyPair,
    reader: Box<dyn Read + Send>,
    writer: Box<dyn Write + Send>,
}

impl PtyDriver {
    /// Spawn a binary in a PTY
    pub fn spawn(binary: &str, args: &[&str], width: u16, height: u16) -> Result<Self> {
        let pty_system = native_pty_system();
        
        let pair = pty_system.openpty(PtySize {
            rows: height,
            cols: width,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        
        let mut cmd = CommandBuilder::new(binary);
        for arg in args {
            cmd.arg(arg);
        }
        
        // Set environment for colors
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        cmd.env("TARK_FORCE_COLOR", "1");
        
        pair.slave.spawn_command(cmd)?;
        
        let reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;
        
        Ok(Self {
            _pair: pair,
            reader,
            writer,
        })
    }

    /// Send a key to the PTY
    pub fn send_key(&mut self, key: &str) -> Result<()> {
        let bytes = crate::tui_test_driver::keys::to_terminal_bytes(key)?;
        self.writer.write_all(&bytes)?;
        self.writer.flush()?;
        Ok(())
    }

    /// Send raw text to the PTY
    pub fn send_text(&mut self, text: &str) -> Result<()> {
        self.writer.write_all(text.as_bytes())?;
        self.writer.flush()?;
        Ok(())
    }

    /// Read available output from PTY (non-blocking)
    pub fn read_available(&mut self) -> Result<String> {
        let mut buf = vec![0u8; 4096];
        let mut output = Vec::new();
        
        // Try to read what's available
        loop {
            match self.reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => output.extend_from_slice(&buf[..n]),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => return Err(e.into()),
            }
        }
        
        Ok(String::from_utf8_lossy(&output).to_string())
    }

    /// Wait for specific text to appear in output (with timeout)
    pub fn wait_for(&mut self, text: &str, timeout_ms: u64) -> Result<()> {
        let start = Instant::now();
        let timeout = Duration::from_millis(timeout_ms);
        let mut accumulated = String::new();
        
        while start.elapsed() < timeout {
            let output = self.read_available()?;
            accumulated.push_str(&output);
            
            if accumulated.contains(text) {
                return Ok(());
            }
            
            std::thread::sleep(Duration::from_millis(50));
        }
        
        Err(anyhow!(
            "Timeout waiting for '{}'. Got: {}",
            text,
            accumulated
        ))
    }

    /// Read the current screen content
    /// This is a simplified version - in production you'd parse ANSI codes
    pub fn read_screen(&mut self) -> Result<String> {
        std::thread::sleep(Duration::from_millis(100));
        self.read_available()
    }
}

impl Drop for PtyDriver {
    fn drop(&mut self) {
        // Send Ctrl+C to gracefully terminate
        let _ = self.writer.write_all(&[0x03]);
        let _ = self.writer.flush();
        std::thread::sleep(Duration::from_millis(100));
    }
}
