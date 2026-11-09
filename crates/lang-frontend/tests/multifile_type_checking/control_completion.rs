//! Statement-context Unit must not erase the producer's control-flow result.
use super::*;

#[test]
fn unit_statement_when_completion_is_source_qualified_and_survives_publication() {
    let mut sources = SourceMap::new();
    let (total_id, total) = parsed(
        &mut sources,
        "total.ko",
        "package a\nenum class Flag { One, Two }\nfun choose(flag: Flag): Int { when(flag) { is Flag.One -> return 7; is Flag.Two -> return 8 } }",
    );
    let (partial_id, partial) = parsed(
        &mut sources,
        "partial.ko",
        "package b\nenum class Flag { One, Two }\nfun choose(flag: Flag): Int { when(flag) { is Flag.One -> return 7; is Flag.Two -> (8) }; return 9 }",
    );
    let total_when = when_expressions(&total)[0];
    let partial_when = when_expressions(&partial)[0];
    assert_eq!(
        total_when, partial_when,
        "colliding local IDs must not share a flow fact"
    );
    for reverse in [false, true] {
        let mut inputs = vec![
            SourceUnitInput::new("root", "a/total.ko", total_id, &total),
            SourceUnitInput::new("root", "b/partial.ko", partial_id, &partial),
        ];
        if reverse {
            inputs.reverse();
        }
        let (environment, types) = standard_environments();
        let names = validated_names(&sources, &inputs, &environment);
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        for (source, expression, falls_through) in [
            (total_id, total_when, false),
            (partial_id, partial_when, true),
        ] {
            let id = UnitExpressionId::new(source_unit(&names, source), expression);
            let ty = typed.expression_type(id).unwrap();
            assert_eq!(
                typed.types().get(ty),
                Some(&UnitTypeKind::Builtin(BuiltinType::Unit))
            );
            assert_eq!(typed.expression_falls_through(id), Some(falls_through));
            assert_eq!(
                typed.clone().expression_falls_through(id),
                Some(falls_through)
            );
        }
    }
}

#[test]
fn unit_when_selector_exit_preserves_earlier_match_and_stops_the_miss_path() {
    for (control, falls_through) in [
        ("when (flag) { true, pred(return 7) -> return 2 }", false),
        ("when { flag, pred(return 7) -> return 2 }", false),
        ("when (flag) { true, pred(return 7) -> 2 }", true),
        ("when { flag, pred(return 7) -> 2 }", true),
        ("when (flag) { true, pred(7) -> return 2 }", true),
        ("when { flag, pred(7) -> return 2 }", true),
        ("when { pred(return 7) -> 2; else -> 3 }", false),
    ] {
        let mut sources = SourceMap::new();
        let (source, file) = parsed(
            &mut sources,
            "when-selector-completion.ko",
            &format!(
                "fun pred(value: Int): Boolean = value == 1\nfun choose(flag: Boolean): Int {{ {control} }}"
            ),
        );
        for input in [
            SourceUnitInput::memory_document(source, &file),
            SourceUnitInput::new("root", "when-selector-completion.ko", source, &file),
        ] {
            let inputs = [input];
            let (name_environment, type_environment) = standard_environments();
            let names = validated_names(&sources, &inputs, &name_environment);
            let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
                .expect("type product");
            let id = UnitExpressionId::new(source_unit(&names, source), when_expressions(&file)[0]);
            assert_eq!(
                typed.expression_falls_through(id),
                Some(falls_through),
                "{control}"
            );
            let diagnostics: Vec<_> = typed
                .diagnostics()
                .iter()
                .map(|diagnostic| diagnostic.code().to_string())
                .collect();
            let expected = if falls_through {
                ["L0088"].as_slice()
            } else {
                [].as_slice()
            };
            assert_eq!(diagnostics, expected, "{control}");
        }
    }
}

#[test]
fn unit_when_selector_exit_publishes_nothing_after_branch_type_checking() {
    for (arms, diagnostics) in [
        ("pred(return 7) -> 2; else -> 3", [].as_slice()),
        (
            "pred(return 7) -> 2; else -> \"mixed\"",
            ["L0112"].as_slice(),
        ),
    ] {
        let mut sources = SourceMap::new();
        let (source, file) = parsed(
            &mut sources,
            "when-selector-result.ko",
            &format!(
                "fun pred(value: Int): Boolean = value == 1
                 fun choose(): Int {{ val result = when {{ {arms} }}; return 0 }}"
            ),
        );
        for input in [
            SourceUnitInput::memory_document(source, &file),
            SourceUnitInput::new("root", "when-selector-result.ko", source, &file),
        ] {
            let inputs = [input];
            let (name_environment, type_environment) = standard_environments();
            let names = validated_names(&sources, &inputs, &name_environment);
            let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
                .expect("type product");
            let actual: Vec<_> = typed
                .diagnostics()
                .iter()
                .map(|diagnostic| diagnostic.code().to_string())
                .collect();
            assert_eq!(actual, diagnostics, "{arms}");
            if diagnostics.is_empty() {
                let id =
                    UnitExpressionId::new(source_unit(&names, source), when_expressions(&file)[0]);
                assert_eq!(typed.expression_falls_through(id), Some(false));
                let ty = typed.expression_type(id).expect("when result");
                assert_eq!(
                    typed.types().get(ty),
                    Some(&UnitTypeKind::Builtin(BuiltinType::Nothing))
                );
            }
        }
    }
}
