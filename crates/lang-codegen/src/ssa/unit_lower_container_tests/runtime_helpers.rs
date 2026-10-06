use super::*;

#[test]
fn runtime_generator_unit_nothing_operand_stops_before_borrow_and_initializer() {
    let mut failures = Vec::new();
    for container in ["Array", "List"] {
        for (operands, size_loans) in [
            ("error(\"size\"), { index -> index }", 0),
            ("1, error(\"initializer\")", 1),
        ] {
            let mut sources = SourceMap::new();
            let text = format!(
                "package test\nfun entry(): Int {{ val unused = {container}<Int>({operands})\nreturn 0 }}"
            );
            let (source, file) = parsed(&mut sources, "test/nothing.ko", &text);
            let inputs = [SourceUnitInput::new(
                "root",
                "test/nothing.ko",
                source,
                &file,
            )];
            let (name_environment, type_environment) = standard_environments();
            let (names, typed, owned) =
                analyze(&sources, &inputs, &name_environment, &type_environment);
            let constructors = typed.types().container_constructions();
            assert_eq!(constructors.len(), 1);
            assert_eq!(
                owned
                    .ownership()
                    .loans()
                    .iter()
                    .filter(|loan| loan.call() == constructors[0].expression())
                    .count(),
                size_loans
            );
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
                    assert!(
                        program.modules[0]
                            .functions
                            .iter()
                            .flat_map(|function| &function.instructions)
                            .all(|instruction| !matches!(
                                instruction.operation,
                                Operation::ContainerGenerateBorrowed { .. }
                            ))
                    );
                    assert!(
                        program.modules[0]
                            .functions
                            .iter()
                            .flat_map(|function| &function.blocks)
                            .any(|block| block.terminator.as_ref().is_some_and(
                                |terminator| matches!(
                                    terminator.kind,
                                    super::super::model::TerminatorKind::Abort
                                )
                            ))
                    );
                    crate::llvm::render_verified_program(&program)
                        .expect("ordinary source Abort verifies through LLVM");
                }
                Err(error) => failures.push(format!("{container}/{operands}: {error:?}")),
            }
            assert_eq!(typed.types().types().len(), arena);
        }
    }
    assert!(
        failures.is_empty(),
        "Nothing must stop before demanding later callable facts: {failures:?}"
    );
}

#[test]
fn runtime_generator_unit_unreachable_initializers_do_not_demand_callable_layouts() {
    for container in ["Array", "List"] {
        for initializer in [
            "{ index -> index }",
            "{ index -> index + scale }",
            "move { index -> index + scale }",
        ] {
            let mut sources = SourceMap::new();
            let text = format!(
                "package test\nfun entry(): Int {{ val scale = 7\nreturn 0\n\
                 val unused = {container}<Int>(1, {initializer}) }}"
            );
            let (source, file) = parsed(&mut sources, "test/unreachable.ko", &text);
            let inputs = [SourceUnitInput::new(
                "root",
                "test/unreachable.ko",
                source,
                &file,
            )];
            let (name_environment, type_environment) = standard_environments();
            let (names, typed, owned) =
                analyze(&sources, &inputs, &name_environment, &type_environment);
            let constructions = typed.types().container_constructions();
            assert_eq!(
                constructions.len(),
                1,
                "Phase 2 still describes the dead call"
            );
            assert!(
                owned
                    .ownership()
                    .loans()
                    .iter()
                    .all(|loan| loan.call() != constructions[0].expression())
            );
            for (expression, node) in file.ast().expressions().iter() {
                if matches!(
                    node.payload(),
                    lang_frontend::parser::Expression::Lambda { .. }
                ) {
                    let id = lang_frontend::type_checking::UnitExpressionId::new(
                        constructions[0].expression().source_unit(),
                        expression,
                    );
                    assert!(
                        owned.ownership().closure(id).is_some(),
                        "static capture metadata exists"
                    );
                    assert!(
                        owned.ownership().callable_origin(id).is_none(),
                        "Phase 3 never evaluated the lambda"
                    );
                }
            }
            let arena = typed.types().types().len();
            let (program, _) = lower_scalar_unit_with_entry(
                &sources,
                &inputs,
                &names,
                &type_environment,
                &typed,
                &owned,
                declaration(&names, "test", "entry"),
            )
            .expect("return-after runtime initializer cannot demand a callable route");
            assert_eq!(
                program.modules[0].functions.len(),
                1,
                "only the reachable source entry is declared"
            );
            assert_eq!(typed.types().types().len(), arena);
            crate::llvm::render_verified_program(&program)
                .expect("reachable entry verifies through LLVM");
        }
    }
}

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
