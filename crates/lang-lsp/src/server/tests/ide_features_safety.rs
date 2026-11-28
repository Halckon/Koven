use super::*;
use lsp_types::{
    DocumentChanges, Hover, HoverContents, HoverParams, RenameParams, TextDocumentPositionParams,
    WorkspaceClientCapabilities, WorkspaceEdit, WorkspaceEditClientCapabilities,
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

        // Deleted/moved closed lib.ko on disk -> rename must ALSO reject!
        std::fs::remove_file(&lib_path).expect("delete lib");
        send_request(
            &client,
            RequestId::from(1111_i32),
            Rename::METHOD,
            serde_json::to_value(RenameParams {
                text_document_position: doc_pos(&app_uri, position_of(app_src, "helper", 0)),
                new_name: "assist".to_owned(),
                work_done_progress_params: Default::default(),
            })
            .expect("deleted sourceSet rename"),
        );
        assert_eq!(
            receive_response(&client)
                .response_result
                .expect_err("deleted closed sourceSet file must reject")
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

    // 3. P2-4 & UX: `prepareRename` on shared overload `import lib.pick` returns readable error.
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
        let prep_err = receive_response(&client)
            .response_result
            .expect_err("prepareRename on shared overload import must explain refusal");
        assert_eq!(prep_err.code, ErrorCode::InvalidParams as i32);
        assert!(
            prep_err.message.contains("overload"),
            "expected readable reason, got: {}",
            prep_err.message
        );
        shutdown(&client);
        server_thread
            .join()
            .expect("server thread")
            .expect("server result");
    }
}

#[test]
fn hover_bounds_const_comparison_and_distinguishes_val_after_var_and_comments() {
    let (server, client) = Connection::memory();
    let server_thread = thread::spawn(|| run(server));
    initialize_with_doc_changes(&client, false, None);
    let uri: lsp_types::Uri = "file:///hover_bounds.ko".parse().expect("uri");
    let source = "\
const val ok: Boolean = 1 < 2
fun next(): Int = 1

fun demo() {
    var alpha = 1; val beta = alpha
    /* var */ val gamma = beta
}
";
    send_notification(
        &client,
        DidOpenTextDocument::METHOD,
        serde_json::to_value(DidOpenTextDocumentParams {
            text_document: TextDocumentItem::new(
                uri.clone(),
                "koven".to_owned(),
                1,
                source.to_owned(),
            ),
        })
        .expect("open"),
    );
    let _ = receive_diagnostics(&client);

    // Hover on `ok` must NOT swallow `fun next()` even though initializer contains `<`.
    send_request(
        &client,
        RequestId::from(120_i32),
        HoverRequest::METHOD,
        serde_json::to_value(HoverParams {
            text_document_position_params: doc_pos(&uri, position_of(source, "ok", 0)),
            work_done_progress_params: Default::default(),
        })
        .expect("hover ok"),
    );
    let ok_hover: Option<Hover> =
        serde_json::from_value(receive_response(&client).response_result.expect("ok"))
            .expect("hover");
    let HoverContents::Markup(ok_md) = ok_hover.expect("hover").contents else {
        panic!("expected markup");
    };
    assert!(ok_md.value.contains("const val ok"), "{}", ok_md.value);
    assert!(!ok_md.value.contains("next"), "{}", ok_md.value);

    // Hover on `beta` (same line after `var alpha = 1;`) must show `val beta`, NOT `var beta`.
    send_request(
        &client,
        RequestId::from(121_i32),
        HoverRequest::METHOD,
        serde_json::to_value(HoverParams {
            text_document_position_params: doc_pos(&uri, position_of(source, "beta", 0)),
            work_done_progress_params: Default::default(),
        })
        .expect("hover beta"),
    );
    let b_hover: Option<Hover> =
        serde_json::from_value(receive_response(&client).response_result.expect("ok"))
            .expect("hover");
    let HoverContents::Markup(b_md) = b_hover.expect("hover").contents else {
        panic!("expected markup");
    };
    assert!(b_md.value.contains("val beta: Int"), "{}", b_md.value);
    assert!(!b_md.value.contains("var beta"), "{}", b_md.value);

    // Hover on `gamma` (preceded by `/* var */ val`) must show `val gamma`, NOT `var gamma`.
    send_request(
        &client,
        RequestId::from(122_i32),
        HoverRequest::METHOD,
        serde_json::to_value(HoverParams {
            text_document_position_params: doc_pos(&uri, position_of(source, "gamma", 0)),
            work_done_progress_params: Default::default(),
        })
        .expect("hover gamma"),
    );
    let c_hover: Option<Hover> =
        serde_json::from_value(receive_response(&client).response_result.expect("ok"))
            .expect("hover");
    let HoverContents::Markup(c_md) = c_hover.expect("hover").contents else {
        panic!("expected markup");
    };
    assert!(c_md.value.contains("val gamma: Int"), "{}", c_md.value);
    assert!(!c_md.value.contains("var gamma"), "{}", c_md.value);

    shutdown(&client);
    server_thread
        .join()
        .expect("server thread")
        .expect("server result");
}

#[test]
fn rename_rejects_moving_same_error_code_to_new_location() {
    let (server, client) = Connection::memory();
    let server_thread = thread::spawn(|| run(server));
    initialize_with_doc_changes(&client, false, None);
    let uri: lsp_types::Uri = "file:///moving_error.ko".parse().expect("uri");
    // Two pre-existing duplicate-signature functions `dup`, plus a third `other`.
    // Renaming one `dup` -> `other` would clear L0097 on `dup` and create a NEW L0097 on `other`
    // (same total L0097 count, different location).
    let source = "\
fun dup(): Int = 1
fun dup(): Int = 2
fun other(): Int = 3
";
    send_notification(
        &client,
        DidOpenTextDocument::METHOD,
        serde_json::to_value(DidOpenTextDocumentParams {
            text_document: TextDocumentItem::new(
                uri.clone(),
                "koven".to_owned(),
                1,
                source.to_owned(),
            ),
        })
        .expect("open"),
    );
    let _ = receive_diagnostics(&client);

    send_request(
        &client,
        RequestId::from(130_i32),
        Rename::METHOD,
        serde_json::to_value(RenameParams {
            text_document_position: doc_pos(&uri, position_of(source, "dup", 0)),
            new_name: "other".to_owned(),
            work_done_progress_params: Default::default(),
        })
        .expect("rename dup -> other"),
    );
    assert_eq!(
        receive_response(&client)
            .response_result
            .expect_err("moving same error code to new location must reject")
            .code,
        ErrorCode::InvalidParams as i32
    );

    shutdown(&client);
    server_thread
        .join()
        .expect("server thread")
        .expect("server result");
}
