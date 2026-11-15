//! Concrete instance, selector and phi replay for the conditional-leaf oracle.
use super::*;

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

pub(super) struct EnvironmentInstance {
    pub(super) closure: ExpressionId,
    pub(super) choices: BTreeMap<crate::ownership_checking::CleanupSelectorId, usize>,
    pub(super) captured: BTreeMap<usize, usize>,
}

#[derive(Default)]
pub(super) struct ReplayState {
    pub(super) next_instance: usize,
    pub(super) instances: BTreeMap<usize, EnvironmentInstance>,
    pub(super) owners: BTreeMap<CleanupOwnerValueId, usize>,
    pub(super) phi_slots: BTreeMap<crate::ownership_checking::CleanupCaptureSlotId, usize>,
    pub(super) choices: BTreeMap<crate::ownership_checking::CleanupSelectorId, usize>,
}

impl ReplayState {
    pub(super) fn pass_closure(
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
        let IterationCleanupAction::PassClosureEnvironment { callee, closure } = **action else {
            unreachable!()
        };
        (
            closure.expect("this fixture has a unique concrete lambda"),
            self.owners[&callee],
        )
    }

    pub(super) fn enter_closure(
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

    pub(super) fn replay_point(
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

    pub(super) fn copy_phi(
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

pub(super) fn read_captured(
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
