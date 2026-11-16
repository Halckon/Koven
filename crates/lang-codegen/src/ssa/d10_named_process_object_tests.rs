//! Process-object inspection, never linking or executing the generated main.
use super::model::Program;
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        loop {
            let path = std::env::temp_dir().join(format!(
                "koven-d10-process-object-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => panic!("private object inspection directory: {e}"),
            }
        }
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
pub(super) type Emit<'a> = dyn Fn(&std::path::Path) -> Result<(), crate::NativeObjectError> + 'a;

pub(super) fn assert_success(program: &Program, emit: &Emit<'_>) {
    let mut expected = vec!["main".to_owned()];
    expected.extend(
        program.modules[0]
            .functions
            .iter()
            .map(|f| format!("f{}.{}", f.id().index(), f.name)),
    );
    assert_eq!(expected.len(), 3);
    let directory = Directory::new();
    let path = directory.0.join("process.o");
    // Second successful transaction replaces an actual valid existing object, not a sentinel.
    for _ in 0..2 {
        emit(&path).unwrap();
        let bytes = fs::read(&path).unwrap();
        crate::test_support::assert_native_object(&bytes);
        if cfg!(target_os = "macos") {
            assert_eq!(bytes.get(12..16), Some([1, 0, 0, 0].as_slice()));
        } else {
            assert_eq!(bytes.get(16..18), Some([1, 0].as_slice()));
        }
        let output = Command::new("/usr/bin/nm")
            .args(if cfg!(target_os = "macos") {
                ["-j", "-U"]
            } else {
                ["-j", "--defined-only"]
            })
            .arg(&path)
            .output()
            .unwrap();
        assert!(output.status.success(), "nm: {:?}", output.stderr);
        let symbols = String::from_utf8(output.stdout).unwrap();
        for expected in &expected {
            let symbol = if cfg!(target_os = "macos") {
                format!("_{expected}")
            } else {
                expected.clone()
            };
            assert_eq!(
                symbols.lines().filter(|line| *line == symbol).count(),
                1,
                "{symbol}"
            );
        }
        let files = fs::read_dir(&directory.0)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect::<Vec<_>>();
        assert_eq!(
            files,
            vec![std::ffi::OsString::from("process.o")],
            "no sibling temporary remains after success"
        );
    }
}
pub(super) fn assert_failure(emit: &Emit<'_>) {
    assert_eq!(
        std::env::var("GITHUB_ACTIONS").as_deref(),
        Ok("true"),
        "CI-only emission failure"
    );
    for present in [false, true] {
        let directory = Directory::new();
        let target = directory.0.join("process.o");
        let blocker = directory.0.join("blocker");
        fs::write(&blocker, b"blocker bytes").unwrap();
        let previous = if present {
            emit(&target).unwrap();
            Some(fs::read(&target).unwrap())
        } else {
            None
        };
        let entries = || {
            let mut names = fs::read_dir(&directory.0)
                .unwrap()
                .map(|e| e.unwrap().file_name())
                .collect::<Vec<_>>();
            names.sort();
            names
        };
        let before = entries();
        let guard = crate::llvm::emission_failure::Guard::new(blocker.join("object.o"));
        let error = emit(&target).expect_err("real object write must fail with ENOTDIR");
        assert_eq!(guard.calls(), 1, "actual LLVM emission reached");
        assert_eq!(error.kind(), crate::NativeObjectErrorKind::Backend);
        assert_eq!(fs::read(&blocker).unwrap(), b"blocker bytes");
        match previous {
            Some(bytes) => assert_eq!(fs::read(&target).unwrap(), bytes),
            None => assert!(!target.exists()),
        }
        assert_eq!(
            entries(),
            before,
            "failed sibling transaction must remove its temporary"
        );
    }
}

// Defined and registered once. Each invocation retains its own source/analyzer/capability route.
macro_rules! suite {
    ($source:expr, $analyze:path, $lower:path) => {
        mod process_object {
            use super::*;
            use $crate::ssa::d10_named_process_object_tests as shared;

            fn with_emitter(test: impl Fn(&$crate::ssa::model::Program, &shared::Emit<'_>)) {
                let mut sources = SourceMap::new();
                let (id, parsed) =
                    unit_lower_test_support::parsed(&mut sources, "named-process.ko", $source);
                let (env, types) = standard_environments();
                for input in [
                    SourceUnitInput::new("root", "named-process.ko", id, &parsed),
                    SourceUnitInput::memory_document(id, &parsed),
                ] {
                    let inputs = [input];
                    let (names, typed, _) =
                        unit_lower_test_support::analyze(&sources, &inputs, &env, &types);
                    let bound = $analyze(&sources, &inputs, &names, &types, &typed)
                        .unwrap()
                        .unwrap();
                    let unit = owned_compilation_unit_view(
                        &sources,
                        &inputs,
                        &names,
                        &types,
                        &typed,
                        bound.ownership(),
                    )
                    .unwrap();
                    let artifact = $lower(unit, &bound).unwrap();
                    let (program, _) = artifact.replay_input();
                    test(program, &|path| artifact.emit_process_object(path));
                }
            }
            #[test]
            fn named_process_object_atomically_publishes_real_main_and_source_symbols() {
                with_emitter(shared::assert_success);
            }
            #[test]
            #[ignore = "CI-only real emission failure; never run locally"]
            fn ci_only_process_object_failure_preserves_target() {
                assert_eq!(
                    std::env::var("GITHUB_ACTIONS").as_deref(),
                    Ok("true"),
                    "CI-only emission failure"
                );
                with_emitter(|_, emit| shared::assert_failure(emit));
            }
        }
    };
}
pub(super) use suite;
