mod client;
mod types;

use once_cell::sync::Lazy;
use std::sync::RwLock;

pub use client::{
    AdapterBufferInfo, AdapterCursor, AdapterDiagnostic, AdapterLocation, AdapterSymbol,
    EditorAdapterClient,
};
pub use types::{EditorCapabilities, EditorContextV1, EditorEndpoint, EDITOR_ADAPTER_API_V1};

static CURRENT_EDITOR_CONTEXT: Lazy<RwLock<Option<EditorContextV1>>> =
    Lazy::new(|| RwLock::new(None));

pub struct ScopedEditorContext {
    previous: Option<EditorContextV1>,
}

impl Drop for ScopedEditorContext {
    fn drop(&mut self) {
        if let Ok(mut slot) = CURRENT_EDITOR_CONTEXT.write() {
            *slot = self.previous.take();
        }
    }
}

pub fn scoped_editor_context(context: Option<EditorContextV1>) -> ScopedEditorContext {
    let previous = if let Ok(mut slot) = CURRENT_EDITOR_CONTEXT.write() {
        let prev = slot.clone();
        *slot = context;
        prev
    } else {
        None
    };

    ScopedEditorContext { previous }
}

pub fn current_editor_context() -> Option<EditorContextV1> {
    CURRENT_EDITOR_CONTEXT
        .read()
        .ok()
        .and_then(|slot| slot.as_ref().cloned())
}
