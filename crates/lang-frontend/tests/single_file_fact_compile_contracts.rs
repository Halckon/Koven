//! 未退役的 raw checker 能力隔离与 iteration validator 合同；合成 Rust 编译仅在 CI 执行。
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

const IMPORTS: &str = r#"
#![allow(dead_code, unused_imports, unused_mut, unused_variables)]
use lang_frontend::{
    name_resolution::NameResolution,
    parser::ParsedFile,
    type_checking::{TypedFile, ValidatedCompilationUnitTypes, ConstEnabledTypedUnit},
    ownership_checking::{OwnershipCheckedFile, ValidatedCompilationUnitOwnership},
};
"#;

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "koven-single-file-fact-compile-{}-{}",
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
        .arg("--crate-name=single_file_fact_contract")
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
fn raw_checker_products_are_not_validated_unit_or_const_capabilities() {
    accepts(
        "raw checker products remain distinct ordinary inputs",
        "fn check(typed: TypedFile, owned: OwnershipCheckedFile) { let _: TypedFile = typed; let _: OwnershipCheckedFile = owned; }",
    );
    for capability in ["ValidatedCompilationUnitTypes", "ConstEnabledTypedUnit"] {
        rejects(
            "raw typed is not unit capability",
            &format!("fn check(typed: TypedFile) {{ let _: {capability} = typed; }}"),
            "E0308",
            &[capability, "TypedFile"],
        );
    }
    rejects(
        "raw ownership is not validated unit",
        "fn check(owned: OwnershipCheckedFile) { let _: ValidatedCompilationUnitOwnership = owned; }",
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
