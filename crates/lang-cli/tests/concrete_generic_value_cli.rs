//! SPEC-0341：独立普通源码锁定具体泛型 value class 的两种 CLI 宿主。

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

#[test]
fn concrete_generic_value_copy_projection_and_enum_composition_use_both_hosts() {
    run_case(
        include_str!("fixtures/concrete-generic-value/copyable.ko"),
        b"copy-and-destructure\ndistinct-layout\nenum-composition\n",
    );
}

#[test]
fn concrete_generic_value_move_destructuring_uses_both_hosts() {
    run_case(
        include_str!("fixtures/concrete-generic-value/move_only.ko"),
        b"owned-text\nmove-and-destructure\nresource-destructure\nscope-end\n",
    );
}

fn run_case(source: &str, expected: &[u8]) {
    let directory = Directory::new();
    let source_path = directory.path().join("src/regression/Entry.ko");
    fs::create_dir_all(source_path.parent().unwrap()).unwrap();
    fs::write(&source_path, source).unwrap();
    fs::write(
        directory.path().join("project.toml"),
        "schema = \"koven.project\"\nversion = 1\n[project]\n\
         name = \"generic-value\"\nsource-roots = [\"src\"]\n",
    )
    .unwrap();
    let mut observations = Vec::new();
    for host in ["single", "project"] {
        for format in ["human", "json"] {
            let executable = directory.path().join(format!("{host}-{format}"));
            let mut command = Command::new(env!("CARGO_BIN_EXE_kovenc"));
            command
                .current_dir(directory.path())
                .arg(format!("--message-format={format}"))
                .arg("build");
            if host == "single" {
                command.arg("src/regression/Entry.ko");
            } else {
                command.args(["--project", "project.toml", "--entry", "regression.main"]);
            }
            let built = command.arg("-o").arg(&executable).output();
            let executed = match &built {
                Ok(output) if output.status.success() => Some(Command::new(&executable).output()),
                _ => None,
            };
            observations.push((host, format, executable.is_file(), built, executed));
        }
    }
    eprintln!("complete ordinary observations: {observations:#?}");
    assert_eq!(fs::read(&source_path).unwrap(), source.as_bytes());
    for (host, format, artifact, built, executed) in &observations {
        assert_success(
            built.as_ref().expect("real CLI build process must launch"),
            b"",
            &format!("{host}/{format} build"),
        );
        assert!(
            *artifact,
            "{host}/{format}: successful build must publish an executable"
        );
        assert_success(
            executed
                .as_ref()
                .expect("successful build must run the original program")
                .as_ref()
                .expect("published program must launch"),
            expected,
            &format!("{host}/{format} program"),
        );
    }
    for entry in fs::read_dir(directory.path()).unwrap() {
        assert!(
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains(".kovenc-")
        );
    }
}

fn assert_success(output: &Output, stdout: &[u8], stage: &str) {
    assert_eq!(output.status.code(), Some(0), "{stage}: {output:?}");
    assert_eq!(output.stdout, stdout, "{stage}: {output:?}");
    assert!(output.stderr.is_empty(), "{stage}: {output:?}");
}

struct Directory(PathBuf);

impl Directory {
    fn new() -> Self {
        static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "koven-concrete-generic-value-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        fs::remove_dir_all(self.path()).expect("owned ordinary fixture directory");
    }
}
