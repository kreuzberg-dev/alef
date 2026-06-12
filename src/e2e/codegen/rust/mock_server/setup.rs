//! Mock-server setup rendering for Rust e2e test functions.

use std::fmt::Write as FmtWrite;

use crate::e2e::config::E2eConfig;
use crate::e2e::escape::rust_raw_string;
use crate::e2e::fixture::Fixture;

/// Emit mock server setup lines into a test function body.
///
/// Builds `MockRoute` objects from the fixture's `mock_response` schema or the
/// `input.mock_responses` route-array schema.
/// The resulting `mock_server` variable is in scope for the rest of the test function.
///
/// `var_name` controls the local binding name (e.g. `"mock_server"` when the rest of
/// the test body references `mock_server.url`, `"_mock_server"` when the server only
/// needs to be kept alive via Drop — typical for error-path fixtures that intentionally
/// never read the URL). The underscore prefix silences `-D unused_variables` without
/// dropping the server early.
pub fn render_mock_server_setup(out: &mut String, fixture: &Fixture, e2e_config: &E2eConfig, var_name: &str) {
    // Prefer the route-array schema when present.
    let mut routes = Vec::new();

    if let Some(mock_responses) = fixture.input.get("mock_responses").and_then(|v| v.as_array()) {
        let call_config = e2e_config.resolve_call(fixture.call.as_deref());
        let default_path = call_config.path.as_deref().unwrap_or("/");
        let default_method = call_config.method.as_deref().unwrap_or("POST");

        for response in mock_responses {
            if let Ok(obj) = serde_json::from_value::<serde_json::Map<String, serde_json::Value>>(response.clone()) {
                let path = obj
                    .get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or(default_path)
                    .to_string();
                let method = obj
                    .get("method")
                    .and_then(|v| v.as_str())
                    .unwrap_or(default_method)
                    .to_string();
                let status: u16 = obj.get("status_code").and_then(|v| v.as_u64()).unwrap_or(200) as u16;

                let headers: Vec<(String, String)> = obj
                    .get("headers")
                    .and_then(|v| v.as_object())
                    .map(|h| {
                        let mut entries: Vec<_> = h
                            .iter()
                            .map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string()))
                            .collect();
                        entries.sort_by(|a, b| a.0.cmp(&b.0));
                        entries
                    })
                    .unwrap_or_default();

                let body_str = if let Some(body_inline) = obj.get("body_inline").and_then(|v| v.as_str()) {
                    rust_raw_string(body_inline)
                } else {
                    // Note: body_file support would require fixture-dir context at codegen time.
                    // For now, we emit a placeholder; the standalone binary handles body_file.
                    rust_raw_string("{}")
                };

                let delay_ms = obj.get("delay_ms").and_then(|v| v.as_u64());

                routes.push((path, method, status, body_str, headers, delay_ms));
            }
        }
    } else if let Some(mock) = fixture.mock_response.as_ref() {
        let call_config = e2e_config.resolve_call(fixture.call.as_deref());
        let path = call_config.path.as_deref().unwrap_or("/");
        let method = call_config.method.as_deref().unwrap_or("POST");

        let status = mock.status;

        // Render headers map as a Vec<(String, String)> literal for stable iteration order.
        let mut header_entries: Vec<(&String, &String)> = mock.headers.iter().collect();
        header_entries.sort_by(|a, b| a.0.cmp(b.0));
        let header_tuples: Vec<(String, String)> = header_entries
            .into_iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        let body_str = match &mock.body {
            Some(b) => {
                let s = serde_json::to_string(b).unwrap_or_default();
                rust_raw_string(&s)
            }
            None => rust_raw_string("{}"),
        };

        // Handle streaming separately within the single-response case.
        if let Some(chunks) = &mock.stream_chunks {
            // Streaming SSE response.
            let _ = writeln!(out, "    let mock_route = MockRoute {{");
            let _ = writeln!(out, "        path: \"{path}\",");
            let _ = writeln!(out, "        method: \"{method}\",");
            let _ = writeln!(out, "        status: {status},");
            let _ = writeln!(out, "        body: String::new(),");
            let _ = writeln!(out, "        is_streaming: true,");
            let _ = writeln!(out, "        stream_chunks: vec![");
            for chunk in chunks {
                let chunk_str = match chunk {
                    serde_json::Value::String(s) => rust_raw_string(s),
                    other => {
                        let s = serde_json::to_string(other).unwrap_or_default();
                        rust_raw_string(&s)
                    }
                };
                let _ = writeln!(out, "            {chunk_str}.to_string(),");
            }
            let _ = writeln!(out, "        ],");
            let _ = writeln!(out, "        headers: vec![");
            for (name, value) in &header_tuples {
                let n = rust_raw_string(name);
                let v = rust_raw_string(value);
                let _ = writeln!(out, "            ({n}.to_string(), {v}.to_string()),");
            }
            let _ = writeln!(out, "        ],");
            let _ = writeln!(out, "        delay_ms: None,");
            let _ = writeln!(out, "    }};");
            let _ = writeln!(out, "    let {var_name} = MockServer::start(vec![mock_route]).await;");
            return;
        }

        routes.push((
            path.to_string(),
            method.to_string(),
            status,
            body_str,
            header_tuples,
            None,
        ));
    } else {
        return;
    }

    // Emit all routes (array schema produces multiple; single schema produces one).
    if routes.len() == 1 {
        let (path, method, status, body_str, header_entries, delay_ms) = routes.pop().unwrap();
        let delay_literal = match delay_ms {
            Some(ms) => format!("Some({ms})"),
            None => "None".to_string(),
        };
        let _ = writeln!(out, "    let mock_route = MockRoute {{");
        let _ = writeln!(out, "        path: \"{path}\",");
        let _ = writeln!(out, "        method: \"{method}\",");
        let _ = writeln!(out, "        status: {status},");
        let _ = writeln!(out, "        body: {body_str}.to_string(),");
        let _ = writeln!(out, "        is_streaming: false,");
        let _ = writeln!(out, "        stream_chunks: vec![],");
        let _ = writeln!(out, "        headers: vec![");
        for (name, value) in &header_entries {
            let n = rust_raw_string(name);
            let v = rust_raw_string(value);
            let _ = writeln!(out, "            ({n}.to_string(), {v}.to_string()),");
        }
        let _ = writeln!(out, "        ],");
        let _ = writeln!(out, "        delay_ms: {delay_literal},");
        let _ = writeln!(out, "    }};");
        let _ = writeln!(out, "    let {var_name} = MockServer::start(vec![mock_route]).await;");
    } else {
        let _ = writeln!(out, "    let mut mock_routes = vec![];");
        for (path, method, status, body_str, header_entries, delay_ms) in routes {
            let delay_literal = match delay_ms {
                Some(ms) => format!("Some({ms})"),
                None => "None".to_string(),
            };
            let _ = writeln!(out, "    mock_routes.push(MockRoute {{");
            let _ = writeln!(out, "        path: \"{path}\",");
            let _ = writeln!(out, "        method: \"{method}\",");
            let _ = writeln!(out, "        status: {status},");
            let _ = writeln!(out, "        body: {body_str}.to_string(),");
            let _ = writeln!(out, "        is_streaming: false,");
            let _ = writeln!(out, "        stream_chunks: vec![],");
            let _ = writeln!(out, "        headers: vec![");
            for (name, value) in &header_entries {
                let n = rust_raw_string(name);
                let v = rust_raw_string(value);
                let _ = writeln!(out, "            ({n}.to_string(), {v}.to_string()),");
            }
            let _ = writeln!(out, "        ],");
            let _ = writeln!(out, "        delay_ms: {delay_literal},");
            let _ = writeln!(out, "    }});");
        }
        let _ = writeln!(out, "    let {var_name} = MockServer::start(mock_routes).await;");
    }
}
