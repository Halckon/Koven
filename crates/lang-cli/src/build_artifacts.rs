//! 单次构建的私有 staging 与最终 no-clobber 发布；不删除已公开的调试包。

use std::os::unix::fs::{DirBuilderExt, MetadataExt};
#[cfg(target_os = "macos")]
use std::{ffi::CString, os::unix::ffi::OsStrExt, process::Command};
use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_STAGE: AtomicU64 = AtomicU64::new(0);

pub(crate) struct BuildArtifacts {
    directory: Option<PathBuf>,
    directory_identity: (u64, u64),
    object: PathBuf,
    executable: PathBuf,
    #[cfg(target_os = "macos")]
    debug_bundle: PathBuf,
}

impl BuildArtifacts {
    pub(crate) fn reserve(output: &Path) -> io::Result<Self> {
        require_absent(output)?;
        #[cfg(target_os = "macos")]
        require_absent(&debug_bundle_path(output))?;
        let parent = output
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let name = output.file_name().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "output has no file name")
        })?;
        for _ in 0..128 {
            let nonce = NEXT_STAGE.fetch_add(1, Ordering::Relaxed);
            let directory = parent.join(format!(".kovenc-stage-{}-{nonce}", std::process::id()));
            match fs::DirBuilder::new().mode(0o700).create(&directory) {
                Ok(()) => {
                    let executable = directory.join("bin").join(name);
                    let object = directory.join("module.o");
                    #[cfg(target_os = "macos")]
                    let debug_bundle = directory.join("debug.dSYM");
                    let metadata = fs::symlink_metadata(&directory).map_err(|error| {
                        io::Error::new(error.kind(), format!(
                            "cannot confirm private staging ownership: {error}; staging retained at {}",
                            directory.display(),
                        ))
                    })?;
                    let directory_identity = (metadata.dev(), metadata.ino());
                    let bin = directory.join("bin");
                    let mut artifacts = Self {
                        directory: Some(directory),
                        directory_identity,
                        object,
                        executable,
                        #[cfg(target_os = "macos")]
                        debug_bundle,
                    };
                    if let Err(error) = fs::create_dir(bin) {
                        let cleanup = artifacts.cleanup();
                        return Err(io::Error::new(
                            error.kind(),
                            format!(
                                "cannot create private build directory: {error}; cleanup: {cleanup:?}"
                            ),
                        ));
                    }
                    return Ok(artifacts);
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "cannot reserve private build staging",
        ))
    }

    pub(crate) fn object(&self) -> &Path {
        &self.object
    }
    pub(crate) fn executable(&self) -> &Path {
        &self.executable
    }

    /// 调试信息收集时对象仍存在；最终 executable 仅在调试包就绪后发布。
    pub(crate) fn publish(&mut self, output: &Path) -> io::Result<()> {
        #[cfg(target_os = "macos")]
        return self.publish_with_debug_tool(output, Path::new("/usr/bin/dsymutil"));
        #[cfg(not(target_os = "macos"))]
        self.publish_prepared(output)
    }

    #[cfg(target_os = "macos")]
    fn publish_with_debug_tool(&mut self, output: &Path, tool: &Path) -> io::Result<()> {
        let collected = Command::new(tool)
            .arg(&self.executable)
            .arg("-o")
            .arg(&self.debug_bundle)
            .output()
            .map_err(|error| {
                io::Error::new(
                    error.kind(),
                    format!(
                        "cannot launch debug information tool {}: {error}",
                        tool.display()
                    ),
                )
            })?;
        if !collected.status.success() {
            return Err(io::Error::other(format!(
                "debug information collection failed ({:?}): {}",
                collected.status.code(),
                String::from_utf8_lossy(&collected.stderr)
            )));
        }
        self.validate_debug_bundle()?;
        self.publish_prepared(output)
    }

    #[cfg(target_os = "macos")]
    fn validate_debug_bundle(&self) -> io::Result<()> {
        let contents = self.debug_bundle.join("Contents");
        let resources = contents.join("Resources");
        let dwarf = resources.join("DWARF");
        let invalid = |path: &Path| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "incomplete debug bundle: expected directory or nonempty regular file at {}",
                    path.display()
                ),
            )
        };
        for directory in [&self.debug_bundle, &contents, &resources, &dwarf] {
            let metadata = fs::symlink_metadata(directory).map_err(|_| invalid(directory))?;
            if !metadata.is_dir() {
                return Err(invalid(directory));
            }
        }
        for file in [
            contents.join("Info.plist"),
            dwarf.join(
                self.executable
                    .file_name()
                    .ok_or_else(|| invalid(&self.executable))?,
            ),
        ] {
            let metadata = fs::symlink_metadata(&file).map_err(|_| invalid(&file))?;
            if !metadata.is_file() || metadata.len() == 0 {
                return Err(invalid(&file));
            }
        }
        Ok(())
    }

    fn publish_prepared(&mut self, output: &Path) -> io::Result<()> {
        self.publish_prepared_with_cleanup(output, Self::cleanup)
    }

    fn publish_prepared_with_cleanup(
        &mut self,
        output: &Path,
        cleanup: impl FnOnce(&mut Self) -> io::Result<()>,
    ) -> io::Result<()> {
        #[cfg(target_os = "macos")]
        publish_directory(&self.debug_bundle, &debug_bundle_path(output))?;
        // hard_link 是最终单路径 no-clobber commit，不能由事先的 exists 检查替代。
        let publication = fs::hard_link(&self.executable, output);
        #[cfg(target_os = "macos")]
        let publication = publication.map_err(|error| {
            io::Error::new(
                error.kind(),
                format!(
                    "cannot publish executable: {error}; published debug bundle retained at {}",
                    debug_bundle_path(output).display()
                ),
            )
        });
        publication?;
        cleanup(self).map_err(|error| io::Error::new(error.kind(), format!(
            "final build artifacts already published at {}; private staging cleanup failed: {error}",
            output.display(),
        )))
    }

    /// 仅清理本轮私有 staging；公开调试包不属于回滚删除对象。
    pub(crate) fn cleanup(&mut self) -> io::Result<()> {
        let Some(directory) = &self.directory else {
            return Ok(());
        };
        let metadata = match fs::symlink_metadata(directory) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                self.directory = None;
                return Ok(());
            }
            Err(error) => return Err(error),
        };
        if !metadata.is_dir() || (metadata.dev(), metadata.ino()) != self.directory_identity {
            let error = io::Error::other(format!(
                "private build staging ownership changed; path left untouched: {}",
                directory.display()
            ));
            self.directory = None;
            return Err(error);
        }
        // 私有 staging 由本轮独占；此检查用于报告已观测到的异常，不声称消除 TOCTOU。
        match fs::remove_dir_all(directory) {
            Ok(()) => {
                self.directory = None;
                Ok(())
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                self.directory = None;
                Ok(())
            }
            Err(error) => Err(error),
        }
    }
}

impl Drop for BuildArtifacts {
    fn drop(&mut self) {
        let _ = self.cleanup();
    }
}

fn require_absent(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("output already exists: {}", path.display()),
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(target_os = "macos")]
fn debug_bundle_path(output: &Path) -> PathBuf {
    let mut name = output.as_os_str().to_os_string();
    name.push(".dSYM");
    PathBuf::from(name)
}

#[cfg(target_os = "macos")]
fn publish_directory(source: &Path, target: &Path) -> io::Result<()> {
    let source = CString::new(source.as_os_str().as_bytes())?;
    let target = CString::new(target.as_os_str().as_bytes())?;
    // SAFETY: 两个 CString 在调用期间有效且 NUL 终止；RENAME_EXCL 来自 macOS SDK sys/stdio.h。
    let status = unsafe { libc::renamex_np(source.as_ptr(), target.as_ptr(), libc::RENAME_EXCL) };
    if status == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn successful_cleanup_disarms_drop_before_directory_name_is_reused() {
        let output =
            std::env::temp_dir().join(format!("kovenc-cleanup-witness-{}", std::process::id()));
        let mut artifacts = BuildArtifacts::reserve(&output).expect("reserve staging");
        let stage = artifacts
            .directory
            .as_ref()
            .expect("owned directory")
            .clone();
        artifacts.cleanup().expect("first cleanup");
        fs::create_dir(&stage).expect("another owner reuses released name");
        let sentinel = stage.join("other-owner.txt");
        fs::write(&sentinel, "keep").expect("sentinel write");
        drop(artifacts);
        assert_eq!(
            fs::read_to_string(&sentinel).expect("other owner must survive"),
            "keep"
        );
        fs::remove_dir_all(&stage).expect("fixture cleanup");
    }
}

#[cfg(test)]
mod replacement_tests {
    use super::*;

    #[test]
    fn observed_staging_replacement_is_reported_and_preserved() {
        let output =
            std::env::temp_dir().join(format!("kovenc-replaced-stage-{}", std::process::id()));
        let mut artifacts = BuildArtifacts::reserve(&output).expect("reserve");
        let stage = artifacts.directory.as_ref().expect("owned stage").clone();
        let moved = stage.with_extension("moved");
        fs::rename(&stage, &moved).expect("move original");
        fs::create_dir(&stage).expect("foreign replacement");
        let sentinel = stage.join("other-owner.txt");
        fs::write(&sentinel, "keep").expect("sentinel");
        assert!(
            artifacts
                .cleanup()
                .expect_err("replacement must be reported")
                .to_string()
                .contains("ownership changed")
        );
        drop(artifacts);
        assert_eq!(
            fs::read_to_string(sentinel).expect("foreign entry retained"),
            "keep"
        );
        fs::remove_dir_all(stage).expect("fixture cleanup");
        fs::remove_dir_all(moved).expect("original fixture cleanup");
    }
}

#[cfg(test)]
mod publication_tests {
    use super::*;

    struct Fixture(PathBuf);
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn prepared() -> (Fixture, BuildArtifacts, PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "kovenc-publish-test-{}-{}",
            std::process::id(),
            NEXT_STAGE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).expect("fixture");
        let fixture = Fixture(root);
        let output = fixture.0.join("program");
        let artifacts = BuildArtifacts::reserve(&output).expect("reserve");
        fs::write(artifacts.executable(), b"complete executable").expect("seed publication unit");
        #[cfg(target_os = "macos")]
        fs::create_dir(&artifacts.debug_bundle).expect("seed prepared debug bundle");
        (fixture, artifacts, output)
    }

    #[cfg(target_os = "macos")]
    fn debug_tool(fixture: &Fixture, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let tool = fixture.0.join("debug-tool");
        fs::write(&tool, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(&tool, fs::Permissions::from_mode(0o700)).unwrap();
        tool
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn malformed_successful_debug_tool_cannot_publish_final_outputs() {
        for malformed in [
            ":",                                                       // Missing DWARF file.
            "touch \"$3/Contents/Resources/DWARF/program\"",           // Empty file.
            "mkdir \"$3/Contents/Resources/DWARF/program\"", // Directory at the file path.
            "ln -s /dev/null \"$3/Contents/Resources/DWARF/program\"", // Symbolic link.
            "printf data > \"$3/Contents/Resources/DWARF/wrong-name\"",
        ] {
            let (fixture, mut artifacts, output) = prepared();
            let tool = debug_tool(
                &fixture,
                &format!(
                    "mkdir -p \"$3/Contents/Resources/DWARF\"\nprintf metadata > \"$3/Contents/Info.plist\"\n{malformed}"
                ),
            );
            let stage = artifacts.directory.as_ref().unwrap().clone();
            let error = artifacts
                .publish_with_debug_tool(&output, &tool)
                .expect_err("tool success with malformed DWARF must not publish");
            assert!(error.to_string().contains("debug bundle"), "{error}");
            assert!(!output.exists());
            assert!(!debug_bundle_path(&output).exists());
            artifacts.cleanup().unwrap();
            assert!(!stage.exists());
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn debug_tool_launch_and_exit_failures_preserve_old_outputs_and_cleanup_stage() {
        for launch_failure in [true, false] {
            let (fixture, mut artifacts, output) = prepared();
            // Other owners may create output paths after reservation. Tool failure
            // must not touch either path, even if it made a partial private bundle.
            fs::write(&output, b"old executable").unwrap();
            let old_bundle = debug_bundle_path(&output);
            fs::create_dir(&old_bundle).unwrap();
            fs::write(old_bundle.join("sentinel"), b"old debug data").unwrap();
            let tool = if launch_failure {
                fixture.0.join("missing-tool")
            } else {
                debug_tool(
                    &fixture,
                    "mkdir -p \"$3/Contents\"; printf partial > \"$3/Contents/partial\"; printf tool-failure >&2; exit 9",
                )
            };
            let stage = artifacts.directory.as_ref().unwrap().clone();
            let error = artifacts
                .publish_with_debug_tool(&output, &tool)
                .unwrap_err();
            if launch_failure {
                assert_eq!(error.kind(), io::ErrorKind::NotFound);
                assert!(
                    error
                        .to_string()
                        .contains("cannot launch debug information tool")
                );
            } else {
                assert!(error.to_string().contains("Some(9)"), "{error}");
                assert!(error.to_string().contains("tool-failure"), "{error}");
            }
            assert_eq!(fs::read(&output).unwrap(), b"old executable");
            assert_eq!(
                fs::read(old_bundle.join("sentinel")).unwrap(),
                b"old debug data"
            );
            artifacts.cleanup().unwrap();
            assert!(!stage.exists());
        }
    }

    #[test]
    fn racing_executable_is_preserved_and_partial_debug_publication_is_reported() {
        let (_fixture, mut artifacts, output) = prepared();
        fs::write(&output, b"race winner").expect("race winner");
        let error = artifacts.publish_prepared(&output).expect_err("no clobber");
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read(&output).expect("winner survives"), b"race winner");
        #[cfg(target_os = "macos")]
        {
            assert!(error.to_string().contains("debug bundle retained"));
            artifacts.cleanup().expect("private cleanup");
            assert!(
                debug_bundle_path(&output).is_dir(),
                "public bundle must not be deleted"
            );
        }
    }

    #[test]
    fn published_output_is_reported_when_private_cleanup_fails() {
        let (_fixture, mut artifacts, output) = prepared();
        let error = artifacts
            .publish_prepared_with_cleanup(&output, |_| {
                Err(io::Error::other("injected staging cleanup failure"))
            })
            .expect_err("cleanup failure must remain observable");
        assert!(error.to_string().contains("already published"));
        assert!(
            error
                .to_string()
                .contains("injected staging cleanup failure")
        );
        assert_eq!(
            fs::read(&output).expect("published output"),
            b"complete executable"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn racing_empty_debug_directory_is_not_replaced() {
        let (_fixture, mut artifacts, output) = prepared();
        let bundle = debug_bundle_path(&output);
        fs::create_dir(&bundle).expect("other owner empty bundle");
        let metadata = fs::symlink_metadata(&bundle).expect("identity");
        let error = artifacts
            .publish_prepared(&output)
            .expect_err("must reject existing directory");
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(
            fs::symlink_metadata(&bundle).expect("still present").ino(),
            metadata.ino()
        );
        assert!(
            !output.exists(),
            "no executable without committed debug bundle"
        );
    }
}
