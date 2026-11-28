//! `UnitSession` 热重载辅助：`didSave` 基线刷新与 `workspace/didChangeWatchedFiles` 重建。

use std::collections::BTreeMap;

use lsp_types::Uri;

use super::{UnitSession, UnitSessionError, UnitSnapshot, UnitUpdate};
use crate::{source_set::SourceSetConfig, workspace_manifest};

impl UnitSession {
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

    /// 以重新发现的 `SourceSetConfig` 热重建 `UnitSession`，保留仍存在的打开 overlay，并返回已移除 URI 列表。
    pub(crate) fn rebuild_from_config(
        &self,
        new_config: SourceSetConfig,
        workspace_manifest: bool,
    ) -> Result<(Self, Vec<Uri>), UnitSessionError> {
        let mut overlays = BTreeMap::new();
        for (uri_str, overlay) in &self.overlays {
            if new_config
                .sources()
                .iter()
                .any(|s| s.uri().as_str() == uri_str)
            {
                overlays.insert(uri_str.clone(), overlay.clone());
            }
        }
        let removed_uris: Vec<Uri> = self
            .config
            .sources()
            .iter()
            .filter(|old| {
                !new_config
                    .sources()
                    .iter()
                    .any(|new_s| new_s.uri().as_str() == old.uri().as_str())
            })
            .map(|old| old.uri().clone())
            .collect();
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
        Ok((
            Self {
                config: new_config,
                overlays,
                snapshot,
                enrich_diagnostics: self.enrich_diagnostics,
                disk_backed_uris,
            },
            removed_uris,
        ))
    }
}
