use super::*;

#[test]
fn call_entry_transports_owned_opaque_function_without_guessing_a_lambda() {
    use lang_frontend::ownership_checking::{DropPoint, IterationCleanupAction};
    for own in ["own ", ""] {
        let (sources, parsed, owned) = checked(&format!(
            "fun run({own}cb: move () -> Unit) {{ val used = cb() }}"
        ));
        assert!(owned.diagnostics().is_empty());
        assert!(owned.deferred().is_empty());
        let call = parsed
            .ast()
            .expressions()
            .iter()
            .find_map(|(id, node)| (sources.slice(node.span()) == Ok("cb()")).then_some(id))
            .unwrap();
        let passes = owned
            .cleanup_steps()
            .iter()
            .filter_map(|(point, action)| match action {
                IterationCleanupAction::PassClosureEnvironment { callee, closure }
                    if *point == DropPoint::CallEntry(call) =>
                {
                    Some((*callee, *closure))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        if own.is_empty() {
            assert!(
                passes.is_empty(),
                "borrowed parameters have no callee-owned value slot"
            );
        } else {
            let [(callee, closure)] = passes.as_slice() else {
                panic!("one owned callee environment");
            };
            assert!(
                closure.is_none(),
                "an opaque function is not a known file lambda"
            );
            assert!(
                owned
                    .drops()
                    .iter()
                    .any(|fact| fact.owner() == Some(*callee)
                        && fact.point() == DropPoint::CallReturn(call))
            );
        }
    }
}

#[test]
fn lambda_unused_owned_parameters_drop_at_callable_entry() {
    use lang_frontend::ownership_checking::{DropPoint, DropTarget, IterationCleanupAction};
    for header in ["xs ->", ""] {
        let (_, parsed, owned) = checked(&format!(
            "fun run() {{ val f: (own List<Int>) -> Unit = {{ {header} for (_ in listOf(1)) {{}} }} }}"
        ));
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        let entries = owned
            .drops()
            .iter()
            .filter(|fact| matches!(fact.point(), DropPoint::LambdaEntry(_)))
            .collect::<Vec<_>>();
        assert_eq!(entries.len(), 1, "{header}");
        assert!(matches!(entries[0].target(), DropTarget::Named(_)));
        if let DropPoint::LambdaEntry(lambda) = entries[0].point() {
            let actions = owned
                .cleanup_steps()
                .iter()
                .filter(|(point, _)| *point == DropPoint::LambdaEntry(lambda))
                .map(|(_, action)| action)
                .collect::<Vec<_>>();
            assert!(matches!(
                actions.first(),
                Some(IterationCleanupAction::BindClosureEnvironment { closure, .. })
                    if *closure == lambda
            ));
            assert!(actions.iter().skip(1).any(|action| matches!(
                action,
                IterationCleanupAction::Drop(fact) if *fact == *entries[0]
            )));
            assert!(matches!(
                parsed.ast().expressions().get(lambda).unwrap().payload(),
                lang_frontend::parser::Expression::Lambda { .. }
            ));
        }
        assert_eq!(owned.iterations().len(), 1);
    }
}

#[test]
fn call_entry_passes_unique_environment_on_the_current_continuation_path() {
    use lang_frontend::ownership_checking::{DropPoint, IterationCleanupAction};

    for body in [
        "val f: move () -> Unit = move { read(xs) }\nif (flag) { return }\nval used = f()",
        "if (flag) { val f: move () -> Unit = move { read(xs) }\nval used = f() }",
    ] {
        let (sources, parsed, owned) = checked(&format!(
            "fun read(xs: List<Int>) {{}}\nfun run(flag: Boolean, own xs: List<Int>) {{ {body} }}"
        ));
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
        let expression = |text: &str| {
            parsed
                .ast()
                .expressions()
                .iter()
                .find_map(|(id, node)| (sources.slice(node.span()) == Ok(text)).then_some(id))
                .unwrap()
        };
        let call = expression("f()");
        let closure = expression("move { read(xs) }");
        let passes = owned
            .cleanup_steps()
            .iter()
            .filter_map(|(point, action)| match action {
                IterationCleanupAction::PassClosureEnvironment {
                    callee,
                    closure: passed,
                } if *point == DropPoint::CallEntry(call) => Some((*callee, *passed)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            passes.len(),
            1,
            "the reachable call must carry its unique environment: {body}"
        );
        assert_eq!(passes[0].1, Some(closure));
        assert!(
            owned
                .drops()
                .iter()
                .any(|drop| drop.point() == DropPoint::CallReturn(call)
                    && drop.owner() == Some(passes[0].0)),
            "the same owner stays live through the call"
        );
    }
}

#[test]
fn call_entry_does_not_publish_mutable_callee_environment_without_retention() {
    use lang_frontend::ownership_checking::{DropPoint, IterationCleanupAction};

    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>) {\nvar f: (Int) -> Unit = { n -> read(xs) }\nval used = f(1) }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    let call = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| (sources.slice(node.span()) == Ok("f(1)")).then_some(id))
        .unwrap();
    assert!(!owned.cleanup_steps().iter().any(|(point, action)| {
        *point == DropPoint::CallEntry(call)
            && matches!(
                action,
                IterationCleanupAction::PassClosureEnvironment { .. }
            )
    }));
}

#[test]
fn call_entry_does_not_reuse_callee_owner_consumed_by_argument() {
    use lang_frontend::ownership_checking::{DropPoint, IterationCleanupAction};

    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun consume(own g: move (Unit) -> Unit) {}\nfun run(own xs: List<Int>) {\nval f: move (Unit) -> Unit = move { arg -> read(xs) }\nval used = f(consume(f)) }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    let call = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| (sources.slice(node.span()) == Ok("f(consume(f))")).then_some(id))
        .unwrap();
    assert!(!owned.cleanup_steps().iter().any(|(point, action)| {
        *point == DropPoint::CallEntry(call)
            && matches!(
                action,
                IterationCleanupAction::PassClosureEnvironment { .. }
            )
    }));
}

#[test]
fn call_entry_does_not_treat_conditional_callee_consumption_as_unconditional() {
    use lang_frontend::ownership_checking::{DropPoint, IterationCleanupAction};

    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun consume(own g: move (Unit) -> Unit) {}\nfun noop(): Unit {}\nfun run(flag: Boolean, own xs: List<Int>) {\nval f: move (Unit) -> Unit = move { arg -> read(xs) }\nval used = f(if (flag) (consume(f)) else (noop())) }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    let call = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| {
            (sources.slice(node.span()) == Ok("f(if (flag) (consume(f)) else (noop()))"))
                .then_some(id)
        })
        .unwrap();
    assert!(!owned.cleanup_steps().iter().any(|(point, action)| {
        *point == DropPoint::CallEntry(call)
            && matches!(
                action,
                IterationCleanupAction::PassClosureEnvironment { .. }
            )
    }));
}

#[test]
fn control_argument_keeps_pending_callee_and_borrowed_argument_capture_loans() {
    for call in [
        "f(if (flag) 0 else 1, take(xs))",
        "invoke(f, if (flag) 0 else 1, take(xs))",
    ] {
        let (_, _, owned) = checked(&format!(
            "fun read(xs: List<Int>) {{}}\nfun take(own xs: List<Int>): Int {{ return 0 }}\nfun invoke(f: (Int, Int) -> Unit, a: Int, b: Int) {{}}\nfun run(own xs: List<Int>, flag: Boolean) {{ val f: (Int, Int) -> Unit = {{ a, b -> read(xs) }}\nval result = {call} }}"
        ));
        assert!(
            owned
                .diagnostics()
                .iter()
                .any(|d| d.code().to_string() == "L0135"),
            "{call}: {:?}",
            owned.diagnostics()
        );
        let completed = call.replace("take(xs)", "0");
        let (_, _, owned) = checked(&format!(
            "fun read(xs: List<Int>) {{}}\nfun take(own xs: List<Int>): Int {{ return 0 }}\nfun invoke(f: (Int, Int) -> Unit, a: Int, b: Int) {{}}\nfun run(own xs: List<Int>, flag: Boolean) {{ val f: (Int, Int) -> Unit = {{ a, b -> read(xs) }}\nval result = {completed}\nval consumed = take(xs) }}"
        ));
        assert!(
            owned.diagnostics().is_empty(),
            "{completed}: {:?}",
            owned.diagnostics()
        );
    }
}

#[test]
fn captured_owner_and_callee_survive_branch_cleanup_until_call_returns() {
    use lang_frontend::ownership_checking::{ClosureCaptureSource, DropPoint, DropTarget};
    for call in ["f(0)", "f(if (flag) 0 else 1)"] {
        let (sources, parsed, owned) = checked(&format!(
            "fun read(xs: List<Int>) {{}}\nfun run(own xs: List<Int>, flag: Boolean) {{ for (_ in listOf(0)) {{ val f: (Int) -> Unit = {{ n -> read(xs) }}\nif (flag) {{}} else {{}}\nval used = {call}\nbreak }} }}"
        ));
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        let source = owned
            .captures()
            .iter()
            .find_map(|capture| match capture.source() {
                ClosureCaptureSource::Symbol(symbol) => Some(symbol),
                ClosureCaptureSource::This => None,
            })
            .unwrap();
        let call_id = parsed
            .ast()
            .expressions()
            .iter()
            .find_map(|(id, node)| (sources.slice(node.span()).unwrap() == call).then_some(id))
            .unwrap();
        let drops = owned
            .drops()
            .iter()
            .filter(|fact| fact.target() == DropTarget::Named(source))
            .collect::<Vec<_>>();
        // The zero-iteration exhaustion path has its own owner cleanup.
        assert_eq!(drops.len(), 2, "{call}: {drops:?}");
        assert_eq!(
            drops
                .iter()
                .filter(|fact| fact.point() == DropPoint::CallReturn(call_id))
                .count(),
            1,
            "{call}: {drops:?}"
        );
        assert_eq!(
            drops
                .iter()
                .filter(|fact| matches!(fact.point(), DropPoint::LoopExit(_)))
                .count(),
            1,
            "{call}: {drops:?}"
        );
    }
}

#[test]
fn pending_callee_scope_cleanup_orders_local_capture_before_its_source() {
    use lang_frontend::ownership_checking::{ClosureCaptureSource, DropPoint, DropTarget};
    for transfer in ["return", "break", "continue"] {
        let (sources, parsed, owned) = checked(&format!(
            "fun read(xs: List<Int>) {{}}\nfun run(flag: Boolean) {{ for (_ in listOf(0)) {{ val xs = listOf(1)\nval f: (Int) -> Unit = {{ n -> read(xs) }}\nval used = f(if (flag) {transfer} else 0) }} }}"
        ));
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        let source = owned
            .captures()
            .iter()
            .find_map(|capture| match capture.source() {
                ClosureCaptureSource::Symbol(symbol) => Some(symbol),
                ClosureCaptureSource::This => None,
            })
            .unwrap();
        let exit = parsed
            .ast()
            .expressions()
            .iter()
            .find_map(|(id, node)| (sources.slice(node.span()).unwrap() == transfer).then_some(id))
            .unwrap();
        let drops = owned
            .drops()
            .iter()
            .filter(|fact| fact.point() == DropPoint::ControlTransfer(exit))
            .collect::<Vec<_>>();
        assert_eq!(
            drops
                .iter()
                .filter(|fact| fact.target() == DropTarget::Named(source))
                .count(),
            1,
            "{transfer}: {drops:?}"
        );
        let source_index = drops
            .iter()
            .position(|fact| fact.target() == DropTarget::Named(source))
            .unwrap();
        assert!(
            source_index > 0,
            "closure owner must drop before its captured local"
        );
    }
}

#[test]
fn pending_conditional_closure_keeps_sources_until_call_or_argument_transfer() {
    use lang_frontend::ownership_checking::{ClosureCaptureSource, DropPoint, DropTarget};
    for template in [
        "(if (flag) first else second)(if (early) return else 0)",
        "invoke(if (flag) ({ read(xs) }) else ({ read(ys) }), if (early) return else 0)",
    ] {
        for argument in ["if (early) return else 0", "if (early) 1 else 0"] {
            let call = template.replace("if (early) return else 0", argument);
            let declarations = if call.starts_with("(if") {
                "val first: (Int) -> Unit = { n -> read(xs) }\nval second: (Int) -> Unit = { n -> read(ys) }\n"
            } else {
                ""
            };
            let (sources, parsed, owned) = checked(&format!(
                "fun read(xs: List<Int>) {{}}\nfun invoke(f: () -> Unit, n: Int) {{ val used = f() }}\nfun run(own xs: List<Int>, own ys: List<Int>, flag: Boolean, early: Boolean) {{ {declarations}val used = {call} }}"
            ));
            assert!(
                owned.diagnostics().is_empty(),
                "{call}: {:?}",
                owned.diagnostics()
            );
            let expression = |text: &str| {
                parsed
                    .ast()
                    .expressions()
                    .iter()
                    .find_map(|(id, node)| {
                        (sources.slice(node.span()).unwrap() == text).then_some(id)
                    })
                    .unwrap()
            };
            for capture in owned.captures() {
                let ClosureCaptureSource::Symbol(symbol) = capture.source() else {
                    continue;
                };
                let mut points = vec![DropPoint::CallReturn(expression(&call))];
                if argument.contains("return") {
                    points.push(DropPoint::ControlTransfer(expression("return")));
                }
                for point in points {
                    let drops = owned
                        .drops()
                        .iter()
                        .filter(|fact| fact.point() == point)
                        .collect::<Vec<_>>();
                    let source = drops
                        .iter()
                        .position(|fact| fact.target() == DropTarget::Named(symbol))
                        .expect("selected source must survive to call completion or transfer");
                    assert!(
                        drops[source].condition().is_some(),
                        "only the selected capture remains at {point:?}"
                    );
                    assert!(
                        drops[..source]
                            .iter()
                            .any(|fact| matches!(fact.target(), DropTarget::Temporary(_))),
                        "temporary closure must drop before its captured source: {drops:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn pending_owned_closure_drops_slots_only_before_value_delivery() {
    use lang_frontend::ownership_checking::{DropPoint, DropTarget};
    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun invoke(own f: move () -> Unit, n: Int) { val used = f() }\nfun run(own xs: List<Int>, own ys: List<Int>, flag: Boolean, early: Boolean) { val used = invoke(if (flag) (move { read(xs) }) else (move { read(ys) }), if (early) return else 0) }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let transfer = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| (sources.slice(node.span()).unwrap() == "return").then_some(id))
        .unwrap();
    let slots = owned
        .drops()
        .iter()
        .filter(|fact| matches!(fact.target(), DropTarget::Captured { .. }))
        .collect::<Vec<_>>();
    assert_eq!(
        slots.len(),
        2,
        "callee owns slots after successful delivery: {slots:?}"
    );
    for slot in slots {
        assert_eq!(slot.point(), DropPoint::ControlTransfer(transfer));
        assert!(
            slot.condition().is_some(),
            "only the selected environment has this slot"
        );
        let at_exit = owned
            .drops()
            .iter()
            .filter(|fact| fact.point() == slot.point())
            .collect::<Vec<_>>();
        let slot_index = at_exit.iter().position(|fact| *fact == slot).unwrap();
        assert!(
            at_exit[slot_index + 1..]
                .iter()
                .any(|fact| matches!(fact.target(), DropTarget::Temporary(_))),
            "environment must outlive its slots"
        );
    }
}

#[test]
fn pending_temporary_capture_ends_between_environment_and_source_drop() {
    use lang_frontend::ownership_checking::{
        ClosureCaptureSource, DropTarget, IterationCleanupAction as Action, IterationExitKind,
    };
    let (_, _, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun invoke(f: () -> Unit, n: Int) { val used = f() }\nfun run(early: Boolean) { for (_ in listOf(0)) { val xs = listOf(1)\nval used = invoke(({ read(xs) }), if (early) return else 0) } }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let capture = owned
        .captures()
        .iter()
        .find(|capture| matches!(capture.source(), ClosureCaptureSource::Symbol(_)))
        .unwrap();
    let ClosureCaptureSource::Symbol(source) = capture.source() else {
        unreachable!()
    };
    let exit = owned
        .iterations()
        .iter()
        .flat_map(|plan| plan.exits())
        .find(|exit| matches!(exit.kind(), IterationExitKind::Return(_)))
        .unwrap();
    let actions = exit.actions();
    let end = actions.iter().position(|action| matches!(action, Action::EndCaptureLoan { source: candidate, .. } if *candidate == capture.source())).unwrap();
    assert!(
        matches!(actions[end-1], Action::Drop(fact) if matches!(fact.target(), DropTarget::Temporary(_))),
        "{actions:?}"
    );
    assert!(
        matches!(actions[end+1], Action::Drop(fact) if fact.target() == DropTarget::Named(source)),
        "{actions:?}"
    );
}

#[test]
fn pending_temporary_capture_loan_end_names_its_created_environment() {
    use lang_frontend::ownership_checking::{CleanupOwnerValue, IterationCleanupAction as Action};
    let (_, _, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>, own ys: List<Int>, flag: Boolean) { val used = (if (flag) ({ read(xs) }) else ({ read(ys) }))() }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let mut count = 0;
    for (_, action) in owned.cleanup_steps() {
        if let Action::EndCaptureLoan { owner, closure, .. } = action {
            assert!(
                matches!(owned.cleanup_conditions().owner_value(*owner), Some(CleanupOwnerValue::Closure { expression, .. }) if expression == closure)
            );
            assert!(owned.cleanup_steps().iter().any(|(_, action)| matches!(action, Action::CreateClosureOwner { owner: created, closure: expression } if created == owner && expression == closure)));
            count += 1;
        }
    }
    assert_eq!(count, 2);
}
