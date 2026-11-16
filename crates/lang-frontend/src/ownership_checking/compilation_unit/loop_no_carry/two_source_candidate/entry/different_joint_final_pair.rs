//! Private distinct-callee wrapper around the exact joint terminal lifetime proof.
use super::*;
type Initializer = (UnitSymbolId, UnitExpressionId, Span, Vec<u8>);
pub(super) fn finish(
    initializers: [Initializer; 2],
    first: EntryCallDescriptor,
    second: EntryCallDescriptor,
    owned: &CompilationUnitOwnership,
) -> Option<EntryCalls> {
    if first.instance() == second.instance()
        || !first.instance().type_arguments().is_empty()
        || !second.instance().type_arguments().is_empty()
    {
        return None;
    }
    let (owners, terminal_drops) = joint_final_pair::prove(initializers, &first, &second, owned)?;
    Some(EntryCalls::DifferentJointFinalNamedPair {
        owners: Box::new(owners),
        terminal_drops: Box::new(terminal_drops),
        calls: Box::new([first, second]),
    })
}
