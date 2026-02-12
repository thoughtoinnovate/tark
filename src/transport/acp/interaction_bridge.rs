use crate::tools::questionnaire::{
    AnswerValue, ApprovalChoice, ApprovalPattern, ApprovalResponse, UserResponse,
};
use crate::transport::acp::session::PendingInteraction;
use anyhow::Result;
use std::collections::HashMap;

pub fn map_approval_decision(
    decision: &str,
    selected_pattern: Option<String>,
    pending: PendingInteraction,
) -> Result<(String, bool)> {
    let PendingInteraction::Approval {
        responder, request, ..
    } = pending
    else {
        anyhow::bail!("interaction is not an approval request");
    };

    let choice = match decision {
        "approve_once" => ApprovalChoice::ApproveOnce,
        "approve_session" => ApprovalChoice::ApproveSession,
        "approve_always" => ApprovalChoice::ApproveAlways,
        "deny_once" => ApprovalChoice::Deny,
        "deny_always" => ApprovalChoice::DenyAlways,
        other => anyhow::bail!("invalid approval decision: {}", other),
    };

    let selected = selected_pattern.and_then(|pattern| {
        request
            .suggested_patterns
            .iter()
            .find(|p| p.pattern == pattern)
            .map(|p| ApprovalPattern::new(request.tool.clone(), p.pattern.clone(), p.match_type))
    });

    let response = match choice {
        ApprovalChoice::ApproveOnce => ApprovalResponse::approve_once(),
        ApprovalChoice::ApproveSession => selected
            .map(ApprovalResponse::approve_session)
            .unwrap_or_else(ApprovalResponse::approve_once),
        ApprovalChoice::ApproveAlways => selected
            .map(ApprovalResponse::approve_always)
            .unwrap_or_else(ApprovalResponse::approve_once),
        ApprovalChoice::Deny => ApprovalResponse::deny(),
        ApprovalChoice::DenyAlways => selected
            .map(ApprovalResponse::deny_always)
            .unwrap_or_else(ApprovalResponse::deny),
    };

    let is_terminal_denial = matches!(
        response.choice,
        ApprovalChoice::Deny | ApprovalChoice::DenyAlways
    );
    let _ = responder.send(response);
    Ok((decision.to_string(), is_terminal_denial))
}

pub fn map_questionnaire_response(
    cancelled: bool,
    answers: HashMap<String, serde_json::Value>,
    pending: PendingInteraction,
) -> Result<()> {
    let PendingInteraction::Questionnaire { responder, .. } = pending else {
        anyhow::bail!("interaction is not a questionnaire request");
    };

    if cancelled {
        let _ = responder.send(UserResponse::cancelled());
        return Ok(());
    }

    let mapped = answers
        .into_iter()
        .map(|(k, v)| {
            let value = match v {
                serde_json::Value::Array(arr) => {
                    let vals = arr
                        .into_iter()
                        .filter_map(|item| item.as_str().map(|s| s.to_string()))
                        .collect::<Vec<_>>();
                    AnswerValue::Multi(vals)
                }
                serde_json::Value::String(s) => AnswerValue::Single(s),
                other => AnswerValue::Single(other.to_string()),
            };
            (k, value)
        })
        .collect();

    let _ = responder.send(UserResponse::with_answers(mapped));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::questionnaire::{ApprovalRequest, SuggestedPattern};
    use crate::tools::risk::{MatchType, RiskLevel};
    use tokio::sync::oneshot;

    #[test]
    fn map_questionnaire_cancelled() {
        let (tx, rx) = oneshot::channel();
        let pending = PendingInteraction::Questionnaire {
            request_id: "req-1".to_string(),
            responder: tx,
        };

        map_questionnaire_response(true, HashMap::new(), pending).unwrap();
        let response = rx.blocking_recv().unwrap();
        assert!(response.cancelled);
    }

    #[test]
    fn map_approval_deny_once() {
        let (tx, rx) = oneshot::channel();
        let pending = PendingInteraction::Approval {
            request_id: "req-1".to_string(),
            responder: tx,
            request: ApprovalRequest {
                tool: "shell".to_string(),
                command: "rm -rf /tmp/x".to_string(),
                risk_level: RiskLevel::Dangerous,
                suggested_patterns: vec![SuggestedPattern {
                    pattern: "rm -rf /tmp/*".to_string(),
                    match_type: MatchType::Prefix,
                    description: "Temp cleanup".to_string(),
                }],
            },
        };

        let (_, denied) = map_approval_decision("deny_once", None, pending).unwrap();
        let response = rx.blocking_recv().unwrap();
        assert!(denied);
        assert!(matches!(response.choice, ApprovalChoice::Deny));
    }
}
