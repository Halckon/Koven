use super::*;

#[test]
fn temporary_move_callback_releases_owned_captures_in_reverse_order_on_call_and_transfer() {
    for operand in ["1", "if (flag) { return } else { 1 }"] {
        let call_text =
            format!("apply(move {{ index -> first == second && marker == index }}, {operand})");
        let text = format!(
            "fun apply(callback: (Int) -> Boolean, count: Int): Unit {{}}\n\
             fun entry(own first: String, own second: String, marker: Int, flag: Boolean): Unit {{\n\
                 {call_text}\n\
             }}"
        );
        let mut sources = SourceMap::new();
        let (source, file) = parsed(&mut sources, "pending-captures.ko", &text);
        let inputs = [SourceUnitInput::new(
            "root",
            "pending-captures.ko",
            source,
            &file,
        )];
        let (name_environment, type_environment) = standard_environments();
        let names = validated_names(&sources, &inputs, &name_environment);
        let typed = validated_types(&sources, &inputs, &names, &type_environment);
        let checked =
            check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
                .expect("checked ownership");
        assert!(
            checked.diagnostics().is_empty(),
            "{:?}",
            checked.diagnostics()
        );
        let unit = source_unit(&names, source);
        let lambda = file
            .ast()
            .expressions()
            .iter()
            .find_map(|(id, node)| {
                matches!(
                    node.payload(),
                    lang_frontend::parser::Expression::Lambda { .. }
                )
                .then_some(UnitExpressionId::new(unit, id))
            })
            .expect("temporary lambda");
        let first = symbol_named(&checked, &names, unit, "first");
        let second = symbol_named(&checked, &names, unit, "second");
        let marker = symbol_named(&checked, &names, unit, "marker");
        let captures = checked.captures_of(lambda).collect::<Vec<_>>();
        assert_eq!(captures.len(), 3);
        assert_eq!(
            captures[2].source(),
            UnitClosureCaptureSource::Symbol(marker)
        );
        assert_eq!(captures[2].effect(), ClosureCaptureEffect::Copy);
        let mut points = vec![UnitDropPoint::CallReturn(UnitExpressionId::new(
            unit,
            expression_with_text(&sources, &file, &call_text),
        ))];
        if operand != "1" {
            points.push(UnitDropPoint::ControlTransfer(UnitExpressionId::new(
                unit,
                expression_with_text(&sources, &file, "return"),
            )));
        }
        for point in points {
            let targets = checked
                .drops()
                .iter()
                .filter(|fact| fact.point() == point)
                .filter_map(|fact| match fact.target() {
                    UnitDropTarget::Captured { closure, .. } if closure == lambda => {
                        Some(fact.target())
                    }
                    UnitDropTarget::Temporary(expression) if expression == lambda => {
                        Some(fact.target())
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(
                targets,
                [
                    UnitDropTarget::Captured {
                        closure: lambda,
                        source: UnitClosureCaptureSource::Symbol(second)
                    },
                    UnitDropTarget::Captured {
                        closure: lambda,
                        source: UnitClosureCaptureSource::Symbol(first)
                    },
                    UnitDropTarget::Temporary(lambda),
                ],
                "each abandoned or completed borrow releases owned slots before the environment; Copy Int has no drop"
            );
        }
        assert!(
            checked.drops().iter().all(|fact| !matches!(fact.target(),
                UnitDropTarget::Named(symbol) if symbol == first || symbol == second
            )),
            "captured owners are never dropped again at their old parameter bindings"
        );
        checked.validate().expect("valid ordered cleanup facts");
    }
}

#[test]
fn temporary_move_callback_does_not_unwind_owned_captures_on_abort() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "pending-captures-abort.ko",
        r#"
fun apply(callback: (Int) -> Boolean, count: Int): Unit {}
fun entry(own first: String, own second: String, marker: Int): Unit {
    apply(move { index -> first == second && marker == index }, error("stop"))
}
"#,
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "pending-captures-abort.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &type_environment);
    let checked =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
            .expect("checked ownership");
    assert!(
        checked.diagnostics().is_empty(),
        "{:?}",
        checked.diagnostics()
    );
    let unit = source_unit(&names, source);
    let lambda = file
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| {
            matches!(
                node.payload(),
                lang_frontend::parser::Expression::Lambda { .. }
            )
            .then_some(UnitExpressionId::new(unit, id))
        })
        .expect("temporary lambda evaluated before abort");
    let captures = checked.captures_of(lambda).collect::<Vec<_>>();
    assert_eq!(captures.len(), 3);
    let first = symbol_named(&checked, &names, unit, "first");
    let second = symbol_named(&checked, &names, unit, "second");
    assert!(checked.drops().iter().all(|fact| !matches!(fact.target(),
        UnitDropTarget::Captured { closure, .. } if closure == lambda
    ) && !matches!(fact.target(), UnitDropTarget::Temporary(expression) if expression == lambda)
      && !matches!(fact.target(), UnitDropTarget::Named(symbol) if symbol == first || symbol == second)
    ), "Nothing has no normal call return and must not synthesize unwind cleanup");
    checked.validate().expect("valid non-unwinding abort");
}
