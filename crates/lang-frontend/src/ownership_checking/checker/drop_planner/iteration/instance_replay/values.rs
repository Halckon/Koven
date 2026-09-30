//! 普通源码求值的资源句柄；形成事实只创建 closure，其他值由求值器交付。
use super::*;
use crate::ownership_checking::{
    CleanupOwnerValue, IterationPhiIncoming, IterationPhiIncomingOrigin,
};

impl Replay {
    pub(super) fn form_place(&mut self, source: crate::ownership_checking::ClosureCaptureSource) {
        let instance = self.instances.len();
        self.instances.push(Instance {
            closure: None,
            captures: BTreeMap::new(),
            copied_captures: BTreeSet::new(),
            cleared_captures: BTreeSet::new(),
            shared: BTreeMap::new(),
            choices: Choices::new(),
            snapshot_sources: BTreeMap::new(),
            live: true,
        });
        assert!(self.places.insert(source, instance).is_none());
    }

    /// 所有路径均从本次根句柄读取，不按静态 lambda 合并实例。
    fn environment_at(&self, root: CleanupOwnerValueId, path: &[usize]) -> usize {
        let mut instance = self.owners[&root];
        for position in path {
            assert!(self.instances[instance].live);
            instance = self.instances[instance].captures[position];
        }
        assert!(self.instances[instance].live);
        instance
    }

    pub(super) fn take_captured_root(
        &mut self,
        table: &CleanupConditions,
        fact: DropFact,
    ) -> Option<usize> {
        let address = table
            .instance_address(
                fact.instance_address()
                    .expect("captured drop needs its runtime address"),
            )
            .unwrap();
        let parent = self.environment_at(address.root(), address.capture_path());
        let slot_id = fact.capture_slot().unwrap();
        let slot = table.capture_slot_value(slot_id).unwrap();
        let DropTarget::Captured {
            closure, source, ..
        } = fact.target()
        else {
            unreachable!()
        };
        assert_eq!(slot.closure(), closure);
        assert_eq!(slot.source(), source);
        assert_eq!(self.instances[parent].closure, Some(closure));
        if self.instances[parent]
            .cleared_captures
            .contains(&slot.position())
        {
            assert!(
                !self.instances[parent]
                    .captures
                    .contains_key(&slot.position())
            );
            assert!(
                !self.cleanup_edges.contains_key(&(parent, slot.position())),
                "captured slot released twice at one cleanup point"
            );
            return None;
        }
        let value = self.instances[parent]
            .captures
            .remove(&slot.position())
            .expect("captured value already consumed");
        assert!(
            self.instances[parent]
                .cleared_captures
                .insert(slot.position())
        );
        assert!(
            self.cleanup_edges
                .insert((parent, slot.position()), value)
                .is_none()
        );
        if self.flattened_slots.contains(&slot_id) {
            assert_eq!(
                self.phi_slots.get(&slot_id),
                Some(&value),
                "flattened slot must alias the addressed instance"
            );
        }
        Some(value)
    }

    /// 读取所有展平输入之后才清空并提交，避免 header 自复制读到半更新状态。
    pub(super) fn read_phi_slots(
        &self,
        table: &CleanupConditions,
        edge: &IterationPhiIncoming,
    ) -> BTreeMap<CleanupCaptureSlotId, usize> {
        let clear = edge
            .bindings()
            .iter()
            .flat_map(|binding| binding.capture_slots_to_clear())
            .copied()
            .collect::<BTreeSet<_>>();
        let mut writes = BTreeMap::new();
        let mut pending: Vec<&IterationPhiIncomingOrigin> = edge
            .bindings()
            .iter()
            .flat_map(|binding| binding.origins())
            .collect();
        while let Some(origin) = pending.pop() {
            if !selected(table, origin.condition(), &self.choices) {
                continue;
            }
            for environment in origin.environments() {
                if !selected(table, environment.condition(), &self.choices) {
                    continue;
                }
                for source in environment.sources() {
                    if !selected(table, source.input().condition(), &self.choices) {
                        continue;
                    }
                    let Some(slot) = source.capture_slot() else {
                        assert!(source.target().is_none());
                        continue;
                    };
                    let (address, source_slot) = source.transport_read().unwrap();
                    let address = table.instance_address(address).unwrap();
                    let parent = self.environment_at(address.root(), address.capture_path());
                    let input = table.capture_slot_value(source_slot).unwrap();
                    let output = table.capture_slot_value(slot).unwrap();
                    assert_eq!(self.instances[parent].closure, Some(input.closure()));
                    assert_eq!(
                        (input.closure(), input.source(), input.position()),
                        (output.closure(), output.source(), output.position())
                    );
                    assert!(matches!(
                        table.owner_value(source.target().unwrap()),
                        Some(CleanupOwnerValue::IterationPhiSourceOwner { environment, closure, source, .. })
                            if (*environment, *closure, *source) == (output.environment(), output.closure(), output.source())
                    ));
                    assert!(clear.contains(&slot));
                    let value = self.saved_value(parent, input.position());
                    assert!(self.instances[value].live);
                    assert!(writes.insert(slot, value).is_none());
                    pending.extend(source.captured());
                }
            }
        }
        writes
    }

    pub(super) fn form_resource(
        &mut self,
        table: &CleanupConditions,
        owner: CleanupOwnerValueId,
        expression: ExpressionId,
    ) {
        assert!(
            matches!(table.owner_value(owner), Some(CleanupOwnerValue::Expression { expression: found, .. }) if *found == expression)
        );
        let instance = self.instances.len();
        self.instances.push(Instance {
            closure: None,
            captures: BTreeMap::new(),
            copied_captures: BTreeSet::new(),
            cleared_captures: BTreeSet::new(),
            shared: BTreeMap::new(),
            choices: self.choices.clone(),
            snapshot_sources: BTreeMap::new(),
            live: true,
        });
        assert!(self.owners.insert(owner, instance).is_none());
        self.result = Some(owner);
    }
}
