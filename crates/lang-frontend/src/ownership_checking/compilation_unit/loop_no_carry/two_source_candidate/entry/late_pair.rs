//! A bounded phase boundary: complete temporary call, then original named formation and call.
use super::*;

pub(super) fn finish(
    first: EntryCallDescriptor,
    second: EntryCallDescriptor,
    owned: &CompilationUnitOwnership,
    parsed: &crate::parser::ParsedFile,
) -> Option<EntryCalls> {
    prove(&first, &second, owned, parsed)?;
    Some(EntryCalls::LateInitializedPair(Box::new([first, second])))
}

pub(super) fn prove(
    first: &EntryCallDescriptor,
    second: &EntryCallDescriptor,
    owned: &CompilationUnitOwnership,
    parsed: &crate::parser::ParsedFile,
) -> Option<()> {
    let StringStorage::Temporary {
        drops: first_drops, ..
    } = &first.storage
    else {
        return None;
    };
    let StringStorage::Named {
        root, span, drop, ..
    } = &second.storage
    else {
        return None;
    };
    if !first.initialization_order.is_empty()
        || second.initialization_order.as_slice() != [EntryInitialization::Named(*root)]
        || first.temporary_scalars.iter().any(Option::is_none)
        || second.temporary_scalars.iter().any(Option::is_none)
    {
        return None;
    }
    let first_call = first.contracts[0].call();
    let second_call = second.contracts[0].call();
    let first_span = parsed
        .ast()
        .expressions()
        .get(first_call.expression())
        .ok()?
        .span();
    let second_span = parsed
        .ast()
        .expressions()
        .get(second_call.expression())
        .ok()?
        .span();
    if first_span.end() >= span.start() || span.end() >= second_span.start() {
        return None;
    }
    let first_targets: Vec<_> = first
        .contracts
        .iter()
        .enumerate()
        .filter(|(index, _)| first.argument_role(*index).is_some_and(|role| role < 2))
        .map(|(_, contract)| UnitDropTarget::Temporary(contract.argument()))
        .collect();
    let [left, right] = first_targets.as_slice() else {
        return None;
    };
    // Preserve the observed planner vector, never sort or synthesize destructor events.
    if first_drops
        .iter()
        .map(|fact| fact.target())
        .collect::<Vec<_>>()
        != [*right, *left]
        || drop.target() != UnitDropTarget::Named(*root)
    {
        return None;
    }
    for (call, expected) in [
        (first_call, first_drops.as_slice()),
        (second_call, std::slice::from_ref(drop)),
    ] {
        let actual: Vec<_> = owned
            .drops()
            .iter()
            .filter(|fact| fact.point() == UnitDropPoint::CallReturn(call))
            .copied()
            .collect();
        if actual.as_slice() != expected {
            return None;
        }
        for fact in expected {
            if owned
                .drops()
                .iter()
                .filter(|candidate| candidate.target() == fact.target())
                .count()
                != 1
            {
                return None;
            }
        }
    }
    Some(())
}
