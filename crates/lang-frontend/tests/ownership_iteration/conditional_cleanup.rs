use super::*;

#[test]
fn conditional_closure_drops_only_the_selected_source_after_call() {
    use lang_frontend::ownership_checking::{
        CleanupCondition, ClosureCaptureSource, DropPoint, DropTarget,
    };
    for (control, call_text) in [
        ("if (flag) ({ read(xs) }) else ({ read(ys) })", "chosen()"),
        (
            "if (flag) ({ read(xs) }) else ({ read(ys) })",
            "invoke(chosen)",
        ),
        (
            "when { flag -> ({ read(xs) })\nelse -> ({ read(ys) }) }",
            "chosen()",
        ),
    ] {
        let (sources, parsed, owned) = checked(&format!(
            "fun read(xs: List<Int>) {{}}\nfun invoke(f: () -> Unit) {{ val used = f() }}\nfun run(own xs: List<Int>, own ys: List<Int>, flag: Boolean) {{ val f: () -> Unit = {control}\nval chosen = f\nval used = {call_text} }}",
        ));
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        let call = parsed
            .ast()
            .expressions()
            .iter()
            .find_map(|(id, node)| (sources.slice(node.span()).unwrap() == call_text).then_some(id))
            .unwrap();
        for (name, selected) in [("xs", 0), ("ys", 1)] {
            let symbol = owned
                .captures()
                .iter()
                .find_map(|capture| {
                    if sources.slice(capture.reference_span()).unwrap() != name {
                        return None;
                    }
                    match capture.source() {
                        ClosureCaptureSource::Symbol(symbol) => Some(symbol),
                        _ => None,
                    }
                })
                .unwrap();
            let drops = owned
                .drops()
                .iter()
                .filter(|fact| fact.target() == DropTarget::Named(symbol))
                .collect::<Vec<_>>();
            let late = drops
                .iter()
                .filter(|fact| fact.point() == DropPoint::CallReturn(call))
                .collect::<Vec<_>>();
            assert_eq!(late.len(), 1, "{name}: {drops:?}");
            let guard = late[0]
                .condition()
                .expect("only the selected source survives the join");
            let Some(CleanupCondition::Choice { branches, .. }) =
                owned.cleanup_conditions().get(guard)
            else {
                panic!("expected saved branch choice")
            };
            assert_eq!(
                owned.cleanup_conditions().get(branches[selected]),
                Some(&CleanupCondition::Always)
            );
            assert_eq!(
                owned.cleanup_conditions().get(branches[1 - selected]),
                Some(&CleanupCondition::Never)
            );
            assert!(drops.iter().any(|fact| matches!(fact.point(), DropPoint::BranchExit { branch, .. } if branch == 1-selected)), "unselected source must release at its branch: {drops:?}");
        }
    }
}

#[test]
fn nested_closure_cleanup_does_not_read_an_unexecuted_inner_choice() {
    assert_selected_closure_source_survives(
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>, own ys: List<Int>, own zs: List<Int>, outer: Boolean, inner: Boolean) { val f: () -> Unit = if (outer) (if (inner) ({ read(xs) }) else ({ read(ys) })) else ({ read(zs) })\nval used = f() }",
        &[
            ("xs", &[(0, 0), (1, 0)]),
            ("ys", &[(0, 0), (1, 1)]),
            ("zs", &[(0, 1)]),
        ],
    );
}

#[test]
fn branch_local_closure_chain_snapshots_remain_guarded_after_the_join() {
    assert_selected_closure_source_survives(
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>, own ys: List<Int>, flag: Boolean) { val f: () -> Unit = if (flag) { val inner: () -> Unit = { read(xs) }\nval g: () -> Unit = { inner() }\ng } else ({ read(ys) })\nval used = f() }",
        &[("xs", &[(0, 0)]), ("ys", &[(0, 1)])],
    );
}

#[test]
fn copying_an_earlier_choice_inside_a_later_branch_keeps_the_branch_guard_first() {
    assert_selected_closure_source_survives(
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>, own ys: List<Int>, flag: Boolean, pick: Boolean) { val previous: () -> Unit = if (flag) ({ read(xs) }) else ({ read(ys) })\nval f: () -> Unit = if (pick) { val g: () -> Unit = { val observed = previous() }\ng } else previous\nval used = f() }",
        &[
            ("xs", &[(0, 0), (1, 0)]),
            ("xs", &[(0, 0), (1, 1)]),
            ("ys", &[(0, 1), (1, 0)]),
            ("ys", &[(0, 1), (1, 1)]),
        ],
    );
}

fn assert_selected_closure_source_survives(text: &str, cases: &[(&str, &[(usize, usize)])]) {
    use lang_frontend::ownership_checking::{
        CleanupCondition, CleanupConditionId, CleanupConditions, ClosureCaptureSource, DropPoint,
        DropTarget,
    };
    fn enabled(
        table: &CleanupConditions,
        condition: CleanupConditionId,
        choices: &[(usize, usize)],
    ) -> bool {
        match table.get(condition).unwrap() {
            CleanupCondition::Always => true,
            CleanupCondition::Never => false,
            CleanupCondition::Choice { selector, branches } => {
                let branch = choices
                    .iter()
                    .find(|(id, _)| *id == selector.index())
                    .expect("cleanup may only read an initialized selector")
                    .1;
                enabled(table, branches[branch], choices)
            }
        }
    }
    let (sources, parsed, owned) = checked(text);
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let expression = |text| {
        parsed
            .ast()
            .expressions()
            .iter()
            .find_map(|(id, node)| (sources.slice(node.span()).unwrap() == text).then_some(id))
            .unwrap()
    };
    let call = expression("f()");
    let mut controls = parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| match node.payload() {
            lang_frontend::parser::Expression::If { .. } => Some((node.span().start(), id)),
            _ => None,
        })
        .collect::<Vec<_>>();
    controls.sort_by_key(|(start, _)| *start);
    for &(selected, choices) in cases {
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
        let mut choices = choices
            .iter()
            .copied()
            .map(|(control, arm)| {
                let selectors = owned
                    .cleanup_conditions()
                    .selectors()
                    .iter()
                    .enumerate()
                    .filter_map(|(index, selector)| {
                        (selector.control() == Some(controls[control].1)
                            && !copied.contains(&index))
                        .then_some(index)
                    })
                    .collect::<Vec<_>>();
                assert_eq!(
                    selectors.len(),
                    1,
                    "this fixture has one direct selector per control"
                );
                (selectors[0], arm)
            })
            .collect::<Vec<_>>();
        for (_, action) in owned.cleanup_steps() {
            let lang_frontend::ownership_checking::IterationCleanupAction::SaveOwnerSnapshot {
                owner,
                condition,
                ..
            } = action
            else {
                continue;
            };
            if condition.is_some_and(|guard| !enabled(owned.cleanup_conditions(), guard, &choices))
            {
                continue;
            }
            let writes = owned
                .cleanup_conditions()
                .owner_snapshot(*owner)
                .unwrap()
                .copies()
                .iter()
                .filter(|copy| enabled(owned.cleanup_conditions(), copy.when(), &choices))
                .map(|copy| {
                    (
                        copy.target().index(),
                        choices
                            .iter()
                            .find(|(id, _)| *id == copy.source().index())
                            .expect("only initialized selectors can be copied")
                            .1,
                    )
                })
                .collect::<Vec<_>>();
            choices.extend(writes);
        }
        for capture in owned.captures() {
            let ClosureCaptureSource::Symbol(symbol) = capture.source() else {
                continue;
            };
            let name = sources.slice(capture.reference_span()).unwrap();
            if !["xs", "ys", "zs"].contains(&name) {
                continue;
            }
            let late = owned
                .drops()
                .iter()
                .filter(|fact| {
                    fact.target() == DropTarget::Named(symbol)
                        && fact.point() == DropPoint::CallReturn(call)
                })
                .collect::<Vec<_>>();
            assert!(!late.is_empty(), "{name}: {:?}", owned.drops());
            assert_eq!(
                late.iter()
                    .filter(|fact| fact.condition().is_none_or(|guard| enabled(
                        owned.cleanup_conditions(),
                        guard,
                        &choices
                    )))
                    .count(),
                usize::from(name == selected)
            );
        }
    }
}

#[test]
fn when_first_alternative_does_not_read_a_skipped_later_alternative() {
    use lang_frontend::ownership_checking::{
        CleanupCondition, CleanupConditionId, CleanupConditions, CleanupSelection,
        ClosureCaptureSource, DropPoint, DropTarget, IterationCleanupAction as Action,
    };
    fn enabled(
        table: &CleanupConditions,
        condition: CleanupConditionId,
        values: &std::collections::BTreeMap<usize, usize>,
    ) -> bool {
        match table.get(condition).unwrap() {
            CleanupCondition::Always => true,
            CleanupCondition::Never => false,
            CleanupCondition::Choice { selector, branches } => enabled(
                table,
                branches[*values
                    .get(&selector.index())
                    .expect("only executed alternatives have selector values")],
                values,
            ),
        }
    }
    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>, own ys: List<Int>, first: Boolean, gate: Boolean) { var f: () -> Unit = { read(xs) }\nval g: () -> Unit = when { first, if (gate) { { f = ({ read(ys) }) }\ntrue } else false -> f\nelse -> f }\nval used = g() }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let control = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| {
            matches!(
                node.payload(),
                lang_frontend::parser::Expression::When { .. }
            )
            .then_some(id)
        })
        .unwrap();
    let call = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| (sources.slice(node.span()).unwrap() == "g()").then_some(id))
        .unwrap();
    let table = owned.cleanup_conditions();
    let copied = owned
        .cleanup_steps()
        .iter()
        .flat_map(|(_, action)| match action {
            Action::SaveOwnerSnapshot { owner, .. } => table
                .owner_snapshot(*owner)
                .unwrap()
                .copies()
                .iter()
                .map(|copy| copy.target().index())
                .collect::<Vec<_>>(),
            _ => Vec::new(),
        })
        .collect::<std::collections::BTreeSet<_>>();
    // First match skips gate entirely; the two other paths evaluate it and select old/new f.
    for (first, gate, selected) in [(0, None, "xs"), (1, Some(0), "ys"), (1, Some(1), "xs")] {
        let mut values = table
            .selectors()
            .iter()
            .enumerate()
            .filter_map(|(index, selector)| {
                if copied.contains(&index) {
                    return None;
                }
                let choice = match selector.selection() {
                    CleanupSelection::WhenAlternative {
                        entry: 0,
                        alternative: 0,
                    } => {
                        assert_eq!(selector.control(), Some(control));
                        Some(first)
                    }
                    CleanupSelection::WhenAlternative {
                        entry: 0,
                        alternative: 1,
                    } => gate,
                    CleanupSelection::Branch => gate,
                    _ => panic!("unexpected control in fixture"),
                };
                choice.map(|choice| (index, choice))
            })
            .collect::<std::collections::BTreeMap<_, _>>();
        for (_, action) in owned.cleanup_steps() {
            if let Action::SaveOwnerSnapshot {
                condition, owner, ..
            } = action
            {
                if condition.is_some_and(|condition| !enabled(table, condition, &values)) {
                    continue;
                }
                let writes = table
                    .owner_snapshot(*owner)
                    .unwrap()
                    .copies()
                    .iter()
                    .filter(|copy| enabled(table, copy.when(), &values))
                    .map(|copy| {
                        (
                            copy.target().index(),
                            *values
                                .get(&copy.source().index())
                                .expect("copied source was evaluated"),
                        )
                    })
                    .collect::<Vec<_>>();
                values.extend(writes);
            }
        }
        for name in ["xs", "ys"] {
            let source = owned
                .captures()
                .iter()
                .find_map(|capture| {
                    (sources.slice(capture.reference_span()).unwrap() == name)
                        .then_some(capture.source())
                })
                .unwrap();
            let ClosureCaptureSource::Symbol(symbol) = source else {
                unreachable!()
            };
            let count = owned
                .drops()
                .iter()
                .filter(|fact| {
                    fact.target() == DropTarget::Named(symbol)
                        && fact.point() == DropPoint::CallReturn(call)
                        && fact
                            .condition()
                            .is_none_or(|condition| enabled(table, condition, &values))
                })
                .count();
            assert_eq!(
                count,
                usize::from(name == selected),
                "{name}: first={first}, gate={gate:?}"
            );
        }
    }
}
