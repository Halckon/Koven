//! Two named roots: select lifetime endpoints from original argument loans, then check Drops.
use super::*;
type Initializer = (UnitSymbolId, UnitExpressionId, Span, Vec<u8>);

pub(super) fn finish(
    initializers: [Initializer; 2],
    first: EntryCallDescriptor,
    second: EntryCallDescriptor,
    owned: &CompilationUnitOwnership,
) -> Option<EntryCalls> {
    let call_roots = |call: &EntryCallDescriptor| -> Option<[UnitSymbolId; 2]> {
        let roots: Vec<_> = (0..4)
            .filter(|i| call.argument_role(*i).is_some_and(|role| role < 2))
            .map(|i| call.named_argument_root(i))
            .collect::<Option<_>>()?;
        roots.try_into().ok()
    };
    let initial_roots = [initializers[0].0, initializers[1].0];
    let first_roots = call_roots(&first)?;
    let second_roots = call_roots(&second)?;
    if initial_roots[0] == initial_roots[1]
        || first_roots[0] == first_roots[1]
        || first_roots.iter().any(|root| !initial_roots.contains(root))
        || second_roots[0] != second_roots[1]
        || !initial_roots.contains(&second_roots[0])
    {
        return None;
    }
    let survivor = second_roots[0];
    let early = *initial_roots.iter().find(|root| **root != survivor)?;
    let mut drops = Vec::new();
    for (call, root) in [(&first, early), (&second, survivor)] {
        let point = UnitDropPoint::CallReturn(call.contracts()[0].call());
        let actual: Vec<_> = owned
            .drops()
            .iter()
            .filter(|drop| drop.point() == point)
            .copied()
            .collect();
        let [drop] = actual.as_slice() else {
            return None;
        };
        if drop.target() != UnitDropTarget::Named(root)
            || owned
                .drops()
                .iter()
                .filter(|d| d.target() == UnitDropTarget::Named(root))
                .count()
                != 1
        {
            return None;
        }
        drops.push(*drop);
    }
    let owners: Vec<_> = initializers
        .into_iter()
        .map(|(root, initializer, span, bytes)| {
            let drop = *drops
                .iter()
                .find(|drop| drop.target() == UnitDropTarget::Named(root))?;
            Some(EntryOwnerDescriptor {
                root,
                initializer,
                span,
                bytes,
                drop,
            })
        })
        .collect::<Option<_>>()?;
    Some(EntryCalls::StaggeredNamedPair {
        owners: Box::new(owners.try_into().ok()?),
        calls: Box::new([first, second]),
    })
}
