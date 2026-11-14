//! Immutable experimental entry closure. Not a normal owned/native driver capability.
use super::{e2_entry, e2_entry_replay, model::*};
use lang_frontend::{
    name_resolution::DeclarationId,
    ownership_checking::two_source_experiment::{
        EntryDescriptor, TwoSourceCounterFact, TwoSourceExperiment,
    },
    type_checking::ValidatedCompilationUnitTypes,
};
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Error {
    Required,
    Selection,
    Emission,
    Replay,
    Verify,
}
/// Private construction; no mutable/consuming Program escape and no synthetic driver appended.
pub(super) struct PublishedEntry<'a> {
    program: Program,
    entry: FunctionId,
    required: &'a EntryDescriptor,
}
impl PublishedEntry<'_> {
    pub(super) fn program(&self) -> &Program {
        &self.program
    }
    pub(super) fn entry(&self) -> FunctionId {
        self.entry
    }
    pub(super) fn declaration(&self) -> DeclarationId {
        self.required.entry()
    }
}
/// Scope: one original entry and its complete bounded caller/helper/leaf closure,
/// not all declarations in the Unit. Selection must cover the required fact exactly once.
pub(super) fn publish<'a>(
    experiment: &'a TwoSourceExperiment,
    required: &'a EntryDescriptor,
    fact: &'a TwoSourceCounterFact,
    selected: &[&TwoSourceCounterFact],
    typed: &'a ValidatedCompilationUnitTypes,
) -> Result<PublishedEntry<'a>, Error> {
    if !experiment.matches_types(typed)
        || !experiment.owns_entry(required)
        || !experiment.owns(fact)
        || experiment.function_shell(fact, typed).is_none()
    {
        return Err(Error::Required);
    }
    let [selection] = selected else {
        return Err(Error::Selection);
    };
    if !experiment.owns(selection)
        || !fact
            .arms()
            .iter()
            .zip(selection.arms())
            .all(|(a, b)| a.same_identity(b))
    {
        return Err(Error::Selection);
    }
    let emitted = e2_entry::emit(experiment, fact, required, typed).ok_or(Error::Emission)?;
    e2_entry_replay::replay(&emitted, experiment, fact, typed).ok_or(Error::Replay)?;
    super::verify::verify_program(&emitted.function.fragments.leaves.program)
        .map_err(|_| Error::Verify)?;
    Ok(PublishedEntry {
        program: emitted.function.fragments.leaves.program,
        entry: emitted.entry,
        required,
    })
}
