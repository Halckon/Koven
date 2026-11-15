//! Distinct-instance authority adds original phase ordering to fixed two-root last-use evidence.
use super::*;
type Initializer = (UnitSymbolId, UnitExpressionId, Span, Vec<u8>);
pub(super) fn finish(
    initializers: [Initializer; 2],
    first: EntryCallDescriptor,
    second: EntryCallDescriptor,
    owned: &CompilationUnitOwnership,
    parsed: &crate::parser::ParsedFile,
) -> Option<EntryCalls> {
    if first.instance == second.instance || initializers[0].2.end() >= initializers[1].2.start() {
        return None;
    }
    let roots = [initializers[0].0, initializers[1].0];
    let mut previous_end = initializers[1].2.end();
    for call in [&first, &second] {
        if !matches!(&call.storage,StringStorage::StaggeredRoots{roots:actual}if *actual==roots)
            || !call.initialization_order.is_empty()
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
        if previous_end >= span.start() {
            return None;
        }
        previous_end = span.end();
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
    let owners = staggered_pair::prove_owners(initializers, &first, &second, owned)?;
    Some(EntryCalls::DifferentStaggeredNamedPair {
        owners: Box::new(owners),
        calls: Box::new([first, second]),
    })
}
