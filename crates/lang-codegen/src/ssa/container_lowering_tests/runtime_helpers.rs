use super::*;

#[test]
fn runtime_generator_single_generic_helper_preserves_three_callable_environments() {
    let mut failures = Vec::new();
    for container in ["Array", "List"] {
        for (environment, initializer) in [
            ("pointer", "{ index -> index }"),
            ("shared", "{ index -> index + scale }"),
            ("owned", "move { index -> index + scale }"),
        ] {
            let text = format!(
                "fun <T> generate(size: Int, initializer: (Int) -> T): {container}<T> = \
                 {container}<T>(size, initializer)\n\
                 fun entry(): Int {{ val scale = 7\n\
                 val callback: (Int) -> Int = {initializer}\n\
                 val items = generate<Int>(3, callback)\nreturn items[2] }}"
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
                    assert!(render_program(&program).contains("container.generate_borrowed"))
                }
                Err(error) => failures.push(format!("{container}/{environment}: {error:?}")),
            }
            assert_eq!(
                analysis.typed.types().len(),
                arena,
                "backend only reads canonical types"
            );
        }
    }
    assert!(
        failures.is_empty(),
        "generic Borrow Fn helper must retain each actual environment: {failures:?}"
    );
}

#[test]
fn runtime_generator_single_pointer_factory_keeps_return_identity() {
    let analysis = analyze(
        "fun factory(): (Int) -> Int { println(\"factory\")\nreturn ({ index -> index }) }\n\
         fun entry(): Int { val items = List<Int>(3, factory())\nreturn items[2] }",
    );
    assert!(analysis.parsed.diagnostics().is_empty());
    assert!(analysis.names.diagnostics().is_empty());
    assert!(
        analysis.typed.diagnostics().is_empty(),
        "{:?}",
        analysis.typed.diagnostics()
    );
    assert!(
        analysis.owned.diagnostics().is_empty(),
        "{:?}",
        analysis.owned.diagnostics()
    );
    lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    )
    .expect("factory result must reach a verified borrowed generator");
}
