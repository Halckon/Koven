use super::{parsed, unit_entries::checked_named_entries};
use lang_frontend::{
    diagnostic::Diagnostic,
    name_resolution::resolve_names,
    parser::{Expression, ParameterModeMarker},
    type_checking::{
        BuiltinType, TypeKind, UnitExpressionId, UnitTypeKind, check_types, standard_environments,
    },
};

fn assert_inout_storage(text: &str, nullable: Option<bool>) {
    let (sources, parsed) = parsed(text);
    let (environment, types) = standard_environments();
    let (call, operand) = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(call, node)| {
            let Expression::Call { arguments, .. } = node.payload() else {
                return None;
            };
            arguments
                .iter()
                .find(|arg| matches!(arg.mode_marker, Some(ParameterModeMarker::Inout(_))))
                .map(|arg| (call, arg.value))
        })
        .expect("explicit inout operand");
    let operand_span = parsed.ast().expressions().get(operand).unwrap().span();
    let ordinary_read = text.find("/*read*/").map(|offset| {
        parsed
            .ast()
            .expressions()
            .iter()
            .find(|(_, node)| {
                node.span().start() == offset + "/*read*/".len()
                    && matches!(node.payload(), Expression::Name)
            })
            .expect("ordinary narrowed read")
            .0
    });
    let assert_diagnostics = |diagnostics: &[Diagnostic]| {
        if nullable.is_some() {
            assert!(diagnostics.is_empty(), "{text}: {diagnostics:?}");
        } else {
            let [diagnostic] = diagnostics else {
                panic!("{text}: expected one storage mismatch, got {diagnostics:?}");
            };
            assert_eq!(diagnostic.code().to_string(), "L0084", "{text}");
            assert_eq!(diagnostic.primary_span(), operand_span, "{text}");
        }
    };

    let names = resolve_names(&sources, &parsed, &environment).unwrap();
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let single = check_types(&sources, &parsed, &names, &types).unwrap();
    assert_diagnostics(single.diagnostics());
    if let Some(nullable) = nullable {
        let operand_ty = single.expression_type(operand).unwrap();
        let actual = single.types().get(operand_ty);
        assert!(
            if nullable {
                matches!(actual, Some(TypeKind::Nullable(inner)) if single.types().get(*inner) == Some(&TypeKind::Builtin(BuiltinType::Int)))
            } else {
                actual == Some(&TypeKind::Builtin(BuiltinType::Int))
            },
            "{text}: {actual:?}"
        );
        assert!(single.non_null_use(operand).is_none(), "{text}");
        assert_eq!(
            single.call(call).unwrap().arguments()[0].parameter_type(),
            operand_ty,
            "{text}"
        );
        if let Some(read) = ordinary_read {
            assert!(single.non_null_use(read).is_some(), "{text}");
        }
    } else {
        assert!(single.call(call).is_none(), "{text}");
    }

    for (names, unit) in checked_named_entries(&sources, &parsed, &environment, &types) {
        let source = names.index().source_units()[0].id();
        let operand = UnitExpressionId::new(source, operand);
        let call = UnitExpressionId::new(source, call);
        assert_diagnostics(unit.diagnostics());
        if let Some(nullable) = nullable {
            let operand_ty = unit.expression_type(operand).unwrap();
            let actual = unit.types().get(operand_ty);
            assert!(
                if nullable {
                    matches!(actual, Some(UnitTypeKind::Nullable(inner)) if unit.types().get(*inner) == Some(&UnitTypeKind::Builtin(BuiltinType::Int)))
                } else {
                    actual == Some(&UnitTypeKind::Builtin(BuiltinType::Int))
                },
                "{text}: {actual:?}"
            );
            assert!(unit.non_null_use(operand).is_none(), "{text}");
            assert_eq!(
                unit.call(call).unwrap().arguments()[0].parameter_type(),
                operand_ty,
                "{text}"
            );
            if let Some(read) = ordinary_read {
                assert!(
                    unit.non_null_use(UnitExpressionId::new(source, read))
                        .is_some(),
                    "{text}"
                );
            }
        } else {
            assert!(unit.call(call).is_none(), "{text}");
        }
    }
}

#[test]
fn inout_storage_rejects_nullable_shape_changes_in_both_directions() {
    for text in [
        "fun set(inout target: Int): Unit { target = 4 }\nfun main(): Unit { var x: Int? = 7; if (x != null) { set(&x) } }",
        "fun set(inout target: Int?): Unit { target = null }\nfun main(): Unit { var x: Int = 7; set(&x) }",
        "fun invoke(set: (inout Int) -> Unit): Unit { var x: Int? = 7; if (x != null) { set(&x) } }",
        "fun invoke(set: (inout Int?) -> Unit): Unit { var x: Int = 7; set(&x) }",
    ] {
        assert_inout_storage(text, None);
    }
}

#[test]
fn inout_storage_does_not_replace_ordinary_nonnull_read_facts() {
    for text in [
        "fun set(inout target: Int?): Unit { target = null }\nfun main(): Unit { var x: Int? = 7; if (x != null) { val before: Int = /*read*/x; set(target = &(x)) } }",
        "fun invoke(set: (inout Int?) -> Unit): Unit { var x: Int? = 7; if (x != null) { val before: Int = /*read*/x; set(&x) } }",
        "fun <T> touch(inout target: T): Unit {}\nfun main(): Unit { var x: Int? = 7; if (x != null) { val before: Int = /*read*/x; touch(&x) } }",
        "fun set(inout target: Int): Unit { target = 4 }\nfun set(inout target: Int?): Unit { target = null }\nfun main(): Unit { var x: Int? = 7; if (x != null) { val before: Int = /*read*/x; set(&x) } }",
    ] {
        assert_inout_storage(text, Some(true));
    }
}

#[test]
fn inout_storage_overload_selects_the_declared_nonnullable_type() {
    for text in [
        "fun set(inout target: Int): Unit { target = 4 }\nfun set(inout target: Int?): Unit { target = null }\nfun main(): Unit { var x: Int = 7; set(&x) }",
        "fun set(inout target: Int, action: () -> Unit): Unit { target = 4 }\nfun set(inout target: Int?, action: () -> Unit): Unit { target = null }\nfun main(): Unit { var x: Int = 7; set(&x, { -> }) }",
    ] {
        assert_inout_storage(text, Some(false));
    }
}
