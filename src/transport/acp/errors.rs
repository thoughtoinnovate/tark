use serde_json::{json, Value};

pub const JSONRPC_INVALID_REQUEST: i32 = -32600;
pub const JSONRPC_METHOD_NOT_FOUND: i32 = -32601;
pub const JSONRPC_INVALID_PARAMS: i32 = -32602;
pub const ACP_SERVER_ERROR: i32 = -32000;
pub const ACP_UNSUPPORTED_VERSION: i32 = -32010;
pub const ACP_SESSION_BUSY: i32 = -32020;
pub const ACP_PROVIDER_MODEL_OVERRIDE: i32 = -32030;
pub const ACP_SESSION_NOT_FOUND: i32 = -32040;
pub const ACP_PAYLOAD_TOO_LARGE: i32 = -32060;
pub const ACP_RATE_LIMITED: i32 = -32070;

pub fn error_data(code: &str, message: impl Into<String>) -> Value {
    json!({
        "code": code,
        "message": message.into(),
    })
}
