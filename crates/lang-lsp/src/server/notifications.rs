//! `workspace/didChangeWatchedFiles` 热重载处理（独立拆分以保持 `server.rs` 精简）。

use lsp_server::{Connection, Notification};
use lsp_types::{DidChangeWatchedFilesParams, FileChangeType, InitializeParams};

use super::{NotificationError, ServerOptions, SessionState, publish, publish_unit};
use crate::{unit_session::UnitSession, workspace_manifest};

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
        let Ok(Some(new_config)) = workspace_manifest::discover_source_set(initialize_params)
        else {
            return Ok(());
        };
        match state {
            SessionState::Unit { session, fallback } => {
                let (rebuilt, removed_uris) =
                    session.rebuild_from_config(new_config, options.workspace_manifest)?;
                let publications = rebuilt.publications()?;
                for removed in removed_uris {
                    publish(connection, removed, None, Vec::new())?;
                }
                for p in &publications {
                    fallback.remove(p.uri.as_str());
                }
                publish_unit(connection, &publications)?;
                **session = rebuilt;
            }
            SessionState::SingleDocument(docs) => {
                let rebuilt = UnitSession::new_with_options(
                    new_config,
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
