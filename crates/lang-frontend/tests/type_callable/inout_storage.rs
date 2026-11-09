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
    } else {
        assert!(single.call(call).is_none(), "{text}");
    }
    if let Some(read) = ordinary_read {
        assert!(single.non_null_use(read).is_some(), "{text}");
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
        } else {
            assert!(unit.call(call).is_none(), "{text}");
        }
        if let Some(read) = ordinary_read {
            assert!(
                unit.non_null_use(UnitExpressionId::new(source, read))
                    .is_some(),
                "{text}"
            );
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

#[test]
fn rejected_inout_storage_does_not_kill_the_next_read_fact() {
    for (declaration, signature) in [
        (
            "fun set(inout target: Int): Boolean { target = 4; return true }",
            "",
        ),
        ("", "set: (inout Int) -> Boolean"),
    ] {
        for body in [
            "if (x != null) { set(&x); val after: Int = /*read*/x }",
            "if (x != null && set(&x)) { val after: Int = /*read*/x }",
            "if (x == null || set(&x)) {} else { val after: Int = /*read*/x }",
        ] {
            let text =
                format!("{declaration}\nfun run({signature}): Unit {{ var x: Int? = 7; {body} }}");
            assert_inout_storage(&text, None);
        }
    }
}

fn assert_inout_diagnostic_spans(text: &str, expected: &[(&str, &str)]) {
    let (sources, parsed) = parsed(text);
    let (environment, types) = standard_environments();
    let assert_diagnostics = |diagnostics: &[Diagnostic]| {
        let actual = diagnostics
            .iter()
            .map(|diagnostic| {
                (
                    diagnostic.code().to_string(),
                    sources.slice(diagnostic.primary_span()).unwrap(),
                )
            })
            .collect::<Vec<_>>();
        let expected = expected
            .iter()
            .map(|(code, span)| (code.to_string(), *span))
            .collect::<Vec<_>>();
        assert_eq!(actual, expected, "{text}");
    };
    let names = resolve_names(&sources, &parsed, &environment).unwrap();
    let single = check_types(&sources, &parsed, &names, &types).unwrap();
    assert_diagnostics(single.diagnostics());
    for (_, unit) in checked_named_entries(&sources, &parsed, &environment, &types) {
        assert_diagnostics(unit.diagnostics());
    }
}

#[test]
fn selected_inout_writes_survive_rejected_enclosing_expressions() {
    for (text, expected) in [
        (
            "fun set(inout target: Int?): Int { target = null; return 0 }\nfun run(): Unit { var x: Int? = 7; if (x != null) { val wrong: Boolean = set(&x); val after: Int = x } }",
            [("L0084", "set(&x)"), ("L0084", "x")],
        ),
        (
            "fun write(inout target: Int?): Int { target = null; return 0 }\nfun reject(inout target: Int, input: Int): Boolean { return true }\nfun run(): Unit { var x: Int? = 7; var y: Int? = 7; if (x != null && y != null && reject(&x, write(&y))) { val retained: Int = x; val invalidated: Int = y } }",
            [("L0084", "x"), ("L0084", "y")],
        ),
    ] {
        assert_inout_diagnostic_spans(text, &expected);
    }
}

#[test]
fn pending_arguments_keep_proven_rejection_separate_from_unresolved_overloads() {
    assert_inout_storage(
        "fun set(inout target: Int, value: Int): Boolean { return true }\nfun run(produce: () -> Int): Unit { var x: Int? = 7; if (x != null) { set(&x, produce<Int>()); val after: Int = /*read*/x } }",
        None,
    );
    assert_inout_diagnostic_spans(
        "fun choose(inout x: Int?, value: Int): Boolean { return true }\nfun choose(inout x: Int?, value: Boolean): Boolean { return true }\nfun run(produce: () -> Int): Unit { var x: Int? = 7; if (x != null) { choose(&x, produce<Int>()); val after: Int = x } }",
        &[("L0084", "x")],
    );
}

#[test]
fn pending_ownership_intrinsics_do_not_write_after_a_proven_rejection() {
    assert_inout_diagnostic_spans(
        "fun run(produce: () -> Int): Unit { var x: Int? = 7; if (x != null) { replace<Int>(&x, produce<Int>()); val after: Int = x } }",
        &[("L0084", "x")],
    );
    assert_inout_diagnostic_spans(
        "fun run(produce: () -> Int): Unit { var x: Int? = 7; if (x != null) { swap<Int>(&x, &produce<Int>()); val after: Int = x } }",
        &[("L0084", "x"), ("L0122", "&produce<Int>()")],
    );
}

#[test]
fn rejected_recovery_values_do_not_enable_outer_inout_writes() {
    let declarations = "fun build(inout target: Int, input: Int, cb: (inout Int?) -> Boolean): (inout Int?) -> Boolean = cb\nfun outer(inout target: Int?, cb: (inout Int?) -> Boolean): Boolean { target = null; return true }";
    for call in [
        "build(&y, produce<Int>(), cb)(&x)",
        "(build(&y, produce<Int>(), cb))(&x)",
        "outer(&x, build(&y, produce<Int>(), cb))",
    ] {
        let text = format!(
            "{declarations}\nfun run(produce: () -> Int, cb: (inout Int?) -> Boolean): Unit {{ var y: Int? = 7; var x: Int? = 7; if (x != null) {{ {call}; val after: Int = x }} }}"
        );
        assert_inout_diagnostic_spans(&text, &[("L0084", "y")]);
    }
}

#[test]
fn pending_recovery_values_keep_outer_inout_writes_conservative() {
    let declarations = "fun choose(value: Int, cb: (inout Int?) -> Boolean): (inout Int?) -> Boolean = cb\nfun choose(value: Boolean, cb: (inout Int?) -> Boolean): (inout Int?) -> Boolean = cb\nfun outer(inout target: Int?, cb: (inout Int?) -> Boolean): Boolean { target = null; return true }";
    for call in [
        "choose(produce<Int>(), cb)(&x)",
        "(choose(produce<Int>(), cb))(&x)",
        "outer(&x, choose(produce<Int>(), cb))",
    ] {
        let text = format!(
            "{declarations}\nfun run(produce: () -> Int, cb: (inout Int?) -> Boolean): Unit {{ var x: Int? = 7; if (x != null) {{ {call}; val after: Int = x }} }}"
        );
        assert_inout_diagnostic_spans(&text, &[("L0084", "x")]);
    }
}

#[test]
fn rejected_recovery_values_through_projections_do_not_enable_outer_writes() {
    let declarations = "class Device { fun touch(inout target: Int?): Boolean { target = null; return true } }\nfun build(inout target: Int, input: Int, device: Device): Device = device\nfun reject(inout target: Int, input: Int): Int = input";
    for (parameter, call) in [
        (
            "device: Device",
            "build(&y, produce<Int>(), device).touch(&x)",
        ),
        (
            "values: List<(inout Int?) -> Boolean>",
            "values[reject(&y, produce<Int>())](&x)",
        ),
    ] {
        let text = format!(
            "{declarations}\nfun run(produce: () -> Int, {parameter}): Unit {{ var y: Int? = 7; var x: Int? = 7; if (x != null) {{ {call}; val after: Int = x }} }}"
        );
        assert_inout_diagnostic_spans(&text, &[("L0084", "y")]);
    }
}

#[test]
fn replace_distinguishes_rejected_and_pending_recovery_operands() {
    let declarations = "fun reject(inout target: Int, input: Int): Int = input\nfun choose(input: Int): Int = input\nfun choose(input: Boolean): Int = 7";
    for (value, expected) in [
        ("reject(&y, produce<Int>())", "y"),
        ("choose(produce<Int>())", "x"),
    ] {
        let text = format!(
            "{declarations}\nfun run(produce: () -> Int): Unit {{ var y: Int? = 7; var x: Int? = 7; if (x != null) {{ replace(&x, {value}); val after: Int = x }} }}"
        );
        assert_inout_diagnostic_spans(&text, &[("L0084", expected)]);
    }
}

#[test]
fn projection_recovery_preserves_local_index_errors_and_pending_inputs() {
    for (text, expected) in [
        (
            "fun run(values: List<(inout Int?) -> Boolean>): Unit { var x: Int? = 7; if (x != null) { values[false](&x); val after: Int = x } }",
            ("L0128", "false"),
        ),
        (
            "fun outer(inout target: Int?, value: Int): Boolean = true\nfun run(produce: () -> Int, table: Map<Int, Int?>): Unit { var x: Int? = 7; if (x != null) { outer(&x, table[produce<Int>()]); val after: Int = x } }",
            ("L0130", "table[produce<Int>()]"),
        ),
        (
            "fun choose(input: Int): Int = input\nfun choose(input: Boolean): Int = 7\nfun run(produce: () -> Int, values: List<(inout Int?) -> Boolean>): Unit { var x: Int? = 7; if (x != null) { values[choose(produce<Int>())](&x); val after: Int = x } }",
            ("L0084", "x"),
        ),
    ] {
        assert_inout_diagnostic_spans(text, &[expected]);
    }
}

#[test]
fn provisional_map_types_preserve_local_rejections_and_executed_key_writes() {
    let choose = "fun choose(input: Int): Int = input\nfun choose(input: Boolean): Int = 7";
    for (value, code) in [("Int?", "L0130"), ("String", "L0136")] {
        let text = format!(
            "{choose}\nfun outer(inout target: Int?, value: Int?): Boolean {{ target = null; return true }}\nfun run(produce: () -> Int, table: Map<Int, {value}>): Unit {{ var x: Int? = 7; if (x != null) {{ outer(&x, table[choose(produce<Int>())]); val after: Int = x }} }}"
        );
        assert_inout_diagnostic_spans(&text, &[(code, "table[choose(produce<Int>())]")]);
    }
    for query in ["table[produce<Int>()]", "table.get(produce<Int>())"] {
        let text = format!(
            "fun outer(inout target: Int?, value: Boolean): Boolean {{ target = null; return true }}\nfun run(produce: () -> Int, table: Map<Int, Int>): Unit {{ var x: Int? = 7; if (x != null) {{ outer(&x, ({query})); val after: Int = x }} }}"
        );
        assert_inout_diagnostic_spans(&text, &[("L0084", query)]);
    }
    assert_inout_diagnostic_spans(
        "fun write(inout target: Int?): Boolean { target = null; return true }\nfun outer(inout target: Int?, value: Int?): Boolean { target = null; return true }\nfun run(table: Map<Int, Int>): Unit { var x: Int? = 7; var y: Int? = 7; if (x != null && y != null) { outer(&x, table[write(&y)]); val after: Int = x; val written: Int = y } }",
        &[("L0084", "write(&y)"), ("L0084", "y")],
    );
}
