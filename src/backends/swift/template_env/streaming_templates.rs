pub(super) static TEMPLATES: &[(&str, &str)] = &[
    (
        "swift_streaming_client_method.swift.jinja",
        include_str!("../templates/swift_streaming_client_method.swift.jinja"),
    ),
    (
        "swift_streaming_free_function.swift.jinja",
        include_str!("../templates/swift_streaming_free_function.swift.jinja"),
    ),
    (
        "swift_streaming_chunk_decode.swift.jinja",
        include_str!("../templates/swift_streaming_chunk_decode.swift.jinja"),
    ),
];
