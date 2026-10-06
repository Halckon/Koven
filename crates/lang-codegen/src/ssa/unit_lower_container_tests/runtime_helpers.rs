use super::*;

#[test]
fn runtime_generator_unit_cross_file_generic_helper_preserves_three_callable_environments() {
    let mut failures = Vec::new();
    for container in ["Array", "List"] {
        for (environment, initializer) in [
            ("pointer", "{ index -> index }"),
            ("shared", "{ index -> index + scale }"),
            ("owned", "move { index -> index + scale }"),
        ] {
            let mut sources = SourceMap::new();
            let (api_source, api) = parsed(
                &mut sources,
                "z/api.ko",
                &format!(
                    "package z\nfun <T> generate(size: Int, initializer: (Int) -> T): {container}<T> = \
                 {container}<T>(size, initializer)"
                ),
            );
            let (source, file) = parsed(
                &mut sources,
                "a/use.ko",
                &format!(
                    "package a\nimport z.generate\nfun entry(): Int {{ val scale = 7\n\
                 val callback: (Int) -> Int = {initializer}\n\
                 val items = generate<Int>(3, callback)\nreturn items[2] }}"
                ),
            );
            let inputs = [
                SourceUnitInput::new("root", "z/api.ko", api_source, &api),
                SourceUnitInput::new("root", "a/use.ko", source, &file),
            ];
            let (name_environment, type_environment) = standard_environments();
            let (names, typed, owned) =
                analyze(&sources, &inputs, &name_environment, &type_environment);
            let arena = typed.types().types().len();
            let result = lower_scalar_unit_with_entry(
                &sources,
                &inputs,
                &names,
                &type_environment,
                &typed,
                &owned,
                declaration(&names, "a", "entry"),
            );
            match result {
                Ok((program, _)) => {
                    let reversed = [inputs[1], inputs[0]];
                    let (reverse_names, reverse_typed, reverse_owned) =
                        analyze(&sources, &reversed, &name_environment, &type_environment);
                    let (reverse, _) = lower_scalar_unit_with_entry(
                        &sources,
                        &reversed,
                        &reverse_names,
                        &type_environment,
                        &reverse_typed,
                        &reverse_owned,
                        declaration(&reverse_names, "a", "entry"),
                    )
                    .expect("reordered helper inputs");
                    assert_eq!(
                        render_program(&program),
                        render_program(&reverse),
                        "caller-first source order cannot change concrete helper identity"
                    );
                }
                Err(error) => failures.push(format!("{container}/{environment}: {error:?}")),
            }
            assert_eq!(
                typed.types().types().len(),
                arena,
                "backend cannot grow canonical arena"
            );
        }
    }
    assert!(
        failures.is_empty(),
        "cross-file generic Borrow Fn helper must retain each environment: {failures:?}"
    );
}

#[test]
fn runtime_generator_unit_caller_first_pointer_factory_keeps_return_identity() {
    let mut sources = SourceMap::new();
    let (api_source, api) = parsed(
        &mut sources,
        "z/api.ko",
        "package z\nfun factory(): (Int) -> Int { println(\"factory\")\nreturn ({ index -> index }) }",
    );
    let (source, file) = parsed(
        &mut sources,
        "a/use.ko",
        "package a\nimport z.factory\nfun entry(): Int { val items = List<Int>(3, factory())\nreturn items[2] }",
    );
    let inputs = [
        SourceUnitInput::new("root", "z/api.ko", api_source, &api),
        SourceUnitInput::new("root", "a/use.ko", source, &file),
    ];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "a", "entry"),
    )
    .expect("caller-first factory result must reach a verified generator");
}
