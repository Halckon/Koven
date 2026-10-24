//! Original single-file fixtures analyzed once as real MemoryDocuments, never Legacy products.
use super::*;

pub(super) fn observe(case: &Case, trace: &mut Trace) -> Outcome<()> {
    unit::observe_impl(case, trace, unit::Layout::Memory, false, false)
}

pub(super) fn observe_read_identity(case: &Case, trace: &mut Trace) -> Outcome<()> {
    unit::observe_impl(case, trace, unit::Layout::Memory, true, false)
}

pub(super) fn observe_consume_identity(case: &Case, trace: &mut Trace) -> Outcome<()> {
    unit::observe_impl(case, trace, unit::Layout::Memory, false, true)
}
