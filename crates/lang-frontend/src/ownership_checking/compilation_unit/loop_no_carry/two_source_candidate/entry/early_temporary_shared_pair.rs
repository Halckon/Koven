//! One shared root outlives exactly one first-call temporary.
use super::*;
pub(super) fn finish(
    (root, initializer, span, bytes): (UnitSymbolId, UnitExpressionId, Span, Vec<u8>),
    first: EntryCallDescriptor,
    second: EntryCallDescriptor,
    owned: &CompilationUnitOwnership,
) -> Option<EntryCalls> {
    if first.instance() != second.instance() || !first.instance().type_arguments().is_empty() {
        return None;
    }
    let (
        StringStorage::SharedMixed {
            root: a,
            drop: temp,
            ..
        },
        StringStorage::SharedRoot { root: b },
    ) = (&first.storage, &second.storage)
    else {
        return None;
    };
    if *a != root || *b != root || !matches!(temp.target(), UnitDropTarget::Temporary(_)) {
        return None;
    }
    for call in [&first, &second] {
        if call.temporary_scalars.iter().any(Option::is_none) {
            return None;
        }
        for (index, end) in call.ends().iter().enumerate() {
            if end.loan().call() != call.contracts()[0].call()
                || end.point() != UnitDropPoint::CallReturn(call.contracts()[0].call())
                || end.preceding_drops() != 0
                || end.preceding_call_loan_ends() != index
            {
                return None;
            }
        }
    }
    let named: Vec<_> = owned
        .drops()
        .iter()
        .filter(|d| d.target() == UnitDropTarget::Named(root))
        .copied()
        .collect();
    let [drop] = named.as_slice() else {
        return None;
    };
    let point = UnitDropPoint::CallReturn(second.contracts()[0].call());
    if drop.point() != point
        || temp.point() != UnitDropPoint::CallReturn(first.contracts()[0].call())
        || owned
            .drops()
            .iter()
            .filter(|d| d.target() == temp.target())
            .count()
            != 1
    {
        return None;
    }
    let at = |call: &EntryCallDescriptor| {
        owned
            .drops()
            .iter()
            .filter(|d| d.point() == UnitDropPoint::CallReturn(call.contracts()[0].call()))
            .copied()
            .collect::<Vec<_>>()
    };
    if at(&first).as_slice() != [*temp] || at(&second).as_slice() != [*drop] {
        return None;
    }
    Some(EntryCalls::EarlyTemporarySharedPair {
        owner: Box::new(EntryOwnerDescriptor {
            root,
            initializer,
            span,
            bytes,
            drop: *drop,
        }),
        calls: Box::new([first, second]),
    })
}
