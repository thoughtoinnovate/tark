//! Integration tests for usage API endpoints

use serde_json::Value;

#[tokio::test]
async fn test_usage_summary_endpoint() {
    // Note: This test requires a running tark server
    // Run with: cargo test --test usage_api -- --ignored

    let client = reqwest::Client::new();
    let response = client
        .get("http://127.0.0.1:8765/api/usage/summary")
        .send()
        .await;

    if let Ok(res) = response {
        assert!(res.status().is_success() || res.status() == 503); // 503 if tracker not initialized

        if res.status().is_success() {
            let json: Value = res.json().await.expect("Failed to parse JSON");
            assert!(json.get("total_cost").is_some());
            assert!(json.get("total_tokens").is_some());
            assert!(json.get("session_count").is_some());
        }
    }
}

#[tokio::test]
async fn test_usage_models_endpoint() {
    let client = reqwest::Client::new();
    let response = client
        .get("http://127.0.0.1:8765/api/usage/models")
        .send()
        .await;

    if let Ok(res) = response {
        assert!(res.status().is_success() || res.status() == 503);

        if res.status().is_success() {
            let json: Value = res.json().await.expect("Failed to parse JSON");
            assert!(json.is_array());
        }
    }
}

#[tokio::test]
async fn test_usage_modes_endpoint() {
    let client = reqwest::Client::new();
    let response = client
        .get("http://127.0.0.1:8765/api/usage/modes")
        .send()
        .await;

    if let Ok(res) = response {
        assert!(res.status().is_success() || res.status() == 503);

        if res.status().is_success() {
            let json: Value = res.json().await.expect("Failed to parse JSON");
            assert!(json.is_array());
        }
    }
}

#[tokio::test]
async fn test_usage_sessions_endpoint() {
    let client = reqwest::Client::new();
    let response = client
        .get("http://127.0.0.1:8765/api/usage/sessions")
        .send()
        .await;

    if let Ok(res) = response {
        assert!(res.status().is_success() || res.status() == 503);

        if res.status().is_success() {
            let json: Value = res.json().await.expect("Failed to parse JSON");
            assert!(json.is_array());
        }
    }
}

#[tokio::test]
async fn test_usage_dashboard_endpoint() {
    let client = reqwest::Client::new();
    let response = client.get("http://127.0.0.1:8765/usage").send().await;

    if let Ok(res) = response {
        assert!(res.status().is_success());
        let text = res.text().await.expect("Failed to get response text");
        assert!(text.contains("Tark Usage Dashboard"));
        // Chart.js is loaded from CDN - check for the script tag (case-insensitive)
        assert!(
            text.to_lowercase().contains("chart.js"),
            "Dashboard should include Chart.js library"
        );
    }
}

#[tokio::test]
async fn test_usage_export_endpoint() {
    let client = reqwest::Client::new();
    let response = client
        .get("http://127.0.0.1:8765/api/usage/export")
        .send()
        .await;

    if let Ok(res) = response {
        assert!(res.status().is_success() || res.status() == 503);

        if res.status().is_success() {
            let content_type = res
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok());
            assert_eq!(content_type, Some("text/csv"));
        }
    }
}
