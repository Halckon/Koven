//! Shared named root and two call-local temporaries, with exact original cleanup vectors.
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
    let mut temporaries = Vec::new();
    for call in [&first, &second] {
        let StringStorage::SharedMixed {
            root: actual,
            temporary_index,
            drop: temporary,
            ..
        } = &call.storage
        else {
            return None;
        };
        if *actual != root
            || temporary.target()
                != UnitDropTarget::Temporary(call.contracts[*temporary_index].argument())
            || temporaries
                .iter()
                .any(|old: &UnitDropFact| old.target() == temporary.target())
            || owned
                .drops()
                .iter()
                .filter(|d| d.target() == temporary.target())
                .count()
                != 1
            || !call.initialization_order.is_empty()
            || call.temporary_scalars.iter().any(Option::is_none)
            || !call.instance.type_arguments().is_empty()
        {
            return None;
        }
        temporaries.push(*temporary);
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
    let [first_temporary, second_temporary]: [_; 2] = temporaries.try_into().ok()?;
    if first_drops.as_slice() != [first_temporary]
        || final_drops.as_slice() != [second_temporary, drop]
    {
        return None;
    }
    Some(EntryCalls::DifferentSharedMixedPair {
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
