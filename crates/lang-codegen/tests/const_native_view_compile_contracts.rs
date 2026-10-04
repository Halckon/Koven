//! SPEC-0254: actual public native entry points keep basic/const capabilities separate.
#[path = "../../../scripts/rust_test_artifact.rs"]
mod linked_artifact;
use lang_codegen::emit_native_const_owned_unit_object;
use lang_frontend::ownership_checking::ConstOwnedCompilationUnitView;
use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};
fn api_exists(
    unit: &ConstOwnedCompilationUnitView<'_, '_>,
    entry: lang_codegen::NativeUnitEntry,
    output: &Path,
) {
    let _ = emit_native_const_owned_unit_object(unit, entry, output);
}
const IMPORTS: &str = r#"
#![allow(dead_code, unused_imports, unused_variables)]
use std::path::Path;
use lang_codegen::{NativeUnitEntry, emit_native_const_owned_unit_object,
    emit_native_owned_unit_object, emit_native_constant_unit_object};
use lang_frontend::{source::SourceMap,
    name_resolution::{SourceUnitInput, ValidatedCompilationUnitNames},
    type_checking::{TypeEnvironment, ConstEnabledTypedUnit, ValidatedCompilationUnitTypes},
    ownership_checking::{ConstOwnedCompilationUnitView, OwnedCompilationUnitView,
        ConstEnabledOwnedUnit, ValidatedCompilationUnitOwnership}};
"#;
struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "koven-native-unit-compile-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&path).expect("create isolated external compile directory");
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn artifact(deps: &Path, name: &str) -> PathBuf {
    linked_artifact::for_current_test(deps, name)
}

fn compile(source: &str) -> Output {
    let executable = std::env::current_exe().expect("locate integration test executable");
    let deps = executable.parent().expect("integration test has a parent");

    let scratch = Scratch::new();
    let source_path = scratch.0.join("contract.rs");
    fs::write(&source_path, format!("{IMPORTS}\n{source}"))
        .expect("write external compilation contract");
    let mut command =
        Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| OsString::from("rustc")));
    command
        .arg("--edition=2024")
        .arg("--crate-name=const_native_unit_contract")
        .arg("--crate-type=lib")
        .arg("--emit=metadata")
        .arg("--error-format=human")
        .arg("--color=never")
        .arg("-L")
        .arg(format!("dependency={}", deps.display()))
        .arg("--out-dir")
        .arg(&scratch.0)
        .arg(source_path);
    for name in ["lang_frontend", "lang_codegen"] {
        let mut external = OsString::from(format!("{name}="));
        external.push(artifact(deps, name));
        command.arg("--extern").arg(external);
    }
    command
        .output()
        .expect("run rustc for external compilation contract")
}

fn accepts(label: &str, source: &str) {
    let output = compile(source);
    assert!(
        output.status.success(),
        "{label}: valid external control must compile\n{}",
        String::from_utf8_lossy(&output.stderr),
    );
}

fn rejects(label: &str, source: &str, code: &str, details: &[&str]) {
    let output = compile(source);
    assert!(
        !output.status.success(),
        "{label}: invalid contract compiled"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    let errors = stderr
        .lines()
        .filter_map(|line| line.strip_prefix("error["))
        .map(|line| line.split_once(']').expect("rustc error code closes").0)
        .collect::<Vec<_>>();
    assert!(
        !errors.is_empty() && errors.iter().all(|actual| *actual == code),
        "{label}: expected only {code}, found {errors:?}\n{stderr}",
    );
    for detail in details {
        assert!(
            stderr.contains(detail),
            "{label}: missing expected diagnostic detail {detail:?}\n{stderr}",
        );
    }
}

#[test]
fn native_view_entry_points_reject_the_opposite_capability() {
    let _ = api_exists;
    for (function, expected, actual) in [
        (
            "emit_native_const_owned_unit_object",
            "ConstOwnedCompilationUnitView",
            "OwnedCompilationUnitView",
        ),
        (
            "emit_native_owned_unit_object",
            "OwnedCompilationUnitView",
            "ConstOwnedCompilationUnitView",
        ),
    ] {
        let source = format!(
            "fn check(unit: &{expected}<'_, '_>, entry: NativeUnitEntry, output: &Path) {{ {function}(unit, entry, output).unwrap(); }}"
        );
        accepts(function, &source);
        rejects(
            function,
            &source.replace(&format!("unit: &{expected}"), &format!("unit: &{actual}")),
            "E0308",
            &["mismatched types", expected, actual],
        );
    }
}

#[test]
fn legacy_const_native_rejects_basic_typed_and_owned_independently() {
    let source = r#"
fn check(sources: &SourceMap, inputs: &[SourceUnitInput<'_>], names: &ValidatedCompilationUnitNames,
    environment: &TypeEnvironment, typed: &ConstEnabledTypedUnit, owned: &ConstEnabledOwnedUnit,
    entry: NativeUnitEntry, output: &Path) {
    emit_native_constant_unit_object(sources, inputs, names, environment, typed, owned, entry, output).unwrap();
}
"#;
    for (argument, expected, actual) in [
        (
            "typed",
            "ConstEnabledTypedUnit",
            "ValidatedCompilationUnitTypes",
        ),
        (
            "owned",
            "ConstEnabledOwnedUnit",
            "ValidatedCompilationUnitOwnership",
        ),
    ] {
        accepts(argument, source);
        rejects(
            argument,
            &source.replace(
                &format!("{argument}: &{expected}"),
                &format!("{argument}: &{actual}"),
            ),
            "E0308",
            &["mismatched types", expected, actual],
        );
    }
}
