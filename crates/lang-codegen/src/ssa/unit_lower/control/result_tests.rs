//! Contextual null delivery must not lend another control's result identity.
use super::*;
use crate::ssa::unit_lower::nullable_proof::guard_tests::support::Fixture;

#[test]
fn contextual_null_result_requires_exact_target_and_control_identity() {
    let fixture = Fixture::new(
        r#"fun entry(flag: Boolean): Unit {
    val value = if (flag) { "value".clone() } else { if (flag) { null } else { null } }
    if (value != null) { println(value) }
}"#,
    );
    let controls = fixture
        .parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| {
            matches!(node.payload(), Expression::If { .. }).then_some((id, node.span()))
        })
        .collect::<Vec<_>>();
    let mut function = fixture.function().clone();
    fixture.with_lowerer(&mut function, |lowerer| {
        let control_with_inner = |builtin| {
            controls
                .iter()
                .copied()
                .find(|(expression, _)| {
                    let ty = lowerer
                        .typed
                        .expression_type(UnitExpressionId::new(lowerer.source_unit, *expression))
                        .unwrap();
                    matches!(lowerer.typed.types().get(ty), Some(UnitTypeKind::Nullable(inner))
                if builtin_type(lowerer.typed, *inner) == Some(builtin))
                })
                .unwrap()
        };
        let (outer, outer_span) = control_with_inner(BuiltinType::String);
        let (nested, nested_span) = control_with_inner(BuiltinType::Nothing);
        let outer_plan = lowerer.control_result(outer, None, outer_span).unwrap();
        let target = outer_plan.string.unwrap();
        let nested_plan = lowerer
            .control_result(nested, Some(target), nested_span)
            .unwrap();
        assert_eq!(
            nested_plan.control,
            UnitExpressionId::new(lowerer.source_unit, nested)
        );
        assert!(nested_plan.required);
        assert_eq!(nested_plan.string, Some(target));
        // An unresolved Nothing? has no stand-alone String? storage entitlement.
        assert!(matches!(
            lowerer.control_result(nested, None, nested_span),
            Err(LoweringError {
                kind: LoweringErrorKind::UnsupportedNode,
                ..
            })
        ));
        let boolean = lowerer.typed.types().builtin(BuiltinType::Boolean).unwrap();
        let boolean_storage = lowerer.type_ids[&boolean];
        let before = lowerer.function.clone();
        for (expression, expected, span) in [
            (nested, (target.0, boolean_storage), nested_span),
            (nested, (boolean, target.1), nested_span),
            (outer, target, outer_span),
        ] {
            assert!(matches!(
                lowerer.control_result(expression, Some(expected), span),
                Err(LoweringError {
                    kind: LoweringErrorKind::MissingFact,
                    ..
                })
            ));
        }
        let Expression::If {
            condition,
            then_branch,
            else_branch,
            ..
        } = *lowerer
            .parsed
            .ast()
            .expressions()
            .get(nested)
            .unwrap()
            .payload()
        else {
            unreachable!()
        };
        assert_eq!(
            lowerer
                .lower_conditional(
                    nested,
                    condition,
                    then_branch,
                    else_branch,
                    0,
                    1,
                    nested_span,
                    outer_plan
                )
                .unwrap_err()
                .kind,
            LoweringErrorKind::MissingFact
        );
        assert_eq!(
            *lowerer.function, before,
            "wrong control must fail before creating CFG or cleanup"
        );
    });
}
