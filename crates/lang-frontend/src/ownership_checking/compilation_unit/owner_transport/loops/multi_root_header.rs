//! Whole-edge substitution into the restricted normal-body rule, not runtime phi lowering.
use super::*;
use crate::name_resolution::UnitSymbolId;
use crate::ownership_checking::UnitDropFact;

impl UnitOwnerMultiRootBoundaryV1 {
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
