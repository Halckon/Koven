//! Lazy exact-key indices for same-owner sequences of shared receiver calls.
//! Never authorizes a call or changes ordering of the original fact tables.
use lang_frontend::{
    name_resolution::UnitSymbolId,
    ownership_checking::{
        OwnershipBindingKind, UnitDropPoint, UnitDropTarget, UnitLoweringFacts,
        UnitReceiverOwnershipFact,
    },
    type_checking::UnitExpressionId,
};
use std::{cell::OnceCell, collections::HashMap, hash::Hash};

#[derive(Default)]
pub(in crate::ssa) struct OwnedReceiverSequenceFacts(OnceCell<Index>);

impl OwnedReceiverSequenceFacts {
    pub(super) fn validates(
        &self,
        owned: &dyn UnitLoweringFacts,
        root: UnitSymbolId,
        checks: &[Option<(UnitExpressionId, &UnitReceiverOwnershipFact)>],
    ) -> bool {
        let Some(Some((last, _))) = checks.last().copied() else {
            return false;
        };
        let index = self.0.get_or_init(|| Index::build(owned));
        index
            .bindings
            .unique(root)
            .is_some_and(|slot| owned.bindings()[slot].kind() == OwnershipBindingKind::Owned)
            && checks.iter().all(|check| {
                check.is_some_and(|(call, fact)| {
                    index.ends.unique(call).is_some_and(|slot| {
                        owned.receiver_call_return_ends()[slot].receiver() == fact
                    })
                })
            })
            && index
                .drops
                .unique(root)
                .is_some_and(|slot| owned.drops()[slot].point() == UnitDropPoint::CallReturn(last))
    }
}

struct Index {
    bindings: Unique<UnitSymbolId>,
    ends: Unique<UnitExpressionId>,
    drops: Unique<UnitSymbolId>,
}

impl Index {
    fn build(owned: &dyn UnitLoweringFacts) -> Self {
        let mut bindings = Unique(HashMap::new());
        let mut ends = Unique(HashMap::new());
        let mut drops = Unique(HashMap::new());
        for (slot, binding) in owned.bindings().iter().enumerate() {
            #[cfg(test)]
            super::two_receiver_route_metrics::visit(|count| count.bindings += 1);
            bindings.insert(binding.symbol(), slot);
        }
        for (slot, end) in owned.receiver_call_return_ends().iter().enumerate() {
            #[cfg(test)]
            super::two_receiver_route_metrics::visit(|count| count.ends += 1);
            if let UnitDropPoint::CallReturn(call) = end.point() {
                ends.insert(call, slot);
            }
        }
        for (slot, drop) in owned.drops().iter().enumerate() {
            #[cfg(test)]
            super::two_receiver_route_metrics::visit(|count| count.drops += 1);
            if let UnitDropTarget::Named(root) = drop.target() {
                drops.insert(root, slot);
            }
        }
        Self {
            bindings,
            ends,
            drops,
        }
    }
}

// A repeated key never recovers a usable slot. This preserves the legacy gate's
// requirement for one actual end per call and one terminal named drop per root.
struct Unique<K>(HashMap<K, Option<usize>>);

impl<K: Eq + Hash> Unique<K> {
    fn insert(&mut self, key: K, slot: usize) {
        self.0
            .entry(key)
            .and_modify(|row| *row = None)
            .or_insert(Some(slot));
    }

    fn unique(&self, key: K) -> Option<usize> {
        #[cfg(test)]
        super::two_receiver_route_metrics::visit(|count| count.probes += 1);
        self.0.get(&key).copied().flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::Unique;
    use std::collections::HashMap;

    #[test]
    fn two_receiver_index_duplicate_and_absent_keys_fail_closed() {
        let mut slots = Unique(HashMap::new());
        assert_eq!(slots.unique(11), None);
        slots.insert(11, 9);
        slots.insert(17, 10);
        assert_eq!(slots.unique(11), Some(9));
        slots.insert(11, 9);
        slots.insert(11, 13);
        assert_eq!(slots.unique(11), None);
        assert_eq!(slots.unique(17), Some(10));
    }
}
