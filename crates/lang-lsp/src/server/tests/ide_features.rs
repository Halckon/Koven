use super::*;
use lsp_types::{
    DocumentChanges, Hover, HoverContents, HoverParams, InlayHint, InlayHintLabel, InlayHintParams,
    Location, PrepareRenameResponse, Range, ReferenceContext, ReferenceParams, RenameParams,
    TextDocumentEdit, TextDocumentPositionParams, WorkspaceEdit,
    request::{HoverRequest, InlayHintRequest, PrepareRenameRequest, References, Rename},
};

fn doc_edits(edit: Option<WorkspaceEdit>) -> Vec<TextDocumentEdit> {
    match edit
        .expect("workspace edit")
        .document_changes
        .expect("document_changes")
    {
        DocumentChanges::Edits(edits) => edits,
        DocumentChanges::Operations(_) => panic!("expected DocumentChanges::Edits"),
    }
}

fn edits_for_uri<'a>(edits: &'a [TextDocumentEdit], uri: &lsp_types::Uri) -> &'a TextDocumentEdit {
    edits
        .iter()
        .find(|entry| &entry.text_document.uri == uri)
        .expect("expected edits for uri")
}

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
    let edits = doc_edits(edit);
    assert_eq!(edits.len(), 2, "edits across math.ko and main.ko");
    assert_eq!(edits_for_uri(&edits, &math_uri).edits.len(), 1);
    assert_eq!(edits_for_uri(&edits, &main_uri).edits.len(), 1);
    assert_eq!(edits_for_uri(&edits, &math_uri).text_document.version, None);

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
    let edits = doc_edits(edit);
    let uri_edits = edits_for_uri(&edits, &uri);
    assert_eq!(uri_edits.edits.len(), 2);
    assert_eq!(uri_edits.text_document.version, Some(1));

    shutdown(&client);
    server_thread
        .join()
        .expect("server thread")
        .expect("server result");
}

#[test]
fn rename_rejects_shadowing_shared_overloads_aliases_and_reserved_words_and_updates_named_args() {
    let (server, client) = Connection::memory();
    let server_thread = thread::spawn(|| run(server));
    initialize(&client);
    let uri: lsp_types::Uri = "file:///rename_safety.ko".parse().expect("uri");
    let source = "\
fun twice(
    value: Int
): Int = value + value

fun demo() {
    val x = 1
    val explicit /* comment */: Int = 2
    val res = twice(value = x)
    if (res > 0) {
        val y = 2
        val z = x + y + explicit
    }
}
";
    send_notification(
        &client,
        DidOpenTextDocument::METHOD,
        serde_json::to_value(DidOpenTextDocumentParams {
            text_document: TextDocumentItem::new(
                uri.clone(),
                "koven".to_owned(),
                3,
                source.to_owned(),
            ),
        })
        .expect("open params"),
    );
    let open_diags = receive_diagnostics(&client);
    assert!(
        open_diags.diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        open_diags.diagnostics
    );

    // P1-1: Renaming outer `x` -> `y` would be captured by inner `val y = 2` -> must reject.
    let shadow_id = RequestId::from(40_i32);
    send_request(
        &client,
        shadow_id.clone(),
        Rename::METHOD,
        serde_json::to_value(RenameParams {
            text_document_position: doc_pos(&uri, position_of(source, "x", 0)),
            new_name: "y".to_owned(),
            work_done_progress_params: Default::default(),
        })
        .expect("shadow rename"),
    );
    let shadow_resp = receive_response(&client);
    assert_eq!(shadow_resp.id, shadow_id);
    assert_eq!(
        shadow_resp
            .response_result
            .expect_err("binding conflict")
            .code,
        ErrorCode::InvalidParams as i32
    );

    // P2-2: Lexer-consistent identifier validation: reject `companion` & `async`, accept `loop`.
    for bad in ["companion", "async"] {
        let req_id = RequestId::from(format!("bad-{bad}"));
        send_request(
            &client,
            req_id.clone(),
            Rename::METHOD,
            serde_json::to_value(RenameParams {
                text_document_position: doc_pos(&uri, position_of(source, "x", 0)),
                new_name: bad.to_owned(),
                work_done_progress_params: Default::default(),
            })
            .expect("bad ident"),
        );
        let resp = receive_response(&client);
        assert_eq!(resp.id, req_id);
        assert_eq!(
            resp.response_result.expect_err("reserved word").code,
            ErrorCode::InvalidParams as i32
        );
    }
    let loop_edit: Option<WorkspaceEdit> = request_typed!(
        &client,
        41,
        Rename::METHOD,
        serde_json::to_value(RenameParams {
            text_document_position: doc_pos(&uri, position_of(source, "x", 0)),
            new_name: "loop".to_owned(),
            work_done_progress_params: Default::default(),
        })
        .expect("loop rename"),
    );
    let loop_edits = doc_edits(loop_edit);
    assert_eq!(edits_for_uri(&loop_edits, &uri).edits.len(), 3);
    assert_eq!(
        edits_for_uri(&loop_edits, &uri).text_document.version,
        Some(3)
    );

    // P2-1: Parameter rename updates declaration, body uses, AND named argument `value = x`.
    let param_edit: Option<WorkspaceEdit> = request_typed!(
        &client,
        42,
        Rename::METHOD,
        serde_json::to_value(RenameParams {
            text_document_position: doc_pos(&uri, position_of(source, "value", 0)),
            new_name: "amount".to_owned(),
            work_done_progress_params: Default::default(),
        })
        .expect("param rename"),
    );
    let param_edits = doc_edits(param_edit);
    assert_eq!(
        edits_for_uri(&param_edits, &uri).edits.len(),
        4,
        "param decl + 2 body uses + named argument label"
    );

    // P2-3: `val explicit /* comment */: Int = 2` must NOT get a duplicate inlay hint.
    let hints: Option<Vec<InlayHint>> = request_typed!(
        &client,
        43,
        InlayHintRequest::METHOD,
        serde_json::to_value(InlayHintParams {
            text_document: TextDocumentIdentifier::new(uri.clone()),
            range: Range::new(Position::new(0, 0), Position::new(20, 0)),
            work_done_progress_params: Default::default(),
        })
        .expect("inlay params"),
    );
    let hints = hints.expect("hints");
    // Only `x`, `res`, `y`, `z` (4 bindings); `explicit` has a type annotation after a comment.
    assert_eq!(
        hints.len(),
        4,
        "comment-separated explicit type must be skipped"
    );

    // P2-4: Hover on multi-line `fun twice(...)` shows complete signature without `= value + value`.
    let hover: Option<Hover> = request_typed!(
        &client,
        44,
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
    assert!(markup.value.contains("value: Int"), "{}", markup.value);
    assert!(markup.value.contains("): Int"), "{}", markup.value);
    assert!(!markup.value.contains("value + value"), "{}", markup.value);

    shutdown(&client);
    server_thread
        .join()
        .expect("server thread")
        .expect("server result");
}

#[test]
fn source_set_rename_protects_shared_overload_imports_and_explicit_aliases() {
    let (server, client) = Connection::memory();
    let server_thread = thread::spawn(|| run(server));
    let lib_uri: lsp_types::Uri = "file:///workspace/lib/api.ko".parse().expect("uri");
    let app_uri: lsp_types::Uri = "file:///workspace/app/use.ko".parse().expect("uri");
    let lib_src = "\
package lib

public fun pick(x: Int): Int = x
public fun pick(s: String): Int = 1
public fun exactValue(): Int = 42
";
    let app_src = "\
package app
import lib.pick
import lib.exactValue as Alias

fun run(): Int {
    val a = pick(1)
    val b = pick(\"hi\")
    return a + b + Alias()
}
";
    initialize_with_options(
        &client,
        Some(serde_json::json!({"koven": {"sourceSet": {
            "schema": "koven.lsp.source-set",
            "version": 1,
            "roots": ["src"],
            "sources": [
                {"root": "src", "logicalPath": "lib/api.ko", "uri": lib_uri.as_str(), "text": lib_src},
                {"root": "src", "logicalPath": "app/use.ko", "uri": app_uri.as_str(), "text": app_src}
            ]
        }}})),
    );
    let _ = receive_diagnostics(&client);
    let _ = receive_diagnostics(&client);

    // P1-3a: Renaming `pick(Int)` via its call site must be rejected because `import lib.pick` is shared.
    let shared_id = RequestId::from(50_i32);
    send_request(
        &client,
        shared_id.clone(),
        Rename::METHOD,
        serde_json::to_value(RenameParams {
            text_document_position: doc_pos(&app_uri, position_of(app_src, "pick", 1)),
            new_name: "pickInt".to_owned(),
            work_done_progress_params: Default::default(),
        })
        .expect("shared overload rename"),
    );
    let shared_resp = receive_response(&client);
    assert_eq!(shared_resp.id, shared_id);
    assert_eq!(
        shared_resp
            .response_result
            .expect_err("shared overload import must reject")
            .code,
        ErrorCode::InvalidParams as i32
    );

    // P1-3b: Renaming `exactValue` updates declaration + import target, leaving `Alias` untouched.
    let alias_safe_edit: Option<WorkspaceEdit> = request_typed!(
        &client,
        51,
        Rename::METHOD,
        serde_json::to_value(RenameParams {
            text_document_position: doc_pos(&lib_uri, position_of(lib_src, "exactValue", 0)),
            new_name: "newExactValue".to_owned(),
            work_done_progress_params: Default::default(),
        })
        .expect("exactValue rename"),
    );
    let edits = doc_edits(alias_safe_edit);
    assert_eq!(edits_for_uri(&edits, &lib_uri).edits.len(), 1);
    assert_eq!(
        edits_for_uri(&edits, &app_uri).edits.len(),
        1,
        "only `exactValue` in import, NOT `Alias()`"
    );

    // P1-3c: Initiating rename from `Alias()` must be rejected.
    let alias_id = RequestId::from(52_i32);
    send_request(
        &client,
        alias_id.clone(),
        Rename::METHOD,
        serde_json::to_value(RenameParams {
            text_document_position: doc_pos(&app_uri, position_of(app_src, "Alias", 1)),
            new_name: "Other".to_owned(),
            work_done_progress_params: Default::default(),
        })
        .expect("alias rename"),
    );
    let alias_resp = receive_response(&client);
    assert_eq!(alias_resp.id, alias_id);
    assert_eq!(
        alias_resp
            .response_result
            .expect_err("alias rename must reject")
            .code,
        ErrorCode::InvalidParams as i32
    );

    shutdown(&client);
    server_thread
        .join()
        .expect("server thread")
        .expect("server result");
}

#[test]
fn workspace_manifest_rename_rejects_stale_closed_disk_file() {
    let dir = std::env::temp_dir().join(format!("koven_lsp_stale_rename_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src/demo")).expect("mkdir");
    let dir = dir.canonicalize().expect("canonicalize temp dir");
    std::fs::write(
        dir.join("project.toml"),
        "schema = \"koven.project\"\nversion = 1\n\n[project]\nname = \"demo\"\nsource-roots = [\"src\"]\n",
    )
    .expect("project.toml");
    let math_path = dir.join("src/demo/math.ko");
    let main_path = dir.join("src/demo/main.ko");
    let math_src = "package demo\npublic fun scale(x: Int): Int = x * 2\n";
    let main_src = "package demo\nfun main() { val r = scale(3) }\n";
    std::fs::write(&math_path, math_src).expect("math.ko");
    std::fs::write(&main_path, main_src).expect("main.ko");

    let root_uri: lsp_types::Uri = format!("file://{}", dir.display())
        .parse()
        .expect("root uri");
    let main_uri: lsp_types::Uri = format!("file://{}", main_path.display())
        .parse()
        .expect("main uri");

    let (server, client) = Connection::memory();
    let server_thread = thread::spawn(|| {
        crate::server::run_with_options(
            server,
            crate::server::ServerOptions {
                workspace_manifest: true,
                diagnostic_context: false,
            },
        )
    });
    let mut init_params = serde_json::to_value(InitializeParams {
        workspace_folders: Some(vec![lsp_types::WorkspaceFolder {
            uri: root_uri,
            name: "demo".to_owned(),
        }]),
        ..InitializeParams::default()
    })
    .expect("init params");
    init_params["capabilities"] = serde_json::json!({});
    send_request(
        &client,
        RequestId::from(1_i32),
        Initialize::METHOD,
        init_params,
    );
    let _ = receive_response(&client);
    send_notification(
        &client,
        Initialized::METHOD,
        serde_json::to_value(InitializedParams {}).expect("initialized"),
    );
    let _ = receive_diagnostics(&client);
    let _ = receive_diagnostics(&client);

    // Mutate closed `math.ko` on disk (insert a line above `scale`) without opening/updating it.
    std::fs::write(&math_path, format!("\n{math_src}")).expect("mutate math.ko");

    let stale_id = RequestId::from(60_i32);
    send_request(
        &client,
        stale_id.clone(),
        Rename::METHOD,
        serde_json::to_value(RenameParams {
            text_document_position: doc_pos(&main_uri, position_of(main_src, "scale", 0)),
            new_name: "rescale".to_owned(),
            work_done_progress_params: Default::default(),
        })
        .expect("stale rename"),
    );
    let stale_resp = receive_response(&client);
    assert_eq!(stale_resp.id, stale_id);
    assert_eq!(
        stale_resp
            .response_result
            .expect_err("stale closed file must reject")
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
