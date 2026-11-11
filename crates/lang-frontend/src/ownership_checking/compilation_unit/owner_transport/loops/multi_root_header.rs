//! Whole-edge substitution into the restricted normal-body rule, not runtime phi lowering.
use super::*;
use crate::name_resolution::UnitSymbolId;
use crate::ownership_checking::UnitDropFact;

impl UnitOwnerMultiRootBoundaryV1 {
    /// Consume either complete header predecessor at the actual implicit-false exit.
    /// Only available when every root was dropped at this loop exit by the planner.
    /// Pairing preserves real exit-drop order; no suffix/live-root, Break or Return claim.
    /// This is a parametric exit rule, not a dynamic trace or generated LLVM phi.
    pub fn condition_false_header_retirements(
        &self,
        control: UnitStatementId,
        incoming: &[(UnitSymbolId, UnitOwnerHeaderSourceV1)],
    ) -> Option<Vec<(UnitOwnerHeaderSourceV1, UnitDropFact)>> {
        // Reuse exact whole-edge validation; do not combine independent per-root choices.
        self.normal_header_retirements(control, incoming)?;
        self.condition_false_drops()?
            .iter()
            .map(|fact| {
                let crate::ownership_checking::UnitDropTarget::Named(root) = fact.target() else {
                    return None;
                };
                let (_, source) = incoming.iter().find(|(known, _)| *known == root)?;
                Some((source.clone(), *fact))
            })
            .collect()
    }

    /// Substitute one complete static header predecessor into normal-body retirements.
    /// Entry sources must all be the original input definitions; Backedge sources must
    /// all be the normal outputs. Mixed edges, partial vectors and foreign controls fail.
    /// Returns consumed sources paired with actual drops, in primitive execution order.
    /// Backedge tokens denote fresh instances of definition sites, not persistent values.
    /// This does not prove predecessor reachability, condition-false exits or lowering.
    pub fn normal_header_retirements(
        &self,
        control: UnitStatementId,
        incoming: &[(UnitSymbolId, UnitOwnerHeaderSourceV1)],
    ) -> Option<Vec<(UnitOwnerHeaderSourceV1, UnitDropFact)>> {
        if control != self.statement() || !self.is_consistent() {
            return None;
        }
        let rule = self.normal_transport()?;
        if incoming.len() != self.slots().len() {
            return None;
        }
        let entry = matches!(incoming.first()?.1, UnitOwnerHeaderSourceV1::Entry(_));
        for ((root, source), slot) in incoming.iter().zip(self.slots()) {
            let expected = if entry {
                UnitOwnerHeaderSourceV1::Entry(slot.entry())
            } else {
                UnitOwnerHeaderSourceV1::Backedge(slot.first_body_output())
            };
            if *root != slot.root() || *source != expected {
                return None;
            }
        }
        // The checked rule consumes every formal Header(root) once. Resolve each
        // formal input against the SAME whole predecessor, never slot-wise phis.
        rule.steps()
            .iter()
            .map(|step| {
                let (_, source) = incoming
                    .iter()
                    .find(|(root, _)| *root == step.header_root())?;
                Some((source.clone(), step.returned_value_drop()))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ownership_checking::compilation_unit::formed_tests::Fixture;
    #[test]
    fn ci_only_header_substitution_rejects_mixed_partial_and_foreign_edges() {
        let fixture = Fixture::new(&[(
            "header-substitution.ko",
            "fun run(flag: Boolean) { var a = \"a\"; var b = \"b\"; while(flag) { val x = replace(&a, \"x\"); val y = replace(&b, \"y\") }; val outA = a; val outB = b }",
        )]);
        let owned = fixture.shared();
        let boundary = owned.owner_observations()[0]
            .multi_root_boundary_v1()
            .unwrap();
        let entry: Vec<_> = boundary
            .slots()
            .iter()
            .map(|slot| (slot.root(), UnitOwnerHeaderSourceV1::Entry(slot.entry())))
            .collect();
        let accepts = |values: &[_]| {
            boundary
                .normal_header_retirements(boundary.statement(), values)
                .is_some()
        };
        assert!(accepts(&entry));
        assert!(!accepts(&entry[..1]));
        let mut mixed = entry.clone();
        mixed[1].1 = UnitOwnerHeaderSourceV1::Backedge(boundary.slots()[1].first_body_output());
        assert!(!accepts(&mixed));
        let mut swapped = entry.clone();
        swapped.swap(0, 1);
        assert!(!accepts(&swapped));
        let mut aliased = entry.clone();
        aliased[1].1 = aliased[0].1.clone();
        assert!(!accepts(&aliased));
        assert!(
            boundary
                .normal_header_retirements(boundary.body(), &entry)
                .is_none()
        );
        let foreign = fixture.shared();
        let foreign = foreign.owner_observations()[0]
            .multi_root_boundary_v1()
            .unwrap();
        assert!(
            foreign
                .normal_header_retirements(foreign.statement(), &entry)
                .is_none()
        );
    }
}

#[cfg(test)]
mod actual_exit_tests {
    use super::*;
    use crate::ownership_checking::compilation_unit::formed_tests::Fixture;
    use crate::ownership_checking::{UnitDropPoint, UnitDropTarget};
    #[test]
    fn actual_false_exit_consumes_whole_header_in_cleanup_order() {
        for (suffix, admitted) in [
            ("", true),
            ("; val keep = a", false),
            ("; val keepA = a; val keepB = b", false),
        ] {
            for body in [
                "val x = replace(&a, \"x\"); val y = replace(&b, \"y\")",
                "val y = replace(&b, \"y\"); val x = replace(&a, \"x\")",
            ] {
                let source = format!(
                    "fun run(flag: Boolean) {{ var a = \"a\"; var b = \"b\"; while(flag) {{ {body} }}{suffix} }}"
                );
                let fixture = Fixture::new(&[("false-header-exit.ko", &source)]);
                let owned = fixture.shared();
                assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
                assert!(owned.clone().validate().is_ok());
                let boundary = owned.owner_observations()[0]
                    .multi_root_boundary_v1()
                    .unwrap();
                for backedge in [false, true] {
                    let incoming: Vec<_> = boundary
                        .slots()
                        .iter()
                        .map(|slot| {
                            (
                                slot.root(),
                                if backedge {
                                    UnitOwnerHeaderSourceV1::Backedge(slot.first_body_output())
                                } else {
                                    UnitOwnerHeaderSourceV1::Entry(slot.entry())
                                },
                            )
                        })
                        .collect();
                    let result = boundary
                        .condition_false_header_retirements(boundary.statement(), &incoming);
                    assert_eq!(result.is_some(), admitted, "{source}");
                    if let Some(result) = result {
                        assert_eq!(result.len(), 2);
                        for ((source, drop), (root, expected)) in
                            result.iter().zip(incoming.iter().rev())
                        {
                            assert_eq!(source, expected);
                            assert_eq!(drop.target(), UnitDropTarget::Named(*root));
                            assert_eq!(drop.point(), UnitDropPoint::LoopExit(boundary.statement()));
                        }
                    }
                }
            }
        }
    }
}
