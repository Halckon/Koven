use super::{Command, TestDirectory, analyze_sources, emit_native_unit_object};

#[test]
fn bitwise_unit_native_width_boundaries_masked_counts_and_eager_operands() {
    let (functions, checks, expected) = crate::bitwise_test_support::binary_fixture("p.");
    run_fixture(functions, checks, expected);
}

#[test]
fn bitwise_inv_unit_native_boundaries_and_single_evaluation() {
    let (functions, checks, expected) = crate::bitwise_test_support::inv_fixture("p.");
    run_fixture(functions, checks, expected);
}

fn run_fixture(functions: String, checks: String, expected: String) {
    let analysis = analyze_sources(
        &format!("package p\n{functions}"),
        &format!("package q\nfun entry(): Unit {{\n{checks}\n}}\n"),
    );
    let directory = TestDirectory::create();
    let object = directory.join("bitwise.o");
    let executable = directory.join("bitwise");
    emit_native_unit_object(
        &analysis.sources,
        &analysis.inputs(),
        &analysis.names,
        &analysis.environment,
        &analysis.typed,
        &analysis.owned,
        analysis.declaration("q", "entry"),
        &object,
    )
    .expect("bitwise unit native object");
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("linker");
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&executable).output().expect("executable");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, expected.as_bytes());
    assert!(run.stderr.is_empty(), "{run:?}");
}

#[test]
fn bitwise_inv_unit_native_literals_fields_elements_and_source_member_identity() {
    run_fixture(
        format!(
            "{}\nfun element(values: List<Int>): Int = values[0].inv()\nclass Sample {{ fun inv(): Int = 7 }}\nfun exerciseElements(): Unit {{ if (Sample().inv() == 7) {{ println(\"source-member\") }}; val values = listOf(3); if (element(values) == -4) {{ println(\"borrow-element\") }}; if (values[0].inv() == -4) {{ println(\"named-element\") }} }}",
            crate::bitwise_test_support::inv_receiver_fixture()
        ),
        "p.exercise()\np.exerciseElements()".to_owned(),
        format!(
            "{}source-member\nborrow-element\nnamed-element\n",
            crate::bitwise_test_support::INV_RECEIVER_STDOUT
        ),
    );
}

#[test]
fn bitwise_unit_native_guide_litmus_12_runs_the_normative_source() {
    let run = run_constant_fixture(
        "demo/interop/litmus.ko",
        crate::bitwise_test_support::guide_litmus_12(),
        "package q\nfun entry(): Unit { demo.interop.main() }",
    );
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"Mask initialized\n");
    assert!(run.stderr.is_empty(), "{run:?}");
}

#[test]
fn bitwise_unit_native_operands_preserve_control_exit_and_order() {
    let (functions, checks, expected) = crate::bitwise_test_support::control_fixture();
    run_fixture(
        functions.to_owned(),
        format!("p.{checks}"),
        expected.to_owned(),
    );
}

fn run_constant_fixture(
    provider_path: &str,
    provider_text: &str,
    consumer_text: &str,
) -> std::process::Output {
    use lang_frontend::{
        name_resolution::{
            SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names,
        },
        ownership_checking::check_compilation_unit_constant_ownership,
        source::SourceMap,
        type_checking::{check_compilation_unit_types, standard_environments},
    };
    let mut sources = SourceMap::new();
    let (provider_source, provider) = super::parsed(&mut sources, provider_path, provider_text);
    let (consumer_source, consumer) = super::parsed(&mut sources, "q/consumer.ko", consumer_text);
    let inputs = [
        SourceUnitInput::new("root", provider_path, provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let (name_environment, environment) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &name_environment)
        .unwrap()
        .validate()
        .unwrap();
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &environment)
        .unwrap()
        .validate_constants()
        .unwrap();
    let owned =
        check_compilation_unit_constant_ownership(&sources, &inputs, &names, &environment, &typed)
            .unwrap()
            .validate()
            .unwrap();
    let entry = names
        .names()
        .index()
        .declarations()
        .iter()
        .find(|declaration| declaration.name() == "entry")
        .unwrap()
        .id();
    let directory = TestDirectory::create();
    let object = directory.join("constant-bitwise.o");
    crate::emit_native_constant_unit_object(
        &sources,
        &inputs,
        &names,
        &environment,
        &typed,
        &owned,
        entry,
        &object,
    )
    .expect("constant bitwise object");
    let executable = directory.join("constant-bitwise");
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("linker");
    assert!(linked.status.success(), "{linked:?}");
    Command::new(&executable).output().expect("executable")
}

#[test]
fn bitwise_unit_native_const_and_runtime_agree_with_independent_oracles() {
    let (functions, checks, expected) = crate::bitwise_test_support::constant_runtime_fixture("p.");
    let run = run_constant_fixture(
        "p/provider.ko",
        &format!("package p\n{functions}"),
        &format!("package q\nfun entry(): Unit {{\n{checks}\n}}"),
    );
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, expected.as_bytes());
    assert!(run.stderr.is_empty(), "{run:?}");
}
