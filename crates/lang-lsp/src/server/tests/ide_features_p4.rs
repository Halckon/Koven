use super::*;
use lsp_types::{
    CodeActionContext, CodeActionKind, CodeActionOrCommand, CodeActionParams, CodeActionResponse,
    CompletionParams, CompletionResponse, DidChangeWatchedFilesParams, DidSaveTextDocumentParams,
    FileChangeType, FileEvent, InsertTextFormat, Range, SemanticTokensDeltaParams,
    SemanticTokensFullDeltaResult, SemanticTokensParams, SemanticTokensResult,
    TextDocumentPositionParams, TextDocumentSyncKind, WorkspaceFolder,
    notification::{DidChangeWatchedFiles, DidSaveTextDocument},
    request::{
        CodeActionRequest, Completion, GotoImplementation, GotoImplementationParams,
        SemanticTokensFullDeltaRequest, SemanticTokensFullRequest,
    },
};

fn p4_caps() -> lsp_types::ClientCapabilities {
    lsp_types::ClientCapabilities {
        text_document: Some(lsp_types::TextDocumentClientCapabilities {
            completion: Some(lsp_types::CompletionClientCapabilities {
                completion_item: Some(lsp_types::CompletionItemCapability {
                    snippet_support: Some(true),
                    ..Default::default()
                }),
                ..Default::default()
            }),
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

/// Covers P4-B + P4-C:
/// - Auto-import completion (`additionalTextEdits`) + Snippet `insertText`
/// - `L0101` "Implement missing interface members" QuickFix
/// - "Remove unused import" QuickFix
/// - `textDocument/implementation` on interface & interface method
/// - `textDocument/semanticTokens/full` + `semanticTokens/full/delta` (unchanged, edited, stale fallback)
#[test]
fn p4_auto_import_completion_snippets_override_unused_import_impl_and_delta_tokens() {
    let (server, client) = Connection::memory();
    let server_thread = thread::spawn(|| run(server));
    let iface_uri: lsp_types::Uri = "file:///p4/api/named.ko".parse().expect("uri");
    let extra_uri: lsp_types::Uri = "file:///p4/extra/util.ko".parse().expect("uri");
    let app_uri: lsp_types::Uri = "file:///p4/app/main.ko".parse().expect("uri");

    let iface_src = "\
package api

public interface Named {
    public fun label(): String
}
";
    let extra_src = "\
package extra

public fun computeBonus(v: Int): Int = v + 10
public fun unusedHelper(): Int = 0
";
    let app_src = "\
package app

import api.Named
import extra.unusedHelper

public class User(public val name: String) : Named {
    public override fun label(): String = name
}

fun main() {
    val u = User(\"koven\")
    val s = u.label()
    val x = comp
}
";
    initialize_with_capabilities_and_options(
        &client,
        p4_caps(),
        Some(serde_json::json!({"koven": {"sourceSet": {
            "schema": "koven.lsp.source-set",
            "version": 1,
            "roots": ["src"],
            "sources": [
                {"root": "src", "logicalPath": "api/named.ko", "uri": iface_uri.as_str(), "text": iface_src},
                {"root": "src", "logicalPath": "extra/util.ko", "uri": extra_uri.as_str(), "text": extra_src},
                {"root": "src", "logicalPath": "app/main.ko", "uri": app_uri.as_str(), "text": app_src}
            ]
        }}})),
    );
    let _ = receive_diagnostics(&client);
    let _ = receive_diagnostics(&client);
    let _ = receive_diagnostics(&client);

    // 1. Auto-import completion + snippet support on `comp` -> `computeBonus($1)` with `additionalTextEdits`!
    let comp_pos = position_of(app_src, "comp", 0);
    send_request(
        &client,
        RequestId::from(900_i32),
        Completion::METHOD,
        serde_json::to_value(CompletionParams {
            text_document_position: TextDocumentPositionParams::new(
                TextDocumentIdentifier::new(app_uri.clone()),
                Position::new(comp_pos.line, comp_pos.character + 4),
            ),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
            context: None,
        })
        .expect("comp"),
    );
    let Some(CompletionResponse::Array(items)) =
        serde_json::from_value::<Option<CompletionResponse>>(
            receive_response(&client).response_result.expect("ok"),
        )
        .expect("resp")
    else {
        panic!("expected CompletionResponse::Array");
    };
    let bonus_item = items
        .iter()
        .find(|i| i.label == "computeBonus")
        .unwrap_or_else(|| panic!("expected auto-import completion computeBonus: {items:?}"));
    assert_eq!(bonus_item.insert_text.as_deref(), Some("computeBonus($1)"));
    assert_eq!(
        bonus_item.insert_text_format,
        Some(InsertTextFormat::SNIPPET)
    );
    let add_edits = bonus_item
        .additional_text_edits
        .as_ref()
        .expect("additionalTextEdits");
    assert!(
        add_edits[0].new_text.contains("import extra.computeBonus"),
        "expected import extra.computeBonus edit: {add_edits:?}"
    );

    // 2. Remove unused import QuickFix on `import extra.unusedHelper`.
    let unused_pos = position_of(app_src, "unusedHelper", 0);
    send_request(
        &client,
        RequestId::from(901_i32),
        CodeActionRequest::METHOD,
        serde_json::to_value(CodeActionParams {
            text_document: TextDocumentIdentifier::new(app_uri.clone()),
            range: Range::new(unused_pos, unused_pos),
            context: CodeActionContext {
                diagnostics: Vec::new(),
                only: Some(vec![CodeActionKind::QUICKFIX]),
                trigger_kind: None,
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .expect("unused ca"),
    );
    let unused_actions: CodeActionResponse =
        serde_json::from_value(receive_response(&client).response_result.expect("ok"))
            .expect("resp");
    assert!(
        unused_actions.iter().any(|a| matches!(
            a,
            CodeActionOrCommand::CodeAction(ca) if ca.title == "Remove unused import"
        )),
        "expected Remove unused import QuickFix: {unused_actions:?}"
    );

    // 3. Fix `comp` -> `1` so unit type-checks cleanly, then test `textDocument/implementation` and `semanticTokens/full/delta`!
    let clean_app = "\
package app

import api.Named

public class User(public val name: String) : Named {
    public override fun label(): String = name
}

fun main() {
    val u = User(\"koven\")
    val s = u.label()
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
                clean_app.to_owned(),
            ),
        })
        .expect("open"),
    );
    let _ = receive_diagnostics(&client);
    let _ = receive_diagnostics(&client);
    let _ = receive_diagnostics(&client);

    // 3a. `textDocument/implementation` on `interface Named` -> jumps to `User` in app/main.ko.
    let named_pos = position_of(iface_src, "Named", 0);
    send_request(
        &client,
        RequestId::from(902_i32),
        GotoImplementation::METHOD,
        serde_json::to_value(GotoImplementationParams {
            text_document_position_params: TextDocumentPositionParams::new(
                TextDocumentIdentifier::new(iface_uri.clone()),
                named_pos,
            ),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .expect("impl"),
    );
    let Some(GotoDefinitionResponse::Array(impl_locs)) =
        serde_json::from_value::<Option<GotoDefinitionResponse>>(
            receive_response(&client).response_result.expect("ok"),
        )
        .expect("resp")
    else {
        panic!("expected Array implementation locations for Named");
    };
    assert_eq!(impl_locs.len(), 1);
    assert_eq!(impl_locs[0].uri, app_uri);
    assert_eq!(impl_locs[0].range.start.line, 4);

    // 3b. `textDocument/implementation` on `Named.label` -> jumps to `User.label`.
    let label_pos = position_of(iface_src, "label", 0);
    send_request(
        &client,
        RequestId::from(903_i32),
        GotoImplementation::METHOD,
        serde_json::to_value(GotoImplementationParams {
            text_document_position_params: TextDocumentPositionParams::new(
                TextDocumentIdentifier::new(iface_uri.clone()),
                label_pos,
            ),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .expect("impl method"),
    );
    let Some(GotoDefinitionResponse::Array(method_locs)) =
        serde_json::from_value::<Option<GotoDefinitionResponse>>(
            receive_response(&client).response_result.expect("ok"),
        )
        .expect("resp")
    else {
        panic!("expected Array implementation locations for Named.label");
    };
    assert_eq!(method_locs.len(), 1);
    assert_eq!(method_locs[0].uri, app_uri);
    assert_eq!(method_locs[0].range.start.line, 5);

    // 4. `semanticTokens/full` -> `semanticTokens/full/delta` (unchanged = 0 edits, stale = fallback full).
    send_request(
        &client,
        RequestId::from(904_i32),
        SemanticTokensFullRequest::METHOD,
        serde_json::to_value(SemanticTokensParams {
            text_document: TextDocumentIdentifier::new(app_uri.clone()),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .expect("sem full"),
    );
    let Some(SemanticTokensResult::Tokens(full_toks)) =
        serde_json::from_value::<Option<SemanticTokensResult>>(
            receive_response(&client).response_result.expect("ok"),
        )
        .expect("resp")
    else {
        panic!("expected full tokens");
    };
    let result_id = full_toks.result_id.expect("result_id");

    send_request(
        &client,
        RequestId::from(905_i32),
        SemanticTokensFullDeltaRequest::METHOD,
        serde_json::to_value(SemanticTokensDeltaParams {
            text_document: TextDocumentIdentifier::new(app_uri.clone()),
            previous_result_id: result_id,
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .expect("sem delta"),
    );
    let Some(SemanticTokensFullDeltaResult::TokensDelta(delta)) =
        serde_json::from_value::<Option<SemanticTokensFullDeltaResult>>(
            receive_response(&client).response_result.expect("ok"),
        )
        .expect("resp")
    else {
        panic!("expected TokensDelta");
    };
    assert!(
        delta.edits.is_empty(),
        "unchanged doc should produce 0 delta edits"
    );

    send_request(
        &client,
        RequestId::from(906_i32),
        SemanticTokensFullDeltaRequest::METHOD,
        serde_json::to_value(SemanticTokensDeltaParams {
            text_document: TextDocumentIdentifier::new(app_uri.clone()),
            previous_result_id: "stale-id".to_owned(),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .expect("sem stale delta"),
    );
    let stale_resp: Option<SemanticTokensFullDeltaResult> =
        serde_json::from_value(receive_response(&client).response_result.expect("ok"))
            .expect("resp");
    assert!(
        matches!(stale_resp, Some(SemanticTokensFullDeltaResult::Tokens(_))),
        "stale previous_result_id must fall back to full Tokens"
    );

    // 5. L0101 "Implement missing interface members" QuickFix.
    let missing_impl_src = "\
package app

import api.Named

public class Broken(public val name: String) : Named {
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
                text: missing_impl_src.to_owned(),
            }],
        })
        .expect("change"),
    );
    let _ = receive_diagnostics(&client);
    let _ = receive_diagnostics(&client);
    let _ = receive_diagnostics(&client);

    let broken_pos = position_of(missing_impl_src, "Broken", 0);
    send_request(
        &client,
        RequestId::from(907_i32),
        CodeActionRequest::METHOD,
        serde_json::to_value(CodeActionParams {
            text_document: TextDocumentIdentifier::new(app_uri.clone()),
            range: Range::new(broken_pos, broken_pos),
            context: CodeActionContext {
                diagnostics: Vec::new(),
                only: Some(vec![CodeActionKind::QUICKFIX]),
                trigger_kind: None,
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .expect("l0101 ca"),
    );
    let l0101_actions: CodeActionResponse =
        serde_json::from_value(receive_response(&client).response_result.expect("ok"))
            .expect("resp");
    let impl_ca = l0101_actions
        .iter()
        .find_map(|a| match a {
            CodeActionOrCommand::CodeAction(ca)
                if ca.title == "Implement missing interface members" =>
            {
                Some(ca)
            }
            _ => None,
        })
        .unwrap_or_else(|| {
            panic!("expected Implement missing interface members: {l0101_actions:?}")
        });
    let impl_edit = &impl_ca
        .edit
        .as_ref()
        .and_then(|e| e.changes.as_ref())
        .and_then(|c| c.get(&app_uri))
        .expect("edit")[0];
    assert!(
        impl_edit
            .new_text
            .contains("public override fun label(): String"),
        "expected override fun label stub: {:?}",
        impl_edit.new_text
    );

    shutdown(&client);
    server_thread
        .join()
        .expect("server thread")
        .expect("clean exit");
}

/// Covers P4-A (`--workspace-manifest` mode):
/// - Advertises `TextDocumentSyncKind::INCREMENTAL` + `save`
/// - Incremental ranged `didChange` splices edits accurately
/// - `didSave` refreshes base snapshot so `didClose` retains saved text
/// - `workspace/didChangeWatchedFiles` hot-reloads newly created `.ko` files without restarting LSP
#[test]
fn p4_workspace_manifest_incremental_sync_did_save_and_watched_files_hot_reload() {
    let tmp_dir = std::env::temp_dir().join(format!("koven_lsp_p4_hot_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp_dir);
    std::fs::create_dir_all(tmp_dir.join("src")).expect("mkdir");
    let tmp_dir = std::fs::canonicalize(&tmp_dir).expect("canonical tmp");
    std::fs::write(
        tmp_dir.join("project.toml"),
        "schema = \"koven.project\"\nversion = 1\n\n[project]\nname = \"demo\"\nsource-roots = [\"src\"]\n",
    )
    .expect("manifest");
    let main_path = tmp_dir.join("src/main.ko");
    let main_src = "fun main() {\n    val x = 1\n}\n";
    std::fs::write(&main_path, main_src).expect("write main");

    let root_uri: lsp_types::Uri = format!("file://{}", tmp_dir.display())
        .parse()
        .expect("uri");
    let main_uri: lsp_types::Uri = format!("file://{}", main_path.display())
        .parse()
        .expect("uri");

    let (server, client) = Connection::memory();
    let server_thread = thread::spawn(|| {
        super::super::run_with_options(
            server,
            super::super::ServerOptions {
                workspace_manifest: true,
                diagnostic_context: false,
            },
        )
    });

    client
        .sender
        .send(Message::Request(Request {
            id: RequestId::from(1_i32),
            method: Initialize::METHOD.to_owned(),
            params: serde_json::to_value(InitializeParams {
                workspace_folders: Some(vec![WorkspaceFolder {
                    uri: root_uri,
                    name: "demo".to_owned(),
                }]),
                ..InitializeParams::default()
            })
            .expect("params"),
        }))
        .expect("init");
    let Message::Response(resp) = client.receiver.recv_timeout(TIMEOUT).expect("resp") else {
        panic!("expected init response");
    };
    let init: InitializeResult =
        serde_json::from_value(resp.response_result.expect("ok")).expect("init");
    assert!(matches!(
        init.capabilities.text_document_sync,
        Some(lsp_types::TextDocumentSyncCapability::Options(ref opts))
            if opts.change == Some(TextDocumentSyncKind::INCREMENTAL) && opts.save.is_some()
    ));
    send_notification(
        &client,
        Initialized::METHOD,
        serde_json::to_value(InitializedParams {}).expect("initialized"),
    );
    let d_init = receive_diagnostics(&client);
    assert!(
        d_init.diagnostics.is_empty(),
        "init: {:?}",
        d_init.diagnostics
    );

    // 1. Open main.ko and apply an INCREMENTAL ranged change (`1` -> `helper()`).
    send_notification(
        &client,
        DidOpenTextDocument::METHOD,
        serde_json::to_value(DidOpenTextDocumentParams {
            text_document: TextDocumentItem::new(
                main_uri.clone(),
                "koven".to_owned(),
                1,
                main_src.to_owned(),
            ),
        })
        .expect("open"),
    );
    let _ = receive_diagnostics(&client);

    send_notification(
        &client,
        DidChangeTextDocument::METHOD,
        serde_json::to_value(DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier::new(main_uri.clone(), 2),
            content_changes: vec![TextDocumentContentChangeEvent {
                range: Some(Range::new(Position::new(1, 12), Position::new(1, 13))),
                range_length: None,
                text: "helper()".to_owned(),
            }],
        })
        .expect("inc change"),
    );
    let d_inc = receive_diagnostics(&client);
    assert!(
        !d_inc.diagnostics.is_empty(),
        "incremental change to unresolved `helper()` should report error"
    );

    // 2. Write `src/helper.ko` on disk and send `workspace/didChangeWatchedFiles` CREATED -> error resolves!
    let helper_path = tmp_dir.join("src/helper.ko");
    let helper_uri: lsp_types::Uri = format!("file://{}", helper_path.display())
        .parse()
        .expect("uri");
    std::fs::write(&helper_path, "public fun helper(): Int = 42\n").expect("write helper");
    send_notification(
        &client,
        DidChangeWatchedFiles::METHOD,
        serde_json::to_value(DidChangeWatchedFilesParams {
            changes: vec![FileEvent::new(helper_uri.clone(), FileChangeType::CREATED)],
        })
        .expect("watched"),
    );
    let d_w1 = receive_diagnostics(&client);
    let d_w2 = receive_diagnostics(&client);
    assert!(d_w1.diagnostics.is_empty(), "d_w1: {:?}", d_w1.diagnostics);
    assert!(d_w2.diagnostics.is_empty(), "d_w2: {:?}", d_w2.diagnostics);

    // 3. Save `main.ko` (updating base text via didSave) and close it -> base remains valid `helper()`.
    std::fs::write(&main_path, "fun main() {\n    val x = helper()\n}\n").expect("save main");
    send_notification(
        &client,
        DidSaveTextDocument::METHOD,
        serde_json::to_value(DidSaveTextDocumentParams {
            text_document: TextDocumentIdentifier::new(main_uri.clone()),
            text: None,
        })
        .expect("save"),
    );
    let _ = receive_diagnostics(&client);
    let _ = receive_diagnostics(&client);

    send_notification(
        &client,
        DidCloseTextDocument::METHOD,
        serde_json::to_value(DidCloseTextDocumentParams {
            text_document: TextDocumentIdentifier::new(main_uri.clone()),
        })
        .expect("close"),
    );
    let d_c1 = receive_diagnostics(&client);
    let d_c2 = receive_diagnostics(&client);
    assert!(d_c1.diagnostics.is_empty(), "d_c1: {:?}", d_c1.diagnostics);
    assert!(d_c2.diagnostics.is_empty(), "d_c2: {:?}", d_c2.diagnostics);

    shutdown(&client);
    server_thread
        .join()
        .expect("server thread")
        .expect("clean exit");
    let _ = std::fs::remove_dir_all(&tmp_dir);
}
