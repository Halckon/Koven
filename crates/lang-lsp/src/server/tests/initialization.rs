use super::*;

#[test]
fn source_set_initialization_accepts_valid_options_and_enters_session() {
    let (server, client) = Connection::memory();
    let server_thread = thread::spawn(|| run(server));
    let mut params = serde_json::to_value(InitializeParams::default()).expect("params");
    params["initializationOptions"] = serde_json::json!({
        "unrelated": true,
        "koven": {
            "sibling": "ignored",
            "sourceSet": {
                "schema": "koven.lsp.source-set",
                "version": 1,
                "roots": ["main"],
                "sources": [{
                    "root": "main",
                    "logicalPath": "app/main.ko",
                    "uri": "file:///not-read/main.ko",
                    "text": "package app"
                }]
            }
        }
    });
    client
        .sender
        .send(Message::Request(Request {
            id: RequestId::from(39_i32),
            method: Initialize::METHOD.to_owned(),
            params,
        }))
        .expect("initialize request");
    let response = receive_response(&client);
    assert!(response.response_result.is_ok());
    send_notification(
        &client,
        Initialized::METHOD,
        serde_json::to_value(InitializedParams {}).expect("initialized params"),
    );
    let initial = receive_diagnostics(&client);
    assert_eq!(initial.version, None);
    assert!(initial.diagnostics.is_empty());
    shutdown(&client);
    server_thread
        .join()
        .expect("server thread")
        .expect("server result");
}

#[test]
fn source_set_initialization_returns_invalid_params_before_session_start() {
    let (server, client) = Connection::memory();
    let server_thread = thread::spawn(|| run(server));
    let mut params = serde_json::to_value(InitializeParams::default()).expect("params");
    params["initializationOptions"] = serde_json::json!({
        "koven": {"sourceSet": {
            "schema": "koven.lsp.source-set",
            "version": 1,
            "roots": [],
            "sources": []
        }}
    });
    client
        .sender
        .send(Message::Request(Request {
            id: RequestId::from(40_i32),
            method: Initialize::METHOD.to_owned(),
            params,
        }))
        .expect("initialize request");

    let response = receive_response(&client);
    let error = response.response_result.expect_err("invalid params");
    assert_eq!(error.code, ErrorCode::InvalidParams as i32);
    assert!(error.message.contains("roots must not be empty"));
    server_thread
        .join()
        .expect("server thread")
        .expect("server result");
}
