//! SPEC-0249: external compilation contracts for the sealed ordinary owned-unit view.
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

use lang_frontend::ownership_checking::{OwnedCompilationUnitView, owned_compilation_unit_view};

const IMPORTS: &str = r#"
#![allow(dead_code, unused_imports, unused_variables)]
use lang_frontend::{
    name_resolution::{
        CompilationUnitNames, SourceUnitInput, ValidatedCompilationUnitNames,
    },
    ownership_checking::{
        CompilationUnitOwnership, ConstEnabledOwnedUnit, OwnedCompilationUnitView,
        ValidatedCompilationUnitOwnership, owned_compilation_unit_view,
    },
    source::SourceMap,
    type_checking::{
        CompilationUnitTypes, ConstEnabledTypedUnit, TypeEnvironment,
        ValidatedCompilationUnitTypes,
    },
};
fn consume<'view, 'parsed: 'view>(view: OwnedCompilationUnitView<'view, 'parsed>) {
    std::hint::black_box(view);
}
"#;

const PARAMETERS: &str = r#"
    sources: &'view SourceMap,
    inputs: &'view [SourceUnitInput<'parsed>],
    names: &'view ValidatedCompilationUnitNames,
    environment: &'view TypeEnvironment,
    typed: &'view ValidatedCompilationUnitTypes,
    owned: &'view ValidatedCompilationUnitOwnership,
"#;

const FACTORY: &str =
    "owned_compilation_unit_view(sources, inputs, names, environment, typed, owned).unwrap()";

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "koven-owned-unit-compile-{}-{}",
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
        .arg("--crate-name=owned_unit_contract")
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

fn function(extra_parameters: &str, body: &str) -> String {
    format!("fn check<'view, 'parsed: 'view>({PARAMETERS}{extra_parameters}) {{\n{body}\n}}")
}

fn accepts_factory(label: &str) {
    accepts(label, &function("", &format!("consume({FACTORY});")));
}

#[test]
fn factory_and_read_only_getters_keep_distinct_view_and_parsed_lifetimes() {
    // These direct references make a missing public API fail the integration target itself,
    // before any negative snippet could accidentally count an unresolved import as success.
    let _ = std::mem::size_of::<OwnedCompilationUnitView<'static, 'static>>();
    let _ = owned_compilation_unit_view;
    accepts(
        "public factory and five read-only getters",
        &format!(
            r#"
fn factory<'view, 'parsed: 'view>({PARAMETERS})
    -> OwnedCompilationUnitView<'view, 'parsed>
{{
    {FACTORY}
}}
fn getters<'view, 'parsed: 'view>(view: &OwnedCompilationUnitView<'view, 'parsed>) {{
    let _: &'view SourceMap = view.sources();
    let _: &'view [SourceUnitInput<'parsed>] = view.inputs();
    let _: &'view ValidatedCompilationUnitNames = view.names();
    let _: &'view CompilationUnitTypes = view.types();
    let _: &'view CompilationUnitOwnership = view.ownership();
}}
fn temporary_input_slice<'parsed>(
    sources: &SourceMap,
    input: SourceUnitInput<'parsed>,
    names: &ValidatedCompilationUnitNames,
    environment: &TypeEnvironment,
    typed: &ValidatedCompilationUnitTypes,
    owned: &ValidatedCompilationUnitOwnership,
) {{
    let inputs = [input];
    let view = factory(sources, &inputs, names, environment, typed, owned);
    getters(&view);
    consume(view);
}}
"#,
        ),
    );
}

#[test]
fn external_struct_literals_updates_and_field_replacements_are_private() {
    accepts_factory("private fields have a usable factory control");
    rejects(
        "external struct literal",
        &function(
            "",
            r#"
let view = OwnedCompilationUnitView {
    sources, inputs, names, _environment: environment, typed, owned,
};
consume(view);
"#,
        ),
        "E0451",
        &[
            "`sources`",
            "`inputs`",
            "`names`",
            "`_environment`",
            "`typed`",
            "`owned`",
            "are private",
        ],
    );
    for (field, value) in [
        ("sources", "sources"),
        ("inputs", "inputs"),
        ("names", "names"),
        ("_environment", "environment"),
        ("typed", "typed"),
        ("owned", "owned"),
    ] {
        rejects(
            &format!("external struct update of {field}"),
            &function(
                "",
                &format!(
                    "let view = {FACTORY};\n\
                     consume(OwnedCompilationUnitView {{ {field}: {value}, ..view }});"
                ),
            ),
            "E0451",
            &[&format!("`{field}`"), "private"],
        );
        rejects(
            &format!("direct field replacement of {field}"),
            &function(
                "",
                &format!("let mut view = {FACTORY};\nview.{field} = {value};"),
            ),
            "E0616",
            &[&format!(
                "field `{field}` of struct `OwnedCompilationUnitView` is private"
            )],
        );
    }
}

#[test]
fn const_typed_and_owned_capabilities_cannot_enter_the_basic_factory() {
    for (argument, actual, expected) in [
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
        let label = format!("constant {argument} capability");
        accepts_factory(&format!("{label} basic control"));
        let source = function("", &format!("consume({FACTORY});")).replace(
            &format!("{argument}: &'view {expected}"),
            &format!("{argument}: &'view {actual}"),
        );
        rejects(
            &label,
            &source,
            "E0308",
            &[
                &format!("expected reference `&{expected}`"),
                &format!("found reference `&'view {actual}`"),
            ],
        );
    }
}

#[test]
fn recovery_names_types_and_ownership_cannot_enter_the_factory() {
    for (argument, actual, expected) in [
        (
            "names",
            "CompilationUnitNames",
            "ValidatedCompilationUnitNames",
        ),
        (
            "typed",
            "CompilationUnitTypes",
            "ValidatedCompilationUnitTypes",
        ),
        (
            "owned",
            "CompilationUnitOwnership",
            "ValidatedCompilationUnitOwnership",
        ),
    ] {
        let label = format!("recovery {argument}");
        accepts_factory(&format!("{label} validated control"));
        let source = function("", &format!("consume({FACTORY});")).replace(
            &format!("{argument}: &'view {expected}"),
            &format!("{argument}: &'view {actual}"),
        );
        rejects(
            &label,
            &source,
            "E0308",
            &[
                &format!("expected reference `&{expected}`"),
                &format!("found reference `&'view {actual}`"),
            ],
        );
    }
}

#[test]
fn view_cannot_escape_any_of_its_six_borrowed_products() {
    for (argument, replacement_type) in [
        ("sources", "SourceMap"),
        ("inputs", "Vec<SourceUnitInput<'parsed>>"),
        ("names", "ValidatedCompilationUnitNames"),
        ("environment", "TypeEnvironment"),
        ("typed", "ValidatedCompilationUnitTypes"),
        ("owned", "ValidatedCompilationUnitOwnership"),
    ] {
        let extra_parameters = format!("replacement: {replacement_type},");
        let arguments = [
            "sources",
            "inputs",
            "names",
            "environment",
            "typed",
            "owned",
        ]
        .map(|name| if name == argument { "&local" } else { name })
        .join(", ");
        let factory = format!("owned_compilation_unit_view({arguments}).unwrap()");
        accepts(
            &format!("{argument} borrowed only inside its scope"),
            &function(
                &extra_parameters,
                &format!("{{ let local = replacement; consume({factory}); }}"),
            ),
        );
        rejects(
            &format!("view escapes borrowed {argument}"),
            &function(
                &extra_parameters,
                &format!(
                    "let view;\n\
                     {{ let local = replacement; view = {factory}; }}\n\
                     consume(view);"
                ),
            ),
            "E0597",
            &["`local` does not live long enough"],
        );
    }
}

#[test]
fn view_cannot_escape_a_temporary_inputs_slice() {
    accepts(
        "named short-lived inputs slice",
        &function(
            "",
            &format!(
                "let bound_inputs = [inputs[0]];\nconsume({});",
                FACTORY.replace("inputs,", "&bound_inputs,"),
            ),
        ),
    );
    rejects(
        "temporary input array expires before view use",
        &function(
            "",
            &format!(
                "let view = {};\nconsume(view);",
                FACTORY.replace("inputs,", "&[inputs[0]],"),
            ),
        ),
        "E0716",
        &[
            "temporary value dropped while borrowed",
            "borrow later used here",
        ],
    );
}

#[test]
fn indexed_signature_compatibility_is_not_an_external_escape_hatch() {
    accepts_factory("indexed compatibility has a public factory control");
    accepts(
        "existing public compatibility remains callable",
        &function(
            "",
            "let _: bool = typed.types().signatures().is_compatible_with(\n\
             sources, inputs, names, environment);",
        ),
    );
    rejects(
        "indexed compatibility remains crate-private",
        &function(
            "",
            "let _: bool = typed.types().signatures().is_compatible_with_index(\n\
             names.names().index(), names, environment);",
        ),
        "E0624",
        &["method `is_compatible_with_index` is private"],
    );
}
