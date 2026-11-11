//! Select only same-analysis Complete protocol 4 records for this refinement instance.
use super::*;
use lang_frontend::ownership_checking::{
    UnitIterationCleanupAction, UnitLoanAction, UnitLoanActionCoverage, UnitLoanTarget,
};
use lang_frontend::type_checking::{UnitCallableTarget, UnitItemId};

impl Plans {
    pub(in crate::ssa::refine) fn new(
        inputs: &super::super::RefineInputs<'_>,
    ) -> Result<Self, LoweringError> {
        let mut result = Self::default();
        let Some(item) = inputs.function_item else {
            return Ok(result);
        };
        let callable = UnitItemId::new(inputs.source_unit, item);
        let fail = || error(LoweringErrorKind::MissingFact, inputs.span);
        for observation in inputs.owned.loan_action_observations() {
            for region in observation.regions().iter().filter(|region| {
                region.protocol_version() == 4
                    && region.coverage() == UnitLoanActionCoverage::Complete
                    && region.callable() == Some(callable)
            }) {
                if observation.source() != inputs.source_unit || !observation.owns_region(region) {
                    return Err(fail());
                }
                let ids = region.action_ids().collect::<Vec<_>>();
                let actions = ids
                    .iter()
                    .map(|id| observation.action(id).ok_or_else(fail))
                    .collect::<Result<Vec<_>, _>>()?;
                let [
                    UnitLoanAction::Begin(source),
                    UnitLoanAction::BorrowConsumerTransfer {
                        source: transferred,
                        consumer: call,
                        argument,
                    },
                    UnitLoanAction::Begin(consumer),
                    UnitLoanAction::BorrowConsumerRetired(retired),
                    UnitLoanAction::Cleanup {
                        point,
                        action: UnitIterationCleanupAction::Drop(drop),
                    },
                ] = actions.as_slice()
                else {
                    return Err(fail());
                };
                let UnitLoanTarget::Place(root) = source.target() else {
                    return Err(fail());
                };
                if source != transferred
                    || consumer.call() != region.call()
                    || *call != region.call()
                    || *argument != source.call()
                    || consumer.argument() != source.call()
                    || source.target() != consumer.target()
                    || !root.is_root()
                    || root.root().source_unit() != inputs.source_unit
                    || retired.source_begin().as_ref() != ids.first()
                    || retired.transfer().as_ref() != ids.get(1)
                    || retired.consumer_begin().as_ref() != ids.get(2)
                    || retired.transaction().ended_loans() != [consumer, source]
                    || *point != UnitDropPoint::CallReturn(*call)
                    || drop.point() != *point
                    || drop.target() != UnitDropTarget::Named(root.root())
                    || result.entries.iter().any(|entry| entry.call == *call)
                {
                    return Err(fail());
                }
                let indices = inputs
                    .owned
                    .drops()
                    .iter()
                    .enumerate()
                    .filter(|(_, fact)| **fact == *drop)
                    .map(|(index, _)| index)
                    .collect::<Vec<_>>();
                let [drop_index] = indices.as_slice() else {
                    return Err(fail());
                };
                let caller_key = inputs
                    .source_plan
                    .source(inputs.source_token)
                    .ok_or_else(fail)?;
                let UnitCallableTarget::Declaration(declaration) = caller_key.target() else {
                    return Err(fail());
                };
                let declaration = inputs
                    .names
                    .names()
                    .index()
                    .declarations()
                    .iter()
                    .find(|candidate| candidate.id() == declaration)
                    .ok_or_else(fail)?;
                if UnitItemId::new(declaration.source_unit(), declaration.root()) != callable {
                    return Err(fail());
                }
                let caller = *inputs.function_ids.get(caller_key).ok_or_else(fail)?;
                if [*call, *argument, source.call(), source.argument()]
                    .iter()
                    .any(|expression| expression.source_unit() != inputs.source_unit)
                {
                    return Err(fail());
                }
                let callee = |call| {
                    let site = inputs
                        .source_plan
                        .call_site(inputs.source_token, call)
                        .ok_or_else(fail)?;
                    if !site.delegation().is_empty() {
                        return Err(fail());
                    }
                    let key = site.dispatch_key().ok_or_else(fail)?;
                    inputs.function_ids.get(key).copied().ok_or_else(fail)
                };
                result.entries.push(Plan {
                    ids,
                    callable,
                    caller,
                    call: *call,
                    argument: *argument,
                    source_call: source.call(),
                    source_argument: source.argument(),
                    root: root.root(),
                    source_callee: callee(source.call())?,
                    consumer_callee: callee(*call)?,
                    drop_index: *drop_index,
                    owner: None,
                    place: None,
                    source: None,
                    result: None,
                    stage: 0,
                    drop_authorized: false,
                    block: None,
                });
            }
        }
        Ok(result)
    }
}
