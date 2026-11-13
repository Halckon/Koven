//! Completed immediate move/bind/drop suffix. No arbitrary downstream path authority.
use super::*;
use crate::ownership_checking::{UnitDropFact, UnitDropTarget};
#[derive(Clone, Debug)]
pub(super) struct Suffix {
    actual: Vec<(UnitSymbolId, UnitDropFact)>,
    manifest: Vec<(UnitSymbolId, UnitDropFact)>,
}
impl UnitOwnerControlBridgeEvidenceV1 {
    pub(in crate::ownership_checking::compilation_unit) fn with_next_false_exit_suffix(
        mut self,
        actual: Option<Vec<(UnitSymbolId, UnitDropFact)>>,
    ) -> Option<Self> {
        if self.next_suffix.is_some() {
            return None;
        }
        self.next_suffix = actual.map(|actual| Suffix {
            manifest: actual.clone(),
            actual,
        });
        self.is_consistent().then_some(self)
    }
    /// Each surviving root's actual immediate val move and binding cleanup, in execution order.
    /// Only complete live-root consumption is published; later lifecycles are excluded.
    pub fn next_false_exit_suffix(&self) -> Option<&[(UnitSymbolId, UnitDropFact)]> {
        self.next_suffix.as_ref().map(|s| s.actual.as_slice())
    }
    pub(super) fn suffix_matches_exit(&self) -> bool {
        self.next_suffix.as_ref().is_none_or(|suffix| {
            let Some((live,_)) = self.next_false_exit_partition() else { return false; };
            !live.is_empty() && suffix.manifest == suffix.actual && suffix.actual.len() == live.len()
                && suffix.actual.iter().enumerate().all(|(i,(root,drop))| {
                    let UnitDropTarget::Named(binding) = drop.target() else { return false; };
                    matches!(drop.point(), UnitDropPoint::AfterStatement(s) if s.source_unit() == self.next_control.source_unit())
                        && binding.source_unit() == self.next_control.source_unit()
                        && !self.targets.contains(&binding)
                        && live.contains(root)
                        && !suffix.actual[..i].iter().any(|(prior,fact)| prior == root || fact.target() == drop.target() || fact.point() == drop.point())
                })
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::ownership_checking::compilation_unit::formed_tests::Fixture;
    #[test]
    fn bridge_suffix_requires_actual_complete_immediate_moves() {
        for (suffix, count) in [
            ("val x = a1; val y = b1", Some(2)),
            ("val y = b1; val x = a1", Some(2)),
            ("val x = a1", Some(1)),
            ("val n = 1; val x = a1; val y = b1", None),
            ("val x = a1; println(b1)", None),
            ("val x = a1; println(x); val y = b1", None),
            (
                "val x = a1; val y = b1; a1 = \"fresh\"; val z = a1",
                Some(2),
            ),
        ] {
            let text = super::super::tests::source(false, false)
                .replace("println(a1); println(b1)", suffix);
            let fixture = Fixture::new(&[("bridge-suffix.ko", &text)]);
            let owned = fixture.shared();
            assert!(
                owned.diagnostics().is_empty(),
                "{suffix}: {:?}",
                owned.diagnostics()
            );
            let bridge = owned.owner_observations()[0]
                .control_bridge_evidence_v1()
                .unwrap();
            assert!(bridge.next_normal_transport().is_some());
            let (live, _) = bridge.next_false_exit_partition().unwrap();
            let actual = bridge.next_false_exit_suffix();
            assert_eq!(actual.map(|s| s.len()), count, "{suffix}");
            if let Some(actual) = actual {
                assert_eq!(actual.len(), live.len());
                for (root, fact) in actual {
                    assert!(live.contains(root));
                    assert!(owned.drops().contains(fact));
                }
                let expected_first = if suffix.starts_with("val y") {
                    bridge.targets()[1]
                } else {
                    bridge.targets()[0]
                };
                assert_eq!(actual[0].0, expected_first);
            }
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
    #[test]
    fn ci_only_bridge_suffix_rejects_partial_duplicate_or_retired_sources() {
        let text = super::super::tests::source(false, false)
            .replace("println(a1); println(b1)", "val x = a1; val y = b1");
        let fixture = Fixture::new(&[("bridge-suffix-fault.ko", &text)]);
        let owned = fixture.shared();
        let bridge = owned.owner_observations()[0]
            .control_bridge_evidence_v1()
            .unwrap();
        assert_eq!(bridge.next_false_exit_suffix().unwrap().len(), 2);
        let mut partial = bridge.clone();
        partial.next_suffix.as_mut().unwrap().actual.pop();
        assert!(!partial.is_consistent());
        let mut duplicate = bridge.clone();
        let s = duplicate.next_suffix.as_mut().unwrap();
        s.actual[1] = s.actual[0];
        s.manifest = s.actual.clone();
        assert!(!duplicate.is_consistent());
        let mut wrong = bridge.clone();
        let s = wrong.next_suffix.as_mut().unwrap();
        s.actual[0].0 = wrong.sources[0];
        s.manifest = s.actual.clone();
        assert!(!wrong.is_consistent());
        let mut detached = bridge.clone();
        detached.next_exit = None;
        assert!(!detached.is_consistent());
    }
}
