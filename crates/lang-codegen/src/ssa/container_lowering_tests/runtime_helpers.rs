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
            let analysis = analyze(&text, "entry");
            assert!(analysis.parsed.diagnostics().is_empty(), "{text}");
            assert!(analysis.names.names().diagnostics().is_empty(), "{text}");
            assert!(
                analysis.typed.types().diagnostics().is_empty(),
                "{text}: {:?}",
                analysis.typed.types().diagnostics()
            );
            assert!(
                analysis.owned.ownership().diagnostics().is_empty(),
                "{text}: {:?}",
                analysis.owned.ownership().diagnostics()
            );
            let arena = analysis.typed.types().types().len();
            match lower_unit(&analysis) {
                Ok(program) => {
                    assert!(render_program(&program).contains("container.generate_borrowed"))
                }
                Err(error) => failures.push(format!("{container}/{environment}: {error:?}")),
            }
            assert_eq!(
                analysis.typed.types().types().len(),
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
        "entry",
    );
    assert!(analysis.parsed.diagnostics().is_empty());
    assert!(analysis.names.names().diagnostics().is_empty());
    assert!(
        analysis.typed.types().diagnostics().is_empty(),
        "{:?}",
        analysis.typed.types().diagnostics()
    );
    assert!(
        analysis.owned.ownership().diagnostics().is_empty(),
        "{:?}",
        analysis.owned.ownership().diagnostics()
    );
    lower_unit(&analysis).expect("factory result must reach a verified borrowed generator");
}
