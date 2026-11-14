//! One same-run ordered source entry dispatch. Not ordinary driver admission or fallback.
use super::*;
use lang_frontend::ownership_checking::d10_catalog_experiment::{
    BoundNamedEntry, NamedEntryOrder, NamedEntryView,
};

/// Immutable checked publication selected by a single sealed frontend analysis.
/// Neither variant can be constructed, swapped, or converted to an ordinary driver capability.
pub struct NamedEntryProgram<'view, 'parsed: 'view> {
    publication: Publication<'view, 'parsed>,
}
enum Publication<'view, 'parsed: 'view> {
    Before(NamedWrapperProgram<'view, 'parsed>),
    After(NamedAfterChildProgram<'view, 'parsed>),
}
impl NamedEntryProgram<'_, '_> {
    /// Exact restricted source order, not a general Carry permission.
    #[must_use]
    pub const fn order(&self) -> NamedEntryOrder {
        match self.publication {
            Publication::Before(_) => NamedEntryOrder::BeforeChild,
            Publication::After(_) => NamedEntryOrder::AfterChild,
        }
    }
    /// Original zero-argument source wrapper identity.
    #[must_use]
    pub fn callable(&self) -> UnitItemId {
        match &self.publication {
            Publication::Before(p) => p.callable(),
            Publication::After(p) => p.callable(),
        }
    }
    /// Render and verify the complete immutable graph using its original order-specific proof.
    pub fn verified_llvm_ir(&self) -> Result<String, Error> {
        match &self.publication {
            Publication::Before(p) => p.verified_llvm_ir(),
            Publication::After(p) => p.verified_llvm_ir(),
        }
    }
    #[cfg(test)]
    pub(crate) fn replay_input(&self) -> (&Program, FunctionId) {
        match &self.publication {
            Publication::Before(p) => p.replay_input(),
            Publication::After(p) => p.replay_input(),
        }
    }
    /// Publish a real process object through the existing order-specific atomic transaction.
    /// No default driver selection, linking, execution or retry with another order is performed.
    pub fn emit_process_object(
        &self,
        output: &std::path::Path,
    ) -> Result<(), crate::NativeObjectError> {
        match &self.publication {
            Publication::Before(p) => p.emit_process_object(output),
            Publication::After(p) => p.emit_process_object(output),
        }
    }
}
/// Dispatch only the already selected same-run capability to its original complete graph checker.
/// Failure is terminal; the other order is never tried and no partial publication is returned.
pub fn lower_named_entry<'view, 'parsed: 'view>(
    unit: OwnedCompilationUnitView<'view, 'parsed>,
    bound: &'view BoundNamedEntry,
) -> Result<NamedEntryProgram<'view, 'parsed>, Error> {
    if !bound.matches(&unit) {
        return Err(Error::new(
            Stage::Identity,
            "named entry final ownership differs",
        ));
    }
    let publication = match bound.view() {
        NamedEntryView::BeforeChild(b) => Publication::Before(lower_named_wrapper(unit, b)?),
        NamedEntryView::AfterChild(b) => {
            Publication::After(lower_named_after_child_wrapper(unit, b)?)
        }
    };
    Ok(NamedEntryProgram { publication })
}
