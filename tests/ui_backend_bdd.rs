//! BDD Tests for UI Backend (BFF) Layer
//!
//! Tests the Backend-for-Frontend abstraction layer that separates
//! business logic from UI rendering.

use cucumber::{given, then, when, World};
use tokio::sync::mpsc;

use tark_cli::ui_backend::{AgentMode, AppEvent, AppService, BuildMode, Command, SharedState};

/// Test world for UI Backend BDD tests
#[derive(Debug, World)]
#[world(init = Self::new)]
#[allow(dead_code)]
pub struct UiBackendWorld {
    /// AppService instance under test
    service: Option<AppService>,

    /// Shared state
    state: Option<SharedState>,

    /// Event receiver for testing
    event_rx: Option<mpsc::UnboundedReceiver<AppEvent>>,

    /// Collected events for assertions
    events: Vec<AppEvent>,

    /// Test provider environment
    env_vars: std::collections::HashMap<String, String>,
}

impl UiBackendWorld {
    fn new() -> Self {
        Self {
            service: None,
            state: None,
            event_rx: None,
            events: Vec::new(),
            env_vars: std::collections::HashMap::new(),
        }
    }

    /// Get the AppService, creating it if needed
    fn service(&mut self) -> &mut AppService {
        if self.service.is_none() {
            let (tx, rx) = mpsc::unbounded_channel();
            self.event_rx = Some(rx);

            let service = AppService::new(std::path::PathBuf::from("."), tx)
                .expect("Failed to create AppService");

            self.state = Some(service.state().clone());
            self.service = Some(service);
        }
        self.service.as_mut().unwrap()
    }

    /// Collect events from the channel
    fn collect_events(&mut self) {
        if let Some(rx) = &mut self.event_rx {
            while let Ok(event) = rx.try_recv() {
                self.events.push(event);
            }
        }
    }

    /// Get the most recent event of a specific type
    fn get_last_event<F>(&self, predicate: F) -> Option<&AppEvent>
    where
        F: Fn(&AppEvent) -> bool,
    {
        self.events.iter().rev().find(|e| predicate(e))
    }
}

// ============================================================================
// STEP DEFINITIONS - APP SERVICE COMMANDS
// ============================================================================

#[given("the AppService is initialized")]
async fn app_service_initialized(w: &mut UiBackendWorld) {
    w.service(); // Initialize service
}

#[given("the event channel is listening")]
async fn event_channel_listening(w: &mut UiBackendWorld) {
    w.service(); // Ensure service exists with event channel
}

#[given(regex = r#"^the current agent mode is "(.*)"$"#)]
async fn set_agent_mode(w: &mut UiBackendWorld, mode: String) {
    let mode = match mode.as_str() {
        "Build" => AgentMode::Build,
        "Plan" => AgentMode::Plan,
        "Ask" => AgentMode::Ask,
        _ => panic!("Unknown agent mode: {}", mode),
    };
    w.service().state().set_agent_mode(mode);
}

#[when("I send the \"CycleAgentMode\" command")]
async fn cycle_agent_mode(w: &mut UiBackendWorld) {
    w.service()
        .handle_command(Command::CycleAgentMode)
        .await
        .unwrap();
    w.collect_events();
}

#[then(regex = r#"^the agent mode should be "(.*)"$"#)]
async fn check_agent_mode(w: &mut UiBackendWorld, expected: String) {
    let mode = w.service().state().agent_mode();
    let mode_str = mode.display_name();
    assert_eq!(mode_str, expected, "Expected agent mode to be {}", expected);
}

#[then("an \"AgentModeChanged\" event should be published")]
async fn check_agent_mode_event(w: &mut UiBackendWorld) {
    let has_event = w
        .get_last_event(|e| matches!(e, AppEvent::StatusChanged(msg) if msg.contains("Agent mode")))
        .is_some();
    assert!(has_event, "Expected AgentModeChanged event");
}

// Build mode steps
#[given(regex = r#"^the current build mode is "(.*)"$"#)]
async fn set_build_mode(w: &mut UiBackendWorld, mode: String) {
    let mode = match mode.as_str() {
        "Manual" => BuildMode::Manual,
        "Balanced" => BuildMode::Balanced,
        "Careful" => BuildMode::Careful,
        _ => panic!("Unknown build mode: {}", mode),
    };
    w.service().state().set_build_mode(mode);
}

#[when("I send the \"CycleBuildMode\" command")]
async fn cycle_build_mode(w: &mut UiBackendWorld) {
    w.service()
        .handle_command(Command::CycleBuildMode)
        .await
        .unwrap();
    w.collect_events();
}

#[then(regex = r#"^the build mode should be "(.*)"$"#)]
async fn check_build_mode(w: &mut UiBackendWorld, expected: String) {
    let mode = w.service().state().build_mode();
    let mode_str = mode.display_name();
    assert_eq!(mode_str, expected, "Expected build mode to be {}", expected);
}

// ============================================================================
// MAIN TEST RUNNER
// ============================================================================

#[tokio::main]
async fn main() {
    UiBackendWorld::cucumber()
        .max_concurrent_scenarios(1)
        .run("tests/bdd/ui_backend/features")
        .await;
}
