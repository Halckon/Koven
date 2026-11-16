//! Separate root authority for exactly one named owner shared by two distinct closed calls.
use super::*;

pub(super) fn finish(
    (root, initializer, span, bytes): (UnitSymbolId, UnitExpressionId, Span, Vec<u8>),
    first: EntryCallDescriptor,
    second: EntryCallDescriptor,
    owned: &CompilationUnitOwnership,
    parsed: &crate::parser::ParsedFile,
) -> Option<EntryCalls> {
    if first.instance == second.instance {
        return None;
    }
    let mut previous_end = span.end();
    for call in [&first, &second] {
        if !matches!(&call.storage, StringStorage::SharedRoot { root: actual } if *actual == root)
            || !call.initialization_order.is_empty()
            || call.temporary_scalars.iter().any(Option::is_none)
            || !call.instance.type_arguments().is_empty()
        {
            return None;
        }
        let origin = call.contracts[0].call();
        let call_span = parsed
            .ast()
            .expressions()
            .get(origin.expression())
            .ok()?
            .span();
        if previous_end >= call_span.start() {
            return None;
        }
        previous_end = call_span.end();
        for (index, end) in call.ends.iter().enumerate() {
            if end.loan().call() != origin
                || end.point() != UnitDropPoint::CallReturn(origin)
                || end.preceding_drops() != 0
                || end.preceding_call_loan_ends() != index
            {
                return None;
            }
        }
    }
    let mut named = owned
        .drops()
        .iter()
        .filter(|d| d.target() == UnitDropTarget::Named(root));
    let drop = *named.next()?;
    if named.next().is_some() {
        return None;
    }
    let first_call = first.contracts[0].call();
    let last_call = second.contracts[0].call();
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
    if !first_drops.is_empty() || final_drops.as_slice() != [drop] {
        return None;
    }
    Some(EntryCalls::DifferentSharedNamedPair {
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
