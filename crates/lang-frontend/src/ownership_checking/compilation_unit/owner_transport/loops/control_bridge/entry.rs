//! Actual scalar initialization identities, not syntax-only or runtime-value evidence.
use super::*;
type Sites = (
    [UnitSymbolId; 2],
    [UnitExpressionId; 2],
    [UnitStatementId; 2],
);
/// Actual successful counter initializer then guard initializer of the second control.
#[derive(Clone, Debug)]
pub struct UnitOwnerBridgeCounterEntryV1 {
    sites: Sites,
    manifest: Sites,
    steps: Vec<UnitOwnerRootReplaceStepV1>,
}
impl UnitOwnerBridgeCounterEntryV1 {
    /// Actual counter binding, consistent with the completed countdown tail.
    pub fn counter(&self) -> UnitSymbolId {
        self.sites.0[0]
    }
    /// Actual Boolean guard binding.
    pub fn guard(&self) -> UnitSymbolId {
        self.sites.0[1]
    }
    /// Successfully evaluated Int Name or literal, not an inferred value.
    pub fn counter_initializer(&self) -> UnitExpressionId {
        self.sites.1[0]
    }
    /// Successfully evaluated counter > 0 expression.
    pub fn guard_initializer(&self) -> UnitExpressionId {
        self.sites.1[1]
    }
    /// Counter initializer followed by guard initializer.
    pub fn initializers(&self) -> [UnitExpressionId; 2] {
        self.sites.1
    }
    /// Actual declarations, in their observed completion order.
    pub fn declarations(&self) -> [UnitStatementId; 2] {
        self.sites.2
    }
}
impl UnitOwnerControlBridgeEvidenceV1 {
    pub(in crate::ownership_checking::compilation_unit) fn with_next_counter_entry(
        mut self,
        sites: Option<Sites>,
    ) -> Option<Self> {
        if self.next_entry.is_some() {
            return None;
        }
        if let Some(sites) = sites {
            // Entry observations without the completed body/tail remain non-authorizing.
            if let (Some(normal), Some(_)) = (&self.next_normal, &self.next_tail) {
                self.next_entry = Some(UnitOwnerBridgeCounterEntryV1 {
                    sites,
                    manifest: sites,
                    steps: normal.steps().to_vec(),
                });
            }
        }
        self.is_consistent().then_some(self)
    }
    /// Observed ordered initialization, distinct from body and exit permissions.
    pub fn next_counter_entry(&self) -> Option<&UnitOwnerBridgeCounterEntryV1> {
        self.next_entry.as_ref()
    }
    pub(super) fn entry_matches_tail(&self) -> bool {
        self.next_entry.as_ref().is_none_or(|entry| {
            let (Some(normal), Some(tail)) = (&self.next_normal, &self.next_tail) else {
                return false;
            };
            entry.sites == entry.manifest
                && entry.steps == normal.steps()
                && entry.counter() == tail.counter()
                && entry.guard() == tail.guard()
                && entry.sites.1[0] != entry.sites.1[1]
                && entry.sites.2[0] != entry.sites.2[1]
                && entry
                    .sites
                    .1
                    .iter()
                    .all(|id| id.source_unit() == self.next_control.source_unit())
                && entry
                    .sites
                    .2
                    .iter()
                    .all(|id| id.source_unit() == self.next_control.source_unit())
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::ownership_checking::compilation_unit::formed_tests::Fixture;
    #[test]
    fn bridge_counter_entry_records_actual_name_and_integer_initializers() {
        for initial in ["n", "0", "2"] {
            let text = super::super::tests::source(false, false)
                .replace("var n1 = n;", &format!("var n1 = {initial};"));
            let fixture = Fixture::new(&[("bridge-entry.ko", &text)]);
            let owned = fixture.shared();
            assert!(owned.diagnostics().is_empty());
            let bridge = owned.owner_observations()[0]
                .control_bridge_evidence_v1()
                .unwrap();
            let entry = bridge
                .next_counter_entry()
                .expect("actual initializers completed");
            let tail = bridge.next_counter_tail().unwrap();
            assert_eq!(entry.counter(), tail.counter());
            assert_eq!(entry.guard(), tail.guard());
            let actual = entry.initializers().map(|id| {
                fixture
                    .sources
                    .slice(
                        fixture.parsed[0]
                            .ast()
                            .expressions()
                            .get(id.expression())
                            .unwrap()
                            .span(),
                    )
                    .unwrap()
            });
            assert_eq!(actual, [initial, "n1 > 0"]);
            let declarations = entry.declarations().map(|id| {
                fixture
                    .sources
                    .slice(
                        fixture.parsed[0]
                            .ast()
                            .statements()
                            .get(id.statement())
                            .unwrap()
                            .span(),
                    )
                    .unwrap()
            });
            assert!(declarations[0].contains("var n1"));
            assert!(declarations[1].contains("var g1"));
            assert!(
                owned.owner_observations()[0]
                    .counter_tail_evidence_v1()
                    .is_none()
            );
        }
    }
    #[test]
    fn ci_only_bridge_counter_entry_rejects_foreign_and_swapped_sites() {
        let text = super::super::tests::source(false, false);
        let fixture = Fixture::new(&[("bridge-entry-identity.ko", &text)]);
        let one = fixture.shared();
        let two = fixture.shared();
        let bridge = one.owner_observations()[0]
            .control_bridge_evidence_v1()
            .unwrap();
        assert!(bridge.next_counter_entry().is_some());
        let mut foreign = bridge.clone();
        foreign.next_entry = two.owner_observations()[0]
            .control_bridge_evidence_v1()
            .unwrap()
            .next_entry
            .clone();
        assert!(!foreign.is_consistent());
        let mut swapped = bridge.clone();
        swapped.next_entry.as_mut().unwrap().sites.1.swap(0, 1);
        assert!(!swapped.is_consistent());
        let mut detached = bridge.clone();
        detached.next_tail = None;
        detached.next_exit = None;
        detached.next_suffix = None;
        assert!(!detached.is_consistent());
    }
}
