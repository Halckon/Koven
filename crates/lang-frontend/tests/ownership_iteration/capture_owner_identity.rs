use super::*;

#[test]
fn branch_replaced_capture_source_keeps_distinct_owner_versions() {
    use lang_frontend::ownership_checking::{
        CleanupCondition, CleanupConditionId, CleanupConditions, CleanupOwnerValue, DropPoint,
        DropTarget,
    };
    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(flag: Boolean) {
            var xs = listOf(1)
            if (flag) { xs = listOf(2) } else { xs = listOf(3) }
            val f: () -> Unit = { read(xs) }
            val used = f()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let drops = owned.drops().iter().filter(|fact| {
        matches!(fact.target(), DropTarget::Named(_))
            && matches!(fact.point(), DropPoint::CallReturn(call)
                if sources.slice(parsed.ast().expressions().get(call).unwrap().span()).unwrap() == "f()")
            && sources.slice(fact.value_origin()).unwrap().starts_with("xs =")
    }).collect::<Vec<_>>();
    assert_eq!(
        drops.len(),
        2,
        "each replacement is a distinct source owner"
    );
    assert!(drops.iter().all(|fact| fact.condition().is_some()));
    assert_ne!(drops[0].owner(), drops[1].owner());
    for fact in &drops {
        let CleanupOwnerValue::Expression { expression, .. } = owned
            .cleanup_conditions()
            .owner_value(fact.owner().expect("source definition"))
            .unwrap()
        else {
            panic!("replacement must retain its evaluated value identity");
        };
        assert!(
            ["listOf(2)", "listOf(3)"].contains(
                &sources
                    .slice(parsed.ast().expressions().get(*expression).unwrap().span())
                    .unwrap()
            )
        );
    }
    fn selected(table: &CleanupConditions, id: CleanupConditionId, arm: usize) -> bool {
        match table.get(id).unwrap() {
            CleanupCondition::Always => true,
            CleanupCondition::Never => false,
            CleanupCondition::Choice { branches, .. } => selected(table, branches[arm], arm),
        }
    }
    for arm in 0..2 {
        assert_eq!(
            drops
                .iter()
                .filter(|fact| selected(owned.cleanup_conditions(), fact.condition().unwrap(), arm))
                .count(),
            1,
            "only the selected source version is destroyed"
        );
    }
}

#[test]
fn moving_a_source_binding_preserves_its_parameter_owner_identity() {
    use lang_frontend::ownership_checking::{CleanupOwnerValue, DropTarget};
    let (sources, _, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>) {
            val ys = xs
            val f: () -> Unit = { read(ys) }
            val used = f()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let fact = owned
        .drops()
        .iter()
        .find(|fact| {
            matches!(fact.target(), DropTarget::Named(_))
                && sources.slice(fact.value_origin()).unwrap() == "ys"
        })
        .unwrap();
    let CleanupOwnerValue::Parameter { origin, .. } = owned
        .cleanup_conditions()
        .owner_value(fact.owner().expect("moved source owner"))
        .unwrap()
    else {
        panic!("binding transfer must not fabricate a new value");
    };
    assert_eq!(sources.slice(*origin).unwrap(), "xs");
}

#[test]
fn source_owner_versions_survive_pending_value_argument_return() {
    use lang_frontend::ownership_checking::{CleanupOwnerValue, DropPoint, DropTarget};
    let (_, parsed, owned) = checked(
        "fun consume(own xs: List<Int>, done: Unit) {}\nfun run(own xs: List<Int>, own ys: List<Int>, flag: Boolean) {
            consume(if (flag) xs else ys, return)
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let drops = owned.drops().iter().filter(|fact|
        matches!(fact.point(), DropPoint::ControlTransfer(_))
        && matches!(fact.target(), DropTarget::Temporary(expression)
            if matches!(parsed.ast().expressions().get(expression).unwrap().payload(), lang_frontend::parser::Expression::If { .. })))
        .collect::<Vec<_>>();
    assert_eq!(
        drops.len(),
        2,
        "each incoming consumed parameter remains a pending obligation"
    );
    assert_ne!(drops[0].owner(), drops[1].owner());
    for fact in drops {
        assert!(fact.condition().is_some());
        assert!(matches!(
            owned
                .cleanup_conditions()
                .owner_value(fact.owner().unwrap()),
            Some(CleanupOwnerValue::Parameter { .. })
        ));
    }
}

#[test]
fn source_owner_identity_survives_non_null_assertion() {
    use lang_frontend::ownership_checking::{CleanupOwnerValue, DropTarget};
    let (sources, _, owned) =
        checked("class Resource {}\nfun run(own source: Resource?) { val extracted = source!! }");
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let fact = owned
        .drops()
        .iter()
        .find(|fact| matches!(fact.target(), DropTarget::Named(_)))
        .unwrap();
    let CleanupOwnerValue::Parameter { origin, .. } = owned
        .cleanup_conditions()
        .owner_value(fact.owner().unwrap())
        .unwrap()
    else {
        panic!("extraction must transport the consumed owner");
    };
    assert_eq!(sources.slice(*origin).unwrap(), "source");
}

#[test]
fn constant_temporary_normalization_preserves_source_owner_identity() {
    use lang_frontend::ownership_checking::{CleanupOwnerValue, DropPoint, DropTarget};
    for text in [
        "const val TEXT = \"hi\"\nfun view(text: String): Unit {}\nfun run(): Unit { val used = view((TEXT)) }",
        "const val TEXT = \"hi\"\nfun consume(own text: String, done: Unit): Unit {}\nfun run(): Unit { consume((TEXT), return) }",
    ] {
        let (_, _, owned) = checked(text);
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        let fact = owned
            .drops()
            .iter()
            .find(|fact| {
                matches!(fact.target(), DropTarget::Temporary(_))
                    && matches!(
                        fact.point(),
                        DropPoint::CallReturn(_) | DropPoint::ControlTransfer(_)
                    )
            })
            .unwrap();
        assert!(matches!(
            owned.cleanup_conditions().owner_value(
                fact.owner()
                    .expect("normalization must preserve the value identity")
            ),
            Some(CleanupOwnerValue::Expression { .. })
        ));
    }
}

#[test]
fn moved_capture_keeps_the_source_value_after_its_binding_is_replaced() {
    use lang_frontend::ownership_checking::{CleanupOwnerValue, DropTarget};
    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run() {
            var xs = listOf(1)
            val f: move () -> Unit = move { read(xs) }
            xs = listOf(2)
            val g: move () -> Unit = move { read(xs) }
            val first = f()
            val second = g()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let captures = owned
        .drops()
        .iter()
        .filter(|fact| matches!(fact.target(), DropTarget::Captured { .. }))
        .collect::<Vec<_>>();
    assert_eq!(captures.len(), 2);
    let mut definitions = Vec::new();
    for fact in captures {
        let CleanupOwnerValue::Expression { expression, .. } = owned
            .cleanup_conditions()
            .owner_value(
                fact.owner()
                    .expect("owned capture must retain its source value identity"),
            )
            .unwrap()
        else {
            panic!("capture must consume the evaluated source, not its later binding");
        };
        definitions.push(
            sources
                .slice(parsed.ast().expressions().get(*expression).unwrap().span())
                .unwrap(),
        );
    }
    assert_eq!(definitions, ["listOf(1)", "listOf(2)"]);
}

#[test]
fn nested_capture_refers_to_the_immediate_environment_slot() {
    use lang_frontend::ownership_checking::{
        CleanupCaptureValue, CleanupOwnerValue, IterationCleanupAction,
    };
    let (_, _, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>) {
            val outer: move () -> Unit = move {
                val inner: () -> Unit = { read(xs) }
                val used = inner()
            }
            val used = outer()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let creations = owned
        .cleanup_steps()
        .iter()
        .filter_map(|(_, action)| match action {
            IterationCleanupAction::CreateClosureOwner { owner, .. } => Some(*owner),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(creations.len(), 2);
    let mut nested = None;
    for owner in creations {
        let CleanupOwnerValue::Closure { inputs, .. } =
            owned.cleanup_conditions().owner_value(owner).unwrap()
        else {
            panic!("created environment");
        };
        assert_eq!(inputs.len(), 1);
        if let CleanupCaptureValue::Environment {
            owner: enclosing,
            source,
            slot,
        } = inputs[0].value()
        {
            assert_eq!(
                owned.cleanup_conditions().capture_slot(enclosing, source),
                Some(slot)
            );
            let layout = owned.cleanup_conditions().capture_slot_value(slot).unwrap();
            assert_eq!(layout.environment(), enclosing);
            assert_eq!(layout.source(), source);
            assert_eq!(layout.position(), 0);
            let inner_slot = owned
                .cleanup_conditions()
                .capture_slot(owner, source)
                .expect("the inner environment has its own capture slot");
            assert_ne!(inner_slot, slot);
            assert_eq!(
                owned
                    .cleanup_conditions()
                    .capture_slot_value(inner_slot)
                    .unwrap()
                    .position(),
                0
            );
            let CleanupOwnerValue::Closure { inputs: outer, .. } =
                owned.cleanup_conditions().owner_value(enclosing).unwrap()
            else {
                panic!("immediate environment");
            };
            assert_eq!(outer[0].source(), source);
            assert!(matches!(outer[0].value(), CleanupCaptureValue::Owner(_)));
            nested = Some((owner, inputs[0].value()));
        }
    }
    let (owner, value) = nested.expect("inner capture must use the outer environment");
    assert!(owned.cleanup_steps().iter().any(|(_, action)|
        matches!(action, IterationCleanupAction::EndCaptureLoan { owner: actual, value: actual_value, .. }
            if *actual == owner && *actual_value == value)));
}

#[test]
fn owned_nested_capture_facts_keep_the_moved_slot_identity() {
    use lang_frontend::ownership_checking::{
        CleanupCaptureValue, CleanupCondition, CleanupOwnerValue, ClosureCaptureEffect,
        ClosureCaptureMode, DropPoint, DropTarget, IterationCleanupAction,
    };
    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>) {
            val outer: move () -> Unit = move {
                val inner: move () -> Unit = move { read(xs) }
                val used = inner()
            }
            val used = outer()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let names = resolve_names(&sources, &parsed, &standard_environments().0).unwrap();
    let xs = names
        .symbols()
        .iter()
        .rfind(|symbol| sources.slice(symbol.span()).unwrap() == "xs")
        .unwrap()
        .id();
    let captured = owned
        .drops()
        .iter()
        .filter(|fact| match fact.target() {
            DropTarget::Captured {
                source: lang_frontend::ownership_checking::ClosureCaptureSource::Symbol(source),
                ..
            } => source == xs,
            _ => false,
        })
        .collect::<Vec<_>>();
    assert_eq!(captured.len(), 2, "both call exits have a candidate drop");
    let mut outer = None;
    let mut inner = None;
    for fact in &captured {
        let DropTarget::Captured {
            owner,
            source,
            value,
            ..
        } = fact.target()
        else {
            unreachable!();
        };
        let slot = owned
            .cleanup_conditions()
            .capture_slot(owner, source)
            .expect("created closure has its capture slot");
        assert_eq!(fact.capture_slot(), Some(slot));
        match value {
            CleanupCaptureValue::Owner(_) => outer = Some(slot),
            CleanupCaptureValue::Environment { slot: source, .. } => {
                inner = Some((owner, slot, source));
            }
            CleanupCaptureValue::Place(_) => panic!("owned capture needs an owner"),
        }
    }
    let outer = outer.unwrap();
    let (inner_owner, inner, source) = inner.unwrap();
    assert_eq!(
        source, outer,
        "inner formation reads the immediate outer slot"
    );
    assert!(owned.cleanup_steps().iter().any(|(_, action)| matches!(
        action,
        IterationCleanupAction::CreateClosureOwner { owner, .. } if *owner == inner_owner
    )));
    let Some(CleanupOwnerValue::Closure { inputs, .. }) =
        owned.cleanup_conditions().owner_value(inner_owner)
    else {
        panic!("the inner value is a formed closure");
    };
    assert_eq!(inputs.len(), 1);
    let edges = owned
        .cleanup_conditions()
        .closure_capture_edges(inner_owner)
        .expect("the formed environment has capture edges");
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].target(), inner);
    assert_eq!(edges[0].input(), inputs[0]);
    let steps = owned.cleanup_steps();
    let creation = steps
        .iter()
        .position(|(_, action)| matches!(action, IterationCleanupAction::CreateClosureOwner { owner, .. } if *owner == inner_owner))
        .unwrap();
    assert!(matches!(
        steps[creation + 1],
        (point, IterationCleanupAction::SaveClosureCapture { owner, target, input })
            if point == steps[creation].0 && owner == inner_owner && target == inner && input == inputs[0]
    ));
    assert!(matches!(
        inputs[0].value(),
        CleanupCaptureValue::Environment { slot, .. } if slot == outer
    ));
    assert_eq!(inputs[0].effect(), ClosureCaptureEffect::Move);
    let outer_owner = owned
        .cleanup_conditions()
        .capture_slot_value(outer)
        .unwrap()
        .environment();
    let capture_write = |target| {
        steps
            .iter()
            .find_map(|(_, action)| match action {
                IterationCleanupAction::SaveClosureCapture {
                    owner,
                    target: actual,
                    input,
                } if *actual == target => Some((*owner, *input)),
                _ => None,
            })
            .unwrap()
    };
    let (written_outer, outer_input) = capture_write(outer);
    let (written_inner, inner_input) = capture_write(inner);
    assert_eq!(written_outer, outer_owner);
    assert_eq!(written_inner, inner_owner);
    let CleanupCaptureValue::Owner(xs_owner) = outer_input.value() else {
        panic!("outer formation must read the current xs owner");
    };
    assert_eq!(
        inner_input.value(),
        CleanupCaptureValue::Environment {
            owner: outer_owner,
            source: outer_input.source(),
            slot: source,
        }
    );
    for input in [outer_input, inner_input] {
        assert_eq!(input.mode(), ClosureCaptureMode::Owned);
        assert_eq!(input.effect(), ClosureCaptureEffect::Move);
        assert_eq!(
            owned.cleanup_conditions().get(input.condition()),
            Some(&CleanupCondition::Always)
        );
    }
    let mut owners = std::collections::BTreeMap::new();
    let mut slots = std::collections::BTreeMap::new();
    let mut instances = Vec::new();
    for round in 0..2_u32 {
        let xs_instance = 100 + round;
        let outer_instance = 200 + round;
        let inner_instance = 300 + round;
        owners.insert(xs_owner, xs_instance);
        owners.insert(outer_owner, outer_instance);
        let moved = owners.remove(&xs_owner).unwrap();
        assert!(
            slots
                .insert((owners[&written_outer], outer), moved)
                .is_none()
        );
        owners.insert(inner_owner, inner_instance);
        let CleanupCaptureValue::Environment { owner, slot, .. } = inner_input.value() else {
            unreachable!();
        };
        let moved = slots.remove(&(owners[&owner], slot)).unwrap();
        assert!(
            slots
                .insert((owners[&written_inner], inner), moved)
                .is_none()
        );
        instances.push((outer_instance, inner_instance, xs_instance));
    }
    assert_eq!(slots[&(instances[0].1, inner)], instances[0].2);
    assert_eq!(slots[&(instances[1].1, inner)], instances[1].2);
    let call = |text| {
        parsed
            .ast()
            .expressions()
            .iter()
            .find_map(|(id, node)| (sources.slice(node.span()).unwrap() == text).then_some(id))
            .unwrap()
    };
    let call_order = [call("inner()"), call("outer()")];
    let drop_index = |call| {
        steps
            .iter()
            .position(|(point, action)| {
                *point == DropPoint::CallReturn(call)
                    && matches!(action, IterationCleanupAction::Drop(fact) if captured.contains(&fact))
            })
            .unwrap()
    };
    assert!(drop_index(call_order[0]) < drop_index(call_order[1]));
    let mut released = Vec::new();
    for (outer_instance, inner_instance, _) in instances {
        for (call, expected_owner, expected_slot, instance, input) in [
            (
                call_order[0],
                inner_owner,
                inner,
                inner_instance,
                inner_input,
            ),
            (
                call_order[1],
                outer_owner,
                outer,
                outer_instance,
                outer_input,
            ),
        ] {
            let actions = steps
                .iter()
                .filter_map(|(point, action)| match action {
                    IterationCleanupAction::Drop(fact)
                        if *point == DropPoint::CallReturn(call) && captured.contains(&fact) =>
                    {
                        Some(fact)
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(actions.len(), 1, "each call needs its captured drop action");
            let fact = actions[0];
            assert_eq!(fact.point(), DropPoint::CallReturn(call));
            assert_eq!(fact.capture_slot(), Some(expected_slot));
            assert!(fact.condition().is_none_or(|condition| {
                owned.cleanup_conditions().get(condition) == Some(&CleanupCondition::Always)
            }));
            assert!(
                matches!(fact.target(), DropTarget::Captured { owner, value, .. }
                if owner == expected_owner && value == input.value())
            );
            if let Some(value) = slots.remove(&(instance, expected_slot)) {
                released.push(value);
            }
        }
    }
    assert_eq!(released, [100, 101], "each invocation releases its own xs");
    assert!(slots.is_empty(), "the moved outer slots stay empty");
}

#[test]
fn recursively_released_closure_keeps_its_capture_slot() {
    use lang_frontend::ownership_checking::DropTarget;
    let (_, _, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>) {
            val base: move () -> Unit = move { read(xs) }
            val outer: move () -> Unit = move { base() }
            val used = outer()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let captured = owned
        .drops()
        .iter()
        .filter(|fact| matches!(fact.target(), DropTarget::Captured { .. }))
        .collect::<Vec<_>>();
    assert_eq!(captured.len(), 2, "outer owns base, base owns xs");
    for fact in captured {
        let DropTarget::Captured {
            closure, source, ..
        } = fact.target()
        else {
            unreachable!();
        };
        let slot = fact
            .capture_slot()
            .expect("recursive release keeps the slot identity");
        let layout = owned.cleanup_conditions().capture_slot_value(slot).unwrap();
        assert_eq!(layout.source(), source);
        assert!(matches!(
            owned.cleanup_conditions().owner_value(layout.environment()),
            Some(lang_frontend::ownership_checking::CleanupOwnerValue::Closure {
                expression,
                ..
            }) if *expression == closure
        ));
    }
}

#[test]
fn capture_inputs_keep_creation_conditions_separate_from_transport_conditions() {
    use lang_frontend::ownership_checking::{
        CleanupCaptureValue, CleanupCondition, CleanupConditionId, CleanupConditions,
        CleanupOwnerValue, CleanupSelectorId, DropTarget, IterationCleanupAction,
    };
    use std::collections::{BTreeMap, BTreeSet};
    let (_, _, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(flag: Boolean) {
            var xs = listOf(0)
            if (flag) { xs = listOf(1) } else { xs = listOf(2) }
            val f: move () -> Unit = move { read(xs) }
            val g = f
            val used = g()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let table = owned.cleanup_conditions();
    fn enabled(
        table: &CleanupConditions,
        id: CleanupConditionId,
        choices: &BTreeMap<CleanupSelectorId, usize>,
    ) -> bool {
        match table.get(id).unwrap() {
            CleanupCondition::Always => true,
            CleanupCondition::Never => false,
            CleanupCondition::Choice { selector, branches } => enabled(
                table,
                branches[*choices
                    .get(selector)
                    .expect("only initialized selections may be read")],
                choices,
            ),
        }
    }
    let copies = owned
        .cleanup_steps()
        .iter()
        .filter_map(|(_, action)| match action {
            IterationCleanupAction::SaveOwnerSnapshot { owner, .. } => {
                Some(table.owner_snapshot(*owner).unwrap())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        copies.len(),
        2,
        "both initialization and moving to g save independent selections"
    );
    let targets = copies
        .iter()
        .flat_map(|snapshot| snapshot.copies().iter().map(|copy| copy.target()))
        .collect::<BTreeSet<_>>();
    let (environment, initial) = owned
        .cleanup_steps()
        .iter()
        .find_map(|(_, action)| match action {
            IterationCleanupAction::CreateClosureOwner { owner, .. } => {
                match table.owner_value(*owner).unwrap() {
                    CleanupOwnerValue::Closure { inputs, .. } => Some((*owner, inputs)),
                    _ => None,
                }
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(initial.len(), 2);
    assert_eq!(initial[0].source(), initial[1].source());
    let slot = table
        .capture_slot(environment, initial[0].source())
        .expect("both conditional source versions share one capture slot");
    let edges = table.closure_capture_edges(environment).unwrap();
    assert_eq!(edges.len(), 2);
    assert_eq!(edges[0].target(), slot);
    assert_eq!(edges[1].target(), slot);
    assert_eq!(edges[0].input(), initial[0]);
    assert_eq!(edges[1].input(), initial[1]);
    let steps = owned.cleanup_steps();
    let creation = steps
        .iter()
        .position(|(_, action)| matches!(action, IterationCleanupAction::CreateClosureOwner { owner, .. } if *owner == environment))
        .unwrap();
    for (offset, edge) in edges.iter().enumerate() {
        assert!(matches!(
            steps[creation + offset + 1],
            (point, IterationCleanupAction::SaveClosureCapture { owner, target, input })
                if point == steps[creation].0 && owner == environment && target == edge.target() && input == edge.input()
        ));
    }
    assert_eq!(
        table.capture_slot_value(slot).unwrap().environment(),
        environment
    );
    assert_eq!(table.capture_slot_value(slot).unwrap().position(), 0);
    let mut direct = None;
    for input in initial {
        let CleanupCondition::Choice { selector, .. } = table.get(input.condition()).unwrap()
        else {
            panic!("source version choice");
        };
        assert!(
            !targets.contains(selector),
            "creation cannot read a later Save target"
        );
        assert!(direct.is_none_or(|prior| prior == *selector));
        direct = Some(*selector);
    }
    let direct = direct.unwrap();
    for arm in 0..2 {
        let mut choices = BTreeMap::from([(direct, arm)]);
        let selected = initial
            .iter()
            .filter(|input| enabled(table, input.condition(), &choices))
            .collect::<Vec<_>>();
        assert_eq!(selected.len(), 1);
        let source = selected[0].value();
        for snapshot in &copies {
            let before = choices.clone();
            for copy in snapshot.copies() {
                if enabled(table, copy.when(), &before) {
                    choices.insert(copy.target(), before[&copy.source()]);
                }
            }
        }
        // The source control can be evaluated again; g must retain its saved source choice.
        choices.insert(direct, 1 - arm);
        let released = owned
            .drops()
            .iter()
            .filter(|fact| {
                matches!(fact.target(), DropTarget::Captured { .. })
                    && fact
                        .condition()
                        .is_none_or(|condition| enabled(table, condition, &choices))
            })
            .collect::<Vec<_>>();
        assert_eq!(released.len(), 1);
        let DropTarget::Captured { owner, value, .. } = released[0].target() else {
            unreachable!();
        };
        assert_eq!(owner, copies.last().unwrap().owner());
        assert_eq!(
            value, source,
            "moving the environment must retain its concrete captured value"
        );
        assert_eq!(
            value,
            CleanupCaptureValue::Owner(released[0].owner().unwrap())
        );
    }
}

#[test]
fn loop_carried_hidden_sources_keep_separate_cleanup_for_prior_and_current_environments() {
    use lang_frontend::ownership_checking::DropPoint;
    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(flags: List<Boolean>) {
            var f: () -> Unit = {}
            var g: () -> Unit = {}
            for (_ in flags) {
                val prior = f
                val xs = listOf(1)
                { g = prior }
                { f = ({ read(xs) }) }
            }
            val first = g()
            val second = f()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let mut owners = Vec::new();
    for call in ["g()", "f()"] {
        let released = owned.drops().iter().filter(|fact|
            sources.slice(fact.value_origin()).unwrap() == "xs"
            && matches!(fact.point(), DropPoint::CallReturn(id)
                if sources.slice(parsed.ast().expressions().get(id).unwrap().span()).unwrap() == call))
            .collect::<Vec<_>>();
        assert!(
            !released.is_empty(),
            "{call}: hidden source must survive its lexical scope until the corresponding environment is released"
        );
        assert!(
            released.iter().all(|fact| fact.condition().is_some()),
            "zero iterations have no hidden source"
        );
        owners.push(
            released
                .iter()
                .map(|fact| fact.owner().expect("transported source owner"))
                .collect::<std::collections::BTreeSet<_>>(),
        );
    }
    assert!(
        owners[0].is_disjoint(&owners[1]),
        "the same local definition creates different live source instances in prior and current environments"
    );
}
