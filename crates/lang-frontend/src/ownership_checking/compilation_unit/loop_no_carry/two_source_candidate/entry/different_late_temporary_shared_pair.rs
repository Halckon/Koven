//! Distinct closed-call authority for shared-only then one late temporary.
use super::*;
pub(super) fn finish(
    owner: (UnitSymbolId, UnitExpressionId, Span, Vec<u8>),
    first: EntryCallDescriptor,
    second: EntryCallDescriptor,
    owned: &CompilationUnitOwnership,
    parsed: &crate::parser::ParsedFile,
) -> Option<EntryCalls> {
    if first.instance() == second.instance() {
        return None;
    }
    let mut previous_end = owner.2.end();
    for call in [&first, &second] {
        if !call.instance().type_arguments().is_empty()
            || !call.initialization_order().is_empty()
            || !call.named_strings().is_empty()
        {
            return None;
        }
        let span = parsed
            .ast()
            .expressions()
            .get(call.contracts()[0].call().expression())
            .ok()?
            .span();
        if previous_end >= span.start() {
            return None;
        }
        previous_end = span.end();
    }
    let owner = late_temporary_shared_pair::prove(owner, &first, &second, owned)?;
    Some(EntryCalls::DifferentLateTemporarySharedPair {
        owner: Box::new(owner),
        calls: Box::new([first, second]),
    })
}
