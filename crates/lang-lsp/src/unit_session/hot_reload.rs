//! `UnitSession` 热重载辅助：`didSave` 基线刷新与 `workspace/didChangeWatchedFiles` 重建。

use std::collections::BTreeMap;

use lsp_types::Uri;

use super::{Overlay, UnitSession, UnitSessionError, UnitSnapshot, UnitUpdate};
use crate::{source_set::SourceSetConfig, workspace_manifest};

pub(crate) struct DetachedOpenDoc {
    pub(crate) uri: Uri,
    pub(crate) version: i32,
    pub(crate) text: String,
}

pub(crate) struct UnitRebuildOutcome {
    pub(crate) session: UnitSession,
    pub(crate) removed_closed_uris: Vec<Uri>,
    pub(crate) detached_open_docs: Vec<DetachedOpenDoc>,
}

impl UnitSession {
    /// 以指定初始 overlays 构造 `UnitSession`（仅纳入属于 `config` 的 URI），用于单文档模式升格到工作区时保留未保存内容与版本。
    pub(crate) fn new_with_overlays(
        config: SourceSetConfig,
        initial_overlays: BTreeMap<String, (i32, String)>,
        enrich_diagnostics: bool,
        workspace_manifest: bool,
    ) -> Result<Self, UnitSessionError> {
        let mut overlays = BTreeMap::new();
        for source in config.sources() {
            if let Some((version, text)) = initial_overlays.get(source.uri().as_str()) {
                overlays.insert(
                    source.uri().as_str().to_owned(),
                    Overlay {
                        version: *version,
                        text: text.clone(),
                    },
                );
            }
        }
        let snapshot = UnitSnapshot::analyze(&config, &overlays)?;
        let disk_backed_uris = config
            .sources()
            .iter()
            .filter(|source| {
                workspace_manifest
                    || workspace_manifest::file_uri_to_path(source.uri())
                        .is_some_and(|path| path.exists())
            })
            .map(|source| source.uri().as_str().to_owned())
            .collect();
        Ok(Self {
            config,
            overlays,
            snapshot,
            enrich_diagnostics,
            disk_backed_uris,
        })
    }

    /// 刷新指定 URI 的 base 文本（用于 `didSave` 或已关闭磁盘文件外部变更）。
    pub(crate) fn prepare_base_refresh(
        &self,
        uri: &Uri,
        new_base_text: String,
    ) -> Result<Option<UnitUpdate>, UnitSessionError> {
        let mut config = self.config.clone();
        if !config.update_base_text(uri, new_base_text) {
            return Ok(None);
        }
        let overlays = self.overlays.clone();
        let snapshot = UnitSnapshot::analyze(&config, &overlays)?;
        let publications =
            snapshot.publications_with_options(&config, &overlays, self.enrich_diagnostics)?;
        Ok(Some(UnitUpdate {
            config: Some(config),
            overlays,
            snapshot,
            publications,
        }))
    }

    /// 以重新发现的 `SourceSetConfig` 热重建 `UnitSession`：
    /// - 保留仍属于 `new_config` 的已有 overlay，并纳入来自单文档回退区的 `extra_overlays`；
    /// - 返回重建结果、需清诊断的已关闭 URI 与需迁回单文档区的已打开文档。
    pub(crate) fn rebuild_from_config(
        &self,
        new_config: SourceSetConfig,
        extra_overlays: BTreeMap<String, (i32, String)>,
        workspace_manifest: bool,
    ) -> Result<UnitRebuildOutcome, UnitSessionError> {
        let mut overlays = BTreeMap::new();
        for source in new_config.sources() {
            let uri_str = source.uri().as_str();
            if let Some(overlay) = self.overlays.get(uri_str) {
                overlays.insert(uri_str.to_owned(), overlay.clone());
            } else if let Some((version, text)) = extra_overlays.get(uri_str) {
                overlays.insert(
                    uri_str.to_owned(),
                    Overlay {
                        version: *version,
                        text: text.clone(),
                    },
                );
            }
        }
        let mut removed_closed_uris = Vec::new();
        let mut detached_open_docs = Vec::new();
        for old in self.config.sources() {
            let still_present = new_config
                .sources()
                .iter()
                .any(|new_s| new_s.uri().as_str() == old.uri().as_str());
            if !still_present {
                if let Some(overlay) = self.overlays.get(old.uri().as_str()) {
                    detached_open_docs.push(DetachedOpenDoc {
                        uri: old.uri().clone(),
                        version: overlay.version,
                        text: overlay.text.clone(),
                    });
                } else {
                    removed_closed_uris.push(old.uri().clone());
                }
            }
        }
        let snapshot = UnitSnapshot::analyze(&new_config, &overlays)?;
        let disk_backed_uris = new_config
            .sources()
            .iter()
            .filter(|source| {
                workspace_manifest
                    || workspace_manifest::file_uri_to_path(source.uri())
                        .is_some_and(|path| path.exists())
            })
            .map(|source| source.uri().as_str().to_owned())
            .collect();
        Ok(UnitRebuildOutcome {
            session: Self {
                config: new_config,
                overlays,
                snapshot,
                enrich_diagnostics: self.enrich_diagnostics,
                disk_backed_uris,
            },
            removed_closed_uris,
            detached_open_docs,
        })
    }

    /// 退出工作区模式时，将当前单元 URI 拆分为仍打开的文档与已关闭 URI。
    pub(crate) fn partition_uris(&self) -> (Vec<DetachedOpenDoc>, Vec<Uri>) {
        let mut open_docs = Vec::new();
        let mut closed_uris = Vec::new();
        for source in self.config.sources() {
            if let Some(overlay) = self.overlays.get(source.uri().as_str()) {
                open_docs.push(DetachedOpenDoc {
                    uri: source.uri().clone(),
                    version: overlay.version,
                    text: overlay.text.clone(),
                });
            } else {
                closed_uris.push(source.uri().clone());
            }
        }
        (open_docs, closed_uris)
    }
}
