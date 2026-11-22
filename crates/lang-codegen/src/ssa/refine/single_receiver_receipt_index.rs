//! Read-only ordinal lookups for optional single-receiver receipt construction.
//! The real original catalog, actions and sealed cleanup records remain authoritative.
use lang_frontend::{
    name_resolution::UnitSymbolId,
    ownership_checking::{
        UnitDropPoint, UnitDropTarget, UnitLoanActionCoverage, UnitLoweringFacts,
    },
    type_checking::{UnitExpressionId, UnitItemId},
};
use std::{cell::OnceCell, collections::HashMap};

type RegionOrdinals = HashMap<UnitItemId, Vec<(usize, usize)>>;
type TerminalDropOrdinals = HashMap<(UnitExpressionId, UnitSymbolId), Vec<usize>>;

#[derive(Default)]
pub(in crate::ssa) struct SingleReceiverReceiptIndex {
    regions: OnceCell<RegionOrdinals>,
    terminal_drops: OnceCell<TerminalDropOrdinals>,
}

impl SingleReceiverReceiptIndex {
    pub(super) fn regions(
        &self,
        owned: &dyn UnitLoweringFacts,
        callable: UnitItemId,
    ) -> &[(usize, usize)] {
        self.regions
            .get_or_init(|| {
                let mut by_callable = RegionOrdinals::new();
                for (catalog, observation) in owned.loan_action_observations().iter().enumerate() {
                    for (ordinal, region) in observation.regions().iter().enumerate() {
                        #[cfg(test)]
                        super::receiver_receipt::work_count::visit(|counts| {
                            counts.region_rows += 1
                        });
                        if region.protocol_version() == 2
                            && region.coverage() == UnitLoanActionCoverage::Complete
                            && let Some(item) = region.callable()
                        {
                            // Preserve original ordering/multiplicity and leave source and
                            // catalog identity validation to the actual plan constructor.
                            by_callable
                                .entry(item)
                                .or_default()
                                .push((catalog, ordinal));
                        }
                    }
                }
                by_callable
            })
            .get(&callable)
            .map_or(&[], Vec::as_slice)
    }

    pub(super) fn terminal_drops(
        &self,
        owned: &dyn UnitLoweringFacts,
        call: UnitExpressionId,
        root: UnitSymbolId,
    ) -> &[usize] {
        self.terminal_drops
            .get_or_init(|| {
                let mut by_point_and_root = TerminalDropOrdinals::new();
                for (ordinal, drop) in owned.drops().iter().enumerate() {
                    #[cfg(test)]
                    super::receiver_receipt::work_count::visit(|counts| counts.drop_rows += 1);
                    if let (UnitDropPoint::CallReturn(call), UnitDropTarget::Named(root)) =
                        (drop.point(), drop.target())
                    {
                        // No deduplication: the caller still filters by full fact equality.
                        by_point_and_root
                            .entry((call, root))
                            .or_default()
                            .push(ordinal);
                    }
                }
                by_point_and_root
            })
            .get(&(call, root))
            .map_or(&[], Vec::as_slice)
    }
}
