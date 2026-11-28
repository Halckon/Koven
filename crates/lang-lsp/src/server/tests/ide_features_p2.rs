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

fn hierarchical_caps() -> lsp_types::ClientCapabilities {
    lsp_types::ClientCapabilities {
        text_document: Some(lsp_types::TextDocumentClientCapabilities {
            document_symbol: Some(lsp_types::DocumentSymbolClientCapabilities {
                hierarchical_document_symbol_support: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        }),
        ..Default::default()
    }
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
    initialize_with_capabilities_and_options(
        &client,
        hierarchical_caps(),
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

#[test]
fn completion_handles_incomplete_dot_trigger_scopes_visibility_interfaces_and_signature_edge_cases()
{
    let (server, client) = Connection::memory();
    let server_thread = thread::spawn(|| run(server));
    let a_uri: lsp_types::Uri = "file:///p2r/a/shared/api.ko".parse().expect("uri");
    let b_uri: lsp_types::Uri = "file:///p2r/b/shared/other.ko".parse().expect("uri");
    let app_uri: lsp_types::Uri = "file:///p2r/app/main.ko".parse().expect("uri");

    let a_src = "\
package a.shared

public interface Greetable {
    public fun greet(): String
}

public class Greeter(public val label: String, private val secret: Int) : Greetable {
    public override fun greet(): String = label
    private fun hiddenMethod(): Int = secret
    companion {
        public const val VERSION: Int = 1
        public fun create(): Greeter = Greeter(\"hi\", 0)
        private fun secretFactory(): Int = 0
    }
}

public class Empty()
public object Singleton {
    public fun ping(): Int = 1
}
public fun <T> identity(item: T): T = item
public fun combine(left: Int, right: Int): Int = left + right
internal fun internalHelper(): Int = 1
private fun filePrivateHelper(): Int = 2
";
    let b_src = "\
package b.shared
public fun fromPackageB(): Int = 99
private fun hiddenInB(): Int = 0
";
    let app_src = "\
package app
import a.shared.Greeter as MyGreeter
import a.shared.Empty
import a.shared.identity
import a.shared.combine

fun sibling(siblingParam: Int) {
    val siblingVar = siblingParam
}

fun testAll() {
    if (true) {
        val exitedBlockVar = 1
    }
    val shadow = 1
    if (true) {
        val shadow = \"inner\"
        val useShadow = shadow
    }
    val g = MyGreeter(\"ok\", 1)
    val a = g.greet()
    val e = Empty()
    val id = identity<Int>(42)
    val c = combine(right = 2, left = 1)
    val pkg = a.shared.internalHelper()
}
";
    initialize_with_capabilities_and_options(
        &client,
        hierarchical_caps(),
        Some(serde_json::json!({"koven": {"sourceSet": {
            "schema": "koven.lsp.source-set",
            "version": 1,
            "roots": ["src"],
            "sources": [
                {"root": "src", "logicalPath": "a/shared/api.ko", "uri": a_uri.as_str(), "text": a_src},
                {"root": "src", "logicalPath": "b/shared/other.ko", "uri": b_uri.as_str(), "text": b_src},
                {"root": "src", "logicalPath": "app/main.ko", "uri": app_uri.as_str(), "text": app_src}
            ]
        }}})),
    );
    let d1 = receive_diagnostics(&client);
    let d2 = receive_diagnostics(&client);
    let d3 = receive_diagnostics(&client);
    assert!(d1.diagnostics.is_empty(), "d1: {:?}", d1.diagnostics);
    assert!(d2.diagnostics.is_empty(), "d2: {:?}", d2.diagnostics);
    assert!(d3.diagnostics.is_empty(), "d3: {:?}", d3.diagnostics);

    // P2-11: `object Singleton` must appear ONCE in documentSymbol and workspace/symbol.
    send_request(
        &client,
        RequestId::from(300_i32),
        DocumentSymbolRequest::METHOD,
        serde_json::to_value(DocumentSymbolParams {
            text_document: TextDocumentIdentifier::new(a_uri.clone()),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .expect("doc symbols"),
    );
    let Some(DocumentSymbolResponse::Nested(a_syms)) =
        serde_json::from_value::<Option<DocumentSymbolResponse>>(
            receive_response(&client).response_result.expect("ok"),
        )
        .expect("resp")
    else {
        panic!("expected nested");
    };
    assert_eq!(
        a_syms.iter().filter(|s| s.name == "Singleton").count(),
        1,
        "object must not be duplicated in documentSymbol"
    );

    send_request(
        &client,
        RequestId::from(301_i32),
        WorkspaceSymbolRequest::METHOD,
        serde_json::to_value(WorkspaceSymbolParams {
            query: "Singleton".to_owned(),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .expect("ws symbols"),
    );
    let Some(WorkspaceSymbolResponse::Flat(ws_singleton)) =
        serde_json::from_value::<Option<WorkspaceSymbolResponse>>(
            receive_response(&client).response_result.expect("ok"),
        )
        .expect("resp")
    else {
        panic!("expected flat");
    };
    assert_eq!(
        ws_singleton
            .iter()
            .filter(|s| s.name == "Singleton")
            .count(),
        1,
        "object must not be duplicated in workspace/symbol"
    );

    // P2-1 & P2-2 & P2-10: Scope completion inside `if (true) { val shadow = "inner"; val useShadow = shadow }`
    let shadow_pos = position_of(app_src, "shadow", 2);
    send_request(
        &client,
        RequestId::from(302_i32),
        Completion::METHOD,
        serde_json::to_value(CompletionParams {
            text_document_position: doc_pos(&app_uri, shadow_pos),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
            context: None,
        })
        .expect("scope comp"),
    );
    let Some(CompletionResponse::Array(scope_items)) =
        serde_json::from_value::<Option<CompletionResponse>>(
            receive_response(&client).response_result.expect("ok"),
        )
        .expect("resp")
    else {
        panic!("expected array");
    };
    assert!(
        !scope_items
            .iter()
            .any(|i| i.label == "siblingParam" || i.label == "siblingVar"),
        "must not leak sibling function locals"
    );
    assert!(
        !scope_items.iter().any(|i| i.label == "exitedBlockVar"),
        "must not leak exited block variables"
    );
    let shadow_item = scope_items
        .iter()
        .find(|i| i.label == "shadow")
        .expect("shadow local");
    assert_eq!(
        shadow_item.detail.as_deref(),
        Some("String"),
        "inner shadowed variable must show inner String type"
    );
    assert!(
        scope_items.iter().any(|i| i.label == "MyGreeter"),
        "explicit import alias must be suggested"
    );
    assert!(
        !scope_items
            .iter()
            .any(|i| i.label == "fromPackageB" || i.label == "filePrivateHelper"),
        "unimported cross-package and other-file private declarations must be excluded"
    );
    let println_item = scope_items
        .iter()
        .find(|i| i.label == "println")
        .expect("println builtin");
    assert_eq!(
        println_item.detail.as_deref(),
        Some("(value: String) -> Unit")
    );
    assert!(scope_items.iter().any(|i| i.label == "Copyable"));
    assert!(scope_items.iter().any(|i| i.label == "rangeView"));

    // P2-3: Multi-segment package `a.shared.` must NOT include `fromPackageB` or `filePrivateHelper`,
    // and MUST include `internalHelper`.
    let mut pkg_dot = position_of(app_src, "a.shared.internalHelper", 0);
    pkg_dot.character += "a.shared.".len() as u32;
    send_request(
        &client,
        RequestId::from(303_i32),
        Completion::METHOD,
        serde_json::to_value(CompletionParams {
            text_document_position: doc_pos(&app_uri, pkg_dot),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
            context: None,
        })
        .expect("pkg comp"),
    );
    let Some(CompletionResponse::Array(pkg_items)) =
        serde_json::from_value::<Option<CompletionResponse>>(
            receive_response(&client).response_result.expect("ok"),
        )
        .expect("resp")
    else {
        panic!("expected array");
    };
    assert!(pkg_items.iter().any(|i| i.label == "internalHelper"));
    assert!(!pkg_items.iter().any(|i| i.label == "fromPackageB"));
    assert!(!pkg_items.iter().any(|i| i.label == "filePrivateHelper"));

    // P2-4 & P2-5: Cursor inside `g.gr|eet()` returns `greet`, excludes `hiddenMethod` & `secret`.
    let mut mid_dot = position_of(app_src, "g.greet()", 0);
    mid_dot.character += "g.gr".len() as u32;
    send_request(
        &client,
        RequestId::from(304_i32),
        Completion::METHOD,
        serde_json::to_value(CompletionParams {
            text_document_position: doc_pos(&app_uri, mid_dot),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
            context: None,
        })
        .expect("mid dot comp"),
    );
    let Some(CompletionResponse::Array(mid_items)) =
        serde_json::from_value::<Option<CompletionResponse>>(
            receive_response(&client).response_result.expect("ok"),
        )
        .expect("resp")
    else {
        panic!("expected array");
    };
    assert!(mid_items.iter().any(|i| i.label == "greet"));
    assert!(
        !mid_items
            .iter()
            .any(|i| i.label == "hiddenMethod" || i.label == "secret")
    );

    // P2-6: Explicit generic call `identity<Int>(42)` gives signature help for `identity`.
    let mut gen_sig = position_of(app_src, "identity<Int>(42)", 0);
    gen_sig.character += "identity<Int>(".len() as u32;
    send_request(
        &client,
        RequestId::from(305_i32),
        SignatureHelpRequest::METHOD,
        serde_json::to_value(SignatureHelpParams {
            text_document_position_params: doc_pos(&app_uri, gen_sig),
            work_done_progress_params: Default::default(),
            context: None,
        })
        .expect("generic sig"),
    );
    let gen_help: Option<SignatureHelp> =
        serde_json::from_value(receive_response(&client).response_result.expect("ok"))
            .expect("resp");
    assert!(
        gen_help.expect("generic signature help").signatures[0]
            .label
            .contains("fun identity(item: T): T")
    );

    // P2-8: Zero-field constructor `Empty()` gives signature help `Empty()`.
    let mut empty_sig = position_of(app_src, "Empty()", 0);
    empty_sig.character += "Empty(".len() as u32;
    send_request(
        &client,
        RequestId::from(306_i32),
        SignatureHelpRequest::METHOD,
        serde_json::to_value(SignatureHelpParams {
            text_document_position_params: doc_pos(&app_uri, empty_sig),
            work_done_progress_params: Default::default(),
            context: None,
        })
        .expect("empty ctor sig"),
    );
    let empty_help: Option<SignatureHelp> =
        serde_json::from_value(receive_response(&client).response_result.expect("ok"))
            .expect("resp");
    assert_eq!(
        empty_help.expect("empty ctor signature").signatures[0].label,
        "Empty()"
    );

    // P2-9: Named argument `combine(right = 2, left = 1)` highlights `left` (index 0) at `left = 1`.
    let left_arg_pos = position_of(app_src, "left = 1", 0);
    send_request(
        &client,
        RequestId::from(307_i32),
        SignatureHelpRequest::METHOD,
        serde_json::to_value(SignatureHelpParams {
            text_document_position_params: doc_pos(&app_uri, left_arg_pos),
            work_done_progress_params: Default::default(),
            context: None,
        })
        .expect("named arg sig"),
    );
    let named_help: Option<SignatureHelp> =
        serde_json::from_value(receive_response(&client).response_result.expect("ok"))
            .expect("resp");
    assert_eq!(
        named_help.expect("named arg signature").active_parameter,
        Some(0),
        "reordered named argument `left = 1` must highlight parameter 0 (`left`), not 1"
    );

    // P1: REAL incomplete dot-trigger edit (`val x = g.` and `val v = MyGreeter.`) with parse error!
    let incomplete_src = "\
package app
import a.shared.Greeter as MyGreeter

fun testDot() {
    val g = MyGreeter(\"ok\", 1)
    val x = g.
}
";
    send_notification(
        &client,
        DidOpenTextDocument::METHOD,
        serde_json::to_value(DidOpenTextDocumentParams {
            text_document: TextDocumentItem::new(
                app_uri.clone(),
                "koven".to_owned(),
                1,
                incomplete_src.to_owned(),
            ),
        })
        .expect("open incomplete"),
    );
    let _ = receive_diagnostics(&client);
    let _ = receive_diagnostics(&client);
    let _ = receive_diagnostics(&client);

    let mut incomplete_dot = position_of(incomplete_src, "g.", 0);
    incomplete_dot.character += 2;
    send_request(
        &client,
        RequestId::from(308_i32),
        Completion::METHOD,
        serde_json::to_value(CompletionParams {
            text_document_position: doc_pos(&app_uri, incomplete_dot),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
            context: None,
        })
        .expect("incomplete dot comp"),
    );
    let Some(CompletionResponse::Array(incomplete_items)) =
        serde_json::from_value::<Option<CompletionResponse>>(
            receive_response(&client).response_result.expect("ok"),
        )
        .expect("resp")
    else {
        panic!("expected array");
    };
    assert!(
        incomplete_items.iter().any(|i| i.label == "label")
            && incomplete_items.iter().any(|i| i.label == "greet"),
        "incomplete `g.` must recover receiver type and suggest fields/methods, got: {incomplete_items:?}"
    );
    assert!(
        !incomplete_items
            .iter()
            .any(|i| i.label == "secret" || i.label == "hiddenMethod"),
        "incomplete `g.` must still exclude private members"
    );

    // Companion constant & method on incomplete `MyGreeter.`
    let companion_src = "\
package app
import a.shared.Greeter as MyGreeter

fun testCompanion() {
    val v = MyGreeter.
}
";
    send_notification(
        &client,
        DidChangeTextDocument::METHOD,
        serde_json::to_value(DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier::new(app_uri.clone(), 2),
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: companion_src.to_owned(),
            }],
        })
        .expect("change companion"),
    );
    let _ = receive_diagnostics(&client);
    let _ = receive_diagnostics(&client);
    let _ = receive_diagnostics(&client);

    let mut comp_dot = position_of(companion_src, "MyGreeter.", 0);
    comp_dot.character += "MyGreeter.".len() as u32;
    send_request(
        &client,
        RequestId::from(309_i32),
        Completion::METHOD,
        serde_json::to_value(CompletionParams {
            text_document_position: doc_pos(&app_uri, comp_dot),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
            context: None,
        })
        .expect("companion dot comp"),
    );
    let Some(CompletionResponse::Array(comp_items)) =
        serde_json::from_value::<Option<CompletionResponse>>(
            receive_response(&client).response_result.expect("ok"),
        )
        .expect("resp")
    else {
        panic!("expected array");
    };
    assert!(
        comp_items.iter().any(|i| i.label == "VERSION")
            && comp_items.iter().any(|i| i.label == "create"),
        "incomplete `MyGreeter.` must suggest companion constant & method, got: {comp_items:?}"
    );
    assert!(!comp_items.iter().any(|i| i.label == "secretFactory"));

    shutdown(&client);
    server_thread
        .join()
        .expect("server thread")
        .expect("server result");
}
