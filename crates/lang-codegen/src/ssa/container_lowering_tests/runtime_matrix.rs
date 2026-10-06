use super::{analyze, lower_scalar_file, render_program, runtime_matrix_support};

#[test]
fn runtime_generator_single_matrix_preserves_element_capture_and_storage_contracts() {
    let mut failures = Vec::new();
    for case in &runtime_matrix_support::CASES {
        for container in ["Array", "List"] {
            for environment in 0..3 {
                for named in [false, true] {
                    for length in [0, 1, 3] {
                        let text = runtime_matrix_support::source(
                            case,
                            container,
                            environment,
                            named,
                            length,
                        );
                        let analysis = analyze(&text);
                        assert!(analysis.parsed.diagnostics().is_empty(), "{text}");
                        assert!(analysis.names.diagnostics().is_empty(), "{text}");
                        assert!(
                            analysis.typed.diagnostics().is_empty(),
                            "{text}: {:?}",
                            analysis.typed.diagnostics()
                        );
                        assert!(
                            analysis.owned.diagnostics().is_empty(),
                            "{text}: {:?}",
                            analysis.owned.diagnostics()
                        );
                        let arena = analysis.typed.types().len();
                        match lower_scalar_file(
                            &analysis.sources,
                            &analysis.parsed,
                            &analysis.names,
                            &analysis.typed,
                            &analysis.owned,
                        ) {
                            Ok(program) => {
                                runtime_matrix_support::assert_layout(&program, environment);
                                assert!(
                                    render_program(&program).contains("container.generate_borrowed"),
                                    "source runtime construction must borrow its initializer"
                                );
                                crate::llvm::render_verified_program(&program)
                                    .expect("the source matrix must reach verified LLVM");
                            }
                            Err(error) => failures.push(format!(
                                "{}/{container}/env{environment}/named{named}/length{length}: {error:?}",
                                case.element
                            )),
                        }
                        assert_eq!(analysis.typed.types().len(), arena);
                    }
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "all 144 selected source cases must reach their concrete ABI: {failures:?}"
    );
}
