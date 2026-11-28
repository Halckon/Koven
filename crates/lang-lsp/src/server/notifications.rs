//! `workspace/didChangeWatchedFiles` 热重载处理（独立拆分以保持 `server.rs` 精简）。

use std::collections::BTreeMap;

use lsp_server::{Connection, Notification};
use lsp_types::{DidChangeWatchedFilesParams, FileChangeType, InitializeParams};

use super::{
    NotificationError, OpenDocument, ServerOptions, SessionState, publish, publish_document,
    publish_unit,
};
use crate::{unit_session::UnitSession, workspace_manifest};

fn collect_open_overlays(docs: &BTreeMap<String, OpenDocument>) -> BTreeMap<String, (i32, String)> {
    let mut overlays = BTreeMap::new();
    for (uri_str, doc) in docs {
        if let Some(text) = doc.text() {
            overlays.insert(uri_str.clone(), (doc.version(), text.to_owned()));
        }
    }
    overlays
}

pub(super) fn handle_watched_files(
    connection: &Connection,
    notification: Notification,
    state: &mut SessionState,
    options: ServerOptions,
    initialize_params: &InitializeParams,
) -> Result<(), NotificationError> {
    let params: DidChangeWatchedFilesParams = serde_json::from_value(notification.params)?;
    let needs_full_reload = params.changes.iter().any(|ev| {
        ev.uri.as_str().ends_with("/project.toml")
            || matches!(ev.typ, FileChangeType::CREATED | FileChangeType::DELETED)
    });
    if needs_full_reload {
        let Ok(discovered) = workspace_manifest::discover_source_set(initialize_params) else {
            return Ok(());
        };
        let Some(new_config) = discovered else {
            if let SessionState::Unit { session, fallback } = state {
                let (open_docs, closed_uris) = session.partition_uris();
                for closed in closed_uris {
                    publish(connection, closed, None, Vec::new())?;
                }
                let mut docs = std::mem::take(fallback);
                for detached in open_docs {
                    let doc =
                        OpenDocument::analyze(&detached.uri, detached.version, &detached.text)?;
                    publish_document(connection, &detached.uri, &doc, options.diagnostic_context)?;
                    docs.insert(detached.uri.as_str().to_owned(), doc);
                }
                *state = SessionState::SingleDocument(docs);
            }
            return Ok(());
        };
        match state {
            SessionState::Unit { session, fallback } => {
                let extra_overlays = collect_open_overlays(fallback);
                let outcome = session.rebuild_from_config(
                    new_config,
                    extra_overlays,
                    options.workspace_manifest,
                )?;
                let publications = outcome.session.publications()?;
                for removed in outcome.removed_closed_uris {
                    publish(connection, removed, None, Vec::new())?;
                }
                for p in &publications {
                    fallback.remove(p.uri.as_str());
                }
                publish_unit(connection, &publications)?;
                for detached in outcome.detached_open_docs {
                    let doc =
                        OpenDocument::analyze(&detached.uri, detached.version, &detached.text)?;
                    publish_document(connection, &detached.uri, &doc, options.diagnostic_context)?;
                    fallback.insert(detached.uri.as_str().to_owned(), doc);
                }
                **session = outcome.session;
            }
            SessionState::SingleDocument(docs) => {
                let initial_overlays = collect_open_overlays(docs);
                let rebuilt = UnitSession::new_with_overlays(
                    new_config,
                    initial_overlays,
                    options.diagnostic_context,
                    options.workspace_manifest,
                )?;
                let publications = rebuilt.publications()?;
                publish_unit(connection, &publications)?;
                let mut fallback = std::mem::take(docs);
                for p in &publications {
                    fallback.remove(p.uri.as_str());
                }
                *state = SessionState::Unit {
                    session: Box::new(rebuilt),
                    fallback,
                };
            }
        }
        return Ok(());
    }
    if let SessionState::Unit { session, .. } = state {
        for ev in params.changes {
            if session.contains_uri(&ev.uri)
                && session.open_document_version(&ev.uri).is_none()
                && let Some(path) = workspace_manifest::file_uri_to_path(&ev.uri)
                && let Ok(disk_text) = std::fs::read_to_string(&path)
                && let Some(update) = session.prepare_base_refresh(&ev.uri, disk_text)?
            {
                publish_unit(connection, update.publications())?;
                session.commit(update);
            }
        }
    }
    Ok(())
}
