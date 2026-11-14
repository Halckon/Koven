//! Default-off atomic process-object bridge. Accepts only the immutable checked publication.
use super::*;
use crate::d10_experiment::SplitNamedEntryProgram;

pub(crate) fn emit(
    publication: &SplitNamedEntryProgram<'_, '_>,
    output: &Path,
) -> Result<(), NativeObjectError> {
    emit_impl(publication, output, false)
}

pub(crate) fn emit_new(
    publication: &SplitNamedEntryProgram<'_, '_>,
    output: &Path,
) -> Result<(), NativeObjectError> {
    emit_impl(publication, output, true)
}

fn emit_impl(
    publication: &SplitNamedEntryProgram<'_, '_>,
    output: &Path,
    create_new: bool,
) -> Result<(), NativeObjectError> {
    let (unit, bound) = publication.process_source();
    let (program, function) = publication.process_graph();
    if !bound.matches(unit) {
        return Err(NativeObjectError {
            kind: NativeObjectErrorKind::MismatchedAnalysis,
            span: None,
            detail: "split named entry final ownership analysis differs".to_owned(),
            diagnostic: None,
        });
    }
    let callable = bound.entry();
    let mut entries = unit
        .names()
        .names()
        .index()
        .declarations()
        .iter()
        .filter(|d| d.source_unit() == callable.source_unit() && d.root() == callable.item());
    let declaration = entries.next().ok_or_else(|| invalid_entry(None))?;
    if entries.next().is_some() {
        return Err(invalid_entry(Some(declaration.name_span())));
    }
    let entry = NativeUnitEntry::NoArguments(declaration.id());
    validate_unit_entry(unit.names(), unit.types(), entry)?;
    // Recheck the original Program and exact function identity, never lower a substitute graph.
    crate::ssa::d10_named_wrapper_graph::check_split(unit, bound, program, function).map_err(
        |_| NativeObjectError {
            kind: NativeObjectErrorKind::InvalidModel,
            span: Some(declaration.name_span()),
            detail: "split named entry graph no longer proves its source/ownership contract"
                .to_owned(),
            diagnostic: None,
        },
    )?;
    let plan = native_unit_entry_plan(program, entry, function)?;
    let module = program
        .module(function.module())
        .ok_or_else(|| invalid_entry(None))?;
    llvm::entry::validate(module, plan)
        .map_err(|error| map_backend_error(unit.sources(), program, error))?;
    // All source/entry/graph/plan checks precede filesystem effects. Backend output remains
    // in the existing sibling transaction; LLVM emits the genuine process main glue.
    let temporary = SiblingObject::reserve(output)?;
    llvm::emit_verified_object(program, unit.sources(), plan, temporary.path())
        .map_err(|error| map_backend_error(unit.sources(), program, error))?;
    if create_new {
        temporary.commit_new(output)
    } else {
        temporary.commit(output)
    }
}
