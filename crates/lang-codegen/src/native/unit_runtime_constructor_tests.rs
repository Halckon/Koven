//! Observable ordinary source behavior through the compilation-unit object facade.
use super::*;
use crate::native_tests::runtime_constructor_tests::{assert_output, cases, evaluation};

/// Keep object publication, native linker and process execution identical for the intent cases.
fn execute(analysis: &UnitAnalysis, entry: &str, label: &str) -> std::process::Output {
    let directory = TestDirectory::create();
    let object = directory.join("runtime.o");
    let executable = directory.join("runtime");
    emit_native_unit_object(
        &analysis.sources,
        &analysis.inputs(),
        &analysis.names,
        &analysis.environment,
        &analysis.typed,
        &analysis.owned,
        analysis.declaration("q", entry),
        &object,
    )
    .unwrap_or_else(|error| panic!("{label}: {error:?}"));
    assert_no_sibling_temporary(&directory.0);
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("native link runs");
    assert!(linked.status.success(), "{label}: {linked:?}");
    Command::new(&executable)
        .output()
        .expect("native program runs")
}

#[test]
fn runtime_constructor_native_unit_matrix_executes_logical_callbacks() {
    let cases = cases();
    assert_eq!(cases.len(), 144);
    for case in &cases {
        let analysis = analyze_sources(
            "package p\nfun unused(): Unit {}",
            &format!("package q\n{}", case.text),
        );
        assert_output(case, &execute(&analysis, "nativeEntry", &case.label));
    }
}

#[test]
fn runtime_constructor_native_unit_orders_callbacks_elements_and_environment_cleanup() {
    for (text, expected) in crate::native_tests::runtime_constructor_tests::ordered_resource_cases()
    {
        let analysis = analyze_sources(
            "package p\nfun unused(): Unit {}",
            &format!("package q\n{text}"),
        );
        run(&analysis, expected.as_bytes());
    }
}

#[test]
fn runtime_constructor_native_unit_cross_file_helper_preserves_element_and_environment() {
    let cases = crate::native_tests::runtime_constructor_tests::helper_cases();
    assert_eq!(cases.len(), 48);
    for (case, api) in cases {
        let analysis = analyze_sources(
            &format!("package p\n{api}"),
            &format!("package q\nimport p.generate\n{}", case.text),
        );
        assert_output(&case, &execute(&analysis, "nativeEntry", &case.label));
    }
}

#[test]
fn runtime_constructor_native_unit_storable_helper_elements() {
    let cases = crate::native_tests::runtime_constructor_tests::storable_helpers::cases();
    assert_eq!(cases.len(), 48);
    for (case, api) in cases {
        let analysis = analyze_sources(
            &format!("package p\n{api}"),
            &format!("package q\nimport p.generate\n{}", case.text),
        );
        assert_output(&case, &execute(&analysis, "nativeEntry", &case.label));
    }
}

#[test]
fn runtime_constructor_native_unit_evaluates_operands_once_in_order() {
    let cases = evaluation::evaluation_cases();
    assert_eq!(cases.len(), 6);
    for case in cases {
        let analysis = analyze_sources(
            &format!("package p\n{}", case.api),
            &format!("package q\nimport p.size\nimport p.factory\n{}", case.entry),
        );
        evaluation::assert_evaluation_output(&case, &execute(&analysis, "entry", &case.label));
    }
}
