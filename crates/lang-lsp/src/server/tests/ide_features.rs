use super::*;
use lsp_types::{
    Hover, HoverContents, HoverParams, InlayHint, InlayHintLabel, InlayHintParams, Location,
    PrepareRenameResponse, Range, ReferenceContext, ReferenceParams, RenameParams,
    TextDocumentPositionParams, WorkspaceEdit,
    request::{HoverRequest, InlayHintRequest, PrepareRenameRequest, References, Rename},
};

macro_rules! request_typed {
    ($client:expr, $id:expr, $method:expr, $params:expr $(,)?) => {{
        let request_id = RequestId::from($id);
        send_request($client, request_id.clone(), $method, $params);
        let response = receive_response($client);
        assert_eq!(response.id, request_id);
        serde_json::from_value(response.response_result.expect("ok response"))
            .expect("typed response")
    }};
}

fn doc_pos(uri: &lsp_types::Uri, position: Position) -> TextDocumentPositionParams {
    TextDocumentPositionParams::new(TextDocumentIdentifier::new(uri.clone()), position)
}

fn hint_label(hint: &InlayHint) -> &str {
    match &hint.label {
        InlayHintLabel::String(text) => text.as_str(),
        InlayHintLabel::LabelParts(_) => panic!("expected string inlay hint label"),
    }
}

#[test]
fn source_set_references_hover_inlay_hints_and_rename_work_across_files() {
    let (server, client) = Connection::memory();
    let server_thread = thread::spawn(|| run(server));
    let math_uri: lsp_types::Uri = "file:///workspace/demo/math.ko".parse().expect("uri");
    let main_uri: lsp_types::Uri = "file:///workspace/demo/main.ko".parse().expect("uri");
    let math_src = "\
package demo

/// 演示常量与基础数值计算
public const val DEFAULT_SCALE: Int = 2

public fun scale(value: Int): Int {
    return value * DEFAULT_SCALE
}
";
    let main_src = "\
package demo

fun main() {
    val inferred = scale(21)
    val explicit: Int = DEFAULT_SCALE
    val total = inferred + explicit
}
";
    initialize_with_options(
        &client,
        Some(serde_json::json!({"koven": {"sourceSet": {
            "schema": "koven.lsp.source-set",
            "version": 1,
            "roots": ["src"],
            "sources": [
                {"root": "src", "logicalPath": "demo/math.ko", "uri": math_uri.as_str(), "text": math_src},
                {"root": "src", "logicalPath": "demo/main.ko", "uri": main_uri.as_str(), "text": main_src}
            ]
        }}})),
    );
    // Drain initial diagnostics for both files
    let _ = receive_diagnostics(&client);
    let _ = receive_diagnostics(&client);

    // 1. Bidirectional references from declaration site (DEFAULT_SCALE in math.ko)
    let decl_pos = position_of(math_src, "DEFAULT_SCALE", 0);
    let refs_with_decl: Option<Vec<Location>> = request_typed!(
        &client,
        10,
        References::METHOD,
        serde_json::to_value(ReferenceParams {
            text_document_position: doc_pos(&math_uri, decl_pos),
            context: ReferenceContext {
                include_declaration: true,
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .expect("ref params"),
    );
    let refs_with_decl = refs_with_decl.expect("references with declaration");
    assert_eq!(refs_with_decl.len(), 3, "decl + math.ko use + main.ko use");
    assert!(refs_with_decl.iter().any(|l| l.uri == main_uri));

    // References without declaration from usage site in main.ko
    let use_pos = position_of(main_src, "DEFAULT_SCALE", 0);
    let refs_no_decl: Option<Vec<Location>> = request_typed!(
        &client,
        11,
        References::METHOD,
        serde_json::to_value(ReferenceParams {
            text_document_position: doc_pos(&main_uri, use_pos),
            context: ReferenceContext {
                include_declaration: false,
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .expect("ref params"),
    );
    let refs_no_decl = refs_no_decl.expect("references without declaration");
    assert_eq!(refs_no_decl.len(), 2, "two usage sites only");

    // 2. Hover on cross-file constant shows package, signature, and leading /// doc comment
    let hover: Option<Hover> = request_typed!(
        &client,
        12,
        HoverRequest::METHOD,
        serde_json::to_value(HoverParams {
            text_document_position_params: doc_pos(&main_uri, use_pos),
            work_done_progress_params: Default::default(),
        })
        .expect("hover params"),
    );
    let hover = hover.expect("hover response");
    let HoverContents::Markup(markup) = hover.contents else {
        panic!("expected markdown hover");
    };
    assert!(markup.value.contains("package demo"), "{}", markup.value);
    assert!(
        markup.value.contains("DEFAULT_SCALE: Int = 2"),
        "{}",
        markup.value
    );
    assert!(
        markup.value.contains("演示常量与基础数值计算"),
        "{}",
        markup.value
    );

    // Hover on local inferred val shows inferred type
    let inferred_pos = position_of(main_src, "inferred", 0);
    let local_hover: Option<Hover> = request_typed!(
        &client,
        13,
        HoverRequest::METHOD,
        serde_json::to_value(HoverParams {
            text_document_position_params: doc_pos(&main_uri, inferred_pos),
            work_done_progress_params: Default::default(),
        })
        .expect("local hover params"),
    );
    let HoverContents::Markup(local_markup) = local_hover.expect("local hover").contents else {
        panic!("expected markdown hover");
    };
    assert!(
        local_markup.value.contains("val inferred: Int"),
        "{}",
        local_markup.value
    );

    // 3. Inlay hints: inferred and total get `: Int`, explicit does NOT
    let hints: Option<Vec<InlayHint>> = request_typed!(
        &client,
        14,
        InlayHintRequest::METHOD,
        serde_json::to_value(InlayHintParams {
            text_document: TextDocumentIdentifier::new(main_uri.clone()),
            range: Range::new(Position::new(0, 0), Position::new(10, 0)),
            work_done_progress_params: Default::default(),
        })
        .expect("inlay hint params"),
    );
    let hints = hints.expect("inlay hints");
    assert_eq!(hints.len(), 2, "only inferred and total should have hints");
    for hint in &hints {
        assert_eq!(hint_label(hint), ": Int");
    }

    // 4. PrepareRename + cross-file Rename on `scale`
    let scale_use_pos = position_of(main_src, "scale", 0);
    let prepare: Option<PrepareRenameResponse> = request_typed!(
        &client,
        15,
        PrepareRenameRequest::METHOD,
        serde_json::to_value(doc_pos(&main_uri, scale_use_pos)).expect("prepare rename params"),
    );
    let Some(PrepareRenameResponse::RangeWithPlaceholder { placeholder, .. }) = prepare else {
        panic!("expected RangeWithPlaceholder");
    };
    assert_eq!(placeholder, "scale");

    let edit: Option<WorkspaceEdit> = request_typed!(
        &client,
        16,
        Rename::METHOD,
        serde_json::to_value(RenameParams {
            text_document_position: doc_pos(&main_uri, scale_use_pos),
            new_name: "multiplyByScale".to_owned(),
            work_done_progress_params: Default::default(),
        })
        .expect("rename params"),
    );
    #[allow(clippy::mutable_key_type)]
    let changes = edit
        .expect("workspace edit")
        .changes
        .expect("workspace edit changes");
    assert_eq!(changes.len(), 2, "edits across math.ko and main.ko");
    assert_eq!(changes.get(&math_uri).expect("math edits").len(), 1);
    assert_eq!(changes.get(&main_uri).expect("main edits").len(), 1);

    // Invalid keyword rename must return InvalidParams
    let bad_id = RequestId::from(17_i32);
    send_request(
        &client,
        bad_id.clone(),
        Rename::METHOD,
        serde_json::to_value(RenameParams {
            text_document_position: doc_pos(&main_uri, scale_use_pos),
            new_name: "class".to_owned(),
            work_done_progress_params: Default::default(),
        })
        .expect("bad rename params"),
    );
    let bad_resp = receive_response(&client);
    assert_eq!(bad_resp.id, bad_id);
    assert_eq!(
        bad_resp.response_result.expect_err("invalid params").code,
        ErrorCode::InvalidParams as i32
    );

    shutdown(&client);
    server_thread
        .join()
        .expect("server thread")
        .expect("server result");
}

#[test]
fn single_document_references_hover_inlay_hints_and_rename() {
    let (server, client) = Connection::memory();
    let server_thread = thread::spawn(|| run(server));
    initialize(&client);
    let uri: lsp_types::Uri = "file:///single_ide.ko".parse().expect("uri");
    let source = "\
/// 计算两倍值
fun twice(x: Int): Int = x + x

fun main() {
    val result = twice(5)
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
        .expect("open params"),
    );
    let _ = receive_diagnostics(&client);

    // References from declaration of `twice`
    let refs: Option<Vec<Location>> = request_typed!(
        &client,
        30,
        References::METHOD,
        serde_json::to_value(ReferenceParams {
            text_document_position: doc_pos(&uri, position_of(source, "twice", 0)),
            context: ReferenceContext {
                include_declaration: true,
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .expect("ref params"),
    );
    assert_eq!(refs.expect("refs").len(), 2);

    // Hover on `twice` call site
    let hover: Option<Hover> = request_typed!(
        &client,
        31,
        HoverRequest::METHOD,
        serde_json::to_value(HoverParams {
            text_document_position_params: doc_pos(&uri, position_of(source, "twice", 1)),
            work_done_progress_params: Default::default(),
        })
        .expect("hover params"),
    );
    let HoverContents::Markup(markup) = hover.expect("hover").contents else {
        panic!("expected markup hover");
    };
    assert!(markup.value.contains("计算两倍值"), "{}", markup.value);

    // Inlay hint for `val result`
    let hints: Option<Vec<InlayHint>> = request_typed!(
        &client,
        32,
        InlayHintRequest::METHOD,
        serde_json::to_value(InlayHintParams {
            text_document: TextDocumentIdentifier::new(uri.clone()),
            range: Range::new(Position::new(0, 0), Position::new(10, 0)),
            work_done_progress_params: Default::default(),
        })
        .expect("inlay params"),
    );
    let hints = hints.expect("hints");
    assert_eq!(hints.len(), 1);
    assert_eq!(hint_label(&hints[0]), ": Int");

    // Rename `twice` -> `doubleVal`
    let edit: Option<WorkspaceEdit> = request_typed!(
        &client,
        33,
        Rename::METHOD,
        serde_json::to_value(RenameParams {
            text_document_position: doc_pos(&uri, position_of(source, "twice", 0)),
            new_name: "doubleVal".to_owned(),
            work_done_progress_params: Default::default(),
        })
        .expect("rename params"),
    );
    #[allow(clippy::mutable_key_type)]
    let changes = edit.expect("edit").changes.expect("changes");
    assert_eq!(changes.get(&uri).expect("uri edits").len(), 2);

    shutdown(&client);
    server_thread
        .join()
        .expect("server thread")
        .expect("server result");
}
