//! Distinct closed instance proof; second-call initialization must stay inside its own phase.
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
    for call in [&first, &second] {
        if !call.instance.type_arguments().is_empty() {
            return None;
        }
        let origin = call.contracts[0].call();
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
    late_pair::prove(&first, &second, owned, parsed)?;
    Some(EntryCalls::DifferentLateInitializedPair(Box::new([
        first, second,
    ])))
}
