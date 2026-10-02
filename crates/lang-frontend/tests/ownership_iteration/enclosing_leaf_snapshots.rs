use super::*;

#[test]
fn loop_phi_preserves_leaf_enclosing_closure_capture() {
    use lang_frontend::ownership_checking::{
        CleanupCaptureValue, DropPoint, DropTarget, IterationCleanupAction as Action,
        IterationPhiIncomingKind,
    };

    let (sources, parsed, owned) = checked(
        "fun run() { val base: move () -> Unit = move {}\nval outer: move () -> Unit = move { var f: move () -> Unit = move { base() }\nfor (_ in listOf(1)) {}\nval used = f() }\nval used = outer() }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    assert_eq!(owned.iterations().len(), 1);
    let incoming = owned.iterations()[0]
        .closure_phi_incomings()
        .iter()
        .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Entry)
        .unwrap();
    let source = incoming
        .bindings()
        .iter()
        .flat_map(|binding| binding.origins())
        .flat_map(|origin| origin.environments())
        .flat_map(|environment| environment.sources())
        .find(|source| matches!(source.value(), CleanupCaptureValue::Environment { .. }))
        .unwrap();
    let CleanupCaptureValue::Environment { slot, .. } = source.value() else {
        unreachable!()
    };
    let formed_slot = source.source_capture_slot().unwrap();
    assert_ne!(
        slot, formed_slot,
        "phi must read the formed inner environment"
    );
    assert!(
        source
            .captured()
            .iter()
            .any(|nested| !nested.environments().is_empty()),
        "the captured leaf environment must retain its origin"
    );
    let call = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| (sources.slice(node.span()).unwrap() == "f()").then_some(id))
        .unwrap();
    assert!(owned.cleanup_steps().iter().any(|(point, action)| {
        *point == DropPoint::CallReturn(call)
            && matches!(action, Action::Drop(fact) if matches!(fact.target(), DropTarget::Captured { value: CleanupCaptureValue::Environment { .. }, .. }))
    }));
}

#[test]
fn loop_phi_keeps_two_leaf_enclosing_capture_sources_distinct() {
    use lang_frontend::ownership_checking::{
        CleanupCaptureValue, IterationCleanupAction as Action, IterationPhiIncomingKind,
    };

    let (_, _, owned) = checked(
        "fun run() { val a: move () -> Unit = move {}\nval b: move () -> Unit = move {}\nval outer: move () -> Unit = move { var f: move () -> Unit = move { val x = a()\nval y = b() }\nfor (_ in listOf(1)) {}\nval used = f() }\nval used = outer() }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    let incoming = owned.iterations()[0]
        .closure_phi_incomings()
        .iter()
        .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Entry)
        .unwrap();
    let mut child_owners = std::collections::BTreeSet::new();
    for source in incoming
        .bindings()
        .iter()
        .flat_map(|binding| binding.origins())
        .flat_map(|origin| origin.environments())
        .flat_map(|environment| environment.sources())
    {
        let CleanupCaptureValue::Environment { slot, .. } = source.value() else {
            continue;
        };
        let formed = owned
            .cleanup_steps()
            .iter()
            .find_map(|(_, action)| match action {
                Action::SaveClosureCapture { target, input, .. } if *target == slot => {
                    Some(input.value())
                }
                _ => None,
            })
            .unwrap();
        let CleanupCaptureValue::Owner(child) = formed else {
            panic!("outer environment must own a formed leaf closure")
        };
        assert!(source.captured().iter().any(|nested| {
            nested
                .environments()
                .iter()
                .any(|environment| environment.owner() == child)
        }));
        child_owners.insert(child);
    }
    assert_eq!(child_owners.len(), 2, "each capture keeps its own child");
}

#[test]
fn loop_phi_publishes_conditional_leaf_after_phi_carries_choice() {
    use std::collections::BTreeMap;

    use lang_frontend::ownership_checking::{
        CleanupCondition, CleanupConditionId, CleanupConditions, CleanupSelectorId,
        CleanupSelectorSource, IterationPhiIncomingKind,
    };

    // 收集条件 DAG 引用的 selector；用于区分“形成时的控制选择”与“循环 presence”。
    fn referenced_selectors(
        conditions: &CleanupConditions,
        condition: CleanupConditionId,
    ) -> Vec<(CleanupSelectorId, CleanupSelectorSource)> {
        let mut out = Vec::new();
        let mut pending = vec![condition];
        while let Some(condition) = pending.pop() {
            if let CleanupCondition::Choice { selector, branches } =
                conditions.get(condition).unwrap()
            {
                out.push((*selector, conditions.selector(*selector).unwrap().source()));
                pending.extend(branches.iter().copied());
            }
        }
        out
    }

    // 未显式赋值的 selector 视为选中分支 1，只用于隔离出 flag 控制选择的影响。
    fn selected(
        conditions: &CleanupConditions,
        condition: CleanupConditionId,
        choices: &BTreeMap<CleanupSelectorId, usize>,
    ) -> bool {
        match conditions.get(condition).unwrap() {
            CleanupCondition::Always => true,
            CleanupCondition::Never => false,
            CleanupCondition::Choice { selector, branches } => selected(
                conditions,
                branches[choices.get(selector).copied().unwrap_or(1)],
                choices,
            ),
        }
    }

    let (_, _, owned) = checked(
        "fun run(flag: Boolean) {\nval base: move () -> Unit = if (flag) (move {}) else (move {})\nval outer: move () -> Unit = move { var f: move () -> Unit = move { base() }\nfor (_ in listOf(1)) {}\nval used = f() }\nval used = outer() }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    assert!(!owned.iterations().is_empty());
    assert!(!owned.cleanup_steps().is_empty());
    assert!(!owned.drops().is_empty());

    let conditions = owned.cleanup_conditions();
    let plan = &owned.iterations()[0];
    let incoming = |kind| {
        plan.closure_phi_incomings()
            .iter()
            .find(|incoming| incoming.kind() == kind)
            .unwrap()
    };
    // 嵌套的 leaf 来源存在位；根 binding origins 自身不直接对应条件分支。
    let entry_leaves: Vec<_> = incoming(IterationPhiIncomingKind::Entry)
        .bindings()
        .iter()
        .flat_map(|binding| binding.origins())
        .flat_map(|origin| origin.environments())
        .flat_map(|environment| environment.sources())
        .flat_map(|source| source.captured())
        .collect();
    assert!(
        entry_leaves.len() >= 2,
        "conditional base must expose both leaf presence slots"
    );

    let flag_selector = entry_leaves
        .iter()
        .flat_map(|leaf| referenced_selectors(conditions, leaf.condition()))
        .find_map(|(id, source)| matches!(source, CleanupSelectorSource::Control(_)).then_some(id))
        .expect("the conditional leaf depends on a source control selection");

    // Entry 必须把形成时的控制选择写成两个互补的 leaf presence 位。
    let mut active_target = BTreeMap::new();
    for flag in [0usize, 1] {
        let choices = BTreeMap::from([(flag_selector, flag)]);
        let active: Vec<_> = entry_leaves
            .iter()
            .filter(|leaf| selected(conditions, leaf.condition(), &choices))
            .map(|leaf| leaf.target())
            .collect();
        assert_eq!(
            active.len(),
            1,
            "exactly one leaf presence is set for flag branch {flag}: {active:?}"
        );
        active_target.insert(flag, active[0]);
    }
    assert_ne!(
        active_target[&0], active_target[&1],
        "the two flag branches select different leaf presence slots"
    );

    // Exhaustion 必须从 header phi presence 转发，而不是重新读取源码控制选择。
    let exhaustion_leaves: Vec<_> = incoming(IterationPhiIncomingKind::Exhaustion)
        .bindings()
        .iter()
        .flat_map(|binding| binding.origins())
        .flat_map(|origin| origin.environments())
        .flat_map(|environment| environment.sources())
        .flat_map(|source| source.captured())
        .collect();
    assert_eq!(exhaustion_leaves.len(), entry_leaves.len());
    for leaf in &exhaustion_leaves {
        let sources = referenced_selectors(conditions, leaf.condition());
        assert!(
            sources
                .iter()
                .any(|(_, source)| matches!(source, CleanupSelectorSource::IterationPhi { .. })),
            "exhaustion presence must be transported from the header phi: {sources:?}"
        );
    }
}

#[test]
fn loop_phi_transports_conditional_leaf_presence_across_jump_edges() {
    use lang_frontend::ownership_checking::{
        CleanupCondition, CleanupConditionId, CleanupConditions, CleanupSelectorSource,
        IterationPhiIncomingKind,
    };

    fn referenced_selectors(
        conditions: &CleanupConditions,
        condition: CleanupConditionId,
    ) -> Vec<CleanupSelectorSource> {
        let mut out = Vec::new();
        let mut pending = vec![condition];
        while let Some(condition) = pending.pop() {
            if let CleanupCondition::Choice { selector, branches } =
                conditions.get(condition).unwrap()
            {
                out.push(conditions.selector(*selector).unwrap().source());
                pending.extend(branches.iter().copied());
            }
        }
        out
    }

    for body in ["continue", "break"] {
        let source = format!(
            "fun run(flag: Boolean) {{\nval base: move () -> Unit = if (flag) (move {{}}) else (move {{}})\nval outer: move () -> Unit = move {{ var f: move () -> Unit = move {{ base() }}\nfor (_ in listOf(1)) {{ {body} }}\nval used = f() }}\nval used = outer() }}"
        );
        let (_, _, owned) = checked(&source);
        assert!(
            owned.diagnostics().is_empty(),
            "body={body}: {:?}",
            owned.diagnostics()
        );
        assert!(
            owned.deferred().is_empty(),
            "body={body}: {:?}",
            owned.deferred()
        );
        let conditions = owned.cleanup_conditions();
        let plan = &owned.iterations()[0];
        let kinds: Vec<_> = plan
            .closure_phi_incomings()
            .iter()
            .map(|incoming| incoming.kind())
            .collect();
        let jump = kinds.iter().any(|kind| {
            matches!(
                (body, kind),
                ("continue", IterationPhiIncomingKind::Continue(_))
                    | ("break", IterationPhiIncomingKind::Break(_))
            )
        });
        assert!(jump, "body={body} must record its jump edge: {kinds:?}");

        // 除 Entry（形成时写入控制选择）外，每条边都必须从 header phi presence 转发。
        for incoming in plan.closure_phi_incomings() {
            if incoming.kind() == IterationPhiIncomingKind::Entry {
                continue;
            }
            let leaves: Vec<_> = incoming
                .bindings()
                .iter()
                .flat_map(|binding| binding.origins())
                .flat_map(|origin| origin.environments())
                .flat_map(|environment| environment.sources())
                .flat_map(|source| source.captured())
                .collect();
            for leaf in leaves {
                let sources = referenced_selectors(conditions, leaf.condition());
                assert!(
                    sources
                        .iter()
                        .any(|source| matches!(source, CleanupSelectorSource::IterationPhi { .. })),
                    "body={body} {:?} leaf presence must be transported from the header phi: {sources:?}",
                    incoming.kind()
                );
            }
        }
    }
}

#[test]
fn enclosing_leaf_snapshot_locates_formation_choice_in_the_parent_capture() {
    use std::collections::BTreeMap;

    use lang_frontend::ownership_checking::{
        CleanupCaptureSlotId, CleanupCaptureValue, CleanupCondition, CleanupConditionId,
        CleanupConditions, CleanupOwnerValueId, CleanupSelectorId, CleanupSelectorSource,
        ClosureCaptureEffect, DropPoint, IterationCleanupAction as Action,
    };

    fn selected(
        conditions: &CleanupConditions,
        condition: CleanupConditionId,
        choices: &BTreeMap<CleanupSelectorId, usize>,
    ) -> bool {
        match conditions.get(condition).unwrap() {
            CleanupCondition::Always => true,
            CleanupCondition::Never => false,
            CleanupCondition::Choice { selector, branches } => {
                selected(conditions, branches[choices[selector]], choices)
            }
        }
    }

    #[derive(Clone)]
    struct Instance {
        environment: usize,
        choices: BTreeMap<CleanupSelectorId, usize>,
    }

    #[derive(Default)]
    struct Replay {
        next_environment: usize,
        owners: BTreeMap<CleanupOwnerValueId, Instance>,
        captures: BTreeMap<(usize, CleanupCaptureSlotId), Instance>,
        control: BTreeMap<CleanupSelectorId, usize>,
    }

    impl Replay {
        fn create(&mut self, steps: &[(DropPoint, Action)], owner: CleanupOwnerValueId) {
            assert!(steps.iter().any(|(_, action)| {
                matches!(action, Action::CreateClosureOwner { owner: formed, .. } if *formed == owner)
            }));
            self.next_environment += 1;
            self.owners.insert(
                owner,
                Instance {
                    environment: self.next_environment,
                    choices: BTreeMap::new(),
                },
            );
        }

        fn capture(
            &mut self,
            steps: &[(DropPoint, Action)],
            conditions: &CleanupConditions,
            owner: CleanupOwnerValueId,
        ) {
            let (_, Action::SaveClosureCapture { target, input, .. }) = steps
                .iter()
                .find(|(_, action)| {
                    matches!(action, Action::SaveClosureCapture { owner: receiver, .. } if *receiver == owner)
                })
                .unwrap()
            else {
                unreachable!()
            };
            assert!(selected(conditions, input.condition(), &self.control));
            assert_eq!(input.effect(), ClosureCaptureEffect::Move);
            let source = match input.value() {
                CleanupCaptureValue::Owner(source) => self.owners.remove(&source).unwrap(),
                CleanupCaptureValue::Environment { owner, slot, .. } => {
                    let environment = self.owners[&owner].environment;
                    self.captures.remove(&(environment, slot)).unwrap()
                }
                CleanupCaptureValue::Place(_) => panic!("owned capture must have a value source"),
            };
            let environment = self.owners[&owner].environment;
            assert!(
                self.captures
                    .insert((environment, *target), source)
                    .is_none()
            );
        }

        fn snapshot(
            &mut self,
            steps: &[(DropPoint, Action)],
            conditions: &CleanupConditions,
            owner: CleanupOwnerValueId,
        ) -> Instance {
            let (_, Action::SaveOwnerSnapshot { condition, value, .. }) = steps
                .iter()
                .find(|(_, action)| {
                    matches!(action, Action::SaveOwnerSnapshot { owner: saved, .. } if *saved == owner)
                })
                .unwrap()
            else {
                unreachable!()
            };
            assert!(condition.is_none_or(|guard| selected(conditions, guard, &self.control)));
            let snapshot = conditions.owner_snapshot(owner).unwrap();
            assert_eq!(*value, snapshot.value());
            let inputs: Vec<_> = snapshot
                .capture_inputs()
                .iter()
                .filter(|input| selected(conditions, input.condition(), &self.control))
                .collect();
            assert_eq!(inputs.len(), 1);
            let mut instance = self.owners.remove(&inputs[0].owner()).unwrap();
            let prior_choices = instance.choices.clone();
            for copy in snapshot.copies() {
                if !selected(conditions, copy.when(), &self.control) {
                    continue;
                }
                let choice = match copy.source_value() {
                    Some(CleanupCaptureValue::Environment { owner, slot, .. }) => {
                        let environment = if owner == inputs[0].owner() {
                            instance.environment
                        } else {
                            self.owners[&owner].environment
                        };
                        self.captures[&(environment, slot)].choices[&copy.source()]
                    }
                    Some(CleanupCaptureValue::Owner(owner)) => {
                        self.owners[&owner].choices[&copy.source()]
                    }
                    Some(CleanupCaptureValue::Place(_)) => {
                        panic!("selector copy cannot read a non-owning place")
                    }
                    None => prior_choices
                        .get(&copy.source())
                        .or_else(|| self.control.get(&copy.source()))
                        .copied()
                        .expect("snapshot copy must read a formed choice"),
                };
                instance.choices.insert(copy.target(), choice);
                self.control.insert(copy.target(), choice);
            }
            self.owners.insert(owner, instance.clone());
            instance
        }
    }

    let (sources, parsed, owned) = checked(
        "fun run(flag: Boolean) {\nval base: move () -> Unit = if (flag) (move {}) else (move {})\nval outer: move () -> Unit = move { val f: move () -> Unit = move { base() }\nval used = f() }\nval used = outer() }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    let (parent_owner, parent_slot, base_input) = owned
        .cleanup_steps()
        .iter()
        .find_map(|(_, action)| match action {
            Action::SaveClosureCapture {
                owner,
                target,
                input,
            } if matches!(input.value(), CleanupCaptureValue::Owner(_)) => {
                Some((*owner, *target, input.value()))
            }
            _ => None,
        })
        .expect("outer saves the formed base instance");
    let CleanupCaptureValue::Owner(base_owner) = base_input else {
        unreachable!()
    };
    assert!(owned.cleanup_steps().iter().any(|(_, action)| {
        matches!(action, Action::CreateClosureOwner { owner, .. } if *owner == parent_owner)
    }));
    let (inner_owner, inner_slot, inner_capture_input) = owned
        .cleanup_steps()
        .iter()
        .find_map(|(_, action)| match action {
            Action::SaveClosureCapture {
                owner,
                target,
                input,
            } if matches!(input.value(), CleanupCaptureValue::Environment { .. }) => {
                Some((*owner, *target, *input))
            }
            _ => None,
        })
        .expect("inner closure reads its immediate environment");
    let inner_input = inner_capture_input.value();
    assert!(owned.cleanup_steps().iter().any(|(_, action)| {
        matches!(action, Action::CreateClosureOwner { owner, .. } if *owner == inner_owner)
    }));
    let CleanupCaptureValue::Environment { owner, slot, .. } = inner_input else {
        unreachable!()
    };
    assert_eq!(owner, parent_owner);
    assert_eq!(slot, parent_slot);
    let f_snapshot = owned
        .cleanup_steps()
        .iter()
        .find_map(|(_, action)| match action {
            Action::SaveOwnerSnapshot { owner, value, .. }
                if sources.slice(parsed.ast().expressions().get(*value).unwrap().span())
                    == Ok("move { base() }") =>
            {
                Some(owned.cleanup_conditions().owner_snapshot(*owner).unwrap())
            }
            _ => None,
        })
        .expect("f saves the selected base leaf");
    assert_eq!(f_snapshot.copies().len(), 1);
    assert_eq!(f_snapshot.capture_inputs()[0].owner(), inner_owner);
    let f_copy = f_snapshot.copies()[0];
    assert_eq!(
        f_copy.source_value(),
        Some(CleanupCaptureValue::Environment {
            owner: inner_owner,
            source: inner_capture_input.source(),
            slot: inner_slot,
        })
    );
    let base_snapshot = owned
        .cleanup_conditions()
        .owner_snapshot(base_owner)
        .unwrap();
    assert_eq!(base_snapshot.copies().len(), 1);
    let base_copy = base_snapshot.copies()[0];
    assert_eq!(f_copy.source(), base_copy.target());
    let base_save = owned
        .cleanup_steps()
        .iter()
        .find(|(_, action)| matches!(action, Action::SaveOwnerSnapshot { owner, value, .. } if *owner == base_owner && *value == base_snapshot.value()))
        .expect("the base snapshot is saved after its RHS");
    assert_eq!(
        base_save.0,
        DropPoint::AfterExpression(base_snapshot.value())
    );
    assert_eq!(base_snapshot.capture_inputs().len(), 2);
    let selector = owned
        .cleanup_conditions()
        .selector(base_copy.source())
        .unwrap();
    assert_eq!(selector.branch_count(), 2);
    let CleanupSelectorSource::Control(control) = selector.source() else {
        panic!("base choice must come from the if expression");
    };
    assert_eq!(
        sources.slice(parsed.ast().expressions().get(control).unwrap().span()),
        Ok("if (flag) (move {}) else (move {})")
    );
    assert!(matches!(
        owned.cleanup_conditions().get(base_copy.when()),
        Some(CleanupCondition::Always)
    ));
    assert!(matches!(
        owned.cleanup_conditions().get(f_copy.when()),
        Some(CleanupCondition::Always)
    ));
    let steps = owned.cleanup_steps();
    let creation = |wanted: CleanupOwnerValueId| {
        steps.iter().position(|(_, action)| {
            matches!(action, Action::CreateClosureOwner { owner, .. } if *owner == wanted)
        }).unwrap()
    };
    let capture = |wanted: CleanupOwnerValueId| {
        steps.iter().position(|(_, action)| {
            matches!(action, Action::SaveClosureCapture { owner, .. } if *owner == wanted)
        }).unwrap()
    };
    assert_eq!(
        steps[creation(parent_owner)].0,
        steps[capture(parent_owner)].0
    );
    assert!(creation(parent_owner) < capture(parent_owner));
    assert_eq!(
        steps[creation(inner_owner)].0,
        steps[capture(inner_owner)].0
    );
    assert!(creation(inner_owner) < capture(inner_owner));
    let base_save_index = steps
        .iter()
        .position(|(_, action)| {
            matches!(action, Action::SaveOwnerSnapshot { owner, .. } if *owner == base_owner)
        })
        .unwrap();
    let f_save_index = steps
        .iter()
        .position(|(_, action)| {
            matches!(action, Action::SaveOwnerSnapshot { owner, .. } if *owner == f_snapshot.owner())
        })
        .unwrap();
    assert!(base_save_index < capture(parent_owner));
    assert!(capture(inner_owner) < f_save_index);
    assert_eq!(
        steps[f_save_index].0,
        DropPoint::AfterExpression(f_snapshot.value())
    );
    for arm in 0..2 {
        let formation_choices = BTreeMap::from([(base_copy.source(), arm)]);
        let selected_inputs: Vec<_> = base_snapshot
            .capture_inputs()
            .iter()
            .filter(|input| {
                selected(
                    owned.cleanup_conditions(),
                    input.condition(),
                    &formation_choices,
                )
            })
            .collect();
        assert_eq!(selected_inputs.len(), 1);
        let branch_owner = selected_inputs[0].owner();
        assert!(steps.iter().any(|(_, action)| {
            matches!(action, Action::CreateClosureOwner { owner, .. } if *owner == branch_owner)
        }));
        let Action::SaveClosureCapture { target, input, .. } = steps[capture(parent_owner)].1
        else {
            unreachable!()
        };
        assert_eq!(input.value(), CleanupCaptureValue::Owner(base_owner));
        assert!(selected(
            owned.cleanup_conditions(),
            input.condition(),
            &formation_choices
        ));
        assert_eq!(target, parent_slot);
        let Action::SaveClosureCapture { target, input, .. } = steps[capture(inner_owner)].1 else {
            unreachable!()
        };
        assert_eq!(input.value(), inner_input);
        assert_eq!(target, inner_slot);
        assert!(selected(
            owned.cleanup_conditions(),
            input.condition(),
            &formation_choices
        ));
        let CleanupCaptureValue::Environment {
            owner: source_owner,
            slot: source_slot,
            ..
        } = f_copy.source_value().unwrap()
        else {
            unreachable!()
        };
        assert_eq!(source_owner, inner_owner);
        assert_eq!(source_slot, inner_slot);
    }

    // 两次形成过程复用同一批静态 owner；只回放选中的形成、捕获、快照动作。
    // 保留第一次环境作隔离性探针；这里不模拟函数调用后的消费和释放。
    let conditions = owned.cleanup_conditions();
    let mut replay = Replay::default();
    let mut observations = Vec::new();
    for arm in 0..2 {
        replay.control.insert(base_copy.source(), arm);
        let branch_owner = base_snapshot
            .capture_inputs()
            .iter()
            .find(|input| selected(conditions, input.condition(), &replay.control))
            .unwrap()
            .owner();
        replay.create(steps, branch_owner);
        replay.snapshot(steps, conditions, base_owner);
        replay.create(steps, parent_owner);
        let parent_environment = replay.owners[&parent_owner].environment;
        replay.capture(steps, conditions, parent_owner);
        // 捕获已把选择搬进环境；后续快照不能依赖先前调用的临时控制状态。
        replay.control.remove(&base_copy.source());
        replay.control.remove(&base_copy.target());
        replay.create(steps, inner_owner);
        let inner_environment = replay.owners[&inner_owner].environment;
        replay.capture(steps, conditions, inner_owner);
        assert!(
            !replay
                .captures
                .contains_key(&(parent_environment, parent_slot))
        );
        let saved = replay.snapshot(steps, conditions, f_snapshot.owner());
        observations.push((
            parent_environment,
            inner_environment,
            saved.choices[&f_copy.target()],
        ));
    }
    assert_ne!(observations[0].0, observations[1].0);
    assert_ne!(observations[0].1, observations[1].1);
    assert_eq!(observations[0].2, 0);
    assert_eq!(observations[1].2, 1);
    assert_eq!(
        replay.captures[&(observations[0].1, inner_slot)].choices[&base_copy.target()],
        0
    );
}

#[test]
fn enclosing_leaf_snapshot_does_not_locate_outer_path_choice_in_the_capture() {
    use lang_frontend::ownership_checking::IterationCleanupAction as Action;

    let (sources, parsed, owned) = checked(
        "fun run(flag: Boolean) {\nval base: move () -> Unit = move {}\nval outer: move () -> Unit = if (flag) (move { val f: move () -> Unit = move { base() }\nval used = f() }) else (move {})\nval used = outer() }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    let f_snapshot = owned
        .cleanup_steps()
        .iter()
        .find_map(|(_, action)| match action {
            Action::SaveOwnerSnapshot { owner, value, .. }
                if sources.slice(parsed.ast().expressions().get(*value).unwrap().span())
                    == Ok("move { base() }") =>
            {
                Some(owned.cleanup_conditions().owner_snapshot(*owner).unwrap())
            }
            _ => None,
        })
        .expect("f saves the outer branch condition");
    assert!(!f_snapshot.copies().is_empty());
    assert!(
        f_snapshot
            .copies()
            .iter()
            .all(|copy| copy.source_value().is_none())
    );
}

#[test]
fn loop_phi_defers_known_or_opaque_leaf_enclosing_capture() {
    use lang_frontend::ownership_checking::OwnershipDeferredReason;

    let (_, _, owned) = checked(
        "fun make(): move () -> Unit = move {}\nfun run(flag: Boolean) {\nval base: move () -> Unit = if (flag) (move {}) else (make())\nval outer: move () -> Unit = move { var f: move () -> Unit = move { base() }\nfor (_ in listOf(1)) {}\nval used = f() }\nval used = outer() }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert_eq!(owned.deferred().len(), 1, "{:?}", owned.deferred());
    assert_eq!(
        owned.deferred()[0].reason(),
        OwnershipDeferredReason::AmbiguousClosureInstanceTransport
    );
    assert!(owned.iterations().is_empty());
    assert!(owned.cleanup_steps().is_empty());
    assert!(owned.drops().is_empty());
    assert!(owned.loan_ends().is_empty());
}
