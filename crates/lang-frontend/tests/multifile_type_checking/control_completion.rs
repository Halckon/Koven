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
