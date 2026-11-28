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

fn completion_labels(
    client: &Connection,
    id: i32,
    uri: &lsp_types::Uri,
    pos: Position,
) -> Vec<String> {
    send_request(
        client,
        RequestId::from(id),
        Completion::METHOD,
        serde_json::to_value(CompletionParams {
            text_document_position: doc_pos(uri, pos),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
            context: None,
        })
        .expect("comp"),
    );
    let Some(CompletionResponse::Array(items)) =
        serde_json::from_value::<Option<CompletionResponse>>(
            receive_response(client).response_result.expect("comp ok"),
        )
        .expect("comp resp")
    else {
        panic!("expected CompletionResponse::Array");
    };
    items.into_iter().map(|i| i.label).collect()
}

fn completion_items(
    client: &Connection,
    id: i32,
    uri: &lsp_types::Uri,
    pos: Position,
) -> Vec<lsp_types::CompletionItem> {
    send_request(
        client,
        RequestId::from(id),
        Completion::METHOD,
        serde_json::to_value(CompletionParams {
            text_document_position: doc_pos(uri, pos),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
            context: None,
        })
        .expect("comp"),
    );
    let Some(CompletionResponse::Array(items)) =
        serde_json::from_value::<Option<CompletionResponse>>(
            receive_response(client).response_result.expect("comp ok"),
        )
        .expect("comp resp")
    else {
        panic!("expected CompletionResponse::Array");
    };
    items
}

fn sig_help(
    client: &Connection,
    id: i32,
    uri: &lsp_types::Uri,
    pos: Position,
) -> Option<SignatureHelp> {
    send_request(
        client,
        RequestId::from(id),
        SignatureHelpRequest::METHOD,
        serde_json::to_value(SignatureHelpParams {
            text_document_position_params: doc_pos(uri, pos),
            work_done_progress_params: Default::default(),
            context: None,
        })
        .expect("sig"),
    );
    serde_json::from_value(receive_response(client).response_result.expect("sig ok"))
        .expect("sig resp")
}

/// P1: Project-mode unclosed calls (`combine(`, `combine(1,`, `p.scale(`) still return signatureHelp.
/// P2-6: Commas inside generic type arguments (`build<Int, String>(1, "x")`) don't skew active_parameter.
#[test]
fn signature_help_recovers_unclosed_calls_and_handles_generic_nested_commas() {
    let (server, client) = Connection::memory();
    let server_thread = thread::spawn(|| run(server));
    let lib_uri: lsp_types::Uri = "file:///r2/demo/lib.ko".parse().expect("uri");
    let main_uri: lsp_types::Uri = "file:///r2/demo/main.ko".parse().expect("uri");
    let lib_src = "\
package demo

public class PairBox(val first: Int, val second: String) {
    public fun scale(factor: Int, offset: Int): Int = first * factor + offset
}

public fun combine(left: Int, right: Int): Int = left + right
public fun <A, B> build(a: A, b: B): Int = 0
public fun outer(first: Int, second: Int): Int = first + second
";
    let main_src = "\
package demo

fun main() {
    val x = outer(build<Int, String>(1, \"x\"), 42)
}
";
    initialize_with_options(
        &client,
        Some(serde_json::json!({"koven": {"sourceSet": {
            "schema": "koven.lsp.source-set",
            "version": 1,
            "roots": ["src"],
            "sources": [
                {"root": "src", "logicalPath": "demo/lib.ko", "uri": lib_uri.as_str(), "text": lib_src},
                {"root": "src", "logicalPath": "demo/main.ko", "uri": main_uri.as_str(), "text": main_src}
            ]
        }}})),
    );
    let _ = receive_diagnostics(&client);
    let _ = receive_diagnostics(&client);

    // P2-6: cursor on `42` in `outer(build<Int, String>(1, "x"), 42)` -> active_parameter = 1 (not 2!).
    let second_arg_pos = position_of(main_src, "42", 0);
    let sh = sig_help(&client, 400, &main_uri, second_arg_pos).expect("outer signature help");
    assert!(
        sh.signatures[0]
            .label
            .contains("fun outer(first: Int, second: Int): Int")
    );
    assert_eq!(
        sh.active_parameter,
        Some(1),
        "generic `<Int, String>` comma must not count"
    );

    // Open main.ko and type unclosed `combine(` -> must still return signatureHelp!
    let unclosed_open = "\
package demo

fun main() {
    val x = combine(
}
";
    send_notification(
        &client,
        DidOpenTextDocument::METHOD,
        serde_json::to_value(DidOpenTextDocumentParams {
            text_document: TextDocumentItem::new(
                main_uri.clone(),
                "koven".to_owned(),
                1,
                unclosed_open.to_owned(),
            ),
        })
        .expect("didOpen"),
    );
    let _ = receive_diagnostics(&client);
    let _ = receive_diagnostics(&client);

    let after_open = Position::new(3, "    val x = combine(".len() as u32);
    let sh_open = sig_help(&client, 401, &main_uri, after_open)
        .expect("unclosed `combine(` must still return signature help");
    assert!(
        sh_open.signatures[0]
            .label
            .contains("fun combine(left: Int, right: Int): Int")
    );
    assert_eq!(sh_open.active_parameter, Some(0));

    // Type `combine(1,` -> active_parameter = 1!
    let unclosed_comma = "\
package demo

fun main() {
    val x = combine(1,
}
";
    send_notification(
        &client,
        DidChangeTextDocument::METHOD,
        serde_json::to_value(DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier::new(main_uri.clone(), 2),
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: unclosed_comma.to_owned(),
            }],
        })
        .expect("didChange"),
    );
    let _ = receive_diagnostics(&client);
    let _ = receive_diagnostics(&client);

    let after_comma = Position::new(3, "    val x = combine(1,".len() as u32);
    let sh_comma = sig_help(&client, 402, &main_uri, after_comma)
        .expect("unclosed `combine(1,` must still return signature help");
    assert!(
        sh_comma.signatures[0]
            .label
            .contains("fun combine(left: Int, right: Int): Int")
    );
    assert_eq!(sh_comma.active_parameter, Some(1));

    // Type unclosed method call `p.scale(` -> must still return instance method signature!
    let unclosed_method = "\
package demo

fun main() {
    val p = PairBox(1, \"a\")
    val x = p.scale(
}
";
    send_notification(
        &client,
        DidChangeTextDocument::METHOD,
        serde_json::to_value(DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier::new(main_uri.clone(), 3),
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: unclosed_method.to_owned(),
            }],
        })
        .expect("didChange"),
    );
    let _ = receive_diagnostics(&client);
    let _ = receive_diagnostics(&client);

    let after_method = Position::new(4, "    val x = p.scale(".len() as u32);
    let sh_method = sig_help(&client, 403, &main_uri, after_method)
        .expect("unclosed `p.scale(` must still return method signature");
    assert!(
        sh_method.signatures[0]
            .label
            .contains("fun scale(factor: Int, offset: Int): Int")
    );
    assert_eq!(sh_method.active_parameter, Some(0));

    shutdown(&client);
    server_thread
        .join()
        .expect("server thread")
        .expect("clean exit");
}

/// P2-1: Chained `obj.field.`, call-result `factory().`, and qualified-type `lib.MyGreeter.` dot completion.
/// P2-2: Dot recovery works even when another file in the compilation unit uses bare `Display`.
#[test]
fn dot_recovery_supports_chains_calls_qualified_types_and_embedded_display() {
    let (server, client) = Connection::memory();
    let server_thread = thread::spawn(|| run(server));
    let lib_uri: lsp_types::Uri = "file:///r2chain/lib/types.ko".parse().expect("uri");
    let disp_uri: lsp_types::Uri = "file:///r2chain/lib/disp.ko".parse().expect("uri");
    let app_uri: lsp_types::Uri = "file:///r2chain/app/main.ko".parse().expect("uri");
    let lib_src = "\
package lib

public class Leaf(val leafVal: Int) {
    public fun leafPing(): Int = leafVal
}

public class Wrapper(val leaf: Leaf)

public fun makeLeaf(): Leaf = Leaf(7)

public class MyGreeter(val name: String) {
    companion {
        public fun create(): MyGreeter = MyGreeter(\"hi\")
    }
}
";
    let disp_src = "\
package lib

public class Tag(val id: Int) : Display {
    public override fun display(): String = \"tag\"
}
";
    let app_src = "\
package app

import lib.Wrapper
import lib.Leaf
import lib.makeLeaf

fun testChain(w: Wrapper) {
    val a = w.leaf.
}
";
    initialize_with_options(
        &client,
        Some(serde_json::json!({"koven": {"sourceSet": {
            "schema": "koven.lsp.source-set",
            "version": 1,
            "roots": ["src"],
            "sources": [
                {"root": "src", "logicalPath": "lib/types.ko", "uri": lib_uri.as_str(), "text": lib_src},
                {"root": "src", "logicalPath": "lib/disp.ko", "uri": disp_uri.as_str(), "text": disp_src},
                {"root": "src", "logicalPath": "app/main.ko", "uri": app_uri.as_str(), "text": app_src}
            ]
        }}})),
    );
    let _ = receive_diagnostics(&client);
    let _ = receive_diagnostics(&client);
    let _ = receive_diagnostics(&client);

    // 1. Chained `w.leaf.` with embedded `Display` present in `lib/disp.ko`.
    let chain_pos = Position::new(7, "    val a = w.leaf.".len() as u32);
    let chain_labels = completion_labels(&client, 410, &app_uri, chain_pos);
    assert!(
        chain_labels.iter().any(|l| l == "leafVal"),
        "chain: {chain_labels:?}"
    );
    assert!(
        chain_labels.iter().any(|l| l == "leafPing"),
        "chain: {chain_labels:?}"
    );

    // 2. Call-result `makeLeaf().`
    let call_src = "\
package app

import lib.makeLeaf

fun testCall() {
    val a = makeLeaf().
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
                call_src.to_owned(),
            ),
        })
        .expect("open"),
    );
    let _ = receive_diagnostics(&client);
    let _ = receive_diagnostics(&client);
    let _ = receive_diagnostics(&client);

    let call_pos = Position::new(5, "    val a = makeLeaf().".len() as u32);
    let call_labels = completion_labels(&client, 411, &app_uri, call_pos);
    assert!(
        call_labels.iter().any(|l| l == "leafVal"),
        "call: {call_labels:?}"
    );
    assert!(
        call_labels.iter().any(|l| l == "leafPing"),
        "call: {call_labels:?}"
    );

    // 3. Qualified type `lib.MyGreeter.`
    let qual_src = "\
package app

fun testQual() {
    val a = lib.MyGreeter.
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
                text: qual_src.to_owned(),
            }],
        })
        .expect("change"),
    );
    let _ = receive_diagnostics(&client);
    let _ = receive_diagnostics(&client);
    let _ = receive_diagnostics(&client);

    let qual_pos = Position::new(3, "    val a = lib.MyGreeter.".len() as u32);
    let qual_labels = completion_labels(&client, 412, &app_uri, qual_pos);
    assert!(
        qual_labels.iter().any(|l| l == "create"),
        "qual: {qual_labels:?}"
    );

    shutdown(&client);
    server_thread
        .join()
        .expect("server thread")
        .expect("clean exit");
}

/// P2-3: Private fields/methods/companion methods/companion constants only visible inside declaring type.
/// P2-4: Default-package single-segment import `import DefaultItem as AliasedItem`.
/// P2-5: Self-initializer `val shadow = shadow` suggests outer `shadow`, not itself.
/// P2-9: Builtin text primitives (`textByteLength`, etc.) appear in scope completion.
#[test]
fn completion_enforces_enclosing_type_privacy_default_imports_self_init_and_text_builtins() {
    let (server, client) = Connection::memory();
    let server_thread = thread::spawn(|| run(server));
    let def_uri: lsp_types::Uri = "file:///r2vis/def.ko".parse().expect("uri");
    let app_uri: lsp_types::Uri = "file:///r2vis/app/main.ko".parse().expect("uri");
    let def_src = "\
public class DefaultItem(val v: Int)
";
    let app_src = "\
package app

import DefaultItem as AliasedItem

public class Vault(public val pubField: Int, private val privField: Int) {
    public fun pubMethod(): Int = privMethod()
    private fun privMethod(): Int = privField
    public fun insideAccess(other: Vault): Int {
        val a = other.pubField
        val b = Vault.PUB_CONST
        return a + b
    }
    companion {
        public const val PUB_CONST: Int = 1
        private const val PRIV_CONST: Int = 2
        public fun pubFactory(): Vault = Vault(1, 2)
        private fun privFactory(): Vault = Vault(3, 4)
    }
}

fun outsideAccess(v: Vault) {
    val a = v.pubField
    val b = Vault.PUB_CONST
    val shadow: String = \"outer\"
    if (true) {
        val shadow = shadow
    }
}
";
    initialize_with_options(
        &client,
        Some(serde_json::json!({"koven": {"sourceSet": {
            "schema": "koven.lsp.source-set",
            "version": 1,
            "roots": ["src"],
            "sources": [
                {"root": "src", "logicalPath": "def.ko", "uri": def_uri.as_str(), "text": def_src},
                {"root": "src", "logicalPath": "app/main.ko", "uri": app_uri.as_str(), "text": app_src}
            ]
        }}})),
    );
    let d1 = receive_diagnostics(&client);
    let d2 = receive_diagnostics(&client);
    assert!(d1.diagnostics.is_empty(), "d1: {:?}", d1.diagnostics);
    assert!(d2.diagnostics.is_empty(), "d2: {:?}", d2.diagnostics);

    // P2-3 (outside declaring class in SAME file): must NOT see privField / privMethod / PRIV_CONST / privFactory.
    let outside_inst = position_of(app_src, "pubField", 2);
    let out_inst_labels = completion_labels(&client, 420, &app_uri, outside_inst);
    assert!(out_inst_labels.iter().any(|l| l == "pubField"));
    assert!(out_inst_labels.iter().any(|l| l == "pubMethod"));
    assert!(
        !out_inst_labels.iter().any(|l| l == "privField"),
        "must hide private field outside class: {out_inst_labels:?}"
    );
    assert!(
        !out_inst_labels.iter().any(|l| l == "privMethod"),
        "must hide private method outside class: {out_inst_labels:?}"
    );

    let outside_comp = position_of(app_src, "PUB_CONST", 2);
    let out_comp_labels = completion_labels(&client, 421, &app_uri, outside_comp);
    assert!(out_comp_labels.iter().any(|l| l == "PUB_CONST"));
    assert!(out_comp_labels.iter().any(|l| l == "pubFactory"));
    assert!(
        !out_comp_labels.iter().any(|l| l == "PRIV_CONST"),
        "must hide private companion const: {out_comp_labels:?}"
    );
    assert!(
        !out_comp_labels.iter().any(|l| l == "privFactory"),
        "must hide private companion method: {out_comp_labels:?}"
    );

    // P2-3 (inside declaring class): MUST see private fields, methods, companion methods & constants.
    let inside_inst = position_of(app_src, "pubField", 1);
    let in_inst_labels = completion_labels(&client, 422, &app_uri, inside_inst);
    assert!(
        in_inst_labels.iter().any(|l| l == "privField"),
        "inside class should see privField: {in_inst_labels:?}"
    );
    assert!(
        in_inst_labels.iter().any(|l| l == "privMethod"),
        "inside class should see privMethod: {in_inst_labels:?}"
    );

    let inside_comp = position_of(app_src, "PUB_CONST", 0);
    let in_comp_labels = completion_labels(&client, 423, &app_uri, inside_comp);
    assert!(
        in_comp_labels.iter().any(|l| l == "PRIV_CONST"),
        "inside class should see PRIV_CONST: {in_comp_labels:?}"
    );
    assert!(
        in_comp_labels.iter().any(|l| l == "privFactory"),
        "inside class should see privFactory: {in_comp_labels:?}"
    );

    // P2-5: Self-initializer `val shadow = shadow` -> RHS `shadow` must resolve to outer `String`, not inner self.
    let rhs_shadow = position_of(app_src, "shadow", 2);
    let shadow_items = completion_items(&client, 424, &app_uri, rhs_shadow);
    let shadow_item = shadow_items
        .iter()
        .find(|i| i.label == "shadow")
        .expect("outer shadow should be suggested in initializer RHS");
    assert_eq!(shadow_item.detail.as_deref(), Some("String"));

    // P2-4 & P2-9: Default-package import alias `AliasedItem` and text builtins in scope completion.
    let scope_pos = Position::new(20, 4);
    let scope_labels = completion_labels(&client, 425, &app_uri, scope_pos);
    assert!(
        scope_labels.iter().any(|l| l == "AliasedItem"),
        "default-pkg import alias missing: {scope_labels:?}"
    );
    for builtin in [
        "textByteLength",
        "textByteAt",
        "textUtf8ErrorOffset",
        "textCopyBytes",
        "textCopySlice",
    ] {
        assert!(
            scope_labels.iter().any(|l| l == builtin),
            "missing builtin {builtin}"
        );
    }

    shutdown(&client);
    server_thread
        .join()
        .expect("server thread")
        .expect("clean exit");
}

/// P2-7: `documentSymbol` and `workspace/symbol` keep class members and enum variants in healthy files
/// even when another file in the compilation unit has a syntax/name error (`typed() == None`).
/// P2-8: Clients without `hierarchicalDocumentSymbolSupport` receive `DocumentSymbolResponse::Flat`.
#[test]
fn symbols_fallback_to_ast_on_unit_error_and_respect_flat_capability() {
    let (server, client) = Connection::memory();
    let server_thread = thread::spawn(|| run(server));
    let healthy_uri: lsp_types::Uri = "file:///r2sym/demo/healthy.ko".parse().expect("uri");
    let broken_uri: lsp_types::Uri = "file:///r2sym/demo/broken.ko".parse().expect("uri");
    let healthy_src = "\
package demo

public class Widget(val id: Int) {
    public fun render(): Int = id
}

public enum class Mode {
    Fast,
    Safe
}
";
    let broken_src = "\
package demo

fun broken(): Int = unknownName
";
    // Default client capabilities -> `hierarchical_document_symbol_support` is None (false) -> Flat!
    initialize_with_options(
        &client,
        Some(serde_json::json!({"koven": {"sourceSet": {
            "schema": "koven.lsp.source-set",
            "version": 1,
            "roots": ["src"],
            "sources": [
                {"root": "src", "logicalPath": "demo/healthy.ko", "uri": healthy_uri.as_str(), "text": healthy_src},
                {"root": "src", "logicalPath": "demo/broken.ko", "uri": broken_uri.as_str(), "text": broken_src}
            ]
        }}})),
    );
    let _ = receive_diagnostics(&client);
    let _ = receive_diagnostics(&client);

    // P2-7 + P2-8: documentSymbol on healthy.ko returns Flat list including members & enum variants!
    send_request(
        &client,
        RequestId::from(430_i32),
        DocumentSymbolRequest::METHOD,
        serde_json::to_value(DocumentSymbolParams {
            text_document: TextDocumentIdentifier::new(healthy_uri.clone()),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .expect("doc symbols"),
    );
    let Some(DocumentSymbolResponse::Flat(flat)) =
        serde_json::from_value::<Option<DocumentSymbolResponse>>(
            receive_response(&client).response_result.expect("ok"),
        )
        .expect("resp")
    else {
        panic!(
            "expected Flat DocumentSymbolResponse when client does not declare hierarchical support"
        );
    };
    assert!(flat.iter().any(|s| s.name == "Widget"), "flat: {flat:?}");
    assert!(
        flat.iter()
            .any(|s| s.name == "id" && s.container_name.as_deref() == Some("Widget")),
        "AST fallback should keep Widget.id: {flat:?}"
    );
    assert!(
        flat.iter()
            .any(|s| s.name == "render" && s.container_name.as_deref() == Some("Widget")),
        "AST fallback should keep Widget.render: {flat:?}"
    );
    assert!(
        flat.iter()
            .any(|s| s.name == "Fast" && s.container_name.as_deref() == Some("Mode")),
        "AST fallback should keep Mode.Fast: {flat:?}"
    );

    // P2-7: workspace/symbol still finds `render` and `Fast` in healthy.ko!
    send_request(
        &client,
        RequestId::from(431_i32),
        WorkspaceSymbolRequest::METHOD,
        serde_json::to_value(WorkspaceSymbolParams {
            query: "render".to_owned(),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .expect("ws"),
    );
    let Some(WorkspaceSymbolResponse::Flat(ws_render)) =
        serde_json::from_value::<Option<WorkspaceSymbolResponse>>(
            receive_response(&client).response_result.expect("ok"),
        )
        .expect("resp")
    else {
        panic!("expected Flat workspace symbols");
    };
    assert!(
        ws_render.iter().any(|s| s.name == "render"),
        "workspace/symbol should keep healthy file methods when another file has errors: {ws_render:?}"
    );

    send_request(
        &client,
        RequestId::from(432_i32),
        WorkspaceSymbolRequest::METHOD,
        serde_json::to_value(WorkspaceSymbolParams {
            query: "Fast".to_owned(),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .expect("ws"),
    );
    let Some(WorkspaceSymbolResponse::Flat(ws_fast)) =
        serde_json::from_value::<Option<WorkspaceSymbolResponse>>(
            receive_response(&client).response_result.expect("ok"),
        )
        .expect("resp")
    else {
        panic!("expected Flat workspace symbols");
    };
    assert!(
        ws_fast.iter().any(|s| s.name == "Fast"),
        "workspace/symbol should keep healthy file enum cases when another file has errors: {ws_fast:?}"
    );

    shutdown(&client);
    server_thread
        .join()
        .expect("server thread")
        .expect("clean exit");
}
