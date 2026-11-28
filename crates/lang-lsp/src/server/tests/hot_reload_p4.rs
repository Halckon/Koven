use super::*;
use lsp_types::{
    DidChangeWatchedFilesParams, FileChangeType, FileEvent, Range, WorkspaceFolder,
    notification::DidChangeWatchedFiles,
};

#[test]
fn p4_watched_files_detaching_open_doc_from_manifest_preserves_unsaved_text_and_version() {
    let tmp_dir =
        std::env::temp_dir().join(format!("koven_lsp_p4_detach_open_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp_dir);
    std::fs::create_dir_all(tmp_dir.join("src")).expect("mkdir src");
    std::fs::create_dir_all(tmp_dir.join("extra")).expect("mkdir extra");
    let tmp_dir = std::fs::canonicalize(&tmp_dir).expect("canonical tmp");

    let manifest_path = tmp_dir.join("project.toml");
    std::fs::write(
        &manifest_path,
        "schema = \"koven.project\"\nversion = 1\n\n[project]\nname = \"detach\"\nsource-roots = [\"src\", \"extra\"]\n",
    )
    .expect("manifest");
    let main_path = tmp_dir.join("src/main.ko");
    std::fs::write(&main_path, "public fun shared(): Int = 1\n").expect("write main");
    let b_path = tmp_dir.join("extra/b.ko");
    let disk_b = "fun other() {\n    val x = 1\n}\n";
    std::fs::write(&b_path, disk_b).expect("write b");

    let root_uri: lsp_types::Uri = format!("file://{}", tmp_dir.display())
        .parse()
        .expect("uri");
    let manifest_uri: lsp_types::Uri = format!("file://{}", manifest_path.display())
        .parse()
        .expect("uri");
    let b_uri: lsp_types::Uri = format!("file://{}", b_path.display()).parse().expect("uri");

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
                    name: "detach".to_owned(),
                }]),
                ..InitializeParams::default()
            })
            .expect("params"),
        }))
        .expect("init");
    let _ = receive_response(&client);
    send_notification(
        &client,
        Initialized::METHOD,
        serde_json::to_value(InitializedParams {}).expect("initialized"),
    );
    let _ = receive_publications(&client, 2);

    // 1. Open `extra/b.ko` and apply an unsaved incremental edit (`1` -> `shared()`).
    send_notification(
        &client,
        DidOpenTextDocument::METHOD,
        serde_json::to_value(DidOpenTextDocumentParams {
            text_document: TextDocumentItem::new(
                b_uri.clone(),
                "koven".to_owned(),
                1,
                disk_b.to_owned(),
            ),
        })
        .expect("open"),
    );
    let _ = receive_publications(&client, 2);

    send_notification(
        &client,
        DidChangeTextDocument::METHOD,
        serde_json::to_value(DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier::new(b_uri.clone(), 2),
            content_changes: vec![TextDocumentContentChangeEvent {
                range: Some(Range::new(Position::new(1, 12), Position::new(1, 13))),
                range_length: None,
                text: "shared()".to_owned(),
            }],
        })
        .expect("unsaved change"),
    );
    let v2_pubs = receive_publications(&client, 2);
    let b_v2 = v2_pubs
        .iter()
        .find(|p| p.uri == b_uri)
        .expect("b.ko v2 publication");
    assert_eq!(b_v2.version, Some(2));
    assert!(
        b_v2.diagnostics.is_empty(),
        "unsaved `shared()` should resolve in Unit mode: {:?}",
        b_v2.diagnostics
    );

    // 2. Remove `"extra"` from `project.toml` source-roots and notify watched files.
    std::fs::write(
        &manifest_path,
        "schema = \"koven.project\"\nversion = 1\n\n[project]\nname = \"detach\"\nsource-roots = [\"src\"]\n",
    )
    .expect("update manifest");
    send_notification(
        &client,
        DidChangeWatchedFiles::METHOD,
        serde_json::to_value(DidChangeWatchedFilesParams {
            changes: vec![FileEvent::new(manifest_uri, FileChangeType::CHANGED)],
        })
        .expect("watched"),
    );
    // Expect 1 unit publication (`src/main.ko`) + 1 single-document publication (`extra/b.ko`).
    let detach_pubs = receive_publications(&client, 2);
    let b_detached = detach_pubs
        .iter()
        .find(|p| p.uri == b_uri)
        .expect("detached b.ko publication");
    assert_eq!(
        b_detached.version,
        Some(2),
        "detached open document must retain version 2 in single-document fallback"
    );
    assert!(
        !b_detached.diagnostics.is_empty(),
        "detached unsaved `shared()` should now fail in single-document mode (proving unsaved text was preserved)"
    );

    // 3. Next incremental `didChange` on detached `extra/b.ko` must not be dropped and must splice against `shared()`.
    send_notification(
        &client,
        DidChangeTextDocument::METHOD,
        serde_json::to_value(DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier::new(b_uri.clone(), 3),
            content_changes: vec![TextDocumentContentChangeEvent {
                range: Some(Range::new(Position::new(1, 12), Position::new(1, 20))),
                range_length: None,
                text: "2".to_owned(),
            }],
        })
        .expect("inc change after detach"),
    );
    let b_v3 = receive_diagnostics(&client);
    assert_eq!(b_v3.uri, b_uri);
    assert_eq!(b_v3.version, Some(3));
    assert!(
        b_v3.diagnostics.is_empty(),
        "incremental change after detaching from manifest should resolve cleanly: {:?}",
        b_v3.diagnostics
    );

    shutdown(&client);
    server_thread
        .join()
        .expect("server thread")
        .expect("clean exit");
    let _ = std::fs::remove_dir_all(&tmp_dir);
}

#[test]
fn p4_watched_files_does_not_override_explicit_source_set() {
    let tmp_dir =
        std::env::temp_dir().join(format!("koven_lsp_p4_explicit_ss_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp_dir);
    std::fs::create_dir_all(tmp_dir.join("src")).expect("mkdir");
    let tmp_dir = std::fs::canonicalize(&tmp_dir).expect("canonical tmp");

    let manifest_path = tmp_dir.join("project.toml");
    std::fs::write(
        &manifest_path,
        "schema = \"koven.project\"\nversion = 1\n\n[project]\nname = \"disk\"\nsource-roots = [\"src\"]\n",
    )
    .expect("manifest");
    std::fs::write(
        tmp_dir.join("src/disk.ko"),
        "fun diskBroken() {\n    val z = nope\n}\n",
    )
    .expect("write disk");

    let root_uri: lsp_types::Uri = format!("file://{}", tmp_dir.display())
        .parse()
        .expect("uri");
    let manifest_uri: lsp_types::Uri = format!("file://{}", manifest_path.display())
        .parse()
        .expect("uri");
    let explicit_uri: lsp_types::Uri = "file:///explicit/main.ko".parse().expect("uri");
    let explicit_src = "fun main() {\n    val x = 1\n}\n";

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

    let mut init_params = serde_json::to_value(InitializeParams {
        workspace_folders: Some(vec![WorkspaceFolder {
            uri: root_uri,
            name: "disk".to_owned(),
        }]),
        ..InitializeParams::default()
    })
    .expect("params");
    init_params["initializationOptions"] = serde_json::json!({"koven": {"sourceSet": {
        "schema": "koven.lsp.source-set",
        "version": 1,
        "roots": ["src"],
        "sources": [
            {"root": "src", "logicalPath": "main.ko", "uri": explicit_uri.as_str(), "text": explicit_src}
        ]
    }}});
    client
        .sender
        .send(Message::Request(Request {
            id: RequestId::from(1_i32),
            method: Initialize::METHOD.to_owned(),
            params: init_params,
        }))
        .expect("init");
    let _ = receive_response(&client);
    send_notification(
        &client,
        Initialized::METHOD,
        serde_json::to_value(InitializedParams {}).expect("initialized"),
    );
    let init_diag = receive_diagnostics(&client);
    assert_eq!(init_diag.uri, explicit_uri);
    assert!(init_diag.diagnostics.is_empty());

    send_notification(
        &client,
        DidOpenTextDocument::METHOD,
        serde_json::to_value(DidOpenTextDocumentParams {
            text_document: TextDocumentItem::new(
                explicit_uri.clone(),
                "koven".to_owned(),
                1,
                explicit_src.to_owned(),
            ),
        })
        .expect("open"),
    );
    let open_diag = receive_diagnostics(&client);
    assert_eq!(open_diag.uri, explicit_uri);
    assert_eq!(open_diag.version, Some(1));

    // Send watched-files CHANGED and DELETED for disk `project.toml`; neither may override explicit `koven.sourceSet`.
    send_notification(
        &client,
        DidChangeWatchedFiles::METHOD,
        serde_json::to_value(DidChangeWatchedFilesParams {
            changes: vec![FileEvent::new(
                manifest_uri.clone(),
                FileChangeType::CHANGED,
            )],
        })
        .expect("watched changed"),
    );
    std::fs::remove_file(&manifest_path).expect("remove manifest");
    send_notification(
        &client,
        DidChangeWatchedFiles::METHOD,
        serde_json::to_value(DidChangeWatchedFilesParams {
            changes: vec![FileEvent::new(manifest_uri, FileChangeType::DELETED)],
        })
        .expect("watched deleted"),
    );

    // Subsequent incremental change on `explicit_uri` must still receive diagnostics directly without any disk publication in between.
    send_notification(
        &client,
        DidChangeTextDocument::METHOD,
        serde_json::to_value(DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier::new(explicit_uri.clone(), 2),
            content_changes: vec![TextDocumentContentChangeEvent {
                range: Some(Range::new(Position::new(1, 12), Position::new(1, 13))),
                range_length: None,
                text: "42".to_owned(),
            }],
        })
        .expect("inc change"),
    );
    let next_diag = receive_diagnostics(&client);
    assert_eq!(
        next_diag.uri, explicit_uri,
        "explicit sourceSet must not be overridden or replaced by disk project.toml reload"
    );
    assert_eq!(next_diag.version, Some(2));
    assert!(next_diag.diagnostics.is_empty());

    shutdown(&client);
    server_thread
        .join()
        .expect("server thread")
        .expect("clean exit");
    let _ = std::fs::remove_dir_all(&tmp_dir);
}
