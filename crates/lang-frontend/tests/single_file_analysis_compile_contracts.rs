//! SPEC-0253：外部 Rust 只读、封闭构造及 observer 高阶借用合同。
#[path = "../../../scripts/rust_test_artifact.rs"]
mod linked_artifact;

use lang_frontend::analysis::{SingleFileAnalysis, SingleFileTypedView};
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

const IMPORTS: &str = r#"
#![allow(dead_code, unused_imports, unused_mut, unused_variables)]
use std::convert::Infallible;
use lang_frontend::{
    analysis::{SingleFileAnalysis, SingleFileAnalysisError, SingleFileStage, SingleFileTypedView, analyze_single_file},
    source::{SourceId, SourceMap},
    name_resolution::{NameEnvironment, NameResolution},
    parser::ParsedFile,
    type_checking::{TypeEnvironment, TypedFile, ValidatedCompilationUnitTypes, ConstEnabledTypedUnit, standard_environments},
    ownership_checking::{OwnershipCheckedFile, ValidatedCompilationUnitOwnership},
};
fn fixture() -> SingleFileAnalysis<Box<str>> {
    let mut sources = SourceMap::new();
    let id = sources.add_source("fixture.ko", "fun main(): Unit {}").unwrap();
    let (ne, te) = standard_environments();
    analyze_single_file(&sources, id, &ne, &te, |_, _| Ok::<_, Infallible>(()), |view| {
        let _: &ParsedFile = view.parsed();
        let _: &NameResolution = view.names();
        let _: &TypedFile = view.typed();
        Ok(String::from("observed").into_boxed_str())
    }).unwrap()
}
"#;

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "koven-single-file-analysis-compile-{}-{}",
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
        .arg("--crate-name=single_file_analysis_contract")
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
fn owned_payload_and_result_outlive_inputs_without_clone() {
    let _ = std::mem::size_of::<SingleFileAnalysis<()>>();
    let _ = std::mem::size_of::<SingleFileTypedView<'_>>();
    accepts(
        "owned result crosses input scopes",
        r#"
fn check() {
    let result;
    { result = fixture(); }
    let _: &Box<str> = result.observed();
    let _: (ParsedFile, NameResolution, TypedFile, OwnershipCheckedFile, Box<str>) = result.into_parts();
}
struct MoveOnly;
fn move_only() -> SingleFileAnalysis<MoveOnly> {
    let mut sources = SourceMap::new();
    let id = sources.add_source("move.ko", "").unwrap();
    let (ne, te) = standard_environments();
    analyze_single_file(&sources, id, &ne, &te, |_, _| Ok::<_, Infallible>(()), |_| Ok(MoveOnly)).unwrap()
}
fn outside_borrow<'a>(message: &'a str) -> SingleFileAnalysis<&'a str> {
    let mut sources = SourceMap::new();
    let id = sources.add_source("borrow.ko", "").unwrap();
    let (ne, te) = standard_environments();
    analyze_single_file(&sources, id, &ne, &te, |_, _| Ok::<_, Infallible>(()), |_| Ok(message)).unwrap()
}
"#,
    );
}

#[test]
fn external_construction_and_field_access_remain_private() {
    accepts(
        "access existing facts immutably",
        "fn check() { let result = fixture(); let _: &ParsedFile = result.parsed(); }",
    );
    rejects(
        "result struct update",
        "fn check() { let result = fixture(); let _ = SingleFileAnalysis { ..result }; }",
        "E0451",
        &["private"],
    );
    rejects(
        "result field access",
        "fn check() { let result = fixture(); let _ = result.typed; }",
        "E0616",
        &["field `typed`", "private"],
    );
    accepts(
        "view accessors",
        "fn check(view: SingleFileTypedView<'_>) { let _: &TypedFile = view.typed(); }",
    );
    rejects(
        "view literal",
        "fn check(parsed: &ParsedFile, names: &NameResolution, typed: &TypedFile) { let _ = SingleFileTypedView { parsed, names, typed }; }",
        "E0451",
        &["private"],
    );
    rejects(
        "view field access",
        "fn check(view: SingleFileTypedView<'_>) { let _ = view.names; }",
        "E0616",
        &["field `names`", "private"],
    );
}

#[test]
fn getters_cannot_replace_or_mutably_borrow_facts() {
    accepts(
        "immutable typed facts",
        "fn check() { let result = fixture(); let _: &TypedFile = result.typed(); }",
    );
    rejects(
        "mutable result facts",
        "fn check() { let result = fixture(); let _: &mut TypedFile = result.typed(); }",
        "E0308",
        &["mutability", "&mut TypedFile"],
    );
    accepts(
        "immutable view facts",
        "fn check(view: SingleFileTypedView<'_>) { let _: &NameResolution = view.names(); }",
    );
    rejects(
        "mutable view facts",
        "fn check(view: SingleFileTypedView<'_>) { let _: &mut NameResolution = view.names(); }",
        "E0308",
        &["mutability", "&mut NameResolution"],
    );
}

#[test]
fn temporary_view_references_cannot_escape_through_payload_error_or_capture() {
    let prefix =
        "fn check(s: &SourceMap, id: SourceId, ne: &NameEnvironment, te: &TypeEnvironment) {";
    accepts(
        "owned observer facts",
        &format!(
            "{prefix} let _ = analyze_single_file(s, id, ne, te, |_, _| Ok::<_, Infallible>(()), |v| Ok(v.names().clone())); }}"
        ),
    );
    for body in [
        "let _ = analyze_single_file(s, id, ne, te, |_, _| Ok::<_, Infallible>(()), |v| Ok(v.parsed()));",
        "let _ = analyze_single_file(s, id, ne, te, |_, _| Ok::<_, Infallible>(()), |v| Ok(v.names()));",
        "let _ = analyze_single_file(s, id, ne, te, |_, _| Ok::<_, Infallible>(()), |v| Ok(v.typed()));",
        "let _ = analyze_single_file(s, id, ne, te, |_, _| Ok(()), |v| Err::<(), _>(v.typed()));",
    ] {
        let output = compile(&format!("{prefix} {body} }}"));
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "borrowed observer output escaped");
        assert!(
            stderr.contains("lifetime may not live long enough"),
            "{stderr}"
        );
        assert!(
            !stderr.contains("error[E"),
            "unexpected coded error: {stderr}"
        );
    }
    accepts(
        "owned outer capture",
        &format!(
            "{prefix} let mut saved = None; let _ = analyze_single_file(s, id, ne, te, |_, _| Ok::<_, Infallible>(()), |v| {{ saved = Some(v.names().clone()); Ok(()) }}); }}"
        ),
    );
    rejects(
        "borrowed outer capture",
        &format!(
            "{prefix} let mut saved = None; let _ = analyze_single_file(s, id, ne, te, |_, _| Ok::<_, Infallible>(()), |v| {{ saved = Some(v.typed()); Ok(()) }}); }}"
        ),
        "E0521",
        &["borrowed data escapes outside of closure"],
    );
}

#[test]
fn raw_single_file_result_is_not_a_validated_unit_or_const_capability() {
    accepts(
        "raw single-file ownership",
        "fn check() { let (_, _, _, owned, _) = fixture().into_parts(); let _: OwnershipCheckedFile = owned; }",
    );
    for capability in ["ValidatedCompilationUnitTypes", "ConstEnabledTypedUnit"] {
        rejects(
            "raw typed is not unit capability",
            &format!(
                "fn check() {{ let (_, _, typed, _, _) = fixture().into_parts(); let _: {capability} = typed; }}"
            ),
            "E0308",
            &[capability, "TypedFile"],
        );
    }
    rejects(
        "raw ownership is not validated unit",
        "fn check() { let (_, _, _, owned, _) = fixture().into_parts(); let _: ValidatedCompilationUnitOwnership = owned; }",
        "E0308",
        &["ValidatedCompilationUnitOwnership", "OwnershipCheckedFile"],
    );
}

#[test]
fn iteration_validator_is_read_only_and_errors_remain_sealed() {
    accepts(
        "read-only validator",
        "fn check(p: &ParsedFile, n: &NameResolution, t: &TypedFile, o: &OwnershipCheckedFile) { let _ = lang_frontend::ownership_checking::validate_iteration_facts(p, n, t, o); }",
    );
    rejects(
        "iteration mutation",
        "fn check(o: &OwnershipCheckedFile) { o.iterations().clear(); }",
        "E0599",
        &["clear"],
    );
    rejects(
        "sealed error",
        "fn check(error: lang_frontend::ownership_checking::IterationFactError) { let _ = error.reason; }",
        "E0616",
        &["private"],
    );
}
