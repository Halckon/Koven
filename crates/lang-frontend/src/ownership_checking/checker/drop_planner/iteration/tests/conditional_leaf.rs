use super::*;

#[test]
fn conditional_leaf_phi_replays_formed_instance_and_presence() {
    for tail in ["", "continue", "break"] {
        assert_conditional_leaf_replay(tail);
    }
}

fn assert_conditional_leaf_replay(tail: &str) {
    fn mentions(
        table: &CleanupConditions,
        condition: CleanupConditionId,
        wanted: crate::ownership_checking::CleanupSelectorId,
    ) -> bool {
        match table.get(condition).unwrap() {
            CleanupCondition::Choice { selector, branches } => {
                *selector == wanted
                    || branches
                        .iter()
                        .any(|branch| mentions(table, *branch, wanted))
            }
            CleanupCondition::Always | CleanupCondition::Never => false,
        }
    }

    fn replay_presence(
        table: &CleanupConditions,
        binding: &IterationPhiIncomingBinding,
        choices: &mut BTreeMap<crate::ownership_checking::CleanupSelectorId, usize>,
    ) {
        let before = choices.clone();
        let mut writes = vec![(
            binding.availability_selector(),
            usize::from(selected(table, binding.available_when(), &before)),
        )];
        for write in binding.selector_writes() {
            writes.push((
                write.target(),
                usize::from(selected(table, write.condition(), &before)),
            ));
        }
        for (target, value) in writes {
            choices.insert(target, value);
        }
    }

    struct EnvironmentInstance {
        closure: ExpressionId,
        choices: BTreeMap<crate::ownership_checking::CleanupSelectorId, usize>,
        captured: BTreeMap<usize, usize>,
    }

    #[derive(Default)]
    struct ReplayState {
        next_instance: usize,
        instances: BTreeMap<usize, EnvironmentInstance>,
        owners: BTreeMap<CleanupOwnerValueId, usize>,
        phi_slots: BTreeMap<crate::ownership_checking::CleanupCaptureSlotId, usize>,
        choices: BTreeMap<crate::ownership_checking::CleanupSelectorId, usize>,
    }

    impl ReplayState {
        fn pass_closure(
            &self,
            steps: &[(DropPoint, IterationCleanupAction)],
            call: ExpressionId,
        ) -> (ExpressionId, usize) {
            let actions = steps
                .iter()
                .filter(|(point, _)| *point == DropPoint::CallEntry(call))
                .map(|(_, action)| action)
                .collect::<Vec<_>>();
            let [action] = actions.as_slice() else {
                panic!("call entry must select exactly one concrete closure environment")
            };
            let IterationCleanupAction::PassClosureEnvironment { callee, closure } = **action
            else {
                unreachable!()
            };
            (
                closure.expect("this fixture has a unique concrete lambda"),
                self.owners[&callee],
            )
        }

        fn enter_closure(
            &mut self,
            steps: &[(DropPoint, IterationCleanupAction)],
            closure: ExpressionId,
            instance: usize,
        ) {
            let mut actions = steps
                .iter()
                .filter(|(point, _)| *point == DropPoint::LambdaEntry(closure))
                .map(|(_, action)| action);
            let Some(IterationCleanupAction::BindClosureEnvironment {
                owner,
                closure: entered,
            }) = actions.next()
            else {
                panic!("lambda entry must bind its incoming environment first")
            };
            assert_eq!(*entered, closure);
            assert!(self.instances.contains_key(&instance));
            assert!(self.owners.insert(*owner, instance).is_none());
            assert!(!actions.any(|action| matches!(
                action,
                IterationCleanupAction::BindClosureEnvironment { .. }
            )));
        }

        fn replay_point(
            &mut self,
            table: &CleanupConditions,
            steps: &[(DropPoint, IterationCleanupAction)],
            point: DropPoint,
        ) -> usize {
            let mut formation_actions = 0;
            for (_, action) in steps.iter().filter(|(at, _)| *at == point) {
                match action {
                    IterationCleanupAction::CreateClosureOwner { .. } => {
                        self.create(action);
                        formation_actions += 1;
                    }
                    IterationCleanupAction::SaveClosureCapture { .. } => {
                        self.capture(table, action);
                        formation_actions += 1;
                    }
                    IterationCleanupAction::SaveOwnerSnapshot { .. } => {
                        self.snapshot(table, action);
                        formation_actions += 1;
                    }
                    _ => {}
                }
            }
            formation_actions
        }

        fn create(&mut self, action: &IterationCleanupAction) -> usize {
            let IterationCleanupAction::CreateClosureOwner { owner, closure } = action else {
                panic!("formation must start with CreateClosureOwner")
            };
            self.next_instance += 1;
            self.instances.insert(
                self.next_instance,
                EnvironmentInstance {
                    closure: *closure,
                    choices: BTreeMap::new(),
                    captured: BTreeMap::new(),
                },
            );
            assert!(self.owners.insert(*owner, self.next_instance).is_none());
            self.next_instance
        }

        fn capture(&mut self, table: &CleanupConditions, action: &IterationCleanupAction) {
            let IterationCleanupAction::SaveClosureCapture {
                owner,
                target,
                input,
            } = action
            else {
                panic!("capture must use SaveClosureCapture")
            };
            assert!(selected(table, input.condition(), &self.choices));
            assert_eq!(input.effect(), ClosureCaptureEffect::Move);
            let source = match input.value() {
                CleanupCaptureValue::Owner(owner) => self.owners.remove(&owner).unwrap(),
                CleanupCaptureValue::Environment { owner, slot, .. } => {
                    let environment = self.owners[&owner];
                    let position = table.capture_slot_value(slot).unwrap().position();
                    self.instances
                        .get_mut(&environment)
                        .unwrap()
                        .captured
                        .remove(&position)
                        .unwrap()
                }
                CleanupCaptureValue::Place(_) => panic!("owned capture needs an instance"),
            };
            let environment = self.owners[owner];
            let position = table.capture_slot_value(*target).unwrap().position();
            assert!(
                self.instances
                    .get_mut(&environment)
                    .unwrap()
                    .captured
                    .insert(position, source)
                    .is_none()
            );
        }

        fn snapshot(&mut self, table: &CleanupConditions, action: &IterationCleanupAction) {
            let IterationCleanupAction::SaveOwnerSnapshot {
                condition,
                owner,
                value,
            } = action
            else {
                panic!("snapshot must use SaveOwnerSnapshot")
            };
            assert!(condition.is_none_or(|guard| selected(table, guard, &self.choices)));
            let snapshot = table.owner_snapshot(*owner).unwrap();
            assert_eq!(snapshot.value(), *value);
            let inputs = snapshot
                .capture_inputs()
                .iter()
                .filter(|input| selected(table, input.condition(), &self.choices))
                .collect::<Vec<_>>();
            assert_eq!(inputs.len(), 1);
            let instance = self.owners.remove(&inputs[0].owner()).unwrap();
            let before = self.choices.clone();
            let mut copies = Vec::new();
            for copy in snapshot.copies() {
                if !selected(table, copy.when(), &before) {
                    continue;
                }
                let value = match copy.source_value() {
                    Some(CleanupCaptureValue::Owner(owner)) => {
                        self.instances[&self.owners[&owner]].choices[&copy.source()]
                    }
                    Some(CleanupCaptureValue::Environment { owner, slot, .. }) => {
                        let parent = if owner == inputs[0].owner() {
                            instance
                        } else {
                            self.owners[&owner]
                        };
                        let position = table.capture_slot_value(slot).unwrap().position();
                        let child = self.instances[&parent].captured[&position];
                        self.instances[&child].choices[&copy.source()]
                    }
                    Some(CleanupCaptureValue::Place(_)) => {
                        panic!("snapshot selector cannot come from a place")
                    }
                    None => self.instances[&instance]
                        .choices
                        .get(&copy.source())
                        .or_else(|| before.get(&copy.source()))
                        .copied()
                        .expect("snapshot must read a formed selector"),
                };
                copies.push((copy.target(), value));
            }
            for (target, value) in copies {
                self.instances
                    .get_mut(&instance)
                    .unwrap()
                    .choices
                    .insert(target, value);
                self.choices.insert(target, value);
            }
            assert!(self.owners.insert(*owner, instance).is_none());
        }

        fn copy_phi(
            &mut self,
            table: &CleanupConditions,
            graph: &IterationCaptureGraph,
            edge: &crate::ownership_checking::IterationPhiIncoming,
            binding: &IterationPhiIncomingBinding,
        ) {
            let before = self.choices.clone();
            assert!(selected(table, edge.condition(), &before));
            let available = selected(table, binding.available_when(), &before);
            let values = binding
                .values()
                .iter()
                .filter(|value| selected(table, value.condition(), &before))
                .collect::<Vec<_>>();
            assert_eq!(values.len(), usize::from(available));
            let roots = binding
                .root_sources()
                .iter()
                .filter(|root| selected(table, root.condition(), &before))
                .collect::<Vec<_>>();
            assert_eq!(roots.len(), values.len());
            let old_owners = self.owners.clone();
            let mut source_writes = Vec::new();
            for origin in binding.origins() {
                if !selected(table, origin.condition(), &before) {
                    continue;
                }
                for environment in origin
                    .environments()
                    .iter()
                    .filter(|environment| selected(table, environment.condition(), &before))
                {
                    for source in environment
                        .sources()
                        .iter()
                        .filter(|source| selected(table, source.input().condition(), &before))
                    {
                        if let Some(target) = source.capture_slot() {
                            source_writes.push((
                                target,
                                read_captured(table, source, &old_owners, &self.instances),
                            ));
                        }
                    }
                }
            }
            let root_write = roots.first().map(|root| {
                assert_eq!(root.source(), values[0].source());
                let instance = old_owners[&root.source()];
                assert_eq!(
                    graph.nodes()[root.node()].closure(),
                    self.instances[&instance].closure
                );
                (root.source(), instance)
            });
            let mut new_choices = before;
            replay_presence(table, binding, &mut new_choices);
            for &slot in binding.capture_slots_to_clear() {
                self.phi_slots.remove(&slot);
            }
            for (slot, instance) in source_writes {
                assert!(self.phi_slots.insert(slot, instance).is_none());
            }
            if let Some((source, instance)) = root_write {
                if source != binding.target() {
                    self.owners.remove(&source).unwrap();
                }
                self.owners.insert(binding.target(), instance);
            } else {
                self.owners.remove(&binding.target());
            }
            self.choices = new_choices;
        }
    }

    fn read_captured(
        table: &CleanupConditions,
        source: &IterationPhiIncomingSource,
        owners: &BTreeMap<CleanupOwnerValueId, usize>,
        instances: &BTreeMap<usize, EnvironmentInstance>,
    ) -> usize {
        let (address, slot) = source.transport_read().unwrap();
        let address = table.instance_address(address).unwrap();
        let mut instance = owners[&address.root()];
        for &position in address.capture_path() {
            instance = instances[&instance].captured[&position];
        }
        let position = table.capture_slot_value(slot).unwrap().position();
        instances[&instance].captured[&position]
    }

    let mut sources = SourceMap::new();
    let source = sources
            .add_source(
                "conditional-leaf.ko",
                format!("fun run(flag: Boolean) {{ val base: move () -> Unit = if (flag) (move {{}}) else (move {{}})\nval outer: move () -> Unit = move {{ var f: move () -> Unit = move {{ base() }}\nfor (_ in listOf(1, 2)) {{ {tail} }}\nval used = f() }}\nval used = outer() }}"),
            )
            .unwrap();
    let parsed =
        crate::parser::parse_file(&sources, &crate::lexer::lex(&sources, source).unwrap()).unwrap();
    assert!(parsed.diagnostics().is_empty());
    let (names, types) = crate::type_checking::standard_environments();
    let names = crate::name_resolution::resolve_names(&sources, &parsed, &names).unwrap();
    let typed = crate::type_checking::check_types(&sources, &parsed, &names, &types).unwrap();
    assert!(typed.diagnostics().is_empty());
    let mut checker =
        super::super::super::super::Checker::new(&sources, &parsed, &names, &typed).unwrap();
    let capture_liveness = super::super::super::capture_liveness(&checker).unwrap();
    checker.expression_live_after = capture_liveness.expression_after;
    checker.statement_live_after = capture_liveness.statement_after;
    let mut state = super::super::super::super::State::default();
    for &root in parsed.roots() {
        checker.check_item(root, &mut state).unwrap();
    }
    assert!(checker.diagnostics.is_empty());
    let liveness = super::super::super::liveness::Liveness::build(&checker).unwrap();
    let (origins, captures) = super::super::super::origins::analyze(&checker).unwrap();
    let mut planner = super::super::super::DropPlanner::new(&checker, liveness, origins, captures);
    for &root in parsed.roots() {
        planner.item(root).unwrap();
    }
    assert!(planner.enclosing_capture_phi.is_none());
    let statement = checker
        .iterations
        .values()
        .next()
        .unwrap()
        .descriptor()
        .statement();
    let crate::parser::Statement::For { source, body, .. } =
        parsed.ast().statements().get(statement).unwrap().payload()
    else {
        panic!("expected for loop");
    };
    let entry = planner.loop_phi_incomings[&statement.index()]
        .iter()
        .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Entry)
        .unwrap();
    assert_eq!(entry.point(), DropPoint::AfterExpression(*source));
    assert_eq!(entry.boundary(), IterationPhiBoundary::Header);
    let parent = entry
        .bindings()
        .iter()
        .flat_map(|binding| binding.origins())
        .flat_map(|origin| origin.environments())
        .find(|environment| {
            environment
                .sources()
                .iter()
                .any(|source| !source.captured().is_empty())
        })
        .unwrap();
    let snapshot = planner.conditions.owner_snapshot(parent.owner()).unwrap();
    let copy = snapshot
        .copies()
        .iter()
        .find(|copy| copy.source_value().is_some())
        .unwrap();
    let nested = parent
        .sources()
        .iter()
        .flat_map(|source| source.captured())
        .collect::<Vec<_>>();
    assert_eq!(nested.len(), 2);
    for origin in &nested {
        assert!(mentions(
            &planner.conditions,
            origin.condition(),
            copy.target()
        ));
    }
    let source = parent
        .sources()
        .iter()
        .find(|source| !source.captured().is_empty())
        .unwrap();
    assert!(source.transport_value().is_some());
    let (address, slot) = source.transport_read().unwrap();
    let address = planner.conditions.instance_address(address).unwrap();
    assert_eq!(address.root(), parent.owner());
    assert!(address.capture_path().is_empty());
    let source_slot = planner.conditions.capture_slot_value(slot).unwrap();
    assert_ne!(source_slot.environment(), parent.owner());
    assert!(matches!(
        planner.conditions.owner_value(source_slot.environment()),
        Some(CleanupOwnerValue::Closure { .. })
    ));
    assert!(matches!(
        planner.conditions.owner_value(parent.owner()),
        Some(CleanupOwnerValue::Snapshot(_))
    ));
    assert_eq!(source_slot.source(), source.input().source());
    assert_eq!(source_slot.position(), 0);
    let header = planner.loop_phis[&statement.index()]
        .iter()
        .find(|phi| phi.boundary() == IterationPhiBoundary::Header)
        .unwrap();
    let entry_binding = entry
        .bindings()
        .iter()
        .find(|binding| binding.target() == header.owner())
        .unwrap();
    assert_eq!(entry_binding.values().len(), 1);
    assert_eq!(entry_binding.values()[0].source(), parent.owner());
    let jump_edge = planner.loop_phi_incomings[&statement.index()]
        .iter()
        .find(|incoming| {
            matches!(
                (tail, incoming.kind()),
                ("continue", IterationPhiIncomingKind::Continue(_))
                    | ("break", IterationPhiIncomingKind::Break(_))
                    | ("", IterationPhiIncomingKind::Fallthrough)
            )
        })
        .unwrap();
    let jump_target = if tail == "break" {
        IterationPhiBoundary::Exit
    } else {
        IterationPhiBoundary::Header
    };
    assert_eq!(jump_edge.boundary(), jump_target);
    match jump_edge.point() {
        DropPoint::AfterStatement(completed) if tail.is_empty() && completed == *body => {}
        DropPoint::ControlTransfer(control) if !tail.is_empty() => {
            assert_eq!(
                sources.slice(parsed.ast().expressions().get(control).unwrap().span()),
                Ok(tail)
            );
        }
        other => panic!("{tail} phi input has the wrong execution point: {other:?}"),
    }
    let carried = jump_edge
        .bindings()
        .iter()
        .find(|binding| {
            planner.loop_phis[&statement.index()].iter().any(|phi| {
                phi.boundary() == jump_target
                    && phi.symbol() == header.symbol()
                    && phi.owner() == binding.target()
            })
        })
        .unwrap();
    assert_eq!(carried.values().len(), 1);
    assert_eq!(carried.values()[0].source(), header.owner());
    let forwarded = carried
        .origins()
        .iter()
        .flat_map(|origin| origin.environments())
        .find(|environment| environment.owner() == header.owner())
        .unwrap();
    assert!(
        planner
            .conditions
            .owner_snapshot(forwarded.owner())
            .is_none()
    );
    let child_selectors = header
        .origins()
        .iter()
        .flat_map(|origin| origin.sources())
        .flat_map(|source| source.captured())
        .map(|&index| header.origins()[index].selector())
        .collect::<Vec<_>>();
    assert_eq!(child_selectors.len(), 2);
    assert!(child_selectors.iter().all(|selector| matches!(
        planner.conditions.selector(*selector).unwrap().source(),
        CleanupSelectorSource::IterationPhi {
            boundary: IterationPhiBoundary::Header,
            ..
        }
    )));
    let forwarded_children = forwarded
        .sources()
        .iter()
        .flat_map(|source| source.captured())
        .collect::<Vec<_>>();
    assert_eq!(forwarded_children.len(), 2);
    for child in &forwarded_children {
        assert!(!mentions(
            &planner.conditions,
            child.condition(),
            copy.target()
        ));
        assert!(child_selectors.iter().any(|&selector| mentions(
            &planner.conditions,
            child.condition(),
            selector
        )));
    }
    let exhaustion = planner.loop_phi_incomings[&statement.index()]
        .iter()
        .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Exhaustion)
        .unwrap();
    assert_eq!(exhaustion.point(), DropPoint::LoopExit(statement));
    assert_eq!(exhaustion.boundary(), IterationPhiBoundary::Exit);
    let exit = planner.loop_phis[&statement.index()]
        .iter()
        .find(|phi| phi.boundary() == IterationPhiBoundary::Exit)
        .unwrap();
    let exit_binding = exhaustion
        .bindings()
        .iter()
        .find(|binding| binding.target() == exit.owner())
        .unwrap();
    assert_eq!(exit_binding.values().len(), 1);
    assert_eq!(exit_binding.values()[0].source(), header.owner());
    let exit_environment = exit_binding
        .origins()
        .iter()
        .flat_map(|origin| origin.environments())
        .find(|environment| environment.owner() == header.owner())
        .unwrap();
    assert_eq!(exit_environment.instance_root(), header.owner());
    let exit_source = exit_environment
        .sources()
        .iter()
        .find(|source| !source.captured().is_empty())
        .unwrap();
    let (exit_address, exit_slot) = exit_source.transport_read().unwrap();
    let exit_address = planner.conditions.instance_address(exit_address).unwrap();
    assert_eq!(exit_address.root(), header.owner());
    assert!(exit_address.capture_path().is_empty());
    let exit_slot = planner.conditions.capture_slot_value(exit_slot).unwrap();
    assert_eq!(exit_slot.environment(), header.owner());
    assert_eq!(exit_slot.source(), exit_source.input().source());
    assert_eq!(exit_slot.position(), 0);
    let exit_children = exit_binding
        .origins()
        .iter()
        .flat_map(|origin| origin.environments())
        .flat_map(|environment| environment.sources())
        .flat_map(|source| source.captured())
        .collect::<Vec<_>>();
    assert_eq!(exit_children.len(), 2);
    let base_snapshot = planner
        .cleanup
        .iter()
        .filter_map(|(_, action)| match action {
            IterationCleanupAction::SaveOwnerSnapshot { owner, .. } => {
                planner.conditions.owner_snapshot(*owner)
            }
            _ => None,
        })
        .find(|snapshot| {
            snapshot
                .copies()
                .iter()
                .any(|candidate| candidate.target() == copy.source())
        })
        .unwrap();
    let base_copy = base_snapshot
        .copies()
        .iter()
        .find(|candidate| candidate.target() == copy.source())
        .unwrap();
    let (outer_owner, outer_slot) = planner
        .cleanup
        .iter()
        .find_map(|(_, action)| match action {
            IterationCleanupAction::SaveClosureCapture {
                owner,
                target,
                input,
            } if input.value() == CleanupCaptureValue::Owner(base_snapshot.owner()) => {
                Some((*owner, *target))
            }
            _ => None,
        })
        .unwrap();
    let (inner_owner, inner_slot) = planner
        .cleanup
        .iter()
        .find_map(|(_, action)| match action {
            IterationCleanupAction::SaveClosureCapture {
                owner,
                target,
                input,
            } if matches!(
                input.value(),
                CleanupCaptureValue::Environment { owner: source, slot, .. }
                    if source == outer_owner && slot == outer_slot
            ) =>
            {
                Some((*owner, *target))
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(inner_owner, source_slot.environment());
    assert_eq!(snapshot.capture_inputs().len(), 1);
    assert_eq!(snapshot.capture_inputs()[0].owner(), inner_owner);
    assert_eq!(
        copy.source_value(),
        Some(CleanupCaptureValue::Environment {
            owner: inner_owner,
            source: source.input().source(),
            slot: inner_slot,
        })
    );
    let outer_position = planner
        .conditions
        .capture_slot_value(outer_slot)
        .unwrap()
        .position();
    let inner_position = planner
        .conditions
        .capture_slot_value(inner_slot)
        .unwrap()
        .position();
    let graph = &planner.loop_capture_graphs[&statement.index()];
    let f_call = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| (sources.slice(node.span()) == Ok("f()")).then_some(id))
        .unwrap();
    let call_drops = planner
        .cleanup
        .iter()
        .filter(|(point, _)| *point == DropPoint::CallReturn(f_call))
        .map(|(_, action)| *action)
        .collect::<Vec<_>>();
    let [
        IterationCleanupAction::Drop(child_drop),
        IterationCleanupAction::Drop(root_drop),
    ] = call_drops.as_slice()
    else {
        panic!("f() must release its captured leaf before its environment: {call_drops:?}")
    };
    let DropTarget::Captured {
        closure: released_closure,
        source: released_source,
        ..
    } = child_drop.target()
    else {
        panic!("the first f() cleanup must release the captured leaf")
    };
    assert_eq!(root_drop.target(), DropTarget::Named(exit.symbol()));
    assert_eq!(
        sources.slice(names.symbols()[exit.symbol().index()].span()),
        Ok("f")
    );
    assert_eq!(child_drop.owner(), None);
    assert_eq!(root_drop.owner(), Some(exit.owner()));
    let child_address = planner
        .conditions
        .instance_address(child_drop.instance_address().unwrap())
        .unwrap();
    assert_eq!(child_address.root(), exit.owner());
    assert!(child_address.capture_path().is_empty());
    let child_slot = planner
        .conditions
        .capture_slot_value(child_drop.capture_slot().unwrap())
        .unwrap();
    let formed_slot = planner.conditions.capture_slot_value(inner_slot).unwrap();
    assert_eq!(child_slot.environment(), exit.owner());
    assert_eq!(child_slot.closure(), released_closure);
    assert_eq!(child_slot.source(), released_source);
    assert_eq!(child_slot.closure(), formed_slot.closure());
    assert_eq!(child_slot.source(), formed_slot.source());
    assert_eq!(child_slot.position(), inner_position);
    let creation = |wanted| {
        planner
            .cleanup
            .iter()
            .enumerate()
            .find_map(|(index, (_, action))| match action {
                IterationCleanupAction::CreateClosureOwner { owner, .. } if *owner == wanted => {
                    Some((index, *action))
                }
                _ => None,
            })
            .unwrap()
    };
    let snapshot_action = |wanted| {
        planner
            .cleanup
            .iter()
            .find_map(|(_, action)| match action {
                IterationCleanupAction::SaveOwnerSnapshot { owner, .. } if *owner == wanted => {
                    Some(*action)
                }
                _ => None,
            })
            .unwrap()
    };
    let (outer_create_index, outer_create) = creation(outer_owner);
    let IterationCleanupAction::CreateClosureOwner {
        closure: outer_closure,
        ..
    } = outer_create
    else {
        unreachable!()
    };
    let outer_call = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| (sources.slice(node.span()) == Ok("outer()")).then_some(id))
        .unwrap();
    let (inner_create_index, _) = creation(inner_owner);
    let (outer_capture_index, _) = planner
        .cleanup
        .iter()
        .enumerate()
        .find_map(|(index, (_, action))| match action {
            IterationCleanupAction::SaveClosureCapture { owner, target, .. }
                if *owner == outer_owner && *target == outer_slot =>
            {
                Some((index, *action))
            }
            _ => None,
        })
        .unwrap();
    let (inner_capture_index, _) = planner
        .cleanup
        .iter()
        .enumerate()
        .find_map(|(index, (_, action))| match action {
            IterationCleanupAction::SaveClosureCapture { owner, target, .. }
                if *owner == inner_owner && *target == inner_slot =>
            {
                Some((index, *action))
            }
            _ => None,
        })
        .unwrap();
    assert!(outer_create_index < outer_capture_index);
    assert!(inner_create_index < inner_capture_index);
    assert_eq!(
        planner.cleanup[outer_create_index].0,
        planner.cleanup[outer_capture_index].0
    );
    assert_eq!(
        planner.cleanup[inner_create_index].0,
        planner.cleanup[inner_capture_index].0
    );
    let base_save = snapshot_action(base_snapshot.owner());
    let inner_save = snapshot_action(parent.owner());
    let base_point = planner
        .cleanup
        .iter()
        .find(|(_, action)| *action == base_save)
        .unwrap()
        .0;
    let outer_point = planner.cleanup[outer_create_index].0;
    let inner_point = planner.cleanup[inner_create_index].0;
    let f_statement = parsed
        .ast()
        .statements()
        .iter()
        .find_map(|(id, node)| {
            (matches!(
                node.payload(),
                crate::parser::Statement::LocalVariable { .. }
            ) && sources
                .slice(node.span())
                .is_ok_and(|text| text.starts_with("var f:")))
            .then_some(id)
        })
        .unwrap();
    let outer_snapshot_owner = planner
        .cleanup
        .iter()
        .find_map(|(point, action)| match action {
            IterationCleanupAction::SaveOwnerSnapshot { owner, .. }
                if *point == outer_point
                    && planner
                        .conditions
                        .owner_snapshot(*owner)
                        .is_some_and(|snapshot| {
                            snapshot
                                .capture_inputs()
                                .iter()
                                .any(|input| input.owner() == outer_owner)
                        }) =>
            {
                Some(*owner)
            }
            _ => None,
        })
        .unwrap();
    assert!(planner.cleanup.iter().any(|(point, action)| {
        *point == DropPoint::CallEntry(outer_call)
            && matches!(
                action,
                IterationCleanupAction::PassClosureEnvironment { callee, closure }
                    if *callee == outer_snapshot_owner && *closure == Some(outer_closure)
            )
    }));
    let inner_save_index = planner
        .cleanup
        .iter()
        .position(|(_, action)| *action == inner_save)
        .unwrap();
    assert!(inner_capture_index < inner_save_index);
    let mut next_instance = 0;
    let mut prior_inner = None;
    for arm in 0..2 {
        let control = BTreeMap::from([(base_copy.source(), arm)]);
        let branch = base_snapshot
            .capture_inputs()
            .iter()
            .filter(|input| selected(&planner.conditions, input.condition(), &control))
            .collect::<Vec<_>>();
        assert_eq!(branch.len(), 1);
        let branch_owner = branch[0].owner();
        let Some(CleanupOwnerValue::Closure {
            expression: branch_closure,
            ..
        }) = planner.conditions.owner_value(branch_owner)
        else {
            panic!("selected base branch must form a closure")
        };
        let (branch_index, branch_create) = creation(branch_owner);
        assert!(
            planner
                .cleanup
                .iter()
                .enumerate()
                .any(|(index, (_, action))| { index > branch_index && *action == base_save })
        );
        let branch_point = planner.cleanup[branch_index].0;
        let (
            DropPoint::AfterExpression(branch_expression),
            DropPoint::AfterExpression(base_expression),
            DropPoint::AfterExpression(outer_expression),
            DropPoint::AfterExpression(inner_expression),
        ) = (branch_point, base_point, outer_point, inner_point)
        else {
            panic!("formation actions must be attached to completed expressions")
        };
        assert_eq!(branch_expression, *branch_closure);
        assert_eq!(base_expression, base_snapshot.value());
        let branch_span = parsed
            .ast()
            .expressions()
            .get(branch_expression)
            .unwrap()
            .span();
        let base_span = parsed
            .ast()
            .expressions()
            .get(base_expression)
            .unwrap()
            .span();
        let outer_span = parsed
            .ast()
            .expressions()
            .get(outer_expression)
            .unwrap()
            .span();
        let inner_span = parsed
            .ast()
            .expressions()
            .get(inner_expression)
            .unwrap()
            .span();
        assert!(base_span.start() <= branch_span.start() && branch_span.end() <= base_span.end());
        assert!(base_span.end() <= outer_span.start());
        assert!(outer_span.start() <= inner_span.start() && inner_span.end() <= outer_span.end());
        assert!(matches!(
            branch_create,
            IterationCleanupAction::CreateClosureOwner { .. }
        ));
        let mut replay = ReplayState {
            next_instance,
            choices: control,
            ..ReplayState::default()
        };
        assert_eq!(
            replay.replay_point(&planner.conditions, &planner.cleanup, branch_point),
            1
        );
        let base_instance = replay.owners[&branch_owner];
        assert_eq!(
            replay.replay_point(&planner.conditions, &planner.cleanup, base_point),
            1
        );
        assert_eq!(
            replay.replay_point(&planner.conditions, &planner.cleanup, outer_point),
            3
        );
        let outer_instance = replay.owners[&outer_snapshot_owner];
        assert_eq!(
            replay.instances[&outer_instance].captured[&outer_position],
            base_instance
        );
        // 调用点按当前 snapshot owner 取实例，入口再绑定 body 环境 owner。
        let (entered, incoming) = replay.pass_closure(&planner.cleanup, outer_call);
        assert_eq!(entered, outer_closure);
        assert_eq!(incoming, outer_instance);
        replay.enter_closure(&planner.cleanup, entered, incoming);
        assert_eq!(replay.owners[&outer_owner], outer_instance);
        replay.choices.remove(&base_copy.source());
        replay.choices.remove(&base_copy.target());
        assert_eq!(
            replay.replay_point(&planner.conditions, &planner.cleanup, inner_point),
            3
        );
        let inner_instance = replay.owners[&parent.owner()];
        if let Some(prior) = prior_inner.replace(inner_instance) {
            assert_ne!(
                prior, inner_instance,
                "the same static lambda forms a new instance"
            );
        }
        assert!(
            !replay.instances[&outer_instance]
                .captured
                .contains_key(&outer_position)
        );
        assert_eq!(
            replay.instances[&inner_instance].captured[&inner_position],
            base_instance
        );
        assert_eq!(replay.owners[&parent.owner()], inner_instance);
        assert_eq!(
            replay.instances[&inner_instance].choices[&copy.target()],
            arm
        );
        next_instance = replay.next_instance;
        let entry_presence = nested
            .iter()
            .map(|origin| selected(&planner.conditions, origin.condition(), &replay.choices))
            .collect::<Vec<_>>();
        assert_eq!(entry_presence.iter().filter(|&&present| present).count(), 1);
        let selected_leaf = nested
            .iter()
            .find(|origin| selected(&planner.conditions, origin.condition(), &replay.choices))
            .unwrap();
        assert_eq!(
            graph.nodes()[selected_leaf.node()].closure(),
            *branch_closure
        );
        assert_eq!(selected_leaf.environments().len(), 1);
        assert_eq!(
            selected_leaf.environments()[0].instance_root(),
            parent.owner()
        );
        assert_eq!(
            selected_leaf.environments()[0].capture_path(),
            &[inner_position]
        );
        assert_eq!(
            read_captured(
                &planner.conditions,
                source,
                &replay.owners,
                &replay.instances,
            ),
            base_instance
        );
        let assert_no_early_release = |point: DropPoint,
                                       choices: &BTreeMap<
            crate::ownership_checking::CleanupSelectorId,
            usize,
        >| {
            for (_, action) in planner.cleanup.iter().filter(|(at, _)| *at == point) {
                let fact = match action {
                    IterationCleanupAction::Drop(fact)
                    | IterationCleanupAction::ReleaseClosureInstances { root: fact, .. } => fact,
                    _ => continue,
                };
                if !fact
                    .condition()
                    .is_none_or(|guard| selected(&planner.conditions, guard, choices))
                {
                    continue;
                }
                let address_root = fact
                    .instance_address()
                    .map(|address| planner.conditions.instance_address(address).unwrap().root());
                assert!(
                    ![branch_owner, parent.owner(), header.owner(), exit.owner()]
                        .into_iter()
                        .any(|owner| fact.owner() == Some(owner) || address_root == Some(owner))
                        && !matches!(fact.target(), DropTarget::Named(symbol) if symbol == header.symbol())
                        && !matches!(fact.target(), DropTarget::Captured { closure, .. } if closure == inner_expression),
                    "{tail}: active f or its leaf released at {point:?}: {fact:?}"
                );
            }
        };
        assert_no_early_release(inner_point, &replay.choices);
        assert_no_early_release(DropPoint::AfterStatement(f_statement), &replay.choices);
        assert_no_early_release(entry.point(), &replay.choices);
        replay.copy_phi(&planner.conditions, graph, entry, entry_binding);
        assert!(!replay.owners.contains_key(&parent.owner()));
        assert_eq!(replay.owners[&header.owner()], inner_instance);
        let header_slot = source.capture_slot().unwrap();
        assert!(
            entry_binding
                .capture_slots_to_clear()
                .contains(&header_slot)
        );
        assert_eq!(replay.phi_slots[&header_slot], base_instance);
        replay.choices.remove(&copy.target());
        for (origin, present) in nested.iter().zip(&entry_presence) {
            assert_eq!(replay.choices[&origin.target()], usize::from(*present));
        }
        let jump_source = forwarded
            .sources()
            .iter()
            .find(|source| !source.captured().is_empty())
            .unwrap();
        let formed_instances = replay.instances.len();
        for round in 0..if tail == "break" { 1 } else { 2 } {
            assert_no_early_release(jump_edge.point(), &replay.choices);
            let jump_presence = forwarded_children
                .iter()
                .map(|origin| selected(&planner.conditions, origin.condition(), &replay.choices))
                .collect::<Vec<_>>();
            assert_eq!(jump_presence, entry_presence, "{tail}, round {round}");
            assert_eq!(replay.owners[&header.owner()], inner_instance);
            assert_eq!(replay.instances.len(), formed_instances);
            assert_eq!(
                read_captured(
                    &planner.conditions,
                    jump_source,
                    &replay.owners,
                    &replay.instances,
                ),
                base_instance
            );
            replay.copy_phi(&planner.conditions, graph, jump_edge, carried);
            if tail != "break" {
                assert_eq!(replay.owners[&header.owner()], inner_instance);
                assert_eq!(replay.phi_slots[&header_slot], base_instance);
                for (origin, present) in nested.iter().zip(&entry_presence) {
                    assert_eq!(replay.choices[&origin.target()], usize::from(*present));
                }
            }
        }
        if tail == "break" {
            assert!(!replay.owners.contains_key(&header.owner()));
            let jump_slot = jump_source.capture_slot().unwrap();
            assert!(carried.capture_slots_to_clear().contains(&jump_slot));
            assert_eq!(replay.phi_slots[&jump_slot], base_instance);
            for (origin, present) in forwarded_children.iter().zip(&entry_presence) {
                assert_eq!(replay.choices[&origin.target()], usize::from(*present));
            }
        } else {
            assert_no_early_release(exhaustion.point(), &replay.choices);
            let exit_presence = exit_children
                .iter()
                .map(|origin| selected(&planner.conditions, origin.condition(), &replay.choices))
                .collect::<Vec<_>>();
            assert_eq!(exit_presence, entry_presence);
            assert_eq!(
                read_captured(
                    &planner.conditions,
                    exit_source,
                    &replay.owners,
                    &replay.instances,
                ),
                base_instance
            );
            replay.copy_phi(&planner.conditions, graph, exhaustion, exit_binding);
            let exit_capture_slot = exit_source.capture_slot().unwrap();
            assert!(
                exit_binding
                    .capture_slots_to_clear()
                    .contains(&exit_capture_slot)
            );
            assert_eq!(replay.phi_slots[&exit_capture_slot], base_instance);
            for (origin, present) in exit_children.iter().zip(&entry_presence) {
                assert_eq!(replay.choices[&origin.target()], usize::from(*present));
            }
        }
        assert!(!replay.owners.contains_key(&header.owner()));
        assert_eq!(replay.owners[&exit.owner()], inner_instance);
        assert_eq!(
            replay.instances[&inner_instance].captured[&inner_position],
            base_instance
        );
        assert!(child_drop.condition().is_none_or(|guard| selected(
            &planner.conditions,
            guard,
            &replay.choices
        )));
        assert!(root_drop.condition().is_none_or(|guard| selected(
            &planner.conditions,
            guard,
            &replay.choices
        )));
        let root_instance = replay.owners.remove(&exit.owner()).unwrap();
        assert_eq!(root_instance, inner_instance);
        let child_instance = replay
            .instances
            .get_mut(&root_instance)
            .unwrap()
            .captured
            .remove(&child_slot.position())
            .unwrap();
        assert_eq!(child_instance, base_instance);
        assert_eq!(replay.instances[&child_instance].closure, *branch_closure);
        assert!(replay.instances[&root_instance].captured.is_empty());
    }
}
