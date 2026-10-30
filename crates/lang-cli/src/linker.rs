//! Host-native linker driver boundary.

use crate::run_control::{RunControl, RunOutcome};

use std::{
    io,
    path::{Path, PathBuf},
    process::Command,
};

const NATIVE_LINK_DRIVER: &str = if cfg!(target_os = "linux") {
    "/usr/bin/cc"
} else {
    "/usr/bin/clang"
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum LinkerError {
    Cancelled {
        signal: i32,
    },
    Launch {
        driver: PathBuf,
        kind: io::ErrorKind,
    },
    Failure {
        status: Option<i32>,
        stderr: Vec<u8>,
    },
}

/// Links one caller-owned native object into one caller-owned executable path.
pub(crate) fn link_native_object(
    object: &Path,
    executable: &Path,
    control: Option<&RunControl>,
) -> Result<(), LinkerError> {
    link_with_driver(Path::new(NATIVE_LINK_DRIVER), object, executable, control)
}

fn link_with_driver(
    driver: &Path,
    object: &Path,
    executable: &Path,
    control: Option<&RunControl>,
) -> Result<(), LinkerError> {
    let arguments = [
        object.as_os_str().to_owned(),
        "-o".into(),
        executable.as_os_str().to_owned(),
    ];
    // run owns the cancellation scope until its private artifacts are cleaned.
    // build keeps its existing driver lifecycle without installing run handlers.
    let outcome = match control {
        Some(control) => control.output(driver, &arguments),
        None => Command::new(driver)
            .args(&arguments)
            .output()
            .map(RunOutcome::Completed),
    }
    .map_err(|error| LinkerError::Launch {
        driver: driver.to_path_buf(),
        kind: error.kind(),
    })?;
    let output = match outcome {
        RunOutcome::CancelledBeforeStart(signal) => return Err(LinkerError::Cancelled { signal }),
        RunOutcome::Completed(output) => output,
    };
    if let Some(signal) = control.and_then(RunControl::cancelled) {
        return Err(LinkerError::Cancelled { signal });
    }
    if output.status.success() {
        Ok(())
    } else {
        Err(LinkerError::Failure {
            status: output.status.code(),
            stderr: output.stderr,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        process::Command,
        sync::atomic::{AtomicU64, Ordering},
    };

    use super::{LinkerError, NATIVE_LINK_DRIVER, link_native_object, link_with_driver};

    static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn create() -> Self {
            let path = std::env::temp_dir().join(format!(
                "koven linker test-{}-{}",
                std::process::id(),
                NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).expect("test directory must be creatable");
            Self(path)
        }

        fn join(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).expect("owned test directory must be removable");
        }
    }

    #[test]
    fn cancellation_during_linking_stops_the_tool_before_run() {
        let _serial = crate::run_control::tests::SIGNAL_TEST.lock().unwrap();
        let control = crate::run_control::RunControl::install().unwrap();
        let directory = TestDirectory::create();
        let script = directory.join("controlled-linker.sh");
        let executable = directory.join("program");
        // Signal only after the tool starts. Its bounded fallback makes the old
        // synchronous driver fail without hanging the test harness indefinitely.
        fs::write(
            &script,
            "kill -TERM \"$PPID\"; sleep 2; printf late > \"$2\"; exit 0\n",
        )
        .unwrap();
        let result = link_with_driver(
            std::path::Path::new("/bin/sh"),
            &script,
            &executable,
            Some(&control),
        );
        assert_eq!(control.cancelled(), Some(libc::SIGTERM));
        control.finish().unwrap();
        assert!(
            matches!(
                result,
                Err(LinkerError::Cancelled {
                    signal: libc::SIGTERM
                })
            ),
            "cancelled linker must not report success: {result:?}"
        );
        assert!(!executable.exists());
    }

    #[test]
    fn pending_cancellation_survives_compilation_and_prevents_link() {
        let _serial = crate::run_control::tests::SIGNAL_TEST.lock().unwrap();
        let control = crate::run_control::RunControl::install().unwrap();
        let directory = TestDirectory::create();
        let source = directory.join("main.ko");
        let object = directory.join("program.o");
        let executable = directory.join("program");
        fs::write(&source, "fun main(): Unit {}\n").unwrap();
        // SAFETY: SIGTERM has the live RunControl handler; raise targets this
        // test process, and this test holds the shared signal-state lock.
        assert_eq!(unsafe { libc::raise(libc::SIGTERM) }, 0);
        let result = crate::bootstrap::bootstrap_build(
            crate::bootstrap::BootstrapTarget {
                source: &source,
                source_root: None,
                entry: crate::bootstrap::BootstrapEntry::ConventionalMain,
                object: &object,
                executable: &executable,
            },
            Some(&control),
        );
        control.finish().unwrap();
        assert!(matches!(
            result,
            Err(crate::bootstrap::BootstrapError::Linker(
                LinkerError::Cancelled {
                    signal: libc::SIGTERM
                }
            ))
        ));
        assert!(object.is_file(), "the synchronous compilation actually ran");
        assert!(
            !executable.exists(),
            "pending cancellation must suppress linking"
        );
    }

    #[test]
    fn system_driver_links_one_object_without_shell() {
        let directory = TestDirectory::create();
        let source = directory.join("entry.c");
        let object = directory.join("entry.o");
        let executable = directory.join("entry");
        fs::write(&source, "int main(void) { return 0; }").expect("source write");
        let compile = Command::new(NATIVE_LINK_DRIVER)
            .args(["-c", "-x", "c"])
            .arg(&source)
            .arg("-o")
            .arg(&object)
            .output()
            .expect("clang must launch");
        assert!(
            compile.status.success(),
            "clang failed: {}",
            String::from_utf8_lossy(&compile.stderr)
        );

        link_native_object(&object, &executable, None).expect("link must succeed");
        assert!(object.exists());
        assert_eq!(
            Command::new(&executable)
                .output()
                .expect("run")
                .status
                .code(),
            Some(0)
        );
    }

    #[test]
    fn launch_and_link_failures_are_distinct_and_preserve_inputs() {
        let directory = TestDirectory::create();
        let object = directory.join("invalid.o");
        let executable = directory.join("invalid");
        fs::write(&object, b"not an object").expect("object write");
        let missing = directory.join("missing-driver");
        let _serial = crate::run_control::tests::SIGNAL_TEST.lock().unwrap();
        let control = crate::run_control::RunControl::install().unwrap();
        for scope in [None, Some(&control)] {
            assert!(matches!(
                link_with_driver(&missing, &object, &executable, scope),
                Err(LinkerError::Launch { driver, .. }) if driver == missing
            ));
            assert!(object.exists());

            assert!(matches!(
                link_native_object(&object, &executable, scope),
                Err(LinkerError::Failure { status: Some(_), stderr }) if !stderr.is_empty()
            ));
            assert!(object.exists());
        }
        control.finish().unwrap();
    }
}
