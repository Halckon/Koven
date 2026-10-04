//! SPEC-0250: external compilation contracts for the sealed unit-name snapshot.
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

use lang_frontend::analysis::{UnitNameSnapshot, UnitSourceDescriptor, analyze_unit_names};

const IMPORTS: &str = r#"
#![allow(dead_code, unused_imports, unused_mut, unused_variables)]
use lang_frontend::{
    analysis::{
        UnitNameAnalysisError, UnitNameSnapshot, UnitSourceDescriptor, analyze_unit_names,
    },
    name_resolution::{
        CompilationUnitNames, NameEnvironment, SourceUnitInput, ValidatedCompilationUnitNames,
        index_compilation_unit,
    },
    parser::ParsedFile,
    source::SourceMap,
};
fn fixture() -> UnitNameSnapshot {
    let mut sources = SourceMap::new();
    let source_id = sources.add_source("model.ko", "public class Model").unwrap();
    let descriptor = UnitSourceDescriptor::new(String::from("workspace"), "model.ko", source_id);
    analyze_unit_names(sources, vec![descriptor], NameEnvironment::new()).unwrap()
}
fn consume(snapshot: UnitNameSnapshot) {
    std::hint::black_box(snapshot);
}
fn consume_inputs(inputs: Vec<SourceUnitInput<'_>>) {
    std::hint::black_box(inputs);
}
"#;

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "koven-unit-name-snapshot-compile-{}-{}",
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
        .arg("--crate-name=unit_name_snapshot_contract")
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
fn factory_and_read_only_getters_allow_snapshot_moves_and_local_inputs() {
    // Missing exports must fail this integration target, not satisfy a negative snippet.
    let _ = std::mem::size_of::<UnitNameSnapshot>();
    let _ = std::mem::size_of::<UnitSourceDescriptor>();
    let _ = analyze_unit_names;
    accepts(
        "owned factory, snapshot moves, and read-only projections",
        r#"
fn factory(
    sources: SourceMap,
    descriptors: Vec<UnitSourceDescriptor>,
    environment: NameEnvironment,
) -> Result<UnitNameSnapshot, UnitNameAnalysisError> {
    analyze_unit_names(sources, descriptors, environment)
}
fn move_snapshot(snapshot: UnitNameSnapshot) -> UnitNameSnapshot {
    snapshot
}
fn getters(snapshot: &UnitNameSnapshot) {
    let _: &SourceMap = snapshot.sources();
    let _: &[UnitSourceDescriptor] = snapshot.descriptors();
    let _: &[ParsedFile] = snapshot.parsed_files();
    let _: &NameEnvironment = snapshot.environment();
    let _: &CompilationUnitNames = snapshot.names();
    let _: Option<&ValidatedCompilationUnitNames> = snapshot.validated_names();
    let _: Vec<SourceUnitInput<'_>> = snapshot.inputs();
}
fn check() {
    let snapshot = move_snapshot(fixture());
    getters(&snapshot);
    {
        let inputs: Vec<SourceUnitInput<'_>> = snapshot.inputs();
        let _ = index_compilation_unit(snapshot.sources(), &inputs).unwrap();
        consume_inputs(inputs);
    }
    consume(move_snapshot(snapshot));
}
"#,
    );
}

#[test]
fn external_snapshot_construction_and_field_access_are_private() {
    accepts(
        "sealed fields have a usable factory and move control",
        "fn check() { let snapshot = fixture(); consume(snapshot); }",
    );
    // Struct update checks every field without naming the private names-state type or
    // manufacturing invalid values. Its only defect is external field visibility.
    rejects(
        "external snapshot struct update",
        "fn check() { let snapshot = fixture(); consume(UnitNameSnapshot { ..snapshot }); }",
        "E0451",
        &[
            "`sources`",
            "`descriptors`",
            "`parsed_files`",
            "`environment`",
            "`names`",
            "are private",
        ],
    );
    for field in [
        "sources",
        "descriptors",
        "parsed_files",
        "environment",
        "names",
    ] {
        rejects(
            &format!("external access to snapshot field {field}"),
            &format!("fn check() {{ let snapshot = fixture(); let _ = &snapshot.{field}; }}"),
            "E0616",
            &[&format!(
                "field `{field}` of struct `UnitNameSnapshot` is private"
            )],
        );
    }
}

#[test]
fn projected_inputs_cannot_outlive_the_snapshot_owner() {
    accepts(
        "inputs consumed while their local owner is alive",
        r#"
fn check() {
    {
        let snapshot = fixture();
        let inputs = snapshot.inputs();
        consume_inputs(inputs);
    }
}
"#,
    );
    rejects(
        "inputs escape their local snapshot owner",
        r#"
fn check() {
    let inputs;
    {
        let snapshot = fixture();
        inputs = snapshot.inputs();
    }
    consume_inputs(inputs);
}
"#,
        "E0597",
        &[
            "`snapshot` does not live long enough",
            "borrow later used here",
        ],
    );
}

#[test]
fn snapshot_getters_do_not_expose_mutable_products() {
    for (getter, product) in [
        ("sources", "SourceMap"),
        ("descriptors", "[UnitSourceDescriptor]"),
        ("parsed_files", "[ParsedFile]"),
        ("environment", "NameEnvironment"),
        ("names", "CompilationUnitNames"),
    ] {
        accepts(
            &format!("{getter} getter exposes a shared reference"),
            &format!(
                "fn check() {{ let snapshot = fixture(); let _: &{product} = snapshot.{getter}(); }}"
            ),
        );
        rejects(
            &format!("{getter} getter does not expose a mutable reference"),
            &format!(
                "fn check() {{ let mut snapshot = fixture(); \
                 let _: &mut {product} = snapshot.{getter}(); }}"
            ),
            "E0308",
            &["types differ in mutability"],
        );
    }
    accepts(
        "validated names expose an optional shared reference",
        "fn check() { let snapshot = fixture(); \
         let _: Option<&ValidatedCompilationUnitNames> = snapshot.validated_names(); }",
    );
    rejects(
        "validated names do not expose an optional mutable reference",
        "fn check() { let mut snapshot = fixture(); \
         let _: Option<&mut ValidatedCompilationUnitNames> = snapshot.validated_names(); }",
        "E0308",
        &["types differ in mutability"],
    );
}
