use super::*;
use lsp_types::{
    DocumentChanges, Hover, HoverContents, HoverParams, PrepareRenameResponse, RenameParams,
    TextDocumentPositionParams, WorkspaceClientCapabilities, WorkspaceEdit,
    WorkspaceEditClientCapabilities,
    request::{HoverRequest, PrepareRenameRequest, Rename},
};

fn doc_pos(uri: &lsp_types::Uri, position: Position) -> TextDocumentPositionParams {
    TextDocumentPositionParams::new(TextDocumentIdentifier::new(uri.clone()), position)
}

fn initialize_with_doc_changes(
    client: &Connection,
    supports_document_changes: bool,
    options: Option<serde_json::Value>,
) {
    let mut params = InitializeParams::default();
    params.capabilities.workspace = Some(WorkspaceClientCapabilities {
        workspace_edit: Some(WorkspaceEditClientCapabilities {
            document_changes: Some(supports_document_changes),
            ..Default::default()
        }),
        ..Default::default()
    });
    let mut val = serde_json::to_value(params).expect("init params");
    if let Some(opts) = options {
        val["initializationOptions"] = opts;
    }
    send_request(client, RequestId::from(1_i32), Initialize::METHOD, val);
    let _ = receive_response(client);
    send_notification(
        client,
        Initialized::METHOD,
        serde_json::to_value(InitializedParams {}).expect("initialized"),
    );
}

#[test]
fn rename_rejects_duplicate_locals_and_top_level_signatures_and_updates_constructor_named_args() {
    let (server, client) = Connection::memory();
    let server_thread = thread::spawn(|| run(server));
    initialize_with_doc_changes(&client, true, None);
    let uri: lsp_types::Uri = "file:///p1_checks.ko".parse().expect("uri");
    let source = "\
class Holder(val field: Int)
fun first(): Int = 1; fun second(): Int = 2

fun demo() {
    val a = 1
    val b = 2
    val h = Holder(field = a)
}
";
    send_notification(
        &client,
        DidOpenTextDocument::METHOD,
        serde_json::to_value(DidOpenTextDocumentParams {
            text_document: TextDocumentItem::new(
                uri.clone(),
                "koven".to_owned(),
                7,
                source.to_owned(),
            ),
        })
        .expect("open"),
    );
    let _ = receive_diagnostics(&client);

    // P1-1a: Renaming unused local `b` -> `a` in same scope introduces L0079 -> must reject.
    let dup_local_id = RequestId::from(100_i32);
    send_request(
        &client,
        dup_local_id.clone(),
        Rename::METHOD,
        serde_json::to_value(RenameParams {
            text_document_position: doc_pos(&uri, position_of(source, "b", 0)),
            new_name: "a".to_owned(),
            work_done_progress_params: Default::default(),
        })
        .expect("dup local"),
    );
    assert_eq!(
        receive_response(&client)
            .response_result
            .expect_err("duplicate local must reject")
            .code,
        ErrorCode::InvalidParams as i32
    );

    // P1-1b: Renaming `second` -> `first` introduces duplicate signature L0097 -> must reject.
    let dup_fn_id = RequestId::from(101_i32);
    send_request(
        &client,
        dup_fn_id.clone(),
        Rename::METHOD,
        serde_json::to_value(RenameParams {
            text_document_position: doc_pos(&uri, position_of(source, "second", 0)),
            new_name: "first".to_owned(),
            work_done_progress_params: Default::default(),
        })
        .expect("dup fn"),
    );
    assert_eq!(
        receive_response(&client)
            .response_result
            .expect_err("duplicate function signature must reject")
            .code,
        ErrorCode::InvalidParams as i32
    );

    // P1-2 & P2-1: Renaming `Holder.field` updates BOTH declaration AND `Holder(field = a)`
    // and returns versioned `documentChanges` when client advertises `documentChanges: true`.
    let ctor_id = RequestId::from(102_i32);
    send_request(
        &client,
        ctor_id.clone(),
        Rename::METHOD,
        serde_json::to_value(RenameParams {
            text_document_position: doc_pos(&uri, position_of(source, "field", 0)),
            new_name: "amount".to_owned(),
            work_done_progress_params: Default::default(),
        })
        .expect("ctor rename"),
    );
    let edit: Option<WorkspaceEdit> = serde_json::from_value(
        receive_response(&client)
            .response_result
            .expect("ctor rename ok"),
    )
    .expect("workspace edit");
    let edit = edit.expect("edit");
    assert!(edit.changes.is_none());
    let Some(DocumentChanges::Edits(doc_edits)) = edit.document_changes else {
        panic!("expected DocumentChanges::Edits");
    };
    assert_eq!(doc_edits.len(), 1);
    assert_eq!(doc_edits[0].text_document.version, Some(7));
    assert_eq!(
        doc_edits[0].edits.len(),
        2,
        "field declaration + constructor named argument `field = a`"
    );

    // P2-3: Hover on same-line `second` shows `fun second(): Int`, NOT `fun first(): Int`.
    let hover_id = RequestId::from(103_i32);
    send_request(
        &client,
        hover_id.clone(),
        HoverRequest::METHOD,
        serde_json::to_value(HoverParams {
            text_document_position_params: doc_pos(&uri, position_of(source, "second", 0)),
            work_done_progress_params: Default::default(),
        })
        .expect("hover"),
    );
    let hover: Option<Hover> =
        serde_json::from_value(receive_response(&client).response_result.expect("hover ok"))
            .expect("hover");
    let HoverContents::Markup(markup) = hover.expect("hover").contents else {
        panic!("expected markup");
    };
    assert!(
        markup.value.contains("fun second(): Int"),
        "{}",
        markup.value
    );
    assert!(!markup.value.contains("first"), "{}", markup.value);

    shutdown(&client);
    server_thread
        .join()
        .expect("server thread")
        .expect("server result");
}

#[test]
fn rename_rejects_incomplete_unit_and_stale_source_set_disk_file_and_overload_prepare_rename() {
    // 1. P1-3: Unrelated file with unresolved name prevents typed analysis -> rename must reject.
    {
        let (server, client) = Connection::memory();
        let server_thread = thread::spawn(|| run(server));
        let lib_uri: lsp_types::Uri = "file:///mem/lib.ko".parse().expect("uri");
        let broken_uri: lsp_types::Uri = "file:///mem/broken.ko".parse().expect("uri");
        let lib_src = "package p\nfun twice(value: Int): Int = value + value\n";
        let broken_src = "package p\nfun bad(): Int = missingSymbol()\n";
        initialize_with_doc_changes(
            &client,
            false,
            Some(serde_json::json!({"koven": {"sourceSet": {
                "schema": "koven.lsp.source-set",
                "version": 1,
                "roots": ["src"],
                "sources": [
                    {"root": "src", "logicalPath": "p/lib.ko", "uri": lib_uri.as_str(), "text": lib_src},
                    {"root": "src", "logicalPath": "p/broken.ko", "uri": broken_uri.as_str(), "text": broken_src}
                ]
            }}})),
        );
        let _ = receive_diagnostics(&client);
        let _ = receive_diagnostics(&client);

        send_request(
            &client,
            RequestId::from(110_i32),
            Rename::METHOD,
            serde_json::to_value(RenameParams {
                text_document_position: doc_pos(&lib_uri, position_of(lib_src, "value", 0)),
                new_name: "v".to_owned(),
                work_done_progress_params: Default::default(),
            })
            .expect("rename"),
        );
        assert_eq!(
            receive_response(&client)
                .response_result
                .expect_err("incomplete unit must reject rename")
                .code,
            ErrorCode::InvalidParams as i32
        );
        shutdown(&client);
        server_thread
            .join()
            .expect("server thread")
            .expect("server result");
    }

    // 2. P1-4: Explicit `koven.sourceSet` checks disk for closed files that exist on disk.
    {
        let dir =
            std::env::temp_dir().join(format!("koven_lsp_sourceset_stale_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src/p")).expect("mkdir");
        let dir = dir.canonicalize().expect("canon");
        let lib_path = dir.join("src/p/lib.ko");
        let app_path = dir.join("src/p/app.ko");
        let lib_src = "package p\npublic fun helper(x: Int): Int = x\n";
        let app_src = "package p\nfun main() { val v = helper(1) }\n";
        std::fs::write(&lib_path, lib_src).expect("write lib");
        std::fs::write(&app_path, app_src).expect("write app");
        let lib_uri: lsp_types::Uri = format!("file://{}", lib_path.display())
            .parse()
            .expect("lib uri");
        let app_uri: lsp_types::Uri = format!("file://{}", app_path.display())
            .parse()
            .expect("app uri");

        let (server, client) = Connection::memory();
        let server_thread = thread::spawn(|| run(server));
        initialize_with_doc_changes(
            &client,
            false,
            Some(serde_json::json!({"koven": {"sourceSet": {
                "schema": "koven.lsp.source-set",
                "version": 1,
                "roots": ["src"],
                "sources": [
                    {"root": "src", "logicalPath": "p/lib.ko", "uri": lib_uri.as_str(), "text": lib_src},
                    {"root": "src", "logicalPath": "p/app.ko", "uri": app_uri.as_str(), "text": app_src}
                ]
            }}})),
        );
        let _ = receive_diagnostics(&client);
        let _ = receive_diagnostics(&client);

        // External edit to closed lib.ko on disk -> rename from app.ko must reject!
        std::fs::write(&lib_path, format!("\n{lib_src}")).expect("mutate lib");
        send_request(
            &client,
            RequestId::from(111_i32),
            Rename::METHOD,
            serde_json::to_value(RenameParams {
                text_document_position: doc_pos(&app_uri, position_of(app_src, "helper", 0)),
                new_name: "assist".to_owned(),
                work_done_progress_params: Default::default(),
            })
            .expect("stale sourceSet rename"),
        );
        assert_eq!(
            receive_response(&client)
                .response_result
                .expect_err("stale closed sourceSet file must reject")
                .code,
            ErrorCode::InvalidParams as i32
        );
        shutdown(&client);
        server_thread
            .join()
            .expect("server thread")
            .expect("server result");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // 3. P2-4: `prepareRename` on shared overload `import lib.pick` returns null (consistent with rename).
    {
        let (server, client) = Connection::memory();
        let server_thread = thread::spawn(|| run(server));
        let lib_uri: lsp_types::Uri = "file:///mem2/lib.ko".parse().expect("uri");
        let app_uri: lsp_types::Uri = "file:///mem2/app.ko".parse().expect("uri");
        let lib_src =
            "package lib\npublic fun pick(x: Int): Int = x\npublic fun pick(s: String): Int = 1\n";
        let app_src = "package app\nimport lib.pick\nfun f(): Int = pick(1)\n";
        initialize_with_doc_changes(
            &client,
            false,
            Some(serde_json::json!({"koven": {"sourceSet": {
                "schema": "koven.lsp.source-set",
                "version": 1,
                "roots": ["src"],
                "sources": [
                    {"root": "src", "logicalPath": "lib/lib.ko", "uri": lib_uri.as_str(), "text": lib_src},
                    {"root": "src", "logicalPath": "app/app.ko", "uri": app_uri.as_str(), "text": app_src}
                ]
            }}})),
        );
        let _ = receive_diagnostics(&client);
        let _ = receive_diagnostics(&client);

        send_request(
            &client,
            RequestId::from(112_i32),
            PrepareRenameRequest::METHOD,
            serde_json::to_value(doc_pos(&app_uri, position_of(app_src, "pick", 0)))
                .expect("prepare rename"),
        );
        let prep: Option<PrepareRenameResponse> = serde_json::from_value(
            receive_response(&client)
                .response_result
                .expect("prepare rename ok"),
        )
        .expect("prepare response");
        assert!(
            prep.is_none(),
            "prepareRename on shared overload import must return null"
        );
        shutdown(&client);
        server_thread
            .join()
            .expect("server thread")
            .expect("server result");
    }
}
