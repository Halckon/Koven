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
    initialize_with_options(
        &client,
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
    assert!(
        decoded_model
            .iter()
            .any(|(_, _, s, t, m)| s == "y" && *t == 8 && (*m & 4) != 0)
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
    // `count` is mutable `var` (mod 4), `origin` is readonly `val` (mod 2), `println` is builtin func (6, mod 16)
    assert!(
        decoded_app
            .iter()
            .any(|(_, _, s, t, m)| s == "count" && *t == 10 && (*m & 4) != 0)
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
