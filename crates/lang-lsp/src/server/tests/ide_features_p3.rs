use super::*;
use lsp_types::{
    CodeActionContext, CodeActionKind, CodeActionOrCommand, CodeActionParams, CodeActionResponse,
    DocumentHighlight, DocumentHighlightKind, DocumentHighlightParams, FoldingRange,
    FoldingRangeKind, FoldingRangeParams, Range, SemanticTokensParams, SemanticTokensResult,
    TextDocumentPositionParams,
    request::{
        CodeActionRequest, DocumentHighlightRequest, FoldingRangeRequest, GotoTypeDefinition,
        GotoTypeDefinitionParams, SemanticTokensFullRequest,
    },
};

fn code_action_literal_caps() -> lsp_types::ClientCapabilities {
    lsp_types::ClientCapabilities {
        text_document: Some(lsp_types::TextDocumentClientCapabilities {
            code_action: Some(lsp_types::CodeActionClientCapabilities {
                code_action_literal_support: Some(lsp_types::CodeActionLiteralSupport {
                    code_action_kind: lsp_types::CodeActionKindLiteralSupport {
                        value_set: vec![
                            CodeActionKind::QUICKFIX.as_str().to_owned(),
                            CodeActionKind::REFACTOR_REWRITE.as_str().to_owned(),
                        ],
                    },
                }),
                ..Default::default()
            }),
            ..Default::default()
        }),
        ..Default::default()
    }
}

fn doc_pos(uri: &lsp_types::Uri, position: Position) -> TextDocumentPositionParams {
    TextDocumentPositionParams::new(TextDocumentIdentifier::new(uri.clone()), position)
}

fn decode_semantic_tokens(
    tokens: &[lsp_types::SemanticToken],
    text: &str,
) -> Vec<(u32, u32, String, u32, u32)> {
    let lines: Vec<&str> = text.lines().collect();
    let mut line = 0u32;
    let mut col = 0u32;
    let mut out = Vec::with_capacity(tokens.len());
    for tok in tokens {
        line += tok.delta_line;
        col = if tok.delta_line == 0 {
            col + tok.delta_start
        } else {
            tok.delta_start
        };
        let slice = lines
            .get(line as usize)
            .map(|l| {
                let utf16: Vec<u16> = l.encode_utf16().collect();
                let start = (col as usize).min(utf16.len());
                let end = (start + tok.length as usize).min(utf16.len());
                String::from_utf16_lossy(&utf16[start..end])
            })
            .unwrap_or_default();
        out.push((line, col, slice, tok.token_type, tok.token_modifiers_bitset));
    }
    out
}

/// Covers all 5 P3 features end-to-end across a multi-file source-set:
/// 1. `textDocument/semanticTokens/full` (classes, interfaces, enums, methods, properties, mutable vs readonly vars, builtins)
/// 2. `textDocument/documentHighlight` (WRITE on declaration & assignment LHS vs READ on uses)
/// 3. `textDocument/foldingRange` (imports block, block & consecutive line comments, class/function/when regions)
/// 4. `textDocument/typeDefinition` (variable & nullable parameter -> cross-file nominal class declaration)
/// 5. `textDocument/codeAction` (auto-import QuickFix for L0080 + explicit type annotation Refactor)
#[test]
fn p3_semantic_tokens_highlights_folding_type_definition_and_code_actions() {
    let (server, client) = Connection::memory();
    let server_thread = thread::spawn(|| run(server));
    let model_uri: lsp_types::Uri = "file:///p3/model/types.ko".parse().expect("uri");
    let extra_uri: lsp_types::Uri = "file:///p3/extra/helper.ko".parse().expect("uri");
    let app_uri: lsp_types::Uri = "file:///p3/app/main.ko".parse().expect("uri");

    let model_src = "\
package model

public interface Named {
    public fun label(): String
}

public enum class Color {
    Red,
    Blue
}

public class Point(public val x: Int, public var y: Int) {
    public fun norm(): Int = x + y
    companion {
        public fun zero(): Point = Point(0, 0)
    }
}
";
    let extra_src = "\
package extra

public fun computeBonus(v: Int): Int = v + 10
";
    let app_src = "\
package app

import model.Point
import model.Color

// header comment line 1
// header comment line 2
/* block
   comment */
fun main(seed: Point?) {
    var count = 1
    count = count + 2
    val origin = Point.zero()
    val sum = origin.norm() + count
    val c = Color.Red
    when (c) {
        Color.Red -> {
            println(\"red\")
        }
        Color.Blue -> {
            println(\"blue\")
        }
    }
}
";
    initialize_with_capabilities_and_options(
        &client,
        code_action_literal_caps(),
        Some(serde_json::json!({"koven": {"sourceSet": {
            "schema": "koven.lsp.source-set",
            "version": 1,
            "roots": ["src"],
            "sources": [
                {"root": "src", "logicalPath": "model/types.ko", "uri": model_uri.as_str(), "text": model_src},
                {"root": "src", "logicalPath": "extra/helper.ko", "uri": extra_uri.as_str(), "text": extra_src},
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

    // 1. textDocument/semanticTokens/full on model/types.ko & app/main.ko
    send_request(
        &client,
        RequestId::from(600_i32),
        SemanticTokensFullRequest::METHOD,
        serde_json::to_value(SemanticTokensParams {
            text_document: TextDocumentIdentifier::new(model_uri.clone()),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .expect("sem"),
    );
    let Some(SemanticTokensResult::Tokens(model_toks)) =
        serde_json::from_value::<Option<SemanticTokensResult>>(
            receive_response(&client).response_result.expect("ok"),
        )
        .expect("resp")
    else {
        panic!("expected SemanticTokensResult::Tokens");
    };
    let decoded_model = decode_semantic_tokens(&model_toks.data, model_src);
    // Legend: 1=CLASS, 2=INTERFACE, 3=ENUM, 4=ENUM_MEMBER, 7=METHOD, 8=PROPERTY; mods: 1=DECL, 2=RO, 4=MOD, 8=STATIC
    assert!(
        decoded_model
            .iter()
            .any(|(_, _, s, t, m)| s == "Named" && *t == 2 && (*m & 1) != 0)
    );
    assert!(
        decoded_model
            .iter()
            .any(|(_, _, s, t, m)| s == "Color" && *t == 3 && (*m & 1) != 0)
    );
    assert!(
        decoded_model
            .iter()
            .any(|(_, _, s, t, m)| s == "Red" && *t == 4 && (*m & 8) != 0)
    );
    assert!(
        decoded_model
            .iter()
            .any(|(_, _, s, t, m)| s == "Point" && *t == 1 && (*m & 1) != 0)
    );
    assert!(
        decoded_model
            .iter()
            .any(|(_, _, s, t, m)| s == "x" && *t == 8 && (*m & 2) != 0)
    );
    // `y` is mutable `var` field -> NOT readonly (mod 2) and NOT modification (mod 4) at declaration!
    assert!(
        decoded_model
            .iter()
            .any(|(_, _, s, t, m)| s == "y" && *t == 8 && (*m & 2) == 0 && (*m & 4) == 0)
    );
    assert!(
        decoded_model
            .iter()
            .any(|(_, _, s, t, m)| s == "zero" && *t == 7 && (*m & 8) != 0)
    );

    send_request(
        &client,
        RequestId::from(601_i32),
        SemanticTokensFullRequest::METHOD,
        serde_json::to_value(SemanticTokensParams {
            text_document: TextDocumentIdentifier::new(app_uri.clone()),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .expect("sem app"),
    );
    let Some(SemanticTokensResult::Tokens(app_toks)) =
        serde_json::from_value::<Option<SemanticTokensResult>>(
            receive_response(&client).response_result.expect("ok"),
        )
        .expect("resp")
    else {
        panic!("expected app SemanticTokens");
    };
    let decoded_app = decode_semantic_tokens(&app_toks.data, app_src);
    // `count` LHS assignment has MOD_MODIFICATION (4), while `count` reads do NOT have MOD_MODIFICATION!
    let count_toks: Vec<_> = decoded_app
        .iter()
        .filter(|(_, _, s, t, _)| s == "count" && *t == 10)
        .collect();
    assert_eq!(
        count_toks
            .iter()
            .filter(|(_, _, _, _, m)| (*m & 4) != 0)
            .count(),
        1,
        "only the assignment LHS of `count` should carry MOD_MODIFICATION: {count_toks:?}"
    );
    assert!(
        decoded_app
            .iter()
            .any(|(_, _, s, t, m)| s == "origin" && *t == 10 && (*m & 2) != 0)
    );
    assert!(
        decoded_app
            .iter()
            .any(|(_, _, s, t, m)| s == "println" && *t == 6 && (*m & 16) != 0)
    );

    // 2. textDocument/documentHighlight on `count` -> 2 WRITE (decl + LHS) and 2 READ
    let count_pos = position_of(app_src, "count", 0);
    send_request(
        &client,
        RequestId::from(602_i32),
        DocumentHighlightRequest::METHOD,
        serde_json::to_value(DocumentHighlightParams {
            text_document_position_params: doc_pos(&app_uri, count_pos),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .expect("hl"),
    );
    let highlights: Vec<DocumentHighlight> =
        serde_json::from_value(receive_response(&client).response_result.expect("hl ok"))
            .expect("hl resp");
    assert_eq!(
        highlights.len(),
        4,
        "expected 4 occurrences of count: {highlights:?}"
    );
    let writes = highlights
        .iter()
        .filter(|h| h.kind == Some(DocumentHighlightKind::WRITE))
        .count();
    let reads = highlights
        .iter()
        .filter(|h| h.kind == Some(DocumentHighlightKind::READ))
        .count();
    assert_eq!((writes, reads), (2, 2), "highlights: {highlights:?}");

    // 3. textDocument/foldingRange on app/main.ko -> imports, comments, function & when regions
    send_request(
        &client,
        RequestId::from(603_i32),
        FoldingRangeRequest::METHOD,
        serde_json::to_value(FoldingRangeParams {
            text_document: TextDocumentIdentifier::new(app_uri.clone()),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .expect("fold"),
    );
    let folds: Vec<FoldingRange> =
        serde_json::from_value(receive_response(&client).response_result.expect("fold ok"))
            .expect("fold resp");
    assert!(
        folds
            .iter()
            .any(|f| f.kind == Some(FoldingRangeKind::Imports)),
        "expected imports fold: {folds:?}"
    );
    assert!(
        folds
            .iter()
            .filter(|f| f.kind == Some(FoldingRangeKind::Comment))
            .count()
            >= 2,
        "expected both line-comment run and block-comment folds: {folds:?}"
    );
    assert!(
        folds
            .iter()
            .any(|f| f.kind == Some(FoldingRangeKind::Region) && f.start_line == 9),
        "expected fun main fold: {folds:?}"
    );

    // 4. textDocument/typeDefinition on `origin` and nullable `seed` -> jumps to `Point` in model/types.ko
    for (req_id, needle) in [(604_i32, "origin"), (605_i32, "seed")] {
        let pos = position_of(app_src, needle, 0);
        send_request(
            &client,
            RequestId::from(req_id),
            GotoTypeDefinition::METHOD,
            serde_json::to_value(GotoTypeDefinitionParams {
                text_document_position_params: doc_pos(&app_uri, pos),
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            })
            .expect("typedef"),
        );
        let Some(GotoDefinitionResponse::Scalar(loc)) =
            serde_json::from_value::<Option<GotoDefinitionResponse>>(
                receive_response(&client)
                    .response_result
                    .expect("typedef ok"),
            )
            .expect("typedef resp")
        else {
            panic!("expected Scalar typeDefinition for {needle}");
        };
        assert_eq!(loc.uri, model_uri);
        assert_eq!(loc.range.start.line, 11, "Point declaration line");
    }

    // 5a. textDocument/codeAction (Refactor: Add explicit type ': Point' on `val origin = Point.zero()`)
    let origin_pos = position_of(app_src, "origin", 0);
    send_request(
        &client,
        RequestId::from(606_i32),
        CodeActionRequest::METHOD,
        serde_json::to_value(CodeActionParams {
            text_document: TextDocumentIdentifier::new(app_uri.clone()),
            range: Range::new(origin_pos, origin_pos),
            context: CodeActionContext {
                diagnostics: Vec::new(),
                only: Some(vec![CodeActionKind::REFACTOR_REWRITE]),
                trigger_kind: None,
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .expect("codeAction refactor"),
    );
    let refactor_actions: CodeActionResponse =
        serde_json::from_value(receive_response(&client).response_result.expect("ca ok"))
            .expect("ca resp");
    assert!(
        refactor_actions.iter().any(|a| matches!(
            a,
            CodeActionOrCommand::CodeAction(ca)
                if ca.title == "Add explicit type ': Point'"
                    && ca.kind == Some(CodeActionKind::REFACTOR_REWRITE)
        )),
        "expected explicit type action: {refactor_actions:?}"
    );

    // 5b. textDocument/codeAction (QuickFix: Auto-import `extra.computeBonus` for L0080)
    let missing_import_src = "\
package app

import model.Point

fun main() {
    val bonus = computeBonus(5)
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
                missing_import_src.to_owned(),
            ),
        })
        .expect("open"),
    );
    let _ = receive_diagnostics(&client);
    let _ = receive_diagnostics(&client);
    let _ = receive_diagnostics(&client);

    let bonus_pos = position_of(missing_import_src, "computeBonus", 0);
    send_request(
        &client,
        RequestId::from(607_i32),
        CodeActionRequest::METHOD,
        serde_json::to_value(CodeActionParams {
            text_document: TextDocumentIdentifier::new(app_uri.clone()),
            range: Range::new(bonus_pos, bonus_pos),
            context: CodeActionContext {
                diagnostics: Vec::new(),
                only: Some(vec![CodeActionKind::QUICKFIX]),
                trigger_kind: None,
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .expect("codeAction quickfix"),
    );
    let qf_actions: CodeActionResponse =
        serde_json::from_value(receive_response(&client).response_result.expect("qf ok"))
            .expect("qf resp");
    let import_action = qf_actions
        .iter()
        .find_map(|a| match a {
            CodeActionOrCommand::CodeAction(ca) if ca.title == "Import extra.computeBonus" => {
                Some(ca)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("expected Import extra.computeBonus action: {qf_actions:?}"));
    assert_eq!(import_action.is_preferred, Some(true));
    let edit = import_action.edit.as_ref().expect("workspace edit");
    let edits = edit
        .changes
        .as_ref()
        .and_then(|c| c.get(&app_uri))
        .expect("changes for app_uri");
    assert_eq!(edits[0].new_text, "import extra.computeBonus\n");
    assert_eq!(edits[0].range.start, Position::new(3, 0));

    shutdown(&client);
    server_thread
        .join()
        .expect("server thread")
        .expect("clean exit");
}

/// PR #501 review regressions (all 10 P2 findings):
/// - Explicit type refactor preserves import alias `import model.Point as P` -> `: P`
/// - Generic field `h.value` on `Holder<Point>` jumps to `Point` in `typeDefinition`
/// - Auto-import filters Type vs Value namespace (`fun f(x: Missing)` rejects `fun Missing()`)
/// - Aliased import `import extra.computeBonus as bonus` does not block bare `computeBonus(5)` import
/// - Single-line `package app; fun main() { ... }` inserts import outside function body
/// - Semantic tokens highlight `Display` via frontend target and do NOT falsely highlight `obj.println()`
/// - Closed disk file modified externally returns empty code actions (no stale edit)
/// - Client without `codeActionLiteralSupport` receives `code_action_provider: None` and empty actions
#[test]
fn p3_review_regressions_aliases_namespaces_insertion_generic_typedef_tokens_disk_and_caps() {
    // 1. Client without `codeActionLiteralSupport` must NOT get `code_action_provider` advertised.
    {
        let (server, client) = Connection::memory();
        let server_thread = thread::spawn(|| run(server));
        client
            .sender
            .send(Message::Request(Request {
                id: RequestId::from(1_i32),
                method: Initialize::METHOD.to_owned(),
                params: serde_json::to_value(InitializeParams::default()).expect("params"),
            }))
            .expect("init");
        let Message::Response(resp) = client.receiver.recv_timeout(TIMEOUT).expect("resp") else {
            panic!("expected init response");
        };
        let init: InitializeResult =
            serde_json::from_value(resp.response_result.expect("ok")).expect("init result");
        assert!(
            init.capabilities.code_action_provider.is_none(),
            "code_action_provider must be None without codeActionLiteralSupport"
        );
        send_notification(
            &client,
            Initialized::METHOD,
            serde_json::to_value(InitializedParams {}).expect("initialized"),
        );
        shutdown(&client);
        server_thread.join().expect("thread").expect("exit");
    }

    // 2. Multi-file project covering alias refactor, generic field typeDefinition, Display & non-builtin member tokens,
    //    type-vs-value auto-import filtering, aliased import non-blocking, same-line package insertion, and stale disk check.
    let tmp_dir = std::env::temp_dir().join(format!("koven_lsp_p3_r1_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp_dir);
    std::fs::create_dir_all(tmp_dir.join("model")).expect("mkdir");
    std::fs::create_dir_all(tmp_dir.join("extra")).expect("mkdir");
    std::fs::create_dir_all(tmp_dir.join("app")).expect("mkdir");

    let model_path = tmp_dir.join("model/types.ko");
    let extra_path = tmp_dir.join("extra/helper.ko");
    let app_path = tmp_dir.join("app/main.ko");

    let model_src = "\
package model

public class Point(public val x: Int, public val y: Int) : Display {
    public override fun toString(): String = \"pt\"
}

public class Holder<T>(public val value: T)
";
    let extra_src = "\
package extra

public fun Missing(): Int = 1
public fun computeBonus(v: Int): Int = v + 10
";
    let app_src = "\
package app

import model.Point as P
import model.Holder

fun main(h: Holder<P>) {
    val p = P(1, 2)
    borrow val item = h.value
}
";
    std::fs::write(&model_path, model_src).expect("write model");
    std::fs::write(&extra_path, extra_src).expect("write extra");
    std::fs::write(&app_path, app_src).expect("write app");

    let model_uri: lsp_types::Uri = format!("file://{}", model_path.display())
        .parse()
        .expect("uri");
    let extra_uri: lsp_types::Uri = format!("file://{}", extra_path.display())
        .parse()
        .expect("uri");
    let app_uri: lsp_types::Uri = format!("file://{}", app_path.display())
        .parse()
        .expect("uri");

    let (server, client) = Connection::memory();
    let server_thread = thread::spawn(|| run(server));
    initialize_with_capabilities_and_options(
        &client,
        code_action_literal_caps(),
        Some(serde_json::json!({"koven": {"sourceSet": {
            "schema": "koven.lsp.source-set",
            "version": 1,
            "roots": ["src"],
            "sources": [
                {"root": "src", "logicalPath": "model/types.ko", "uri": model_uri.as_str(), "text": model_src},
                {"root": "src", "logicalPath": "extra/helper.ko", "uri": extra_uri.as_str(), "text": extra_src},
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

    // 2a. Explicit type refactor on `val p = P(1, 2)` must preserve alias `: P` (not `: Point`).
    let p_pos = position_of(app_src, "val p", 0);
    send_request(
        &client,
        RequestId::from(700_i32),
        CodeActionRequest::METHOD,
        serde_json::to_value(CodeActionParams {
            text_document: TextDocumentIdentifier::new(app_uri.clone()),
            range: Range::new(p_pos, p_pos),
            context: CodeActionContext {
                diagnostics: Vec::new(),
                only: Some(vec![CodeActionKind::REFACTOR_REWRITE]),
                trigger_kind: None,
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .expect("ca"),
    );
    let alias_actions: CodeActionResponse =
        serde_json::from_value(receive_response(&client).response_result.expect("ok"))
            .expect("resp");
    assert!(
        alias_actions.iter().any(|a| matches!(
            a,
            CodeActionOrCommand::CodeAction(ca) if ca.title == "Add explicit type ': P'"
        )),
        "explicit type refactor must preserve import alias P: {alias_actions:?}"
    );

    // 2b. Generic field `h.value` on `Holder<P>` must jump to `Point` in `typeDefinition`!
    let value_pos = position_of(app_src, "value", 0);
    send_request(
        &client,
        RequestId::from(701_i32),
        GotoTypeDefinition::METHOD,
        serde_json::to_value(GotoTypeDefinitionParams {
            text_document_position_params: doc_pos(&app_uri, value_pos),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .expect("typedef"),
    );
    let Some(GotoDefinitionResponse::Scalar(loc)) =
        serde_json::from_value::<Option<GotoDefinitionResponse>>(
            receive_response(&client).response_result.expect("ok"),
        )
        .expect("resp")
    else {
        panic!("expected concrete Point typeDefinition for generic field h.value");
    };
    assert_eq!(loc.uri, model_uri);
    assert_eq!(loc.range.start.line, 2, "Point declaration line");

    // 2c. Semantic tokens on model/types.ko must highlight `Display` as INTERFACE (2) + DEFAULT_LIBRARY (16).
    send_request(
        &client,
        RequestId::from(702_i32),
        SemanticTokensFullRequest::METHOD,
        serde_json::to_value(SemanticTokensParams {
            text_document: TextDocumentIdentifier::new(model_uri.clone()),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .expect("sem"),
    );
    let Some(SemanticTokensResult::Tokens(m_toks)) =
        serde_json::from_value::<Option<SemanticTokensResult>>(
            receive_response(&client).response_result.expect("ok"),
        )
        .expect("resp")
    else {
        panic!("expected tokens");
    };
    let decoded_m = decode_semantic_tokens(&m_toks.data, model_src);
    assert!(
        decoded_m
            .iter()
            .any(|(_, _, s, t, m)| s == "Display" && *t == 2 && (*m & 16) != 0),
        "Display must be highlighted as INTERFACE + DEFAULT_LIBRARY: {decoded_m:?}"
    );

    // 2d. Stale closed disk file: modify app/main.ko on disk while closed -> codeAction must return empty!
    std::fs::write(&app_path, format!("// externally modified\n{app_src}")).expect("modify disk");
    send_request(
        &client,
        RequestId::from(703_i32),
        CodeActionRequest::METHOD,
        serde_json::to_value(CodeActionParams {
            text_document: TextDocumentIdentifier::new(app_uri.clone()),
            range: Range::new(p_pos, p_pos),
            context: CodeActionContext {
                diagnostics: Vec::new(),
                only: Some(vec![CodeActionKind::REFACTOR_REWRITE]),
                trigger_kind: None,
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .expect("stale ca"),
    );
    let stale_actions: CodeActionResponse =
        serde_json::from_value(receive_response(&client).response_result.expect("ok"))
            .expect("resp");
    assert!(
        stale_actions.is_empty(),
        "stale closed disk file must return empty code actions: {stale_actions:?}"
    );

    // 2e. Open app/main.ko with:
    //     - single-line `package app; fun f(x: Missing) { ... }` -> must NOT suggest `fun extra.Missing()` for Type position!
    //     - `import extra.computeBonus as bonus` + bare `computeBonus(5)` -> MUST still suggest `Import extra.computeBonus`!
    //     - unresolved `x.println()` -> must NOT get DEFAULT_LIBRARY semantic token!
    let tricky_src = "package app; import extra.computeBonus as bonus; fun f(x: Missing) { val b = computeBonus(5); val u = x.println() }\n";
    send_notification(
        &client,
        DidOpenTextDocument::METHOD,
        serde_json::to_value(DidOpenTextDocumentParams {
            text_document: TextDocumentItem::new(
                app_uri.clone(),
                "koven".to_owned(),
                1,
                tricky_src.to_owned(),
            ),
        })
        .expect("open"),
    );
    let _ = receive_diagnostics(&client);
    let _ = receive_diagnostics(&client);
    let _ = receive_diagnostics(&client);

    // Type-position `Missing` must NOT suggest `extra.Missing` (which is a function, not a type).
    let missing_pos = position_of(tricky_src, "Missing", 0);
    send_request(
        &client,
        RequestId::from(704_i32),
        CodeActionRequest::METHOD,
        serde_json::to_value(CodeActionParams {
            text_document: TextDocumentIdentifier::new(app_uri.clone()),
            range: Range::new(missing_pos, missing_pos),
            context: CodeActionContext {
                diagnostics: Vec::new(),
                only: Some(vec![CodeActionKind::QUICKFIX]),
                trigger_kind: None,
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .expect("missing ca"),
    );
    let missing_actions: CodeActionResponse =
        serde_json::from_value(receive_response(&client).response_result.expect("ok"))
            .expect("resp");
    assert!(
        !missing_actions.iter().any(|a| matches!(
            a,
            CodeActionOrCommand::CodeAction(ca) if ca.title.contains("extra.Missing")
        )),
        "type-position Missing must not suggest function extra.Missing: {missing_actions:?}"
    );

    // Bare `computeBonus(5)` with existing `import extra.computeBonus as bonus` MUST still suggest `Import extra.computeBonus`
    // and insert it on line 0 right after the import semicolon (NOT on line 1 after the function!).
    let bonus_pos = position_of(tricky_src, "computeBonus(5)", 0);
    send_request(
        &client,
        RequestId::from(705_i32),
        CodeActionRequest::METHOD,
        serde_json::to_value(CodeActionParams {
            text_document: TextDocumentIdentifier::new(app_uri.clone()),
            range: Range::new(bonus_pos, bonus_pos),
            context: CodeActionContext {
                diagnostics: Vec::new(),
                only: Some(vec![CodeActionKind::QUICKFIX]),
                trigger_kind: None,
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .expect("bonus ca"),
    );
    let bonus_actions: CodeActionResponse =
        serde_json::from_value(receive_response(&client).response_result.expect("ok"))
            .expect("resp");
    let bonus_ca = bonus_actions
        .iter()
        .find_map(|a| match a {
            CodeActionOrCommand::CodeAction(ca) if ca.title == "Import extra.computeBonus" => {
                Some(ca)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("expected Import extra.computeBonus: {bonus_actions:?}"));
    let bonus_edit = &bonus_ca
        .edit
        .as_ref()
        .and_then(|e| e.changes.as_ref())
        .and_then(|c| c.get(&app_uri))
        .expect("edit")[0];
    assert_eq!(
        bonus_edit.range.start.line, 0,
        "must insert before `fun f` on line 0, not after line 0"
    );
    assert!(
        bonus_edit.range.start.character < tricky_src.find("fun f").unwrap() as u32 + 1,
        "must not insert inside function body: {:?}",
        bonus_edit.range
    );

    // Unresolved `x.println()` must NOT get DEFAULT_LIBRARY (16) semantic token.
    send_request(
        &client,
        RequestId::from(706_i32),
        SemanticTokensFullRequest::METHOD,
        serde_json::to_value(SemanticTokensParams {
            text_document: TextDocumentIdentifier::new(app_uri.clone()),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .expect("sem tricky"),
    );
    let Some(SemanticTokensResult::Tokens(t_toks)) =
        serde_json::from_value::<Option<SemanticTokensResult>>(
            receive_response(&client).response_result.expect("ok"),
        )
        .expect("resp")
    else {
        panic!("expected tokens");
    };
    let decoded_t = decode_semantic_tokens(&t_toks.data, tricky_src);
    assert!(
        !decoded_t
            .iter()
            .any(|(_, _, s, _, m)| s == "println" && (*m & 16) != 0),
        "unresolved member `x.println()` must not get DEFAULT_LIBRARY token: {decoded_t:?}"
    );

    // 2f. Missing member `p.computeBonus()` reports L0080, but MUST NOT suggest importing top-level `extra.computeBonus`!
    let member_err_src = "\
package app

import model.Point

fun testMember(p: Point) {
    val bad = p.computeBonus()
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
                text: member_err_src.to_owned(),
            }],
        })
        .expect("change"),
    );
    let _ = receive_diagnostics(&client);
    let _ = receive_diagnostics(&client);
    let _ = receive_diagnostics(&client);

    let member_pos = position_of(member_err_src, "computeBonus", 0);
    send_request(
        &client,
        RequestId::from(707_i32),
        CodeActionRequest::METHOD,
        serde_json::to_value(CodeActionParams {
            text_document: TextDocumentIdentifier::new(app_uri.clone()),
            range: Range::new(member_pos, member_pos),
            context: CodeActionContext {
                diagnostics: Vec::new(),
                only: Some(vec![CodeActionKind::QUICKFIX]),
                trigger_kind: None,
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .expect("member ca"),
    );
    let member_actions: CodeActionResponse =
        serde_json::from_value(receive_response(&client).response_result.expect("ok"))
            .expect("resp");
    assert!(
        member_actions.is_empty(),
        "missing member `p.computeBonus()` must NOT suggest auto-importing top-level `extra.computeBonus`: {member_actions:?}"
    );

    shutdown(&client);
    server_thread
        .join()
        .expect("server thread")
        .expect("clean exit");
    let _ = std::fs::remove_dir_all(&tmp_dir);
}
