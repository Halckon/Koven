//! Root-only final-use proof; retain original per-point Drop order and exact facts.
use super::*;

pub(super) fn finish(
    (root, initializer, span, bytes): (UnitSymbolId, UnitExpressionId, Span, Vec<u8>),
    first: EntryCallDescriptor,
    second: EntryCallDescriptor,
    owned: &CompilationUnitOwnership,
) -> Option<EntryCalls> {
    let first_call = first.contracts()[0].call();
    let last_call = second.contracts()[0].call();
    let first_drops: Vec<_> = owned
        .drops()
        .iter()
        .filter(|d| d.point() == UnitDropPoint::CallReturn(first_call))
        .copied()
        .collect();
    let final_drops: Vec<_> = owned
        .drops()
        .iter()
        .filter(|d| d.point() == UnitDropPoint::CallReturn(last_call))
        .copied()
        .collect();
    let mut named = owned
        .drops()
        .iter()
        .filter(|d| d.target() == UnitDropTarget::Named(root));
    let drop = *named.next()?;
    if named.next().is_some() || drop.point() != UnitDropPoint::CallReturn(last_call) {
        return None;
    }
    match (&first.storage, &second.storage) {
        (StringStorage::SharedRoot { root: a }, StringStorage::SharedRoot { root: b })
            if *a == root && *b == root =>
        {
            if !first_drops.is_empty() || final_drops.as_slice() != [drop] {
                return None;
            }
        }
        (
            StringStorage::SharedMixed {
                root: a,
                drop: first_temp,
                ..
            },
            StringStorage::SharedMixed {
                root: b,
                drop: second_temp,
                ..
            },
        ) if *a == root && *b == root => {
            if first_temp.target() == second_temp.target()
                || first_drops.as_slice() != [*first_temp]
                || final_drops.as_slice() != [*second_temp, drop]
            {
                return None;
            }
            for temporary in [first_temp, second_temp] {
                if !matches!(temporary.target(), UnitDropTarget::Temporary(_))
                    || owned
                        .drops()
                        .iter()
                        .filter(|d| d.target() == temporary.target())
                        .count()
                        != 1
                {
                    return None;
                }
            }
        }
        _ => return None,
    }
    Some(EntryCalls::SharedNamedPair {
        owner: Box::new(EntryOwnerDescriptor {
            root,
            initializer,
            span,
            bytes,
            drop,
        }),
        calls: Box::new([first, second]),
    })
}
