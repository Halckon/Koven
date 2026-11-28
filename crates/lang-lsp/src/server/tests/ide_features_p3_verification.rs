use super::*;
use lsp_types::{
    CodeActionContext, CodeActionKind, CodeActionOrCommand, CodeActionParams, CodeActionResponse,
    Range, TextDocumentPositionParams, TextEdit,
    request::{CodeActionRequest, GotoDefinition},
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

fn apply_single_edit(text: &str, edit: &TextEdit) -> String {
    let mut line_starts = vec![0usize];
    for (idx, b) in text.bytes().enumerate() {
        if b == b'\n' {
            line_starts.push(idx + 1);
        }
    }
    let to_byte = |pos: Position| -> usize {
        let start = line_starts[pos.line as usize];
        let mut utf16 = 0u32;
        for (rel, ch) in text[start..].char_indices() {
            if utf16 == pos.character {
                return start + rel;
            }
            utf16 += ch.len_utf16() as u32;
        }
        text.len()
    };
    let s = to_byte(edit.range.start);
    let e = to_byte(edit.range.end);
    format!("{}{}{}", &text[..s], edit.new_text, &text[e..])
}

fn collect_diagnostics_for(
    client: &Connection,
    count: usize,
    target_uri: &lsp_types::Uri,
) -> Vec<lsp_types::Diagnostic> {
    let mut out = Vec::new();
    for _ in 0..count {
        let pub_diag = receive_diagnostics(client);
        if &pub_diag.uri == target_uri {
            out = pub_diag.diagnostics;
        }
    }
    out
}

/// Pre-merge verification regressions for PR #501:
/// 1. Private member access `p.secretBonus()` emits L0080 while another package has `public fun secretBonus()`,
///    yet codeAction MUST NOT offer an auto-import Quick Fix.
/// 2. Applying explicit type annotation `: P` (for `import model.Point as P`) compiles with 0 diagnostics
///    and Go-to-Definition on `P` in `val p: P` resolves back to `model/types.ko`'s `Point`.
/// 3. Auto-import preserves a trailing multi-line block comment after `package app` and compiles cleanly.
/// 4. Single-segment `import DefaultItem` from default package is offered and compiles cleanly when applied.
#[test]
fn p3_post_edit_verification_private_member_l0080_alias_type_comment_and_default_pkg_import() {
    let (server, client) = Connection::memory();
    let server_thread = thread::spawn(|| run(server));
    let def_uri: lsp_types::Uri = "file:///p3v/def.ko".parse().expect("uri");
    let model_uri: lsp_types::Uri = "file:///p3v/model/types.ko".parse().expect("uri");
    let extra_uri: lsp_types::Uri = "file:///p3v/extra/helper.ko".parse().expect("uri");
    let app_uri: lsp_types::Uri = "file:///p3v/app/main.ko".parse().expect("uri");

    let def_src = "public class DefaultItem(public val v: Int)\n";
    let model_src = "\
package model

public class Point(public val x: Int, public val y: Int) {
    private fun secretBonus(): Int = 1
}
";
    let extra_src = "\
package extra

public fun secretBonus(): Int = 2
public fun computeBonus(v: Int): Int = v + 10
";
    let app_src = "\
package app

import model.Point as P

fun main() {
    val p = P(1, 2)
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
                {"root": "src", "logicalPath": "def.ko", "uri": def_uri.as_str(), "text": def_src},
                {"root": "src", "logicalPath": "model/types.ko", "uri": model_uri.as_str(), "text": model_src},
                {"root": "src", "logicalPath": "extra/helper.ko", "uri": extra_uri.as_str(), "text": extra_src},
                {"root": "src", "logicalPath": "app/main.ko", "uri": app_uri.as_str(), "text": app_src}
            ]
        }}})),
    );
    let initial_app_diags = collect_diagnostics_for(&client, 4, &app_uri);
    assert!(initial_app_diags.is_empty(), "init: {initial_app_diags:?}");

    // 2. Apply explicit type annotation edit `: P`, send didOpen, verify 0 diagnostics AND definition of `P` -> model.Point.
    let p_pos = position_of(app_src, "val p", 0);
    send_request(
        &client,
        RequestId::from(800_i32),
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
    let actions: CodeActionResponse =
        serde_json::from_value(receive_response(&client).response_result.expect("ok"))
            .expect("resp");
    let type_edit = actions
        .iter()
        .find_map(|a| match a {
            CodeActionOrCommand::CodeAction(ca) if ca.title == "Add explicit type ': P'" => ca
                .edit
                .as_ref()
                .and_then(|e| e.changes.as_ref())
                .and_then(|c| c.get(&app_uri))
                .and_then(|edits| edits.first()),
            _ => None,
        })
        .expect("explicit type edit");
    let annotated_src = apply_single_edit(app_src, type_edit);
    assert!(
        annotated_src.contains("val p: P = P(1, 2)"),
        "edited: {annotated_src}"
    );
    send_notification(
        &client,
        DidOpenTextDocument::METHOD,
        serde_json::to_value(DidOpenTextDocumentParams {
            text_document: TextDocumentItem::new(
                app_uri.clone(),
                "koven".to_owned(),
                1,
                annotated_src.clone(),
            ),
        })
        .expect("open"),
    );
    let post_type_diags = collect_diagnostics_for(&client, 4, &app_uri);
    assert!(
        post_type_diags.is_empty(),
        "annotated alias must compile cleanly: {post_type_diags:?}"
    );
    let alias_ty_pos = position_of(&annotated_src, ": P", 0);
    send_request(
        &client,
        RequestId::from(801_i32),
        GotoDefinition::METHOD,
        serde_json::to_value(TextDocumentPositionParams::new(
            TextDocumentIdentifier::new(app_uri.clone()),
            Position::new(alias_ty_pos.line, alias_ty_pos.character + 2),
        ))
        .expect("def"),
    );
    let Some(GotoDefinitionResponse::Scalar(loc)) =
        serde_json::from_value::<Option<GotoDefinitionResponse>>(
            receive_response(&client).response_result.expect("ok"),
        )
        .expect("resp")
    else {
        panic!("expected Scalar definition for alias P");
    };
    assert_eq!(loc.uri, model_uri);
    assert_eq!(loc.range.start.line, 2, "must resolve back to model.Point");

    // 1. Private member `p.secretBonus()` genuinely produces L0080 while `extra.secretBonus` exists -> NO Quick Fix!
    let priv_member_src = "\
package app

import model.Point

fun main(p: Point) {
    val bad = p.secretBonus()
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
                text: priv_member_src.to_owned(),
            }],
        })
        .expect("change"),
    );
    let priv_diags = collect_diagnostics_for(&client, 4, &app_uri);
    assert!(
        priv_diags
            .iter()
            .any(|d| d.code == Some(lsp_types::NumberOrString::String("L0080".to_owned()))),
        "private member access must produce L0080: {priv_diags:?}"
    );
    let priv_pos = position_of(priv_member_src, "secretBonus", 0);
    send_request(
        &client,
        RequestId::from(802_i32),
        CodeActionRequest::METHOD,
        serde_json::to_value(CodeActionParams {
            text_document: TextDocumentIdentifier::new(app_uri.clone()),
            range: Range::new(priv_pos, priv_pos),
            context: CodeActionContext {
                diagnostics: Vec::new(),
                only: Some(vec![CodeActionKind::QUICKFIX]),
                trigger_kind: None,
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .expect("priv ca"),
    );
    let priv_actions: CodeActionResponse =
        serde_json::from_value(receive_response(&client).response_result.expect("ok"))
            .expect("resp");
    assert!(
        priv_actions.is_empty(),
        "private member L0080 must NOT offer auto-import of extra.secretBonus: {priv_actions:?}"
    );

    // 3. Auto-import preserves trailing multi-line block comment after `package app` and compiles cleanly.
    let comment_src = "\
package app /* keep-comment
   line 2 */
fun main() {
    val b = computeBonus(5)
}
";
    send_notification(
        &client,
        DidChangeTextDocument::METHOD,
        serde_json::to_value(DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier::new(app_uri.clone(), 3),
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: comment_src.to_owned(),
            }],
        })
        .expect("change"),
    );
    let _ = collect_diagnostics_for(&client, 4, &app_uri);
    let bonus_pos = position_of(comment_src, "computeBonus", 0);
    send_request(
        &client,
        RequestId::from(803_i32),
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
        .expect("comment ca"),
    );
    let comment_actions: CodeActionResponse =
        serde_json::from_value(receive_response(&client).response_result.expect("ok"))
            .expect("resp");
    let comment_edit = comment_actions
        .iter()
        .find_map(|a| match a {
            CodeActionOrCommand::CodeAction(ca) if ca.title == "Import extra.computeBonus" => ca
                .edit
                .as_ref()
                .and_then(|e| e.changes.as_ref())
                .and_then(|c| c.get(&app_uri))
                .and_then(|edits| edits.first()),
            _ => None,
        })
        .expect("import edit");
    let comment_fixed = apply_single_edit(comment_src, comment_edit);
    assert!(
        comment_fixed.contains("/* keep-comment\n   line 2 */"),
        "block comment must remain intact: {comment_fixed}"
    );
    send_notification(
        &client,
        DidChangeTextDocument::METHOD,
        serde_json::to_value(DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier::new(app_uri.clone(), 4),
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: comment_fixed.clone(),
            }],
        })
        .expect("change"),
    );
    let post_comment_diags = collect_diagnostics_for(&client, 4, &app_uri);
    assert!(
        post_comment_diags.is_empty(),
        "import with trailing block comment must compile cleanly: {post_comment_diags:?}\nsrc:\n{comment_fixed}"
    );

    // 4. Single-segment `import DefaultItem` from default package resolves cleanly when applied.
    let default_pkg_src = "\
package app

fun main() {
    val item = DefaultItem(1)
}
";
    send_notification(
        &client,
        DidChangeTextDocument::METHOD,
        serde_json::to_value(DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier::new(app_uri.clone(), 5),
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: default_pkg_src.to_owned(),
            }],
        })
        .expect("change"),
    );
    let _ = collect_diagnostics_for(&client, 4, &app_uri);
    let def_pos = position_of(default_pkg_src, "DefaultItem", 0);
    send_request(
        &client,
        RequestId::from(804_i32),
        CodeActionRequest::METHOD,
        serde_json::to_value(CodeActionParams {
            text_document: TextDocumentIdentifier::new(app_uri.clone()),
            range: Range::new(def_pos, def_pos),
            context: CodeActionContext {
                diagnostics: Vec::new(),
                only: Some(vec![CodeActionKind::QUICKFIX]),
                trigger_kind: None,
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .expect("def ca"),
    );
    let def_actions: CodeActionResponse =
        serde_json::from_value(receive_response(&client).response_result.expect("ok"))
            .expect("resp");
    let def_edit = def_actions
        .iter()
        .find_map(|a| match a {
            CodeActionOrCommand::CodeAction(ca) if ca.title == "Import DefaultItem" => ca
                .edit
                .as_ref()
                .and_then(|e| e.changes.as_ref())
                .and_then(|c| c.get(&app_uri))
                .and_then(|edits| edits.first()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("expected single-segment Import DefaultItem: {def_actions:?}"));
    let def_fixed = apply_single_edit(default_pkg_src, def_edit);
    send_notification(
        &client,
        DidChangeTextDocument::METHOD,
        serde_json::to_value(DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier::new(app_uri.clone(), 6),
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: def_fixed.clone(),
            }],
        })
        .expect("change"),
    );
    let post_def_diags = collect_diagnostics_for(&client, 4, &app_uri);
    assert!(
        post_def_diags.is_empty(),
        "single-segment `import DefaultItem` must resolve cleanly: {post_def_diags:?}\nsrc:\n{def_fixed}"
    );

    shutdown(&client);
    server_thread
        .join()
        .expect("server thread")
        .expect("clean exit");
}
