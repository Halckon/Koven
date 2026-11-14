//! Closed single-callable experimental publication; never a normal lowering capability.
use super::{e2_function, e2_replay, model::*};
use lang_frontend::{
    name_resolution::DeclarationId,
    ownership_checking::two_source_experiment::{TwoSourceCounterFact, TwoSourceExperiment},
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
/// No mutable Program出口; copying the Program does not copy this evidence envelope.
pub(super) struct Published<'a> {
    program: Program,
    caller: FunctionId,
    required: Required<'a>,
}
impl Published<'_> {
    pub(super) fn program(&self) -> &Program {
        &self.program
    }
    pub(super) fn caller(&self) -> FunctionId {
        self.caller
    }
    pub(super) fn callable(&self) -> DeclarationId {
        self.required.callable
    }
}
struct Required<'a> {
    experiment: &'a TwoSourceExperiment,
    fact: &'a TwoSourceCounterFact,
    typed: &'a ValidatedCompilationUnitTypes,
    callable: DeclarationId,
}
/// The scope is one explicitly requested complete original callable, not the whole Unit.
/// Selection is supplied separately and must exactly cover that requirement once.
pub(super) fn publish<'a>(
    experiment: &'a TwoSourceExperiment,
    required: &'a TwoSourceCounterFact,
    selected: &[&TwoSourceCounterFact],
    typed: &'a ValidatedCompilationUnitTypes,
) -> Result<Published<'a>, Error> {
    publish_inner(experiment, required, selected, typed, None)
}
/// Closed fault vocabulary: no caller-provided mutation or gate replacement.
pub(super) enum CiFault {
    AfterEmission,
    AfterReplay,
}
pub(super) fn publish_ci<'a>(
    experiment: &'a TwoSourceExperiment,
    required: &'a TwoSourceCounterFact,
    typed: &'a ValidatedCompilationUnitTypes,
    fault: CiFault,
) -> Result<Published<'a>, Error> {
    assert_eq!(std::env::var("CI").as_deref(), Ok("true"));
    assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(), Ok("true"));
    publish_inner(experiment, required, &[required], typed, Some(fault))
}
fn publish_inner<'a>(
    experiment: &'a TwoSourceExperiment,
    required: &'a TwoSourceCounterFact,
    selected: &[&TwoSourceCounterFact],
    typed: &'a ValidatedCompilationUnitTypes,
    fault: Option<CiFault>,
) -> Result<Published<'a>, Error> {
    let shell = experiment
        .function_shell(required, typed)
        .ok_or(Error::Required)?;
    let required = Required {
        experiment,
        fact: required,
        typed,
        callable: shell.callable(),
    };
    let [selected] = selected else {
        return Err(Error::Selection);
    };
    if !experiment.owns(selected)
        || !required
            .fact
            .arms()
            .iter()
            .zip(selected.arms())
            .all(|(a, b)| a.same_identity(b))
    {
        return Err(Error::Selection);
    }
    let mut emitted =
        e2_function::emit(required.experiment, selected, required.typed).ok_or(Error::Emission)?;
    if matches!(fault, Some(CiFault::AfterEmission)) {
        let caller = emitted.fragments.leaves.program.modules[0]
            .function_mut(emitted.caller)
            .ok_or(Error::Emission)?;
        caller.receiver = Some(
            caller
                .entity(caller.blocks[0].parameters[0])
                .ok_or(Error::Emission)?
                .ty,
        );
    }
    e2_replay::replay(&emitted, required.experiment, required.fact, required.typed)
        .ok_or(Error::Replay)?;
    if matches!(fault, Some(CiFault::AfterReplay)) {
        emitted.fragments.leaves.program.modules[0]
            .function_mut(emitted.caller)
            .ok_or(Error::Emission)?
            .blocks[0]
            .terminator = None;
    }
    super::verify::verify_program(&emitted.fragments.leaves.program).map_err(|_| Error::Verify)?;
    Ok(Published {
        program: emitted.fragments.leaves.program,
        caller: emitted.caller,
        required,
    })
}
