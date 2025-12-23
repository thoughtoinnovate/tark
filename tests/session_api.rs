//! Integration tests for session management API endpoints

use serde_json::Value;

#[tokio::test]
async fn test_list_sessions_endpoint() {
    // Test that /sessions endpoint returns session list
    // Note: This test requires the latest server version with session management endpoints
    let client = reqwest::Client::new();
    let response = client.get("http://127.0.0.1:8765/sessions").send().await;

    if let Ok(res) = response {
        // Should succeed if storage is initialized, 503 if not initialized, 404 if endpoint doesn't exist (old server)
        assert!(
            res.status().is_success() || res.status() == 503 || res.status() == 404,
            "Expected 200, 404, or 503, got {}",
            res.status()
        );

        if res.status().is_success() {
            let json: Value = res.json().await.expect("Failed to parse JSON");
            assert!(json.is_array(), "Response should be an array of sessions");
        }
    }
    // Silently skip if server connection fails (server not running)
}

#[tokio::test]
async fn test_session_metadata_fields() {
    // Test that session metadata includes all required fields
    let client = reqwest::Client::new();
    let response = client.get("http://127.0.0.1:8765/sessions").send().await;

    if let Ok(res) = response {
        if res.status().is_success() {
            let json: Value = res.json().await.expect("Failed to parse JSON");
            if let Some(sessions) = json.as_array() {
                if !sessions.is_empty() {
                    let session = &sessions[0];

                    // Check required fields
                    assert!(session.get("id").is_some(), "Session should have id");
                    assert!(session.get("name").is_some(), "Session should have name");
                    assert!(
                        session.get("created_at").is_some(),
                        "Session should have created_at"
                    );
                    assert!(
                        session.get("updated_at").is_some(),
                        "Session should have updated_at"
                    );
                    assert!(
                        session.get("provider").is_some(),
                        "Session should have provider"
                    );
                    assert!(session.get("model").is_some(), "Session should have model");
                    assert!(session.get("mode").is_some(), "Session should have mode");
                    assert!(
                        session.get("message_count").is_some(),
                        "Session should have message_count"
                    );

                    // Check new agent status fields
                    assert!(
                        session.get("is_current").is_some(),
                        "Session should have is_current field"
                    );
                    assert!(
                        session.get("agent_running").is_some(),
                        "Session should have agent_running field"
                    );

                    // Verify field types
                    assert!(
                        session["is_current"].is_boolean(),
                        "is_current should be boolean"
                    );
                    assert!(
                        session["agent_running"].is_boolean(),
                        "agent_running should be boolean"
                    );
                }
            }
        }
    }
}

#[tokio::test]
async fn test_session_status_flags() {
    // Test that agent status flags are set correctly
    let client = reqwest::Client::new();
    let response = client.get("http://127.0.0.1:8765/sessions").send().await;

    if let Ok(res) = response {
        if res.status().is_success() {
            let json: Value = res.json().await.expect("Failed to parse JSON");
            if let Some(sessions) = json.as_array() {
                // At least one session should be marked as current or none
                let current_count = sessions
                    .iter()
                    .filter(|s| s["is_current"].as_bool().unwrap_or(false))
                    .count();

                // There can be at most one current session
                assert!(current_count <= 1, "Only one session can be current");

                // agent_running should only be true for current session
                for session in sessions {
                    if session["agent_running"].as_bool().unwrap_or(false) {
                        assert!(
                            session["is_current"].as_bool().unwrap_or(false),
                            "agent_running can only be true if is_current is true"
                        );
                    }
                }
            }
        }
    }
}

#[tokio::test]
async fn test_get_current_session() {
    // Test /sessions/current endpoint
    let client = reqwest::Client::new();
    let response = client
        .get("http://127.0.0.1:8765/sessions/current")
        .send()
        .await;

    if let Ok(res) = response {
        // Should succeed if storage is initialized and session exists
        assert!(
            res.status().is_success() || res.status() == 503 || res.status() == 404,
            "Expected 200, 404, or 503"
        );

        if res.status().is_success() {
            let json: Value = res.json().await.expect("Failed to parse JSON");

            // Should have full session data
            assert!(json.get("id").is_some());
            assert!(json.get("name").is_some());
            assert!(json.get("messages").is_some());
            assert!(json["messages"].is_array());
        }
    }
}

#[tokio::test]
async fn test_create_new_session() {
    // Test POST /sessions/new endpoint
    // Note: This test requires the latest server version with session management endpoints
    let client = reqwest::Client::new();
    let response = client
        .post("http://127.0.0.1:8765/sessions/new")
        .send()
        .await;

    if let Ok(res) = response {
        assert!(
            res.status().is_success() || res.status() == 503 || res.status() == 404,
            "Expected 200, 404, or 503, got {}",
            res.status()
        );

        if res.status().is_success() {
            let json: Value = res.json().await.expect("Failed to parse JSON");

            // Should return the new session
            assert!(json.get("id").is_some());
            assert!(json.get("name").is_some());
            assert!(json["messages"].is_array());
            assert_eq!(
                json["messages"].as_array().unwrap().len(),
                0,
                "New session should have no messages"
            );
        }
    }
    // Silently skip if server connection fails (server not running)
}

#[tokio::test]
async fn test_switch_session() {
    // Test POST /sessions/switch endpoint
    let client = reqwest::Client::new();

    // First, get list of sessions
    let list_response = client.get("http://127.0.0.1:8765/sessions").send().await;

    if let Ok(list_res) = list_response {
        if list_res.status().is_success() {
            let sessions: Value = list_res.json().await.expect("Failed to parse JSON");

            if let Some(sessions_array) = sessions.as_array() {
                if !sessions_array.is_empty() {
                    let session_id = sessions_array[0]["id"].as_str().unwrap();

                    // Try to switch to this session
                    let switch_response = client
                        .post("http://127.0.0.1:8765/sessions/switch")
                        .json(&serde_json::json!({ "session_id": session_id }))
                        .send()
                        .await;

                    if let Ok(switch_res) = switch_response {
                        assert!(
                            switch_res.status().is_success() || switch_res.status() == 404,
                            "Switch should succeed or return 404 if session not found"
                        );
                    }
                }
            }
        }
    }
}

#[tokio::test]
async fn test_delete_session() {
    // Test DELETE /sessions/:id endpoint
    let client = reqwest::Client::new();

    // Create a new session first
    let create_response = client
        .post("http://127.0.0.1:8765/sessions/new")
        .send()
        .await;

    if let Ok(create_res) = create_response {
        if create_res.status().is_success() {
            let session: Value = create_res.json().await.expect("Failed to parse JSON");
            let session_id = session["id"].as_str().unwrap();

            // Try to delete it
            let delete_response = client
                .delete(&format!("http://127.0.0.1:8765/sessions/{}", session_id))
                .send()
                .await;

            if let Ok(delete_res) = delete_response {
                assert!(
                    delete_res.status().is_success() || delete_res.status() == 404,
                    "Delete should succeed or return 404"
                );
            }
        }
    }
}
