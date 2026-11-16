//! One shared root and exactly one second-call temporary; no reverse-shape authority.
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
    let owner = prove((root, initializer, span, bytes), &first, &second, owned)?;
    Some(EntryCalls::LateTemporarySharedPair {
        owner: Box::new(owner),
        calls: Box::new([first, second]),
    })
}
/// Pure fixed owner payload; no entry authority or callable-key decision.
pub(super) fn prove(
    (root, initializer, span, bytes): (UnitSymbolId, UnitExpressionId, Span, Vec<u8>),
    first: &EntryCallDescriptor,
    second: &EntryCallDescriptor,
    owned: &CompilationUnitOwnership,
) -> Option<EntryOwnerDescriptor> {
    let (
        StringStorage::SharedRoot { root: a },
        StringStorage::SharedMixed {
            root: b,
            drop: temp,
            ..
        },
    ) = (&first.storage, &second.storage)
    else {
        return None;
    };
    if *a != root || *b != root || !matches!(temp.target(), UnitDropTarget::Temporary(_)) {
        return None;
    }
    for call in [first, second] {
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
        || temp.point() != point
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
    if !at(first).is_empty() || at(second).as_slice() != [*temp, *drop] {
        return None;
    }
    Some(EntryOwnerDescriptor {
        root,
        initializer,
        span,
        bytes,
        drop: *drop,
    })
}
