//! Detached closed-arm helpers only; these are not the original enclosing function or loop.
use super::{e2_callee_leaves, e2_two_source_plan::Plan, model::*};
use lang_frontend::{
    ownership_checking::two_source_experiment::{TwoSourceCounterFact, TwoSourceExperiment},
    type_checking::ValidatedCompilationUnitTypes,
};
pub(super) struct EmittedArms {
    pub(super) leaves: e2_callee_leaves::EmittedLeaves,
    pub(super) arms: [FunctionId; 2],
    pub(super) columns: [usize; 2],
}
pub(super) fn emit(
    experiment: &TwoSourceExperiment,
    fact: &TwoSourceCounterFact,
    typed: &ValidatedCompilationUnitTypes,
) -> Option<EmittedArms> {
    let plan = Plan::new(experiment, fact, typed)?;
    let spans = experiment.arm_spans(fact)?;
    let mut leaves = e2_callee_leaves::emit(experiment, typed)?;
    // The helper has only the two root parameters, ordered by their original signature roles.
    let columns = if plan.source_roles()[0] < plan.source_roles()[1] {
        [0, 1]
    } else {
        [1, 0]
    };
    let module = leaves.program.modules.first_mut()?;
    let string = module.intern_type(SsaTypeKind::StringOwner);
    let shared = EntityType::Loan {
        kind: LoanKind::Shared,
        target: string,
    };
    let mut arms = Vec::new();
    for arm_index in 0..2 {
        let source = &spans[arm_index];
        let calls = &fact.calls()[arm_index];
        if calls.len() != source.calls().len() || calls.len() < 2 {
            return None;
        }
        let origin = Origin::Synthetic {
            anchor: source.body(),
            reason: "detached verified E2 arm".to_owned(),
        };
        let id = module
            .add_function(format!("e2_arm_{arm_index}"), vec![], origin.clone())
            .ok()?;
        let function = module.function_mut(id)?;
        let block = function
            .add_block(vec![shared, shared], origin.clone())
            .ok()?;
        let EntityId::Loan(parent) = function.block(block)?.parameters[columns[arm_index]] else {
            return None;
        };
        let callee = |key| {
            leaves
                .functions
                .iter()
                .find(|(instance, _)| instance == key)
                .map(|(_, id)| *id)
        };
        let (_, results) = function
            .append_instruction(
                block,
                Operation::BorrowCall {
                    callee: callee(&calls[0].1)?,
                    arguments: vec![EntityId::Loan(parent)],
                    source: parent,
                },
                vec![shared],
                Origin::Source(source.calls()[0]),
            )
            .ok()?;
        let [EntityId::Loan(child)] = results.as_slice() else {
            return None;
        };
        for (position, (_, key)) in calls.iter().enumerate().skip(1) {
            function
                .append_instruction(
                    block,
                    Operation::DirectCall {
                        callee: callee(key)?,
                        receiver: None,
                        arguments: vec![EntityId::Loan(*child)],
                    },
                    vec![],
                    Origin::Source(source.calls()[position]),
                )
                .ok()?;
        }
        function
            .append_instruction(
                block,
                Operation::BorrowEnd { loan: *child },
                vec![],
                Origin::Source(source.end()),
            )
            .ok()?;
        function
            .set_terminator(block, TerminatorKind::Return { values: vec![] }, origin)
            .ok()?;
        arms.push(id);
    }
    super::verify::verify_program(&leaves.program).ok()?;
    Some(EmittedArms {
        leaves,
        arms: arms.try_into().ok()?,
        columns,
    })
}
