use crate::agent::ChatAgent;
use crate::tools::questionnaire::{
    ApprovalRequest, ApprovalResponse, InteractionSender, UserResponse,
};
use crate::transport::acp::protocol::{BufferSummary, CursorPos, SelectionContext};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, AtomicU64},
    Arc,
};
use std::time::Instant;
use tokio::sync::{oneshot, Mutex};

#[derive(Debug, Clone, Default)]
pub struct SessionContext {
    pub active_file: Option<String>,
    pub cursor: Option<CursorPos>,
    pub selection: Option<SelectionContext>,
    pub active_excerpt: Option<String>,
    pub buffers: Vec<BufferSummary>,
}

pub struct AcpSession {
    pub id: String,
    pub cwd: PathBuf,
    pub provider: String,
    pub model: Option<String>,
    pub agent: Arc<Mutex<ChatAgent>>,
    pub context: Arc<Mutex<SessionContext>>,
    pub current_request: Arc<Mutex<Option<String>>>,
    pub interrupt: Arc<AtomicBool>,
    pub interaction_tx: InteractionSender,
    pub pending_interactions: Arc<Mutex<HashMap<String, PendingInteraction>>>,
    pub next_interaction_id: AtomicU64,
    pub request_times: Arc<Mutex<VecDeque<Instant>>>,
}

pub enum PendingInteraction {
    Approval {
        request_id: String,
        responder: oneshot::Sender<ApprovalResponse>,
        request: ApprovalRequest,
    },
    Questionnaire {
        request_id: String,
        responder: oneshot::Sender<UserResponse>,
    },
}

pub struct ActiveRequestGuard {
    current_request: Arc<Mutex<Option<String>>>,
    active: bool,
}

impl ActiveRequestGuard {
    pub fn new(current_request: Arc<Mutex<Option<String>>>) -> Self {
        Self {
            current_request,
            active: true,
        }
    }

    pub async fn clear_now(&mut self) {
        if self.active {
            *self.current_request.lock().await = None;
            self.active = false;
        }
    }
}

impl Drop for ActiveRequestGuard {
    fn drop(&mut self) {
        if self.active {
            let current_request = Arc::clone(&self.current_request);
            tokio::spawn(async move {
                *current_request.lock().await = None;
            });
        }
    }
}
