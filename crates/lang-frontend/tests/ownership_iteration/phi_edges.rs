use super::*;

#[test]
fn loop_phi_entry_writes_actual_owner_and_initializes_absent_origins() {
    use lang_frontend::ownership_checking::{
        CleanupCaptureValue, CleanupCondition, CleanupOwnerValue, IterationPhiBoundary,
        IterationPhiIncomingKind,
    };
    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>, own ys: List<Int>, flags: List<Boolean>) {
            var f: () -> Unit = { read(xs) }
            for (_ in flags) { f = ({ read(ys) }) }
            val used = f()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let names = resolve_names(&sources, &parsed, &standard_environments().0).unwrap();
    let f = names
        .symbols()
        .iter()
        .find(|symbol| sources.slice(symbol.span()).unwrap() == "f")
        .unwrap()
        .id();
    let plan = &owned.iterations()[0];
    let phi = plan
        .closure_phis()
        .iter()
        .find(|phi| phi.boundary() == IterationPhiBoundary::Header && phi.symbol() == f)
        .unwrap();
    let entry = plan
        .closure_phi_incomings()
        .iter()
        .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Entry)
        .unwrap();
    assert_eq!(entry.boundary(), IterationPhiBoundary::Header);
    let binding = entry
        .bindings()
        .iter()
        .find(|binding| binding.target() == phi.owner())
        .unwrap();
    assert_eq!(binding.availability_selector(), phi.availability_selector());
    assert_eq!(
        owned.cleanup_conditions().get(binding.available_when()),
        Some(&CleanupCondition::Always)
    );
    let value = binding.values().first().unwrap();
    assert_ne!(value.source(), phi.owner());
    assert!(matches!(
        owned.cleanup_conditions().owner_value(value.source()),
        Some(CleanupOwnerValue::Closure { .. })
    ));
    let mut present = 0;
    let mut absent = 0;
    for origin in phi.origins() {
        let input = binding
            .origins()
            .iter()
            .find(|input| input.target() == origin.selector())
            .unwrap();
        let text = sources
            .slice(
                parsed
                    .ast()
                    .expressions()
                    .get(origin.closure())
                    .unwrap()
                    .span(),
            )
            .unwrap();
        if text.contains("read(xs)") {
            assert_eq!(
                owned.cleanup_conditions().get(input.condition()),
                Some(&CleanupCondition::Always)
            );
            assert_eq!(input.environments().len(), 1);
            let source = &input.environments()[0].sources()[0];
            assert_eq!(source.target(), Some(origin.sources()[0].owner()));
            assert!(matches!(source.value(), CleanupCaptureValue::Owner(_)));
            present += 1;
        } else {
            assert_eq!(
                owned.cleanup_conditions().get(input.condition()),
                Some(&CleanupCondition::Never)
            );
            assert!(input.environments().is_empty());
            absent += 1;
        }
    }
    assert_eq!((present, absent), (1, 1));

    let exhaustion = plan
        .closure_phi_incomings()
        .iter()
        .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Exhaustion)
        .unwrap();
    let exit = plan
        .closure_phis()
        .iter()
        .find(|candidate| {
            candidate.boundary() == IterationPhiBoundary::Exit && candidate.symbol() == f
        })
        .unwrap();
    let exit_input = exhaustion
        .bindings()
        .iter()
        .find(|binding| binding.target() == exit.owner())
        .unwrap();
    assert_eq!(exit_input.values()[0].source(), phi.owner());
    assert_eq!(exit_input.available_when(), phi.availability_condition());
    for origin in exit.origins() {
        let prior = phi
            .origins()
            .iter()
            .find(|prior| prior.closure() == origin.closure())
            .unwrap();
        let input = exit_input
            .origins()
            .iter()
            .find(|input| input.target() == origin.selector())
            .unwrap();
        assert_eq!(input.condition(), prior.condition());
        assert_eq!(input.environments()[0].owner(), phi.owner());
        let source = &input.environments()[0].sources()[0];
        assert_eq!(source.target(), Some(origin.sources()[0].owner()));
        assert_eq!(
            source.value(),
            CleanupCaptureValue::Owner(prior.sources()[0].owner())
        );
    }
}

#[test]
fn loop_phi_completed_body_edges_use_the_replacement_environment() {
    use lang_frontend::ownership_checking::{
        CleanupCaptureValue, CleanupCondition, IterationPhiBoundary, IterationPhiIncomingKind,
    };
    for (transfer, boundary) in [
        ("", IterationPhiBoundary::Header),
        ("continue", IterationPhiBoundary::Header),
        ("break", IterationPhiBoundary::Exit),
    ] {
        let (sources, parsed, owned) = checked(&format!(
            "fun read(xs: List<Int>) {{}}\nfun run(own xs: List<Int>, own ys: List<Int>, flags: List<Boolean>) {{
                var f: () -> Unit = {{ read(xs) }}
                for (_ in flags) {{ f = ({{ read(ys) }})\n{transfer} }}
                val used = f()
            }}"
        ));
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        let names = resolve_names(&sources, &parsed, &standard_environments().0).unwrap();
        let f = names
            .symbols()
            .iter()
            .find(|symbol| sources.slice(symbol.span()).unwrap() == "f")
            .unwrap()
            .id();
        let plan = &owned.iterations()[0];
        let incoming = plan
            .closure_phi_incomings()
            .iter()
            .find(|incoming| {
                incoming.boundary() == boundary
                    && matches!(
                        (transfer, incoming.kind()),
                        ("", IterationPhiIncomingKind::Fallthrough)
                            | ("continue", IterationPhiIncomingKind::Continue(_))
                            | ("break", IterationPhiIncomingKind::Break(_))
                    )
            })
            .unwrap();
        let phi = plan
            .closure_phis()
            .iter()
            .find(|phi| phi.boundary() == boundary && phi.symbol() == f)
            .unwrap();
        let binding = incoming
            .bindings()
            .iter()
            .find(|binding| binding.target() == phi.owner())
            .unwrap();
        assert_eq!(
            owned.cleanup_conditions().get(binding.available_when()),
            Some(&CleanupCondition::Always)
        );
        let mut selected = 0;
        for origin in phi.origins() {
            let input = binding
                .origins()
                .iter()
                .find(|input| input.target() == origin.selector())
                .unwrap();
            let text = sources
                .slice(
                    parsed
                        .ast()
                        .expressions()
                        .get(origin.closure())
                        .unwrap()
                        .span(),
                )
                .unwrap();
            if text.contains("read(ys)") {
                assert_eq!(
                    owned.cleanup_conditions().get(input.condition()),
                    Some(&CleanupCondition::Always)
                );
                assert_eq!(input.environments().len(), 1);
                let source = &input.environments()[0].sources()[0];
                assert_eq!(source.target(), Some(origin.sources()[0].owner()));
                assert!(matches!(source.value(), CleanupCaptureValue::Owner(_)));
                selected += 1;
            } else {
                assert_eq!(
                    owned.cleanup_conditions().get(input.condition()),
                    Some(&CleanupCondition::Never)
                );
            }
        }
        assert_eq!(selected, 1, "{transfer}");
    }
}

#[test]
fn loop_phi_entry_keeps_conditional_capture_sources_separate() {
    use lang_frontend::ownership_checking::{
        CleanupCaptureValue, CleanupCondition, IterationPhiBoundary, IterationPhiIncomingKind,
    };
    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>, own ys: List<Int>, flag: Boolean, flags: List<Boolean>) {
            var f: () -> Unit = if (flag) ({ read(xs) }) else ({ read(ys) })
            for (_ in flags) { val invoked = f() }
            val used = f()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let names = resolve_names(&sources, &parsed, &standard_environments().0).unwrap();
    let f = names
        .symbols()
        .iter()
        .find(|symbol| sources.slice(symbol.span()).unwrap() == "f")
        .unwrap()
        .id();
    let plan = &owned.iterations()[0];
    let phi = plan
        .closure_phis()
        .iter()
        .find(|phi| phi.boundary() == IterationPhiBoundary::Header && phi.symbol() == f)
        .unwrap();
    let entry = plan
        .closure_phi_incomings()
        .iter()
        .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Entry)
        .unwrap();
    let binding = entry
        .bindings()
        .iter()
        .find(|binding| binding.target() == phi.owner())
        .unwrap();
    assert_eq!(binding.origins().len(), 2);
    let mut sources_by_branch = Vec::new();
    for input in binding.origins() {
        let CleanupCondition::Choice { selector, branches } =
            owned.cleanup_conditions().get(input.condition()).unwrap()
        else {
            panic!("conditional origin must retain a choice")
        };
        assert_eq!(input.environments().len(), 1);
        let source = &input.environments()[0].sources()[0];
        let CleanupCaptureValue::Owner(owner) = source.value() else {
            panic!("MoveOnly captured source must keep its owner")
        };
        sources_by_branch.push((*selector, branches.clone(), owner));
    }
    assert_eq!(sources_by_branch[0].0, sources_by_branch[1].0);
    assert_ne!(sources_by_branch[0].1, sources_by_branch[1].1);
    assert_ne!(sources_by_branch[0].2, sources_by_branch[1].2);
}

#[test]
fn loop_phi_exhaustion_does_not_transport_an_owner_dropped_at_loop_exit() {
    use lang_frontend::ownership_checking::{
        CleanupCondition, DropPoint, DropTarget, IterationPhiIncomingKind,
    };
    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>, flags: List<Boolean>) {
            for (_ in flags) { val used = read(xs) }
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let names = resolve_names(&sources, &parsed, &standard_environments().0).unwrap();
    let xs = names
        .symbols()
        .iter()
        .rev()
        .find(|symbol| sources.slice(symbol.span()).unwrap() == "xs")
        .unwrap()
        .id();
    let plan = &owned.iterations()[0];
    let loop_id = plan.descriptor().statement();
    assert!(owned.drops().iter().any(|fact| {
        fact.point() == DropPoint::LoopExit(loop_id) && fact.target() == DropTarget::Named(xs)
    }));
    let exit = plan
        .closure_phis()
        .iter()
        .find(|phi| {
            phi.symbol() == xs
                && phi.boundary() == lang_frontend::ownership_checking::IterationPhiBoundary::Exit
        })
        .unwrap();
    let exhaustion = plan
        .closure_phi_incomings()
        .iter()
        .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Exhaustion)
        .unwrap();
    let input = exhaustion
        .bindings()
        .iter()
        .find(|input| input.target() == exit.owner())
        .unwrap();
    assert_eq!(
        owned.cleanup_conditions().get(input.available_when()),
        Some(&CleanupCondition::Never)
    );
    assert!(input.values().is_empty());
}

#[test]
fn loop_phi_exit_transports_a_held_source_only_through_its_closure() {
    use lang_frontend::ownership_checking::{
        CleanupCaptureValue, CleanupCondition, IterationPhiBoundary, IterationPhiIncomingKind,
    };
    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>, flags: List<Boolean>) {
            var f: () -> Unit = { read(xs) }
            for (_ in flags) { break }
            val used = f()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let names = resolve_names(&sources, &parsed, &standard_environments().0).unwrap();
    let xs = owned
        .captures()
        .iter()
        .find_map(|capture| {
            (sources.slice(capture.reference_span()).unwrap() == "xs").then_some(capture.source())
        })
        .unwrap();
    let lang_frontend::ownership_checking::ClosureCaptureSource::Symbol(xs) = xs else {
        panic!("xs is a named owner")
    };
    let f = names
        .symbols()
        .iter()
        .find(|symbol| sources.slice(symbol.span()).unwrap() == "f")
        .unwrap()
        .id();
    let plan = &owned.iterations()[0];
    let xs_exit = plan
        .closure_phis()
        .iter()
        .find(|phi| phi.boundary() == IterationPhiBoundary::Exit && phi.symbol() == xs)
        .unwrap();
    let f_exit = plan
        .closure_phis()
        .iter()
        .find(|phi| phi.boundary() == IterationPhiBoundary::Exit && phi.symbol() == f)
        .unwrap();
    for incoming in plan.closure_phi_incomings().iter().filter(|incoming| {
        matches!(
            incoming.kind(),
            IterationPhiIncomingKind::Break(_) | IterationPhiIncomingKind::Exhaustion
        )
    }) {
        let source_binding = incoming
            .bindings()
            .iter()
            .find(|binding| binding.target() == xs_exit.owner())
            .unwrap();
        assert_eq!(
            owned
                .cleanup_conditions()
                .get(source_binding.available_when()),
            Some(&CleanupCondition::Never)
        );
        assert!(source_binding.values().is_empty());
        let closure_binding = incoming
            .bindings()
            .iter()
            .find(|binding| binding.target() == f_exit.owner())
            .unwrap();
        assert!(closure_binding.origins().iter().any(|origin| {
            origin.environments().iter().any(|environment| {
                environment
                    .sources()
                    .iter()
                    .any(|source| matches!(source.value(), CleanupCaptureValue::Owner(_)))
            })
        }));
    }
}

#[test]
fn loop_phi_exhaustion_preserves_an_outer_pending_call_loan() {
    use lang_frontend::ownership_checking::{IterationPhiBoundary, IterationPhiIncomingKind};
    let (sources, parsed, owned) = checked(
        "fun use(xs: List<Int>, n: Int) {}\nfun run(own xs: List<Int>, flags: List<Boolean>) {
            use(xs, if (true) { for (_ in flags) {}\n0 } else 0)
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let names = resolve_names(&sources, &parsed, &standard_environments().0).unwrap();
    let xs = names
        .symbols()
        .iter()
        .rev()
        .find(|symbol| sources.slice(symbol.span()).unwrap() == "xs")
        .unwrap()
        .id();
    let plan = &owned.iterations()[0];
    let exit = plan
        .closure_phis()
        .iter()
        .find(|phi| phi.boundary() == IterationPhiBoundary::Exit && phi.symbol() == xs)
        .unwrap();
    let header = plan
        .closure_phis()
        .iter()
        .find(|phi| phi.boundary() == IterationPhiBoundary::Header && phi.symbol() == xs)
        .unwrap();
    let exhaustion = plan
        .closure_phi_incomings()
        .iter()
        .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Exhaustion)
        .unwrap();
    let input = exhaustion
        .bindings()
        .iter()
        .find(|binding| binding.target() == exit.owner())
        .unwrap();
    assert_eq!(input.available_when(), header.availability_condition());
    assert_eq!(input.values().len(), 1);
}

#[test]
fn loop_phi_exhaustion_preserves_the_old_owner_during_replacement_rhs() {
    use lang_frontend::ownership_checking::{IterationPhiBoundary, IterationPhiIncomingKind};
    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(flags: List<Boolean>) {
            var target = listOf(1)
            { target = if (true) { val observed = read(target)\nfor (_ in flags) {}\nlistOf(2) } else listOf(3) }
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let names = resolve_names(&sources, &parsed, &standard_environments().0).unwrap();
    let target = names
        .symbols()
        .iter()
        .find(|symbol| sources.slice(symbol.span()).unwrap() == "target")
        .unwrap()
        .id();
    let plan = &owned.iterations()[0];
    let header = plan
        .closure_phis()
        .iter()
        .find(|phi| phi.boundary() == IterationPhiBoundary::Header && phi.symbol() == target)
        .unwrap();
    let exit = plan
        .closure_phis()
        .iter()
        .find(|phi| phi.boundary() == IterationPhiBoundary::Exit && phi.symbol() == target)
        .unwrap();
    let exhaustion = plan
        .closure_phi_incomings()
        .iter()
        .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Exhaustion)
        .unwrap();
    let input = exhaustion
        .bindings()
        .iter()
        .find(|binding| binding.target() == exit.owner())
        .unwrap();
    assert_eq!(input.available_when(), header.availability_condition());
    assert_eq!(input.values()[0].source(), header.owner());
}

#[test]
fn loop_phi_backedge_reads_the_previous_header_environment() {
    use lang_frontend::ownership_checking::{
        CleanupCondition, CleanupOwnerValue, IterationPhiBoundary, IterationPhiIncomingKind,
    };
    let (sources, parsed, owned) = checked(
        "fun run(flags: List<Boolean>) {
            var f: () -> Unit = {}
            var g: () -> Unit = {}
            for (_ in flags) {
                val prior = f
                { g = prior }
                { f = ({ val marker = 1 }) }
            }
            val first = g()
            val second = f()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let names = resolve_names(&sources, &parsed, &standard_environments().0).unwrap();
    let g = names
        .symbols()
        .iter()
        .find(|symbol| sources.slice(symbol.span()).unwrap() == "g")
        .unwrap()
        .id();
    let f = names
        .symbols()
        .iter()
        .find(|symbol| sources.slice(symbol.span()).unwrap() == "f")
        .unwrap()
        .id();
    let plan = &owned.iterations()[0];
    let f_header = plan
        .closure_phis()
        .iter()
        .find(|phi| phi.boundary() == IterationPhiBoundary::Header && phi.symbol() == f)
        .unwrap();
    let phi = plan
        .closure_phis()
        .iter()
        .find(|phi| phi.boundary() == IterationPhiBoundary::Header && phi.symbol() == g)
        .unwrap();
    let carried = phi
        .origins()
        .iter()
        .find(|origin| {
            sources
                .slice(
                    parsed
                        .ast()
                        .expressions()
                        .get(origin.closure())
                        .unwrap()
                        .span(),
                )
                .unwrap()
                .contains("marker")
        })
        .unwrap();
    let fallthrough = plan
        .closure_phi_incomings()
        .iter()
        .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Fallthrough)
        .unwrap();
    let binding = fallthrough
        .bindings()
        .iter()
        .find(|binding| binding.target() == phi.owner())
        .unwrap();
    let input = binding
        .origins()
        .iter()
        .find(|input| input.target() == carried.selector())
        .unwrap();
    assert_ne!(
        owned.cleanup_conditions().get(input.condition()),
        Some(&CleanupCondition::Never),
        "after the first round, g receives f's prior header environment"
    );
    let owner = input.environments()[0].owner();
    let mut pending = vec![owner];
    let mut seen = std::collections::BTreeSet::new();
    while let Some(current) = pending.pop() {
        if !seen.insert(current) {
            continue;
        }
        if let Some(CleanupOwnerValue::Snapshot(snapshot)) =
            owned.cleanup_conditions().owner_value(current)
        {
            pending.extend(snapshot.capture_inputs().iter().map(|input| input.owner()));
        }
    }
    assert!(
        seen.contains(&f_header.owner()),
        "g must read header f through saved values, not the zero-iteration entry: {seen:?}"
    );
}

#[test]
fn skipped_outer_branch_does_not_read_uninitialized_loop_phi_selectors() {
    use lang_frontend::ownership_checking::{
        CleanupCondition, CleanupConditionId, CleanupConditions, CleanupSelection, DropPoint,
        IterationPhiIncomingKind,
    };
    use lang_frontend::parser::Expression;

    fn selected(
        table: &CleanupConditions,
        condition: CleanupConditionId,
        choices: &std::collections::BTreeMap<usize, usize>,
    ) -> bool {
        match table.get(condition).unwrap() {
            CleanupCondition::Always => true,
            CleanupCondition::Never => false,
            CleanupCondition::Choice { selector, branches } => {
                let branch = choices
                    .get(&selector.index())
                    .expect("a skipped loop cannot read its uninitialized phi selector");
                selected(table, branches[*branch], choices)
            }
        }
    }

    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>, own ys: List<Int>, flag: Boolean, flags: List<Boolean>) {
            var f: () -> Unit = { read(xs) }
            if (flag) { for (_ in flags) { f = ({ read(ys) }) } }
            val used = f()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let outer = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| {
            (matches!(node.payload(), Expression::If { .. })
                && sources.slice(node.span()).unwrap().starts_with("if (flag)"))
            .then_some(id)
        })
        .unwrap();
    let copied = owned
        .cleanup_steps()
        .iter()
        .flat_map(|(_, action)| match action {
            lang_frontend::ownership_checking::IterationCleanupAction::SaveOwnerSnapshot {
                owner,
                ..
            } => owned
                .cleanup_conditions()
                .owner_snapshot(*owner)
                .unwrap()
                .copies()
                .iter()
                .map(|copy| copy.target().index())
                .collect::<Vec<_>>(),
            _ => Vec::new(),
        })
        .collect::<std::collections::BTreeSet<_>>();
    let selectors = owned
        .cleanup_conditions()
        .selectors()
        .iter()
        .enumerate()
        .filter_map(|(index, selector)| {
            (selector.control() == Some(outer)
                && selector.selection() == CleanupSelection::Branch
                && !copied.contains(&index))
            .then_some(index)
        })
        .collect::<Vec<_>>();
    assert_eq!(selectors.len(), 1);
    let choices = std::collections::BTreeMap::from([(selectors[0], 1)]);
    let plan = &owned.iterations()[0];
    let exhaustion = plan
        .closure_phi_incomings()
        .iter()
        .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Exhaustion)
        .unwrap();
    assert!(
        !selected(owned.cleanup_conditions(), exhaustion.condition(), &choices),
        "an outer false branch never enters the loop's exhaustion edge"
    );
    let call = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| (sources.slice(node.span()).unwrap() == "f()").then_some(id))
        .unwrap();
    let call_drops = owned
        .drops()
        .iter()
        .filter(|fact| fact.point() == DropPoint::CallReturn(call))
        .collect::<Vec<_>>();
    assert!(
        !call_drops.is_empty(),
        "the final call has cleanup to check"
    );
    for fact in call_drops {
        if let Some(condition) = fact.condition() {
            selected(owned.cleanup_conditions(), condition, &choices);
        }
    }
    let capture_actions = owned
        .cleanup_steps()
        .iter()
        .filter(|(point, _)| *point == DropPoint::CallReturn(call))
        .filter_map(|(_, action)| match action {
            lang_frontend::ownership_checking::IterationCleanupAction::EndCaptureLoan {
                condition,
                ..
            }
            | lang_frontend::ownership_checking::IterationCleanupAction::TestLastCaptureLoan {
                condition,
                ..
            } => Some(*condition),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(
        !capture_actions.is_empty(),
        "the call releases a capture loan"
    );
    for condition in capture_actions.into_iter().flatten() {
        selected(owned.cleanup_conditions(), condition, &choices);
    }
}
