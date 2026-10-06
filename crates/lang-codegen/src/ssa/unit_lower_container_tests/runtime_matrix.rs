use super::*;
use crate::ssa::container_lowering_tests::runtime_matrix_support;

#[test]
fn runtime_generator_unit_matrix_preserves_element_capture_and_storage_contracts() {
    let mut failures = Vec::new();
    for case in &runtime_matrix_support::CASES {
        for container in ["Array", "List"] {
            for environment in 0..3 {
                for named in [false, true] {
                    for length in [0, 1, 3] {
                        let text = format!(
                            "package test\n{}",
                            runtime_matrix_support::source(
                                case,
                                container,
                                environment,
                                named,
                                length,
                            )
                        );
                        let mut sources = SourceMap::new();
                        let (source, file) = parsed(&mut sources, "test/runtime-matrix.ko", &text);
                        let inputs = [SourceUnitInput::new(
                            "root",
                            "test/runtime-matrix.ko",
                            source,
                            &file,
                        )];
                        let (name_environment, type_environment) = standard_environments();
                        let (names, typed, owned) =
                            analyze(&sources, &inputs, &name_environment, &type_environment);
                        let arena = typed.types().types().len();
                        match lower_scalar_unit_with_entry(
                            &sources,
                            &inputs,
                            &names,
                            &type_environment,
                            &typed,
                            &owned,
                            declaration(&names, "test", "entry"),
                        ) {
                            Ok((program, _)) => {
                                runtime_matrix_support::assert_layout(&program, environment);
                                assert!(
                                    render_program(&program).contains("container.generate_borrowed")
                                );
                                crate::llvm::render_verified_program(&program)
                                    .expect("the source matrix must reach verified LLVM");
                            }
                            Err(error) => failures.push(format!(
                                "{}/{container}/env{environment}/named{named}/length{length}: {error:?}",
                                case.element
                            )),
                        }
                        assert_eq!(typed.types().types().len(), arena);
                    }
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "all 144 selected unit cases must reach their concrete ABI: {failures:?}"
    );
}
