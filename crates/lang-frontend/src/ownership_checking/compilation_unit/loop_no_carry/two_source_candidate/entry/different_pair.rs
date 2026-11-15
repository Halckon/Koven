//! Two complete temporary lifetimes with different original closed callable instances.
use super::*;
pub(super) fn finish(
    first: EntryCallDescriptor,
    second: EntryCallDescriptor,
    owned: &CompilationUnitOwnership,
    parsed: &crate::parser::ParsedFile,
) -> Option<EntryCalls> {
    if first.instance == second.instance {
        return None;
    }
    let mut previous_end = None;
    let mut targets = Vec::new();
    for call in [&first, &second] {
        let StringStorage::Temporary { drops, .. } = &call.storage else {
            return None;
        };
        if !call.initialization_order.is_empty()
            || call.temporary_scalars.iter().any(Option::is_none)
            || !call.instance.type_arguments().is_empty()
        {
            return None;
        }
        let origin = call.contracts[0].call();
        let span = parsed
            .ast()
            .expressions()
            .get(origin.expression())
            .ok()?
            .span();
        if previous_end.is_some_and(|end| end >= span.start()) {
            return None;
        }
        previous_end = Some(span.end());
        let original: Vec<_> = owned
            .drops()
            .iter()
            .filter(|fact| fact.point() == UnitDropPoint::CallReturn(origin))
            .copied()
            .collect();
        if original.as_slice() != drops {
            return None;
        }
        let strings: Vec<_> = call
            .contracts
            .iter()
            .enumerate()
            .filter(|(i, _)| call.argument_role(*i).is_some_and(|role| role < 2))
            .map(|(_, contract)| UnitDropTarget::Temporary(contract.argument()))
            .collect();
        let [left, right] = strings.as_slice() else {
            return None;
        };
        if drops.iter().map(|fact| fact.target()).collect::<Vec<_>>() != [*right, *left] {
            return None;
        }
        for drop in drops {
            if targets.contains(&drop.target())
                || owned
                    .drops()
                    .iter()
                    .filter(|fact| fact.target() == drop.target())
                    .count()
                    != 1
            {
                return None;
            }
            targets.push(drop.target());
        }
        // Existing collector establishes every complete loan's target, kind and source operand.
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
    Some(EntryCalls::DifferentTemporaryPair(Box::new([
        first, second,
    ])))
}
