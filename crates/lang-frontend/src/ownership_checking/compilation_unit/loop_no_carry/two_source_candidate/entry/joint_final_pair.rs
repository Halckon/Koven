//! Exact two-root, two-call joint final-use proof; preserve independent declaration/drop orders.
use super::*;
type Initializer = (UnitSymbolId, UnitExpressionId, Span, Vec<u8>);
pub(super) fn finish(
    initializers: [Initializer; 2],
    first: EntryCallDescriptor,
    second: EntryCallDescriptor,
    owned: &CompilationUnitOwnership,
) -> Option<EntryCalls> {
    if first.instance() != second.instance() || !first.instance().type_arguments().is_empty() {
        return None;
    }
    let (owners, terminal_drops) = prove(initializers, &first, &second, owned)?;
    Some(EntryCalls::JointFinalNamedPair {
        owners: Box::new(owners),
        terminal_drops: Box::new(terminal_drops),
        calls: Box::new([first, second]),
    })
}
pub(super) fn prove(
    initializers: [Initializer; 2],
    first: &EntryCallDescriptor,
    second: &EntryCallDescriptor,
    owned: &CompilationUnitOwnership,
) -> Option<([EntryOwnerDescriptor; 2], [UnitDropFact; 2])> {
    let roots = [initializers[0].0, initializers[1].0];
    if roots[0] == roots[1] {
        return None;
    }
    for call in [first, second] {
        let StringStorage::StaggeredRoots { roots: original } = call.storage else {
            return None;
        };
        if original != roots || call.temporary_scalars.iter().any(Option::is_none) {
            return None;
        }
        let borrowed: Vec<_> = (0..4)
            .filter(|i| call.argument_role(*i).is_some_and(|r| r < 2))
            .map(|i| call.named_argument_root(i))
            .collect::<Option<_>>()?;
        if borrowed.len() != 2
            || borrowed[0] == borrowed[1]
            || borrowed.iter().any(|r| !roots.contains(r))
        {
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
    let at = |call: &EntryCallDescriptor| {
        owned
            .drops()
            .iter()
            .filter(|d| d.point() == UnitDropPoint::CallReturn(call.contracts()[0].call()))
            .copied()
            .collect::<Vec<_>>()
    };
    if !at(first).is_empty() {
        return None;
    }
    let terminal_drops: [UnitDropFact; 2] = at(second).try_into().ok()?;
    if terminal_drops[0].target() == terminal_drops[1].target() {
        return None;
    }
    for drop in &terminal_drops {
        let UnitDropTarget::Named(root) = drop.target() else {
            return None;
        };
        if !roots.contains(&root)
            || owned
                .drops()
                .iter()
                .filter(|d| d.target() == drop.target())
                .count()
                != 1
        {
            return None;
        }
    }
    let owners: Vec<_> = initializers
        .into_iter()
        .map(|(root, initializer, span, bytes)| {
            let drop = *terminal_drops
                .iter()
                .find(|d| d.target() == UnitDropTarget::Named(root))?;
            Some(EntryOwnerDescriptor {
                root,
                initializer,
                span,
                bytes,
                drop,
            })
        })
        .collect::<Option<_>>()?;
    Some((owners.try_into().ok()?, terminal_drops))
}
