//! Same-catalog Complete2, narrowed by the existing caller shape rather than coverage alone.
use super::*;
use lang_frontend::{
    ownership_checking::{
        UnitDropPoint, UnitIterationCleanupAction, UnitLoanAction, UnitLoanActionCoverage,
        UnitReceiverOwnershipTarget,
    },
    type_checking::{BuiltinType, UnitCallableTarget, UnitTypeKind},
};
impl Plans {
    pub(in crate::ssa::refine) fn new(
        inputs: &super::super::RefineInputs<'_>,
    ) -> Result<Self, LoweringError> {
        let mut result = Self::default();
        let Some(call) = super::super::subset::bounded_receiver_call(inputs) else {
            return Ok(result);
        };
        let item = inputs.function_item.ok_or_else(|| invalid(inputs.span))?;
        let callable = UnitItemId::new(inputs.source_unit, item);
        let fail = || error(LoweringErrorKind::MissingFact, inputs.span);
        for observation in inputs.owned.loan_action_observations() {
            for region in observation.regions().iter().filter(|r| {
                r.protocol_version() == 2
                    && r.coverage() == UnitLoanActionCoverage::Complete
                    && r.callable() == Some(callable)
            }) {
                if observation.source() != inputs.source_unit
                    || !observation.owns_region(region)
                    || region.call() != call
                    || !result.entries.is_empty()
                {
                    return Err(fail());
                }
                let ids = region.action_ids().collect::<Vec<_>>();
                let actions = ids
                    .iter()
                    .map(|id| observation.action(id).ok_or_else(fail))
                    .collect::<Result<Vec<_>, _>>()?;
                let [
                    UnitLoanAction::ReceiverBegin(begin),
                    UnitLoanAction::ReceiverEnd(end),
                    UnitLoanAction::Cleanup {
                        point,
                        action: UnitIterationCleanupAction::Drop(drop),
                    },
                ] = actions.as_slice()
                else {
                    return Err(fail());
                };
                let UnitReceiverOwnershipTarget::Place(root) = begin.target() else {
                    return Err(fail());
                };
                if begin.call() != call
                    || end.receiver() != begin
                    || inputs.owned.receiver_fact(call) != Some(begin)
                    || !inputs.owned.receiver_call_return_ends().contains(end)
                    || !root.is_root()
                    || root.root().source_unit() != inputs.source_unit
                    || *point != UnitDropPoint::CallReturn(call)
                    || end.point() != *point
                    || drop.point() != *point
                    || drop.target() != UnitDropTarget::Named(root.root())
                {
                    return Err(fail());
                }
                let indices = inputs
                    .owned
                    .drops()
                    .iter()
                    .enumerate()
                    .filter(|(_, fact)| **fact == *drop)
                    .map(|(i, _)| i)
                    .collect::<Vec<_>>();
                let [drop_index] = indices.as_slice() else {
                    return Err(fail());
                };
                let caller_key = inputs
                    .source_plan
                    .source(inputs.source_token)
                    .ok_or_else(fail)?;
                let UnitCallableTarget::Declaration(id) = caller_key.target() else {
                    return Err(fail());
                };
                let declaration = inputs
                    .names
                    .names()
                    .index()
                    .declarations()
                    .iter()
                    .find(|d| d.id() == id)
                    .ok_or_else(fail)?;
                if UnitItemId::new(declaration.source_unit(), declaration.root()) != callable {
                    return Err(fail());
                }
                let caller = *inputs.function_ids.get(caller_key).ok_or_else(fail)?;
                let site = inputs
                    .source_plan
                    .call_site(inputs.source_token, call)
                    .ok_or_else(fail)?;
                if !site.delegation().is_empty() {
                    return Err(fail());
                }
                let callee = *inputs
                    .function_ids
                    .get(site.dispatch_key().ok_or_else(fail)?)
                    .ok_or_else(fail)?;
                if callee == caller {
                    return Err(fail());
                }
                let ty = inputs
                    .concrete(inputs.typed.call(call).ok_or_else(fail)?.return_type())
                    .ok_or_else(fail)?;
                let returns_value = match inputs.typed.types().get(ty) {
                    Some(UnitTypeKind::Builtin(BuiltinType::Int)) => true,
                    Some(UnitTypeKind::Builtin(BuiltinType::Unit)) => false,
                    _ => return Err(fail()),
                };
                result.entries.push(Plan {
                    ids,
                    callable,
                    caller,
                    call,
                    callee,
                    fact: (*begin).clone(),
                    root: root.root(),
                    drop_index: *drop_index,
                    returns_value,
                    owner: None,
                    place: None,
                    loan: None,
                    stage: 0,
                    drop_authorized: false,
                    block: None,
                });
            }
        }
        Ok(result)
    }
}
