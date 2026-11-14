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
    publish_inner(experiment, required, fact, selected, typed, None)
}
/// Closed CI-only fault vocabulary, not an arbitrary mutation callback.
pub(super) enum CiFault {
    WrongStringArguments,
    WrongRounds,
    MissingTerminator,
}
pub(super) fn publish_ci<'a>(
    experiment: &'a TwoSourceExperiment,
    required: &'a EntryDescriptor,
    fact: &'a TwoSourceCounterFact,
    typed: &'a ValidatedCompilationUnitTypes,
    fault: CiFault,
) -> Result<PublishedEntry<'a>, Error> {
    assert_eq!(std::env::var("CI").as_deref(), Ok("true"));
    assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(), Ok("true"));
    publish_inner(experiment, required, fact, &[fact], typed, Some(fault))
}
fn publish_inner<'a>(
    experiment: &'a TwoSourceExperiment,
    required: &'a EntryDescriptor,
    fact: &'a TwoSourceCounterFact,
    selected: &[&TwoSourceCounterFact],
    typed: &'a ValidatedCompilationUnitTypes,
    fault: Option<CiFault>,
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
    let mut emitted = e2_entry::emit(experiment, fact, required, typed).ok_or(Error::Emission)?;
    if matches!(
        fault,
        Some(CiFault::WrongStringArguments | CiFault::WrongRounds)
    ) {
        let f = emitted.function.fragments.leaves.program.modules[0]
            .function_mut(emitted.entry)
            .ok_or(Error::Emission)?;
        match fault {
            Some(CiFault::WrongStringArguments) => {
                let call = f
                    .instructions
                    .iter_mut()
                    .find(|i| matches!(i.operation, Operation::DirectCall { .. }))
                    .ok_or(Error::Emission)?;
                let Operation::DirectCall { arguments, .. } = &mut call.operation else {
                    return Err(Error::Emission);
                };
                arguments.swap(0, 1);
            }
            Some(CiFault::WrongRounds) => {
                let first = f.instructions.first_mut().ok_or(Error::Emission)?;
                if !matches!(
                    first.operation,
                    Operation::Constant(ScalarConstant::Integer(_))
                ) {
                    return Err(Error::Emission);
                }
                first.operation =
                    Operation::Constant(ScalarConstant::Integer(i128::from(required.rounds()) + 1));
            }
            _ => unreachable!(),
        }
        // These faults must exercise semantic replay, not malformed SSA rejection.
        super::verify::verify_program(&emitted.function.fragments.leaves.program)
            .map_err(|_| Error::Verify)?;
    }
    e2_entry_replay::replay(&emitted, experiment, fact, typed).ok_or(Error::Replay)?;
    if matches!(fault, Some(CiFault::MissingTerminator)) {
        emitted.function.fragments.leaves.program.modules[0]
            .function_mut(emitted.entry)
            .ok_or(Error::Emission)?
            .blocks[0]
            .terminator = None;
    }
    super::verify::verify_program(&emitted.function.fragments.leaves.program)
        .map_err(|_| Error::Verify)?;
    Ok(PublishedEntry {
        program: emitted.function.fragments.leaves.program,
        entry: emitted.entry,
        required,
    })
}
