//! Shared loan 按环境实例/原始槽保存 source；清理点内保留已释放边的读取凭据。
use super::*;
use crate::ownership_checking::{CleanupInstanceAddressId, ClosureCaptureSource};

impl Replay {
    /// 不扫描全局 retained；只有本次根遍历实际结束的最后 loan 才能交付源义务。
    pub(super) fn release_retained_sources(
        &mut self,
        facts: &DropPlan,
        receipts: Vec<(usize, bool)>,
    ) {
        let mut pending = std::collections::VecDeque::from(receipts);
        while let Some((source, last)) = pending.pop_front() {
            if !last || !self.retained.remove(&source) {
                continue;
            }
            assert!(!self.loans.values().any(|&value| value == source));
            let first_loan = self.loan_ends.len();
            if self.instances[source].closure.is_some() {
                self.release(facts, ClosureReleaseLayout::File, source);
            } else {
                self.finish(source);
            }
            // retained closure 自身也可能持有其它 retained source；级联按形成的凭据顺序。
            pending.extend(
                self.loan_ends[first_loan..]
                    .iter()
                    .map(|loan| self.ended_captures[loan]),
            );
        }
    }

    pub(super) fn saved_value(&self, instance: usize, position: usize) -> usize {
        let value = &self.instances[instance];
        assert!(!(value.captures.contains_key(&position) && value.shared.contains_key(&position)));
        *value
            .captures
            .get(&position)
            .or_else(|| value.shared.get(&position))
            .expect("capture slot must hold a value")
    }

    pub(super) fn save_shared_capture(
        &mut self,
        table: &CleanupConditions,
        instance: usize,
        position: usize,
        value: CleanupCaptureValue,
    ) {
        let source = match value {
            CleanupCaptureValue::Owner(owner) => self.owners[&owner],
            CleanupCaptureValue::Environment {
                owner,
                slot,
                source,
            } => {
                let parent = self.owners[&owner];
                let slot = table.capture_slot_value(slot).unwrap();
                assert_eq!(slot.source(), source);
                assert_eq!(self.instances[parent].closure, Some(slot.closure()));
                self.saved_value(parent, slot.position())
            }
            CleanupCaptureValue::Place(source) => self.places[&source],
        };
        assert!(self.instances[source].live && source < instance);
        assert!(
            self.instances[instance]
                .shared
                .insert(position, source)
                .is_none()
        );
        assert!(self.loans.insert((instance, position), source).is_none());
    }

    pub(super) fn cleanup_source(
        &self,
        table: &CleanupConditions,
        address: CleanupInstanceAddressId,
        slot: CleanupCaptureSlotId,
    ) -> Option<(usize, usize, usize)> {
        let slot = table.capture_slot_value(slot).unwrap();
        self.cleanup_source_position(table, address, slot.closure(), slot.position())
    }

    fn cleanup_source_position(
        &self,
        table: &CleanupConditions,
        address: CleanupInstanceAddressId,
        closure: ExpressionId,
        position: usize,
    ) -> Option<(usize, usize, usize)> {
        let address = table.instance_address(address).unwrap();
        let mut instance = *self
            .owners
            .get(&address.root())
            .or_else(|| self.cleanup_roots.get(&address.root()))
            .expect("cleanup root must still be addressable at this point");
        for &position in address.capture_path() {
            let child = self.instances[instance]
                .captures
                .get(&position)
                .or_else(|| self.cleanup_edges.get(&(instance, position)));
            if child.is_none()
                && self.instances[instance]
                    .cleared_captures
                    .contains(&position)
            {
                return None;
            }
            instance = *child.expect("cleanup path must retain the released child");
        }
        let actual_closure = self.instances[instance]
            .closure
            .expect("loan address must resolve to an environment");
        if actual_closure != closure {
            return None;
        }
        let source = *self.instances[instance]
            .shared
            .get(&position)
            .or_else(|| {
                self.ended_captures
                    .get(&(instance, position))
                    .map(|(source, _)| source)
            })
            .expect("loan end must read its saved source");
        Some((instance, position, source))
    }

    pub(super) fn end_capture_loan(
        &mut self,
        table: &CleanupConditions,
        address: CleanupInstanceAddressId,
        slot: Option<CleanupCaptureSlotId>,
        closure: ExpressionId,
        source: ClosureCaptureSource,
    ) {
        let position = if let Some(slot) = slot {
            let layout = table.capture_slot_value(slot).unwrap();
            assert_eq!((layout.closure(), layout.source()), (closure, source));
            layout.position()
        } else {
            // 非 owning place 没有 phi source 槽，仍从公开已检查的 lambda 布局定位。
            self.file_captures
                .iter()
                .filter(|capture| capture.lambda() == closure)
                .position(|capture| {
                    capture.source() == source && capture.mode() == ClosureCaptureMode::Shared
                })
                .expect("place loan requires a checked shared capture")
        };
        let Some((instance, position, resource)) =
            self.cleanup_source_position(table, address, closure, position)
        else {
            return;
        };
        self.end_instance_loan(instance, position, resource);
    }

    /// 树形事实与递归根动作共用逐实例恰好一次的 loan 账本。
    pub(super) fn end_instance_loan(&mut self, instance: usize, position: usize, resource: usize) {
        assert!(
            !self.instances[instance].live,
            "capture loan ends after its environment drop"
        );
        assert_eq!(
            self.loans.remove(&(instance, position)),
            Some(resource),
            "capture loan must end once"
        );
        assert_eq!(
            self.instances[instance].shared.remove(&position),
            Some(resource)
        );
        let last = !self.loans.values().any(|source| *source == resource);
        assert!(
            self.ended_captures
                .insert((instance, position), (resource, last))
                .is_none()
        );
        self.loan_ends.push((instance, position));
    }
}
