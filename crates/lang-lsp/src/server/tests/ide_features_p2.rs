use super::*;
use lsp_types::{
    CompletionParams, CompletionResponse, DocumentSymbolParams, DocumentSymbolResponse,
    SignatureHelp, SignatureHelpParams, TextDocumentPositionParams, WorkspaceSymbolParams,
    WorkspaceSymbolResponse,
    request::{Completion, DocumentSymbolRequest, SignatureHelpRequest, WorkspaceSymbolRequest},
};

fn doc_pos(uri: &lsp_types::Uri, position: Position) -> TextDocumentPositionParams {
    TextDocumentPositionParams::new(TextDocumentIdentifier::new(uri.clone()), position)
}

#[test]
fn document_and_workspace_symbols_completion_and_signature_help_work_across_files() {
    let (server, client) = Connection::memory();
    let server_thread = thread::spawn(|| run(server));
    let math_uri: lsp_types::Uri = "file:///p2/demo/math.ko".parse().expect("uri");
    let main_uri: lsp_types::Uri = "file:///p2/demo/main.ko".parse().expect("uri");
    let math_src = "\
package demo

public const val DEFAULT_SCALE: Int = 2

public value class Point(val x: Int, val y: Int) {
    public fun norm(): Int = x + y
}

public enum class Color {
    Red,
    Blue
}

public fun combine(left: Int, right: Int): Int = left + right
";
    let main_src = "\
package demo

fun main() {
    val origin = Point(3, 4)
    val d = origin.norm()
    val c = Color.Red
    val s = combine(d, DEFAULT_SCALE)
    val q = demo.DEFAULT_SCALE
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
    let d1 = receive_diagnostics(&client);
    let d2 = receive_diagnostics(&client);
    assert!(d1.diagnostics.is_empty(), "d1: {:?}", d1.diagnostics);
    assert!(d2.diagnostics.is_empty(), "d2: {:?}", d2.diagnostics);

    // 1. textDocument/documentSymbol: nested tree for math.ko
    send_request(
        &client,
        RequestId::from(200_i32),
        DocumentSymbolRequest::METHOD,
        serde_json::to_value(DocumentSymbolParams {
            text_document: TextDocumentIdentifier::new(math_uri.clone()),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .expect("doc symbols"),
    );
    let doc_syms: Option<DocumentSymbolResponse> = serde_json::from_value(
        receive_response(&client)
            .response_result
            .expect("doc symbols ok"),
    )
    .expect("doc symbols response");
    let Some(DocumentSymbolResponse::Nested(nested)) = doc_syms else {
        panic!("expected nested DocumentSymbolResponse");
    };
    let point = nested
        .iter()
        .find(|s| s.name == "Point")
        .expect("Point symbol");
    let point_children = point.children.as_ref().expect("Point children");
    assert!(point_children.iter().any(|c| c.name == "x"));
    assert!(point_children.iter().any(|c| c.name == "y"));
    assert!(point_children.iter().any(|c| c.name == "norm"));
    let color = nested
        .iter()
        .find(|s| s.name == "Color")
        .expect("Color symbol");
    let color_children = color.children.as_ref().expect("Color children");
    assert!(color_children.iter().any(|c| c.name == "Red"));
    assert!(color_children.iter().any(|c| c.name == "Blue"));

    // 2. workspace/symbol: case-insensitive query across files
    send_request(
        &client,
        RequestId::from(201_i32),
        WorkspaceSymbolRequest::METHOD,
        serde_json::to_value(WorkspaceSymbolParams {
            query: "nor".to_owned(),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .expect("workspace symbols"),
    );
    let ws_syms: Option<WorkspaceSymbolResponse> = serde_json::from_value(
        receive_response(&client)
            .response_result
            .expect("ws symbols ok"),
    )
    .expect("ws symbols response");
    let Some(WorkspaceSymbolResponse::Flat(flat)) = ws_syms else {
        panic!("expected Flat WorkspaceSymbolResponse");
    };
    assert!(
        flat.iter()
            .any(|s| s.name == "norm" && s.container_name.as_deref() == Some("demo.Point")),
        "expected demo.Point.norm, got: {flat:?}"
    );

    // 3a. Dot completion on nominal instance (`origin.norm()` -> cursor right after `origin.`)
    let mut dot_pos = position_of(main_src, "origin.norm", 0);
    dot_pos.character += "origin.".len() as u32;
    send_request(
        &client,
        RequestId::from(202_i32),
        Completion::METHOD,
        serde_json::to_value(CompletionParams {
            text_document_position: doc_pos(&main_uri, dot_pos),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
            context: None,
        })
        .expect("completion"),
    );
    let comp: Option<CompletionResponse> = serde_json::from_value(
        receive_response(&client)
            .response_result
            .expect("completion ok"),
    )
    .expect("completion response");
    let Some(CompletionResponse::Array(items)) = comp else {
        panic!("expected completion array");
    };
    assert!(items.iter().any(|i| i.label == "x"), "{items:?}");
    assert!(items.iter().any(|i| i.label == "y"), "{items:?}");
    assert!(items.iter().any(|i| i.label == "norm"), "{items:?}");

    // 3b. Dot completion on enum (`Color.` -> Red, Blue)
    let mut enum_dot = position_of(main_src, "Color.Red", 0);
    enum_dot.character += "Color.".len() as u32;
    send_request(
        &client,
        RequestId::from(203_i32),
        Completion::METHOD,
        serde_json::to_value(CompletionParams {
            text_document_position: doc_pos(&main_uri, enum_dot),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
            context: None,
        })
        .expect("enum completion"),
    );
    let Some(CompletionResponse::Array(enum_items)) =
        serde_json::from_value::<Option<CompletionResponse>>(
            receive_response(&client)
                .response_result
                .expect("enum completion ok"),
        )
        .expect("enum completion response")
    else {
        panic!("expected enum completion array");
    };
    assert!(enum_items.iter().any(|i| i.label == "Red"));
    assert!(enum_items.iter().any(|i| i.label == "Blue"));

    // 3c. Dot completion on package (`demo.` -> DEFAULT_SCALE, combine, Point, Color)
    let mut pkg_dot = position_of(main_src, "demo.DEFAULT_SCALE", 0);
    pkg_dot.character += "demo.".len() as u32;
    send_request(
        &client,
        RequestId::from(204_i32),
        Completion::METHOD,
        serde_json::to_value(CompletionParams {
            text_document_position: doc_pos(&main_uri, pkg_dot),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
            context: None,
        })
        .expect("pkg completion"),
    );
    let Some(CompletionResponse::Array(pkg_items)) =
        serde_json::from_value::<Option<CompletionResponse>>(
            receive_response(&client)
                .response_result
                .expect("pkg completion ok"),
        )
        .expect("pkg completion response")
    else {
        panic!("expected pkg completion array");
    };
    assert!(pkg_items.iter().any(|i| i.label == "DEFAULT_SCALE"));
    assert!(pkg_items.iter().any(|i| i.label == "combine"));

    // 3d. Scope completion on prefix `ori` -> local `origin`
    let mut prefix_pos = position_of(main_src, "origin.norm", 0);
    prefix_pos.character += 3;
    send_request(
        &client,
        RequestId::from(205_i32),
        Completion::METHOD,
        serde_json::to_value(CompletionParams {
            text_document_position: doc_pos(&main_uri, prefix_pos),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
            context: None,
        })
        .expect("scope completion"),
    );
    let Some(CompletionResponse::Array(scope_items)) =
        serde_json::from_value::<Option<CompletionResponse>>(
            receive_response(&client)
                .response_result
                .expect("scope completion ok"),
        )
        .expect("scope completion response")
    else {
        panic!("expected scope completion array");
    };
    assert!(scope_items.iter().any(|i| i.label == "origin"));

    // 4a. Signature help on 2nd argument of `combine(d, DEFAULT_SCALE)` -> active_parameter = 1
    let sig_pos = position_of(main_src, "DEFAULT_SCALE", 0);
    send_request(
        &client,
        RequestId::from(206_i32),
        SignatureHelpRequest::METHOD,
        serde_json::to_value(SignatureHelpParams {
            text_document_position_params: doc_pos(&main_uri, sig_pos),
            work_done_progress_params: Default::default(),
            context: None,
        })
        .expect("sig help"),
    );
    let sig_help: Option<SignatureHelp> = serde_json::from_value(
        receive_response(&client)
            .response_result
            .expect("sig help ok"),
    )
    .expect("sig help response");
    let sig_help = sig_help.expect("signature help");
    assert_eq!(sig_help.active_parameter, Some(1));
    assert_eq!(sig_help.signatures.len(), 1);
    assert!(
        sig_help.signatures[0]
            .label
            .contains("fun combine(left: Int, right: Int): Int"),
        "{}",
        sig_help.signatures[0].label
    );

    // 4b. Signature help on constructor `Point(3, 4)` at 1st argument -> active_parameter = 0
    let mut ctor_pos = position_of(main_src, "Point(3, 4)", 0);
    ctor_pos.character += "Point(".len() as u32;
    send_request(
        &client,
        RequestId::from(207_i32),
        SignatureHelpRequest::METHOD,
        serde_json::to_value(SignatureHelpParams {
            text_document_position_params: doc_pos(&main_uri, ctor_pos),
            work_done_progress_params: Default::default(),
            context: None,
        })
        .expect("ctor sig help"),
    );
    let ctor_sig: Option<SignatureHelp> = serde_json::from_value(
        receive_response(&client)
            .response_result
            .expect("ctor sig ok"),
    )
    .expect("ctor sig response");
    let ctor_sig = ctor_sig.expect("ctor signature help");
    assert_eq!(ctor_sig.active_parameter, Some(0));
    assert!(
        ctor_sig.signatures[0]
            .label
            .contains("Point(x: Int, y: Int)"),
        "{}",
        ctor_sig.signatures[0].label
    );

    shutdown(&client);
    server_thread
        .join()
        .expect("server thread")
        .expect("server result");
}
