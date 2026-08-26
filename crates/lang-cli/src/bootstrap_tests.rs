use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

use lang_codegen::NativeObjectErrorKind;

use crate::bootstrap::{
    BootstrapEntry, BootstrapError, BootstrapTarget, FrontendStage, bootstrap_and_run,
};

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn create() -> Self {
        let path = std::env::temp_dir().join(format!(
            "koven std bootstrap test-{}-{}",
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

fn run_fixture(source_text: &str, entry: &str) -> (TestDirectory, Result<(), BootstrapError>) {
    let directory = TestDirectory::create();
    let source = directory.join("fixture.ko");
    fs::write(&source, source_text).expect("fixture write");
    let result = bootstrap_and_run(BootstrapTarget {
        source: &source,
        entry: BootstrapEntry::Explicit(entry),
        object: &directory.join("fixture.o"),
        executable: &directory.join("fixture"),
    });
    (directory, result)
}

fn standard_prelude() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates directory")
        .join("lang-std")
        .join("koven")
        .join("prelude.ko")
}

#[test]
fn repository_prelude_is_the_single_enumerated_bootstrap_source_and_runs() {
    let prelude = standard_prelude();
    let standard_root = prelude.parent().expect("standard source directory");
    let mut sources = fs::read_dir(standard_root)
        .expect("standard source directory")
        .map(|entry| entry.expect("source entry").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "ko"))
        .collect::<Vec<_>>();
    sources.sort();
    assert_eq!(sources.as_slice(), std::slice::from_ref(&prelude));
    let original = fs::read(&prelude).expect("prelude bytes");
    assert!(
        original
            .windows("bootstrapSmoke".len())
            .any(|window| window == b"bootstrapSmoke")
    );
    assert!(
        original
            .windows("bootstrapAbort".len())
            .any(|window| window == b"bootstrapAbort")
    );
    assert!(
        original
            .windows("bootstrapHello".len())
            .any(|window| window == b"bootstrapHello")
    );
    for required in [
        "value class Pair<A, B>(val first: A, val second: B)",
        "enum class Result<T, E>",
        "Ok(success: T)",
        "Err(error: E)",
        "bootstrapPairResult",
    ] {
        assert!(
            original
                .windows(required.len())
                .any(|window| window == required.as_bytes()),
            "missing standard declaration {required:?}"
        );
    }

    let directory = TestDirectory::create();
    let object = directory.join("prelude.o");
    let executable = directory.join("prelude");
    bootstrap_and_run(BootstrapTarget {
        source: &prelude,
        entry: BootstrapEntry::Explicit("bootstrapSmoke"),
        object: &object,
        executable: &executable,
    })
    .expect("Koven prelude bootstrap must run");

    assert!(object.is_file());
    assert!(executable.is_file());

    let hello_object = directory.join("prelude-hello.o");
    let hello_executable = directory.join("prelude-hello");
    bootstrap_and_run(BootstrapTarget {
        source: &prelude,
        entry: BootstrapEntry::Explicit("bootstrapHello"),
        object: &hello_object,
        executable: &hello_executable,
    })
    .expect("Koven prelude println entry must run");
    let hello = Command::new(&hello_executable)
        .output()
        .expect("linked Koven prelude entry must launch");
    assert!(hello.status.success(), "{hello:?}");
    assert_eq!(hello.stdout, b"Hello, World!\n");
    assert!(hello.stderr.is_empty(), "{hello:?}");

    let pair_result_object = directory.join("prelude-pair-result.o");
    let pair_result_executable = directory.join("prelude-pair-result");
    bootstrap_and_run(BootstrapTarget {
        source: &prelude,
        entry: BootstrapEntry::Explicit("bootstrapPairResult"),
        object: &pair_result_object,
        executable: &pair_result_executable,
    })
    .expect("standard Pair/Result entry must construct, project and run");
    assert!(pair_result_object.is_file());
    assert!(pair_result_executable.is_file());

    let abort_object = directory.join("prelude-abort.o");
    let abort_executable = directory.join("prelude-abort");
    let abort = bootstrap_and_run(BootstrapTarget {
        source: &prelude,
        entry: BootstrapEntry::Explicit("bootstrapAbort"),
        object: &abort_object,
        executable: &abort_executable,
    });
    assert!(
        matches!(abort, Err(BootstrapError::ProcessFailure { .. })),
        "{abort:?}"
    );
    assert!(abort_object.is_file());
    assert!(abort_executable.is_file());
    assert_eq!(
        fs::read(&prelude).expect("prelude remains caller-owned"),
        original
    );
    assert!(matches!(
        bootstrap_and_run(BootstrapTarget {
            source: &prelude,
            entry: BootstrapEntry::Explicit("bootstrapSmoke"),
            object: &object,
            executable: &executable,
        }),
        Err(BootstrapError::OutputExists(path)) if path == object
    ));
}

#[test]
fn standard_pair_and_result_are_move_only_when_an_argument_is_move_only() {
    let prelude = fs::read_to_string(standard_prelude()).expect("standard prelude text");
    for (index, invalid) in [
        "fun invalidPair(own pair: Pair<String, Int>): Unit {\n\
             val moved = pair\n\
             val invalid = pair\n\
         }",
        "fun invalidResult(own result: Result<Int, String>): Unit {\n\
             val moved = result\n\
             val invalid = result\n\
         }",
    ]
    .into_iter()
    .enumerate()
    {
        let directory = TestDirectory::create();
        let source = directory.join(&format!("invalid-{index}.ko"));
        fs::write(&source, format!("{prelude}\n{invalid}\n")).expect("derived source write");
        let object = directory.join("invalid.o");
        let executable = directory.join("invalid");
        let result = bootstrap_and_run(BootstrapTarget {
            source: &source,
            entry: BootstrapEntry::Explicit(if index == 0 {
                "invalidPair"
            } else {
                "invalidResult"
            }),
            object: &object,
            executable: &executable,
        });
        assert!(
            matches!(
                result,
                Err(BootstrapError::FrontendDiagnostics {
                    stage: FrontendStage::OwnershipChecking,
                    diagnostics,
                    ..
                }) if diagnostics.len() == 1 && diagnostics[0].code().to_string() == "L0131"
            ),
            "derived standard ownership case {index} must fail with one L0131"
        );
        assert!(!object.exists());
        assert!(!executable.exists());
    }
}

#[test]
fn source_diagnostics_and_entry_selection_fail_before_link_or_run() {
    let (diagnostic_directory, diagnostic) =
        run_fixture("fun broken(): Unit { missing }", "broken");
    assert!(matches!(
        diagnostic,
        Err(BootstrapError::FrontendDiagnostics {
            stage: FrontendStage::NameResolution,
            diagnostics,
            ..
        }) if !diagnostics.is_empty()
    ));
    assert!(!diagnostic_directory.join("fixture.o").exists());
    assert!(!diagnostic_directory.join("fixture").exists());

    let (missing_directory, missing) = run_fixture("fun other(): Unit {}", "entry");
    assert!(matches!(
        missing,
        Err(BootstrapError::MissingEntry(name)) if name == "entry"
    ));
    assert!(!missing_directory.join("fixture.o").exists());

    let (ambiguous_directory, ambiguous) = run_fixture(
        "fun entry(): Unit {}\nfun entry(input: Int): Unit {}",
        "entry",
    );
    assert!(
        matches!(
            &ambiguous,
            Err(BootstrapError::AmbiguousEntry { name, count: 2 }) if name == "entry"
        ),
        "{ambiguous:?}"
    );
    assert!(!ambiguous_directory.join("fixture.o").exists());

    let (signature_directory, signature) = run_fixture("fun entry(input: Int): Unit {}", "entry");
    assert!(
        matches!(
            &signature,
            Err(BootstrapError::Codegen(error))
                if error.kind() == NativeObjectErrorKind::InvalidEntry
        ),
        "{signature:?}"
    );
    assert!(!signature_directory.join("fixture.o").exists());
}

#[test]
fn path_link_and_process_failures_are_distinct() {
    let directory = TestDirectory::create();
    let missing = directory.join("missing.ko");
    assert!(matches!(
        bootstrap_and_run(BootstrapTarget {
            source: &missing,
            entry: BootstrapEntry::Explicit("entry"),
            object: &directory.join("missing.o"),
            executable: &directory.join("missing"),
        }),
        Err(BootstrapError::ReadSource { path, .. }) if path == missing
    ));

    let source = directory.join("valid.ko");
    fs::write(&source, "fun entry(): Unit {}").expect("valid source");
    let object = directory.join("valid.o");
    let missing_parent = directory.join("missing-parent").join("executable");
    assert!(matches!(
        bootstrap_and_run(BootstrapTarget {
            source: &source,
            entry: BootstrapEntry::Explicit("entry"),
            object: &object,
            executable: &missing_parent,
        }),
        Err(BootstrapError::Linker(_))
    ));
    assert!(object.is_file());
    assert!(!missing_parent.exists());

    let (abort_directory, abort) = run_fixture(
        "fun entry(): Unit {\nval one: Int = 1\nval zero: Int = 0\nval result: Int = one / zero\n}",
        "entry",
    );
    assert!(
        matches!(&abort, Err(BootstrapError::ProcessFailure { status: None })),
        "{abort:?}"
    );
    assert!(abort_directory.join("fixture.o").is_file());
    assert!(abort_directory.join("fixture").is_file());
}
