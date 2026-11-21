//! Legacy fact lookup for the bounded single owned receiver route only.
use lang_frontend::{
    name_resolution::UnitSymbolId,
    ownership_checking::{
        UnitDropPoint, UnitDropTarget, UnitLoweringFacts, UnitOwnershipBindingDescriptor,
        UnitReceiverCallReturnEndFact,
    },
    type_checking::UnitExpressionId,
};
use std::{
    cell::OnceCell,
    collections::{HashMap, HashSet},
};

#[derive(Default)]
pub(in crate::ssa) struct SingleReceiverFacts {
    bindings: OnceCell<HashMap<UnitSymbolId, usize>>,
    ends: OnceCell<HashMap<UnitExpressionId, Vec<usize>>>,
    named_drops: OnceCell<HashSet<(UnitExpressionId, UnitSymbolId)>>,
}

impl SingleReceiverFacts {
    pub(super) fn binding<'a>(
        &self,
        owned: &'a dyn UnitLoweringFacts,
        symbol: UnitSymbolId,
    ) -> Option<&'a UnitOwnershipBindingDescriptor> {
        let index = self.bindings.get_or_init(|| {
            first_binding_slots(owned.bindings().iter().map(|binding| {
                #[cfg(test)]
                super::receiver_receipt::work_count::visit(|c| c.route_bindings += 1);
                binding.symbol()
            }))
        });
        let matched = index
            .get(&symbol)
            .and_then(|&slot| owned.bindings().get(slot))?;
        (matched.symbol() == symbol).then_some(matched)
    }

    pub(super) fn has_end(
        &self,
        owned: &dyn UnitLoweringFacts,
        call: UnitExpressionId,
        mut matches: impl FnMut(&UnitReceiverCallReturnEndFact) -> bool,
    ) -> bool {
        let index = self.ends.get_or_init(|| {
            let mut index = HashMap::<_, Vec<_>>::new();
            for (slot, end) in owned.receiver_call_return_ends().iter().enumerate() {
                #[cfg(test)]
                super::receiver_receipt::work_count::visit(|c| c.route_ends += 1);
                if let UnitDropPoint::CallReturn(id) = end.point() {
                    index.entry(id).or_default().push(slot);
                }
            }
            index
        });
        index.get(&call).into_iter().flatten().any(|&slot| {
            owned
                .receiver_call_return_ends()
                .get(slot)
                .is_some_and(|end| end.point() == UnitDropPoint::CallReturn(call) && matches(end))
        })
    }

    pub(super) fn has_named_drop(
        &self,
        owned: &dyn UnitLoweringFacts,
        call: UnitExpressionId,
        symbol: UnitSymbolId,
    ) -> bool {
        let index = self.named_drops.get_or_init(|| {
            let mut index = HashSet::new();
            for drop in owned.drops() {
                #[cfg(test)]
                super::receiver_receipt::work_count::visit(|c| c.route_drops += 1);
                if let (UnitDropPoint::CallReturn(call), UnitDropTarget::Named(symbol)) =
                    (drop.point(), drop.target())
                {
                    // This gate asks only for existence; receipt validation still uses all slots.
                    index.insert((call, symbol));
                }
            }
            index
        });
        index.contains(&(call, symbol))
    }
}

fn first_binding_slots(
    symbols: impl Iterator<Item = UnitSymbolId>,
) -> HashMap<UnitSymbolId, usize> {
    let mut index = HashMap::new();
    for (slot, symbol) in symbols.enumerate() {
        // Retain the first match, even when it is not Owned.
        index.entry(symbol).or_insert(slot);
    }
    index
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ssa::unit_lower_test_support;
    use lang_frontend::{
        name_resolution::SourceUnitInput, ownership_checking::UnitReceiverOwnershipTarget,
        source::SourceMap, type_checking::standard_environments,
    };

    #[test]
    fn single_receiver_facts_preserve_first_match_and_reject_wrong_source_and_slots() {
        let mut sources = SourceMap::new();
        let (main, parsed) = unit_lower_test_support::parsed(
            &mut sources,
            "main.ko",
            "class Node() { fun touch(): Int = 1 }\nfun a(own node: Node): Unit { node.touch() }\nfun b(own node: Node): Unit { node.touch() }",
        );
        let (aux, helper) =
            unit_lower_test_support::parsed(&mut sources, "aux.ko", "fun other(): Int = 0");
        let inputs = [
            SourceUnitInput::new("root", "main.ko", main, &parsed),
            SourceUnitInput::new("root", "aux.ko", aux, &helper),
        ];
        let (environment, types) = standard_environments();
        let (names, _, owned) =
            unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
        let owned = owned.ownership();
        let [wanted, other] = owned.receiver_call_return_ends() else {
            panic!("two actual ends");
        };
        let call = wanted.receiver().call();
        let wrong_source = UnitExpressionId::new(
            names
                .names()
                .index()
                .source_units()
                .iter()
                .find(|source| source.source_id() == aux)
                .unwrap()
                .id(),
            call.expression(),
        );
        let UnitReceiverOwnershipTarget::Place(place) = wanted.receiver().target() else {
            panic!("root");
        };
        let UnitReceiverOwnershipTarget::Place(other_place) = other.receiver().target() else {
            panic!("second root");
        };
        let root = place.root();
        assert_ne!(wrong_source, call);
        assert_eq!(wrong_source.expression(), call.expression());
        assert_eq!(
            first_binding_slots([root, other_place.root(), root].into_iter())[&root],
            0,
        );

        let index = SingleReceiverFacts::default();
        assert_eq!(index.binding(owned, root).unwrap().symbol(), root);
        assert!(index.has_end(owned, call, |end| end == wanted));
        assert!(!index.has_end(owned, call, |end| end == other));
        assert!(!index.has_end(owned, wrong_source, |_| true));
        assert!(index.has_named_drop(owned, call, root));
        assert!(!index.has_named_drop(owned, wrong_source, root));
        assert!(!index.has_named_drop(owned, call, other_place.root()));

        // Mutate only the private copied lookup, not the actual front-end records.
        let mut index = index;
        let slot = owned
            .bindings()
            .iter()
            .position(|binding| binding.symbol() == other_place.root())
            .unwrap();
        index.bindings.get_mut().unwrap().insert(root, slot);
        assert!(index.binding(owned, root).is_none());
        let slots = index.ends.get_mut().unwrap().get_mut(&call).unwrap();
        *slots = vec![usize::MAX, 1, 0, 0];
        let mut compared = 0;
        assert!(index.has_end(owned, call, |end| {
            compared += 1;
            end == wanted
        }));
        assert_eq!(compared, 1);
        assert!(!index.has_end(owned, call, |end| end == other));
    }
}
