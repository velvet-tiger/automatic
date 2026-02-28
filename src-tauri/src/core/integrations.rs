// ── Newsletter subscription (Attio) ──────────────────────────────────────────
//
// Flow:
//   1. Assert (upsert) a Person record matched on email_addresses.
//   2. Assert a list entry on the "automatic-users" list (UUID 0c68f5fc-f912-4b2b-bf69-792920c020d4).
//
// The Attio API key is stored in the system keychain under the provider name
// "attio" using the same save_api_key / get_api_key mechanism used elsewhere.

/// Subscribe an email address to the Automatic newsletter via Attio.
/// Returns `Ok(())` on success, or a human-readable error string.
pub async fn subscribe_newsletter(email: &str) -> Result<(), String> {
    let api_key = option_env!("ATTIO_API_KEY")
        .ok_or("Newsletter subscription is not configured in this build")?;

    let client = reqwest::Client::new();
    let auth = format!("Bearer {}", api_key);

    // ── Step 1: assert person ─────────────────────────────────────────────────
    let person_body = serde_json::json!({
        "data": {
            "values": {
                "email_addresses": [{ "email_address": email }]
            }
        }
    });

    let person_resp = client
        .put("https://api.attio.com/v2/objects/people/records")
        .header("Authorization", &auth)
        .header("Content-Type", "application/json")
        .query(&[("matching_attribute", "email_addresses")])
        .json(&person_body)
        .send()
        .await
        .map_err(|e| format!("Attio request failed: {e}"))?;

    let person_status = person_resp.status();

    if !person_status.is_success() {
        let body = person_resp.text().await.unwrap_or_default();
        return Err(format!("Attio person upsert failed ({person_status}): {body}"));
    }

    let person_json: serde_json::Value = person_resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse Attio person response: {e}"))?;

    let record_id = person_json
        .pointer("/data/id/record_id")
        .and_then(|v| v.as_str())
        .ok_or("Attio response missing record_id")?
        .to_string();

    // ── Step 2: assert list entry ─────────────────────────────────────────────
    let entry_body = serde_json::json!({
        "data": {
            "parent_record_id": record_id,
            "parent_object": "people",
            "entry_values": {}
        }
    });
    // Use the list UUID directly — avoids the list_configuration:read scope
    // required to resolve a slug.

    let entry_resp = client
        .put("https://api.attio.com/v2/lists/0c68f5fc-f912-4b2b-bf69-792920c020d4/entries")
        .header("Authorization", &auth)
        .header("Content-Type", "application/json")
        .json(&entry_body)
        .send()
        .await
        .map_err(|e| format!("Attio list entry request failed: {e}"))?;

    let entry_status = entry_resp.status();

    if !entry_status.is_success() {
        let body = entry_resp.text().await.unwrap_or_default();
        return Err(format!("Attio list entry upsert failed ({entry_status}): {body}"));
    }

    Ok(())
}

// ── Analytics (Amplitude HTTP API v2) ────────────────────────────────────────
//
// Events are sent directly from Rust via reqwest so that:
//   - The API key never appears in the JS bundle.
//   - We are not subject to WKWebView network quirks (sendBeacon / fetch).
//
// The key is baked in at compile time via option_env!("AMPLITUDE_API_KEY").
// Leave the env var unset in local dev to disable event sending silently.

/// Send a single event to Amplitude's HTTP API v2.
///
/// `user_id`   — stable user identifier (clerk_id from profile)
/// `event`     — event name, e.g. "skill_created"
/// `properties`— optional JSON object of event properties
/// `enabled`   — the user's analytics opt-in preference from Settings
///
/// Returns `Ok(())` whether or not the key is set (missing key is a silent
/// no-op) so callers never need to handle the disabled case.
pub async fn track_event(
    user_id: &str,
    event: &str,
    properties: Option<serde_json::Value>,
    enabled: bool,
) -> Result<(), String> {
    if !enabled {
        return Ok(());
    }

    let api_key = match option_env!("AMPLITUDE_API_KEY") {
        Some(k) if !k.is_empty() => k,
        _ => return Ok(()), // no key compiled in — silent no-op
    };

    let mut event_obj = serde_json::json!({
        "event_type": event,
        "user_id": user_id,
    });

    if let Some(props) = properties {
        event_obj["event_properties"] = props;
    }

    let body = serde_json::json!({
        "api_key": api_key,
        "events": [event_obj],
    });

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| format!("HTTP client error: {e}"))?;

    let resp = client
        .post("https://api.eu.amplitude.com/2/httpapi")
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Amplitude request failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        eprintln!("[analytics] Amplitude error {status}: {text}");
    }

    Ok(())
}
