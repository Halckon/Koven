//! Host-native linker driver boundary.

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
pub(crate) fn link_native_object(object: &Path, executable: &Path) -> Result<(), LinkerError> {
    link_with_driver(Path::new(NATIVE_LINK_DRIVER), object, executable)
}

fn link_with_driver(driver: &Path, object: &Path, executable: &Path) -> Result<(), LinkerError> {
    let output = Command::new(driver)
        .arg(object)
        .arg("-o")
        .arg(executable)
        .output()
        .map_err(|error| LinkerError::Launch {
            driver: driver.to_path_buf(),
            kind: error.kind(),
        })?;
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

        link_native_object(&object, &executable).expect("link must succeed");
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
        assert!(matches!(
            link_with_driver(&missing, &object, &executable),
            Err(LinkerError::Launch { driver, .. }) if driver == missing
        ));
        assert!(object.exists());

        assert!(matches!(
            link_native_object(&object, &executable),
            Err(LinkerError::Failure { status: Some(_), stderr }) if !stderr.is_empty()
        ));
        assert!(object.exists());
    }
}
