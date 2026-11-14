//! Exact actual false-exit partition; no live-root suffix cleanup is inferred.
use super::*;
use crate::ownership_checking::{UnitDropFact, UnitDropTarget};
#[derive(Clone, Debug)]
pub(super) struct Partition {
    live: Vec<UnitSymbolId>,
    drops: Vec<UnitDropFact>,
    manifest: (Vec<UnitSymbolId>, Vec<UnitDropFact>),
}
impl UnitOwnerControlBridgeEvidenceV1 {
    pub(in crate::ownership_checking::compilation_unit) fn with_next_false_exit_partition(
        mut self,
        actual: Option<(Vec<UnitSymbolId>, Vec<UnitDropFact>)>,
    ) -> Option<Self> {
        if self.next_exit.is_some() {
            return None;
        }
        self.next_exit = actual.map(|(live, drops)| Partition {
            manifest: (live.clone(), drops.clone()),
            live,
            drops,
        });
        self.is_consistent().then_some(self)
    }
    /// Surviving header roots and actual LoopExit drops, not concrete runtime tokens.
    /// Live roots carry no permission for subsequent statements or cleanup.
    pub fn next_false_exit_partition(&self) -> Option<(&[UnitSymbolId], &[UnitDropFact])> {
        self.next_exit
            .as_ref()
            .map(|p| (p.live.as_slice(), p.drops.as_slice()))
    }
    pub(super) fn exit_matches_normal(&self) -> bool {
        self.next_exit.as_ref().is_none_or(|p| {
            self.next_normal.is_some()
                && self.next_tail.is_some()
                && p.manifest == (p.live.clone(), p.drops.clone())
                && p.live
                    .iter()
                    .enumerate()
                    .all(|(i, r)| self.targets.contains(r) && !p.live[..i].contains(r))
                && p.live.len() + p.drops.len() == self.targets.len()
                && self.targets.iter().all(|root| {
                    usize::from(p.live.contains(root))
                        + p.drops
                            .iter()
                            .filter(|drop| {
                                drop.target() == UnitDropTarget::Named(*root)
                                    && drop.point() == UnitDropPoint::LoopExit(self.next_control)
                            })
                            .count()
                        == 1
                })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ownership_checking::compilation_unit::formed_tests::Fixture;
    #[test]
    fn bridge_false_exit_uses_actual_live_and_drop_partition() {
        for live_count in 0..=2 {
            for reverse in [false, true] {
                let suffix = match live_count {
                    0 => "",
                    1 => "println(a1)",
                    _ => "println(a1); println(b1)",
                };
                let source = super::super::tests::source(reverse, false)
                    .replace("println(a1); println(b1)", suffix);
                let fixture = Fixture::new(&[("bridge-exit-partition.ko", &source)]);
                let owned = fixture.shared();
                assert!(owned.diagnostics().is_empty());
                let bridge = owned.owner_observations()[0]
                    .control_bridge_evidence_v1()
                    .unwrap();
                let (live, drops) = bridge
                    .next_false_exit_partition()
                    .expect("actual false exit");
                assert_eq!(live.len(), live_count);
                assert_eq!(drops.len(), 2 - live_count);
                for root in bridge.targets() {
                    assert_eq!(
                        usize::from(live.contains(root))
                            + drops
                                .iter()
                                .filter(|d| d.target() == UnitDropTarget::Named(*root))
                                .count(),
                        1
                    );
                }
                for drop in drops {
                    assert_eq!(drop.point(), UnitDropPoint::LoopExit(bridge.next_control()));
                    assert!(owned.drops().contains(drop));
                }
                assert!(bridge.next_counter_tail().is_some());
                assert!(
                    owned.owner_observations()[0]
                        .counter_tail_evidence_v1()
                        .is_none()
                );
                assert!(
                    owned.owner_observations()[0]
                        .multi_root_boundary_v1()
                        .is_none()
                );
            }
        }
    }
    #[test]
    fn ci_only_bridge_exit_partition_requires_complete_disjoint_actual_manifest() {
        let source = super::super::tests::source(false, false)
            .replace("println(a1); println(b1)", "println(a1)");
        let fixture = Fixture::new(&[("bridge-exit-fault.ko", &source)]);
        let owned = fixture.shared();
        let bridge = owned.owner_observations()[0]
            .control_bridge_evidence_v1()
            .unwrap();
        let (_, drops) = bridge.next_false_exit_partition().unwrap();
        assert_eq!(drops.len(), 1);
        let mut missing = bridge.clone();
        missing.next_exit.as_mut().unwrap().drops.clear();
        assert!(!missing.is_consistent());
        let mut duplicate = bridge.clone();
        let p = duplicate.next_exit.as_mut().unwrap();
        p.drops.push(p.drops[0]);
        p.manifest = (p.live.clone(), p.drops.clone());
        assert!(!duplicate.is_consistent());
        let mut overlap = bridge.clone();
        let p = overlap.next_exit.as_mut().unwrap();
        let UnitDropTarget::Named(root) = p.drops[0].target() else {
            panic!()
        };
        p.live = vec![root];
        p.manifest = (p.live.clone(), p.drops.clone());
        assert!(!overlap.is_consistent());
        let mut detached = bridge.clone();
        detached.next_tail = None;
        assert!(!detached.is_consistent());
    }
}
