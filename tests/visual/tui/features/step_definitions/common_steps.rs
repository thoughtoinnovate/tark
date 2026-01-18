// Common Step Definitions for TUI BDD Tests
// This file provides step definition templates for cucumber-rs

use cucumber::{given, when, then};

// =============================================================================
// WORLD STATE
// =============================================================================

/// The test world state that holds the application and terminal
#[derive(Debug, Default, cucumber::World)]
pub struct TuiWorld {
    /// The TUI application instance
    pub app: Option<App>,
    /// Test terminal for rendering
    pub terminal: Option<TestTerminal>,
    /// Current theme
    pub theme: String,
    /// Last captured output
    pub last_output: String,
    /// Modal state tracking
    pub active_modal: Option<String>,
}

// =============================================================================
// GIVEN STEPS - Setup preconditions
// =============================================================================

#[given("the TUI application is running")]
async fn app_running(world: &mut TuiWorld) {
    world.app = Some(App::new());
    world.terminal = Some(TestTerminal::new(80, 24));
    world.theme = "catppuccin-mocha".to_string();
}

#[given(regex = r"the terminal has at least (\d+) columns and (\d+) rows")]
async fn terminal_size(world: &mut TuiWorld, cols: u16, rows: u16) {
    if let Some(terminal) = &mut world.terminal {
        terminal.resize(cols, rows);
    }
}

#[given(regex = r"the theme is set to (.+)")]
async fn theme_set(world: &mut TuiWorld, theme: String) {
    world.theme = theme.trim_matches('"').to_string();
    if let Some(app) = &mut world.app {
        app.set_theme(&world.theme);
    }
}

#[given("the sidebar is expanded")]
async fn sidebar_expanded(world: &mut TuiWorld) {
    if let Some(app) = &mut world.app {
        app.sidebar_visible = true;
    }
}

#[given("the sidebar is collapsed")]
async fn sidebar_collapsed(world: &mut TuiWorld) {
    if let Some(app) = &mut world.app {
        app.sidebar_visible = false;
    }
}

#[given(regex = r"the current agent mode is (.+)")]
async fn agent_mode_set(world: &mut TuiWorld, mode: String) {
    if let Some(app) = &mut world.app {
        app.agent_mode = AgentMode::from_str(&mode.trim_matches('"')).unwrap();
    }
}

#[given(regex = r"I have typed (.+)")]
async fn have_typed(world: &mut TuiWorld, text: String) {
    if let Some(app) = &mut world.app {
        app.input = text.trim_matches('"').to_string();
    }
}

#[given("the input area has focus")]
async fn input_focused(world: &mut TuiWorld) {
    if let Some(app) = &mut world.app {
        app.focus = Focus::Input;
    }
}

#[given("thinking mode is enabled")]
async fn thinking_enabled(world: &mut TuiWorld) {
    if let Some(app) = &mut world.app {
        app.thinking_enabled = true;
    }
}

#[given("thinking mode is disabled")]
async fn thinking_disabled(world: &mut TuiWorld) {
    if let Some(app) = &mut world.app {
        app.thinking_enabled = false;
    }
}

#[given(regex = r"file (.+) is in context")]
async fn file_in_context(world: &mut TuiWorld, filename: String) {
    if let Some(app) = &mut world.app {
        app.context_files.push(filename.trim_matches('"').to_string());
    }
}

#[given("the provider picker modal is open")]
async fn provider_picker_open(world: &mut TuiWorld) {
    world.active_modal = Some("provider_picker".to_string());
    if let Some(app) = &mut world.app {
        app.active_modal = Some(Modal::ProviderPicker);
    }
}

#[given("the model picker modal is open")]
async fn model_picker_open(world: &mut TuiWorld) {
    world.active_modal = Some("model_picker".to_string());
    if let Some(app) = &mut world.app {
        app.active_modal = Some(Modal::ModelPicker);
    }
}

#[given("the file picker modal is open")]
async fn file_picker_open(world: &mut TuiWorld) {
    world.active_modal = Some("file_picker".to_string());
    if let Some(app) = &mut world.app {
        app.active_modal = Some(Modal::FilePicker);
    }
}

#[given("the agent asks a multiple choice question")]
async fn multiple_choice_question(world: &mut TuiWorld) {
    if let Some(app) = &mut world.app {
        app.add_message(Message::Question(QuestionData {
            question_type: QuestionType::MultipleChoice,
            text: "Which features do you want?".to_string(),
            options: vec!["Option A".to_string(), "Option B".to_string()],
            selected: vec![],
            answered: false,
        }));
    }
}

#[given("the agent asks a single choice question")]
async fn single_choice_question(world: &mut TuiWorld) {
    if let Some(app) = &mut world.app {
        app.add_message(Message::Question(QuestionData {
            question_type: QuestionType::SingleChoice,
            text: "Which option do you prefer?".to_string(),
            options: vec!["Option A".to_string(), "Option B".to_string()],
            selected: vec![],
            answered: false,
        }));
    }
}

// =============================================================================
// WHEN STEPS - Actions
// =============================================================================

#[when(regex = r#"I type "(.+)""#)]
async fn type_text(world: &mut TuiWorld, text: String) {
    if let Some(app) = &mut world.app {
        for c in text.chars() {
            app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::empty()));
        }
    }
}

#[when(regex = r#"I press "(.+)""#)]
async fn press_key(world: &mut TuiWorld, key: String) {
    if let Some(app) = &mut world.app {
        let key_event = parse_key_event(&key);
        app.handle_key(key_event);
    }
}

#[when("I click on the model selector in the status bar")]
async fn click_model_selector(world: &mut TuiWorld) {
    if let Some(app) = &mut world.app {
        app.open_provider_picker();
    }
}

#[when(regex = r#"I select provider "(.+)""#)]
async fn select_provider(world: &mut TuiWorld, provider: String) {
    if let Some(app) = &mut world.app {
        app.select_provider(&provider);
    }
}

#[when(regex = r#"I select model "(.+)""#)]
async fn select_model(world: &mut TuiWorld, model: String) {
    if let Some(app) = &mut world.app {
        app.select_model(&model);
    }
}

#[when(regex = r#"I select file "(.+)""#)]
async fn select_file(world: &mut TuiWorld, filename: String) {
    if let Some(app) = &mut world.app {
        app.select_file(&filename);
    }
}

#[when("I click on the thinking toggle")]
async fn click_thinking_toggle(world: &mut TuiWorld) {
    if let Some(app) = &mut world.app {
        app.toggle_thinking();
    }
}

#[when("I press the sidebar toggle key")]
async fn toggle_sidebar(world: &mut TuiWorld) {
    if let Some(app) = &mut world.app {
        app.toggle_sidebar();
    }
}

// =============================================================================
// THEN STEPS - Assertions
// =============================================================================

#[then("I should see the terminal header at the top")]
async fn see_terminal_header(world: &mut TuiWorld) {
    if let Some(app) = &world.app {
        assert!(app.has_header());
    }
}

#[then("I should see the message area in the center")]
async fn see_message_area(world: &mut TuiWorld) {
    if let Some(app) = &world.app {
        assert!(app.has_message_area());
    }
}

#[then("I should see the input area at the bottom")]
async fn see_input_area(world: &mut TuiWorld) {
    if let Some(app) = &world.app {
        assert!(app.has_input_area());
    }
}

#[then("I should see the status bar below the input area")]
async fn see_status_bar(world: &mut TuiWorld) {
    if let Some(app) = &world.app {
        assert!(app.has_status_bar());
    }
}

#[then(regex = r#"the input area should display "(.+)""#)]
async fn input_displays(world: &mut TuiWorld, expected: String) {
    if let Some(app) = &world.app {
        assert_eq!(app.input, expected);
    }
}

#[then("the message should be submitted")]
async fn message_submitted(world: &mut TuiWorld) {
    if let Some(app) = &world.app {
        assert!(app.last_submitted.is_some());
    }
}

#[then("the input area should be cleared")]
async fn input_cleared(world: &mut TuiWorld) {
    if let Some(app) = &world.app {
        assert!(app.input.is_empty());
    }
}

#[then("the provider picker modal should open")]
async fn provider_picker_opens(world: &mut TuiWorld) {
    if let Some(app) = &world.app {
        assert!(matches!(app.active_modal, Some(Modal::ProviderPicker)));
    }
}

#[then("the model picker modal should open")]
async fn model_picker_opens(world: &mut TuiWorld) {
    if let Some(app) = &world.app {
        assert!(matches!(app.active_modal, Some(Modal::ModelPicker)));
    }
}

#[then("the file picker modal should open")]
async fn file_picker_opens(world: &mut TuiWorld) {
    if let Some(app) = &world.app {
        assert!(matches!(app.active_modal, Some(Modal::FilePicker)));
    }
}

#[then("the theme picker modal should open")]
async fn theme_picker_opens(world: &mut TuiWorld) {
    if let Some(app) = &world.app {
        assert!(matches!(app.active_modal, Some(Modal::ThemePicker)));
    }
}

#[then("the help modal should open")]
async fn help_modal_opens(world: &mut TuiWorld) {
    if let Some(app) = &world.app {
        assert!(matches!(app.active_modal, Some(Modal::Help)));
    }
}

#[then("the modal should close")]
async fn modal_closes(world: &mut TuiWorld) {
    if let Some(app) = &world.app {
        assert!(app.active_modal.is_none());
    }
}

#[then("the sidebar should collapse")]
async fn sidebar_collapses(world: &mut TuiWorld) {
    if let Some(app) = &world.app {
        assert!(!app.sidebar_visible);
    }
}

#[then("the sidebar should expand")]
async fn sidebar_expands(world: &mut TuiWorld) {
    if let Some(app) = &world.app {
        assert!(app.sidebar_visible);
    }
}

#[then("thinking mode should be enabled")]
async fn thinking_is_enabled(world: &mut TuiWorld) {
    if let Some(app) = &world.app {
        assert!(app.thinking_enabled);
    }
}

#[then("thinking mode should be disabled")]
async fn thinking_is_disabled(world: &mut TuiWorld) {
    if let Some(app) = &world.app {
        assert!(!app.thinking_enabled);
    }
}

#[then(regex = r#"the status bar should show "(.+)""#)]
async fn status_bar_shows(world: &mut TuiWorld, expected: String) {
    if let Some(app) = &world.app {
        let status_bar_text = app.render_status_bar();
        assert!(status_bar_text.contains(&expected));
    }
}

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

fn parse_key_event(key: &str) -> KeyEvent {
    match key {
        "Enter" => KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()),
        "Escape" => KeyEvent::new(KeyCode::Esc, KeyModifiers::empty()),
        "Space" => KeyEvent::new(KeyCode::Char(' '), KeyModifiers::empty()),
        "Tab" => KeyEvent::new(KeyCode::Tab, KeyModifiers::empty()),
        "Backspace" => KeyEvent::new(KeyCode::Backspace, KeyModifiers::empty()),
        "Up Arrow" | "Up" => KeyEvent::new(KeyCode::Up, KeyModifiers::empty()),
        "Down Arrow" | "Down" => KeyEvent::new(KeyCode::Down, KeyModifiers::empty()),
        "Left Arrow" | "Left" => KeyEvent::new(KeyCode::Left, KeyModifiers::empty()),
        "Right Arrow" | "Right" => KeyEvent::new(KeyCode::Right, KeyModifiers::empty()),
        "Page Up" => KeyEvent::new(KeyCode::PageUp, KeyModifiers::empty()),
        "Page Down" => KeyEvent::new(KeyCode::PageDown, KeyModifiers::empty()),
        "Home" => KeyEvent::new(KeyCode::Home, KeyModifiers::empty()),
        "End" => KeyEvent::new(KeyCode::End, KeyModifiers::empty()),
        s if s.starts_with("Ctrl+") => {
            let c = s.chars().last().unwrap().to_ascii_lowercase();
            KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
        }
        s if s.len() == 1 => {
            KeyEvent::new(KeyCode::Char(s.chars().next().unwrap()), KeyModifiers::empty())
        }
        _ => KeyEvent::new(KeyCode::Null, KeyModifiers::empty()),
    }
}

// =============================================================================
// TEST TERMINAL
// =============================================================================

/// Mock terminal for testing rendering
pub struct TestTerminal {
    width: u16,
    height: u16,
    buffer: Vec<Vec<char>>,
}

impl TestTerminal {
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            width,
            height,
            buffer: vec![vec![' '; width as usize]; height as usize],
        }
    }

    pub fn resize(&mut self, width: u16, height: u16) {
        self.width = width;
        self.height = height;
        self.buffer = vec![vec![' '; width as usize]; height as usize];
    }

    pub fn get_content(&self) -> String {
        self.buffer
            .iter()
            .map(|row| row.iter().collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }
}
