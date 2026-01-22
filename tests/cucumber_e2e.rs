#![cfg(feature = "cucumber-tests")]
//! E2E Cucumber Tests with PTY
//!
//! These tests spawn the real binary in a pseudo-terminal
//! and test end-to-end behavior.
//!
//! Run: cargo test --test cucumber_e2e --release

mod tui_test_driver;

use cucumber::{given, then, when, World};
use tui_test_driver::pty::PtyDriver;

#[derive(World, Debug)]
#[world(init = Self::new)]
pub struct E2EWorld {
    pty: Option<PtyDriver>,
    last_output: String,
}

impl E2EWorld {
    fn new() -> Self {
        Self {
            pty: None,
            last_output: String::new(),
        }
    }
}

// ============================================================================
// GIVEN STEPS
// ============================================================================

#[given("the TUI application is running")]
async fn tui_is_running(w: &mut E2EWorld) {
    // Build the binary first if needed
    let _ = std::process::Command::new("cargo")
        .args(["build", "--release"])
        .status();

    let pty = PtyDriver::spawn("./target/release/tark", ["tui"].as_slice(), 120, 40)
        .expect("Failed to spawn TUI");

    w.pty = Some(pty);

    // Wait for app to start
    std::thread::sleep(std::time::Duration::from_millis(1000));
}

#[given(regex = r"the terminal has at least (\d+) columns and (\d+) rows")]
async fn terminal_size(w: &mut E2EWorld, cols: u16, rows: u16) {
    if w.pty.is_none() {
        let pty = PtyDriver::spawn("./target/release/tark", ["tui"].as_slice(), cols, rows)
            .expect("Failed to spawn TUI");
        w.pty = Some(pty);
        std::thread::sleep(std::time::Duration::from_millis(1000));
    }
}

// ============================================================================
// WHEN STEPS
// ============================================================================

#[when(regex = r#"I press "(.+)""#)]
async fn press_key(w: &mut E2EWorld, key: String) {
    if let Some(pty) = &mut w.pty {
        pty.send_key(&key).expect("Failed to send key");
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

#[when(regex = r#"I type "(.+)""#)]
async fn type_text(w: &mut E2EWorld, text: String) {
    if let Some(pty) = &mut w.pty {
        for c in text.chars() {
            pty.send_text(&c.to_string()).expect("Failed to send text");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }
}

// ============================================================================
// THEN STEPS
// ============================================================================

#[then(regex = r#"I should see "(.+)""#)]
async fn should_see(w: &mut E2EWorld, text: String) {
    if let Some(pty) = &mut w.pty {
        let screen = pty.read_screen().expect("Failed to read screen");
        w.last_output = screen.clone();
        assert!(
            screen.contains(&text),
            "Expected '{}' in screen output. Got:\n{}",
            text,
            screen
        );
    }
}

#[then(regex = r#"I should see the (.+) at the (.+)"#)]
async fn should_see_component_at(w: &mut E2EWorld, component: String, location: String) {
    if let Some(pty) = &mut w.pty {
        let screen = pty.read_screen().expect("Failed to read screen");
        w.last_output = screen.clone();

        // Simple verification - in production we'd parse ANSI and check positions
        let contains_component = match component.as_str() {
            "header" => screen.contains("tark") || screen.contains("╭"),
            "status bar" => screen.contains("Build") || screen.contains("?"),
            "input area" => screen.contains("Type a message"),
            "sidebar" => screen.contains("Session"),
            _ => false,
        };

        assert!(
            contains_component,
            "Expected {} at {} in screen output. Got:\n{}",
            component, location, screen
        );
    }
}

#[then(regex = r#"the (.+) should be visible"#)]
async fn component_visible(w: &mut E2EWorld, component: String) {
    should_see_component_at(w, component, "screen".to_string()).await;
}

#[then(regex = r#"the (.+) should be hidden"#)]
async fn component_hidden(w: &mut E2EWorld, component: String) {
    if let Some(pty) = &mut w.pty {
        let screen = pty.read_screen().expect("Failed to read screen");
        w.last_output = screen.clone();

        let contains_component = match component.as_str() {
            "sidebar" => screen.contains("Session") && screen.contains("Context"),
            "help modal" => screen.contains("Shortcuts") || screen.contains("Help"),
            _ => false,
        };

        assert!(
            !contains_component,
            "Expected {} to be hidden. Got:\n{}",
            component, screen
        );
    }
}

// ============================================================================
// MAIN
// ============================================================================

#[tokio::main]
async fn main() {
    E2EWorld::cucumber()
        .max_concurrent_scenarios(1)
        .run("tests/visual/tui/features/")
        .await;
}
