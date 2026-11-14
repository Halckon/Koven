//! Real lowering of the restricted source leaf closure; no loop body or receipt publication.
use super::model::*;
use lang_frontend::{
    ownership_checking::two_source_experiment::{CalleeLeafBody, TwoSourceExperiment},
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
    if !experiment.matches_types(typed) {
        return None;
    }
    let leaves = experiment.callee_leaves()?;
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
