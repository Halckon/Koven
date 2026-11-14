//! Real lowering of the restricted source leaf closure; no loop body or receipt publication.
use super::model::*;
use lang_frontend::{
    ownership_checking::two_source_experiment::{
        CalleeLeaf, CalleeLeafBody, ExperimentalUnitHandoff, TwoSourceCounterFact,
        TwoSourceExperiment,
    },
    type_checking::{UnitCallableInstanceKey, ValidatedCompilationUnitTypes},
};

pub(super) struct EmittedLeaves {
    pub(super) program: Program,
    pub(super) functions: Vec<(UnitCallableInstanceKey, FunctionId)>,
}

pub(super) fn emit(
    experiment: &TwoSourceExperiment,
    typed: &ValidatedCompilationUnitTypes,
) -> Option<EmittedLeaves> {
    emit_selected(
        experiment,
        typed,
        experiment.callee_leaves()?.iter().collect(),
    )
}
/// Emit only the exact source-leaf closure of this owned fact.
pub(super) fn emit_for_fact(
    experiment: &TwoSourceExperiment,
    fact: &TwoSourceCounterFact,
    typed: &ValidatedCompilationUnitTypes,
) -> Option<EmittedLeaves> {
    if !experiment.owns(fact) {
        return None;
    }
    let keys: Vec<_> = fact.calls().iter().flatten().map(|(_, key)| key).collect();
    let leaves: Vec<_> = experiment
        .callee_leaves_for(fact, typed)?
        .iter()
        .filter(|leaf| keys.contains(&leaf.instance()))
        .collect();
    if keys
        .iter()
        .any(|key| leaves.iter().filter(|leaf| leaf.instance() == *key).count() != 1)
    {
        return None;
    }
    emit_selected(experiment, typed, leaves)
}
/// One allocator for the explicit set's exact leaf union. No callers, helpers or entry yet.
pub(super) fn emit_for_facts(
    handoff: &ExperimentalUnitHandoff<'_, '_>,
    experiment: &TwoSourceExperiment,
    facts: &[&TwoSourceCounterFact],
) -> Option<EmittedLeaves> {
    let typed = handoff.types();
    if !experiment.matches_types(typed) || facts.iter().any(|f| !experiment.owns(f)) {
        return None;
    }
    let selection = handoff.for_facts(facts)?;
    let parsed =
        super::lowering_support::parsed_by_source_unit(selection.inputs(), selection.names())
            .ok()?;
    super::unit_lower::preflight::validate(&parsed, selection.names(), typed.types(), &selection)
        .ok()?;
    let mut leaves: Vec<&CalleeLeaf> = Vec::new();
    for fact in facts {
        for leaf in experiment.callee_leaves_for(fact, typed)? {
            if let Some(previous) = leaves.iter().find(|old| old.instance() == leaf.instance()) {
                if previous.body() != leaf.body() || previous.span() != leaf.span() {
                    return None;
                }
            } else {
                leaves.push(leaf);
            }
        }
    }
    let emitted = emit_selected(experiment, typed, leaves)?;
    // Re-derive required keys from actual typed calls, not the emitter's selected definitions.
    let mut required = Vec::new();
    for fact in facts {
        for (call, recorded) in fact.calls().iter().flatten() {
            let actual = typed.types().call(*call)?.instance();
            if actual != recorded {
                return None;
            }
            if !required.contains(actual) {
                required.push(actual.clone());
            }
        }
    }
    let [module] = emitted.program.modules.as_slice() else {
        return None;
    };
    if emitted.functions.len() != required.len() || module.functions.len() != required.len() {
        return None;
    }
    let mut ids = Vec::new();
    for key in required {
        let mut selected = emitted
            .functions
            .iter()
            .filter(|(candidate, _)| *candidate == key);
        let (_, id) = selected.next()?;
        if selected.next().is_some() || ids.contains(id) {
            return None;
        }
        ids.push(*id);
        for fact in facts {
            if !fact
                .calls()
                .iter()
                .flatten()
                .any(|(_, needed)| *needed == key)
            {
                continue;
            }
            let mut witnesses = experiment
                .callee_leaves_for(fact, typed)?
                .iter()
                .filter(|leaf| *leaf.instance() == key);
            let leaf = witnesses.next()?;
            if witnesses.next().is_some() || !experiment.owns_leaf(leaf) {
                return None;
            }
            super::e2_replay::replay_leaf(module, *id, leaf)?;
        }
    }
    Some(emitted)
}
pub(super) fn emit_selected(
    experiment: &TwoSourceExperiment,
    typed: &ValidatedCompilationUnitTypes,
    leaves: Vec<&CalleeLeaf>,
) -> Option<EmittedLeaves> {
    if !experiment.matches_types(typed) {
        return None;
    }
    if leaves.is_empty() {
        return None;
    }
    let mut program = Program::default();
    let id = program.add_module("e2-experimental-leaves");
    let module = program.module_mut(id)?;
    let string = module.intern_type(SsaTypeKind::StringOwner);
    let mut functions = Vec::new();
    for (index, leaf) in leaves.iter().enumerate() {
        if !experiment.owns_leaf(leaf) || functions.iter().any(|(key, _)| key == leaf.instance()) {
            return None;
        }
        let identity = leaf.body() == CalleeLeafBody::SharedIdentity;
        let function = module
            .add_function(
                format!("e2_leaf_{index}"),
                if identity { vec![string] } else { vec![] },
                Origin::Source(leaf.span()),
            )
            .ok()?;
        let body = module.function_mut(function)?;
        body.borrow_return = identity.then_some(0);
        let entry = body
            .add_block(
                vec![EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: string,
                }],
                Origin::Source(leaf.span()),
            )
            .ok()?;
        if leaf.body() == CalleeLeafBody::PrintSharedParameter {
            let [EntityId::Loan(parameter)] = body.block(entry)?.parameters.as_slice() else {
                return None;
            };
            body.append_instruction(
                entry,
                Operation::PrintString { value: *parameter },
                vec![],
                Origin::Source(leaf.print_loan()?.end_span()),
            )
            .ok()?;
        }
        let terminator = if identity {
            let [EntityId::Loan(loan)] = body.block(entry)?.parameters.as_slice() else {
                return None;
            };
            TerminatorKind::BorrowReturn { loan: *loan }
        } else {
            TerminatorKind::Return { values: vec![] }
        };
        body.set_terminator(entry, terminator, Origin::Source(leaf.span()))
            .ok()?;
        functions.push((leaf.instance().clone(), function));
    }
    // No partially emitted module escapes on model failure.
    super::verify::verify_program(&program).ok()?;
    Some(EmittedLeaves { program, functions })
}
