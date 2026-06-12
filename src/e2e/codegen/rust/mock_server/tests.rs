use super::*;

#[test]
fn render_mock_server_module_contains_struct_definition() {
    let out = render_mock_server_module();
    assert!(out.contains("pub struct MockRoute"));
    assert!(out.contains("pub struct MockServer"));
}

#[test]
fn render_mock_server_binary_contains_main() {
    let out = render_mock_server_binary();
    assert!(out.contains("async fn main()"));
    assert!(out.contains("MOCK_SERVER_URL=http://"));
}

#[test]
fn render_mock_server_binary_spawn_before_print() {
    let out = render_mock_server_binary();
    // The shared server must be spawned (and every listener probed for
    // readiness) BEFORE the MOCK_SERVER_URL line is printed, so consumers
    // that connect immediately after reading the line do not get ECONNREFUSED.
    let spawn_pos = out
        .find("axum::serve(shared_listener, shared_app)")
        .expect("shared spawn missing");
    let probe_pos = out.find("TcpStream::connect").expect("readiness probe missing");
    let print_pos = out.find("println!(\"MOCK_SERVER_URL=http://").expect("print missing");
    assert!(
        spawn_pos < print_pos,
        "shared server spawn must appear before MOCK_SERVER_URL print"
    );
    assert!(
        probe_pos < print_pos,
        "readiness probe must appear before MOCK_SERVER_URL print"
    );
}

#[test]
fn render_mock_server_binary_uses_generic_fixture_schema_terms() {
    let out = render_mock_server_binary();
    assert!(
        out.contains("Route-array fixture schema"),
        "missing generic route-array schema docs"
    );
    assert!(
        out.contains("ORIGIN_ROOT_ROUTE_PREFIXES"),
        "missing named origin-root route prefixes"
    );
    assert!(
        !out.contains("sample-"),
        "must not mention project-specific fixture names"
    );
}

#[test]
fn render_mock_server_binary_keeps_route_loading_paths() {
    let out = render_mock_server_binary();
    assert!(out.contains("fn load_routes("), "missing route loader");
    assert!(
        out.contains("fn load_routes_recursive("),
        "missing recursive fixture directory loading"
    );
    assert!(
        out.contains("fixtures_dir.join(\"responses\").join(file)"),
        "missing responses/body_file fallback path"
    );
    assert!(
        out.contains("per_fixture"),
        "missing per-fixture origin-root route table"
    );
    assert!(
        out.contains("is_host_root_path(&resolved.original_path)"),
        "missing host-root route split"
    );
}

#[test]
fn render_common_module_has_expected_symbols() {
    let src = render_common_module();
    assert!(src.contains("pub fn mock_server_url"), "missing mock_server_url");
    assert!(src.contains("OnceLock"), "missing OnceLock");
    assert!(src.contains("MOCK_SERVER_URL"), "missing MOCK_SERVER_URL");
    assert!(src.contains("MOCK_SERVERS"), "missing MOCK_SERVERS");
    assert!(src.contains("serde_json"), "missing serde_json parsing");
}
