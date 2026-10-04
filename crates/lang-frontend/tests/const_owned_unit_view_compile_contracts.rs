//! SPEC-0254: external compilation contracts for the sealed constant owned-unit view.
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

use lang_frontend::ownership_checking::{
    ConstOwnedCompilationUnitView, OwnedCompilationUnitViewError, const_owned_compilation_unit_view,
};

const IMPORTS: &str = r#"
#![allow(dead_code, unused_imports, unused_variables)]
use lang_frontend::{
    name_resolution::{
        CompilationUnitNames, SourceUnitInput, ValidatedCompilationUnitNames,
    },
    ownership_checking::{
        CompilationUnitConstantOwnership, CompilationUnitOwnership, ConstEnabledOwnedUnit,
        ConstOwnedCompilationUnitView, OwnedCompilationUnitView, OwnedCompilationUnitViewError,
        ValidatedCompilationUnitOwnership, const_owned_compilation_unit_view,
        owned_compilation_unit_view,
    },
    parser::ParsedFile,
    source::SourceMap,
    type_checking::{
        CompilationUnitTypes, ConstEnabledTypedUnit, TypeEnvironment,
        ValidatedCompilationUnitTypes,
    },
};
fn consume<'view, 'parsed: 'view>(view: ConstOwnedCompilationUnitView<'view, 'parsed>) {
    std::hint::black_box(view);
}
"#;

const PARAMETERS: &str = r#"
    sources: &'view SourceMap,
    inputs: &'view [SourceUnitInput<'parsed>],
    names: &'view ValidatedCompilationUnitNames,
    environment: &'view TypeEnvironment,
    typed: &'view ConstEnabledTypedUnit,
    owned: &'view ConstEnabledOwnedUnit,
"#;

const FACTORY: &str =
    "const_owned_compilation_unit_view(sources, inputs, names, environment, typed, owned).unwrap()";

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "koven-const-owned-unit-compile-{}-{}",
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
        .arg("--crate-name=const_owned_unit_contract")
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
    let _ = std::mem::size_of::<ConstOwnedCompilationUnitView<'static, 'static>>();
    let _ = std::mem::size_of::<OwnedCompilationUnitViewError>();
    let _ = const_owned_compilation_unit_view;
    accepts(
        "public factory and six read-only getters",
        &format!(
            r#"
fn factory<'view, 'parsed: 'view>({PARAMETERS})
    -> ConstOwnedCompilationUnitView<'view, 'parsed>
{{
    {FACTORY}
}}
fn checked_factory<'view, 'parsed: 'view>({PARAMETERS})
    -> Result<ConstOwnedCompilationUnitView<'view, 'parsed>, OwnedCompilationUnitViewError>
{{
    const_owned_compilation_unit_view(sources, inputs, names, environment, typed, owned)
}}
fn getters<'view, 'parsed: 'view>(view: &ConstOwnedCompilationUnitView<'view, 'parsed>) {{
    let _: &'view SourceMap = view.sources();
    let _: &'view [SourceUnitInput<'parsed>] = view.inputs();
    let _: &'view ValidatedCompilationUnitNames = view.names();
    let _: &'view CompilationUnitTypes = view.types();
    let _: &'view CompilationUnitOwnership = view.ownership();
    let _: &'view ConstEnabledOwnedUnit = view.constant_ownership();
}}
fn temporary_input_slice<'parsed>(
    sources: &SourceMap,
    input: SourceUnitInput<'parsed>,
    names: &ValidatedCompilationUnitNames,
    environment: &TypeEnvironment,
    typed: &ConstEnabledTypedUnit,
    owned: &ConstEnabledOwnedUnit,
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
let view = ConstOwnedCompilationUnitView {
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
                     consume(ConstOwnedCompilationUnitView {{ {field}: {value}, ..view }});"
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
                "field `{field}` of struct `ConstOwnedCompilationUnitView` is private"
            )],
        );
    }
}

#[test]
fn basic_typed_and_owned_capabilities_cannot_enter_the_const_factory() {
    for (argument, actual, expected) in [
        (
            "typed",
            "ValidatedCompilationUnitTypes",
            "ConstEnabledTypedUnit",
        ),
        (
            "owned",
            "ValidatedCompilationUnitOwnership",
            "ConstEnabledOwnedUnit",
        ),
    ] {
        let label = format!("basic {argument} capability");
        accepts_factory(&format!("{label} constant control"));
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
        ("typed", "CompilationUnitTypes", "ConstEnabledTypedUnit"),
        (
            "owned",
            "CompilationUnitConstantOwnership",
            "ConstEnabledOwnedUnit",
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
        ("typed", "ConstEnabledTypedUnit"),
        ("owned", "ConstEnabledOwnedUnit"),
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
        let factory = format!("const_owned_compilation_unit_view({arguments}).unwrap()");
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

#[test]
fn view_cannot_escape_parsed_file_root_or_logical_path_storage() {
    for (label, replacement_type, root, path, parsed) in [
        (
            "parsed file",
            "ParsedFile",
            "inputs[0].root_identity()",
            "inputs[0].logical_path()",
            "&local",
        ),
        (
            "root identity",
            "String",
            "local.as_str()",
            "inputs[0].logical_path()",
            "inputs[0].parsed()",
        ),
        (
            "logical path",
            "String",
            "inputs[0].root_identity()",
            "local.as_str()",
            "inputs[0].parsed()",
        ),
    ] {
        let extra = format!("replacement: {replacement_type},");
        let input =
            format!("SourceUnitInput::new({root}, {path}, inputs[0].source_id(), {parsed})");
        let factory = FACTORY.replace("inputs,", "&local_inputs,");
        accepts(
            &format!("view stays inside {label} storage scope"),
            &function(
                &extra,
                &format!(
                    "{{ let local = replacement; let local_inputs = [{input}]; \
                     consume({factory}); }}"
                ),
            ),
        );
        // Keep the input array outside the block so only its backing storage expires.
        rejects(
            &format!("view escapes {label} storage"),
            &function(
                &extra,
                &format!(
                    "let local_inputs; let view;\n\
                     {{ let local = replacement; local_inputs = [{input}];\n\
                     view = {factory}; }}\nconsume(view);"
                ),
            ),
            "E0597",
            &[
                "`local` does not live long enough",
                "borrow later used here",
            ],
        );
    }
}

#[test]
fn all_six_getters_expose_only_shared_references() {
    for (getter, product) in [
        ("sources", "SourceMap"),
        ("inputs", "[SourceUnitInput<'parsed>]"),
        ("names", "ValidatedCompilationUnitNames"),
        ("types", "CompilationUnitTypes"),
        ("ownership", "CompilationUnitOwnership"),
        ("constant_ownership", "ConstEnabledOwnedUnit"),
    ] {
        accepts(
            &format!("{getter} returns a shared reference"),
            &function(
                "",
                &format!("let view = {FACTORY}; let _: &{product} = view.{getter}();"),
            ),
        );
        rejects(
            &format!("{getter} cannot expose a mutable reference"),
            &function(
                "",
                &format!("let mut view = {FACTORY}; let _: &mut {product} = view.{getter}();"),
            ),
            "E0308",
            &["types differ in mutability"],
        );
    }
}

#[test]
fn basic_and_constant_views_cannot_substitute_for_each_other() {
    for (actual, expected) in [
        ("ConstOwnedCompilationUnitView", "OwnedCompilationUnitView"),
        ("OwnedCompilationUnitView", "ConstOwnedCompilationUnitView"),
    ] {
        accepts(
            &format!("{actual} passes its own view control"),
            &format!(
                "fn check<'view, 'parsed: 'view>(view: {actual}<'view, 'parsed>) {{ \
                 let _: {actual}<'view, 'parsed> = view; }}"
            ),
        );
        rejects(
            &format!("{actual} cannot become {expected}"),
            &format!(
                "fn check<'view, 'parsed: 'view>(view: {actual}<'view, 'parsed>) {{ \
                 let _: {expected}<'view, 'parsed> = view; }}"
            ),
            "E0308",
            &["mismatched types", actual, expected],
        );
    }
}
