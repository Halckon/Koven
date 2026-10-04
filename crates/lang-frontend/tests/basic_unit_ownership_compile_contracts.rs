//! SPEC-0252: exact external type and encapsulation contracts for basic advancement.
#[path = "../../../scripts/rust_test_artifact.rs"]
mod linked_artifact;

use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::{
        OnceLock,
        atomic::{AtomicUsize, Ordering},
    },
};

use lang_frontend::analysis::{BasicOwnershipOutcome, analyze_basic_unit_ownership};

const IMPORTS: &str = r#"
#![allow(dead_code, unused_imports, unused_mut, unused_variables)]
use lang_frontend::{
    analysis::{BasicOwnershipOutcome, UnitSourceDescriptor, analyze_basic_unit_ownership, analyze_unit_names},
    ownership_checking::{CompilationUnitOwnership, check_compilation_unit_ownership, check_compilation_unit_constant_ownership},
    source::SourceMap,
    type_checking::{CompilationUnitTypes, ConstEnabledTypedUnit, ValidatedCompilationUnitTypes, check_compilation_unit_types, standard_environments},
};
fn fixture() -> BasicOwnershipOutcome {
    let mut sources = SourceMap::new();
    let id = sources.add_source("model.ko", "fun main(): Unit {}").unwrap();
    let (ne, te) = standard_environments();
    let snapshot = analyze_unit_names(sources, vec![UnitSourceDescriptor::new("root", "model.ko", id)], ne).unwrap();
    let inputs = snapshot.inputs();
    let names = snapshot.validated_names().unwrap();
    let typed = check_compilation_unit_types(snapshot.sources(), &inputs, names, &te).unwrap();
    // The outcome owns facts: no inputs, environment, source or snapshot borrow escapes.
    analyze_basic_unit_ownership(snapshot.sources(), &inputs, names, &te, typed).unwrap()
}
fn consume(outcome: BasicOwnershipOutcome) { std::hint::black_box(outcome); }
"#;

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "koven-basic-unit-ownership-compile-{}-{}",
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

fn frontend_artifact(deps: &Path) -> PathBuf {
    static ARTIFACT: OnceLock<PathBuf> = OnceLock::new();
    ARTIFACT
        .get_or_init(|| linked_artifact::for_current_test(deps, "lang_frontend"))
        .clone()
}

fn compile(source: &str) -> Output {
    let executable = std::env::current_exe().expect("locate integration test executable");
    let deps = executable.parent().expect("integration test has a parent");
    let artifact = frontend_artifact(deps);
    let scratch = Scratch::new();
    let source_path = scratch.0.join("contract.rs");
    fs::write(&source_path, format!("{IMPORTS}\n{source}"))
        .expect("write external compilation contract");
    let mut external = OsString::from("lang_frontend=");
    external.push(artifact);
    Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| OsString::from("rustc")))
        .arg("--edition=2024")
        .arg("--crate-name=basic_unit_ownership_contract")
        .arg("--crate-type=lib")
        .arg("--emit=metadata")
        .arg("--error-format=human")
        .arg("--color=never")
        .arg("--extern")
        .arg(external)
        .arg("-L")
        .arg(format!("dependency={}", deps.display()))
        .arg("--out-dir")
        .arg(&scratch.0)
        .arg(source_path)
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
fn owned_outcome_outlives_inputs_and_moves_without_cloning() {
    // Missing API must fail this target, never accidentally satisfy a negative contract.
    let _ = std::mem::size_of::<BasicOwnershipOutcome>();
    let _ = analyze_basic_unit_ownership;
    accepts(
        "owned result outlives fixture and a local inputs scope",
        r#"
fn move_result(outcome: BasicOwnershipOutcome) -> BasicOwnershipOutcome { outcome }
fn check() {
    let result;
    { result = fixture(); }
    let result = move_result(result);
    let _: Result<(ValidatedCompilationUnitTypes, CompilationUnitOwnership), Box<CompilationUnitTypes>> = result.into_result();
}
"#,
    );
}

#[test]
fn external_construction_and_field_access_are_private() {
    accepts("factory is available", "fn check() { consume(fixture()); }");
    rejects(
        "external struct update",
        "fn check() { let outcome = fixture(); consume(BasicOwnershipOutcome { ..outcome }); }",
        "E0451",
        &["field `result`", "is private"],
    );
    rejects(
        "external field access",
        "fn check() { let outcome = fixture(); let _ = outcome.result; }",
        "E0616",
        &["field `result` of struct `BasicOwnershipOutcome` is private"],
    );
}

#[test]
fn basic_const_and_recovery_capabilities_are_not_interchangeable() {
    let parameters = r#"
fn check(sources: &SourceMap, inputs: &[lang_frontend::name_resolution::SourceUnitInput<'_>], names: &lang_frontend::name_resolution::ValidatedCompilationUnitNames, environment: &lang_frontend::type_checking::TypeEnvironment, raw: CompilationUnitTypes, basic: ValidatedCompilationUnitTypes, constant: ConstEnabledTypedUnit) {
"#;
    for (label, call) in [
        (
            "basic checker capability",
            "let _ = check_compilation_unit_ownership(sources, inputs, names, environment, &basic);",
        ),
        (
            "const checker capability",
            "let _ = check_compilation_unit_constant_ownership(sources, inputs, names, environment, &constant);",
        ),
        (
            "raw input to adapter",
            "let _ = analyze_basic_unit_ownership(sources, inputs, names, environment, raw);",
        ),
    ] {
        accepts(label, &format!("{parameters}{call}\n}}"));
    }
    for (label, call, details) in [
        (
            "raw is not basic checker capability",
            "let _ = check_compilation_unit_ownership(sources, inputs, names, environment, &raw);",
            vec!["&ValidatedCompilationUnitTypes", "&CompilationUnitTypes"],
        ),
        (
            "const is not basic checker capability",
            "let _ = check_compilation_unit_ownership(sources, inputs, names, environment, &constant);",
            vec!["&ValidatedCompilationUnitTypes", "&ConstEnabledTypedUnit"],
        ),
        (
            "basic is not const checker capability",
            "let _ = check_compilation_unit_constant_ownership(sources, inputs, names, environment, &basic);",
            vec!["&ConstEnabledTypedUnit", "&ValidatedCompilationUnitTypes"],
        ),
        (
            "const is not raw adapter input",
            "let _ = analyze_basic_unit_ownership(sources, inputs, names, environment, constant);",
            vec![
                "expected `CompilationUnitTypes`",
                "found `ConstEnabledTypedUnit`",
            ],
        ),
        (
            "basic is not raw adapter input",
            "let _ = analyze_basic_unit_ownership(sources, inputs, names, environment, basic);",
            vec![
                "expected `CompilationUnitTypes`",
                "found `ValidatedCompilationUnitTypes`",
            ],
        ),
    ] {
        rejects(label, &format!("{parameters}{call}\n}}"), "E0308", &details);
    }
}

#[test]
fn outcome_does_not_itself_grant_a_validated_capability() {
    accepts(
        "explicitly match the actual capability",
        "fn check() { if let Ok((typed, owned)) = fixture().into_result() { let _: ValidatedCompilationUnitTypes = typed; let _: CompilationUnitOwnership = owned; } }",
    );
    rejects(
        "sealed branching result is not a typed capability",
        "fn check() { let _: ValidatedCompilationUnitTypes = fixture(); }",
        "E0308",
        &[
            "expected `ValidatedCompilationUnitTypes`",
            "found `BasicOwnershipOutcome`",
        ],
    );
    rejects(
        "raw ownership must still be validated",
        "fn check() { if let Ok((_, owned)) = fixture().into_result() { let _: lang_frontend::ownership_checking::ValidatedCompilationUnitOwnership = owned; } }",
        "E0308",
        &[
            "expected `ValidatedCompilationUnitOwnership`",
            "found `CompilationUnitOwnership`",
        ],
    );
}
