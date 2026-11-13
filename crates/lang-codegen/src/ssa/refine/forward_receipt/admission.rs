//! Admit only a certified original live-forward interval, never reconstruct a loan.
use super::super::{RefineBody, RefineInputs};
use super::*;
use lang_frontend::{
    ownership_checking::{
        LoanKind as FrontendLoanKind, UnitCallArgumentOwnershipKind, UnitDropPoint, UnitLoanAction,
        UnitLoanActionCoverage, UnitLoanTarget,
    },
    parser::Expression,
    type_checking::{
        BorrowReturnOrigin, BuiltinType, ExpressionCategory, ParameterMode, UnitCallTarget,
        UnitCallableTarget, UnitTypeKind,
    },
};

impl Plans {
    pub(in crate::ssa::refine) fn new(inputs: &RefineInputs<'_>) -> Result<Self, LoweringError> {
        let mut plans = Self::default();
        let Some(item) = inputs.function_item else {
            return Ok(plans);
        };
        let callable = UnitItemId::new(inputs.source_unit, item);
        let fail = || error(LoweringErrorKind::MissingFact, inputs.span);
        for catalog in inputs.owned.loan_action_observations() {
            for region in catalog.forward_regions().iter().filter(|r| {
                r.callable() == callable && r.coverage() == UnitLoanActionCoverage::Complete
            }) {
                if catalog.source() != inputs.source_unit
                    || !catalog.owns_forward_region(region)
                    || !plans.entries.is_empty()
                {
                    return Err(fail());
                }
                let root = region.source_parameter();
                let call = region.body();
                if !matches!(inputs.body, RefineBody::Expression(e) if UnitExpressionId::new(inputs.source_unit, e) == call)
                    || inputs.parameters != [root]
                    || inputs.modes != [ParameterMode::Borrow]
                    || inputs.receiver.is_some()
                    || !inputs.substitutions.is_empty()
                    || inputs.static_self.is_some()
                    || inputs.borrow_return != Some(0)
                    || inputs.typed.symbol_type(root) != Some(inputs.return_type)
                    || inputs.typed.types().get(inputs.return_type)
                        != Some(&UnitTypeKind::Builtin(BuiltinType::String))
                {
                    return Err(fail());
                }
                let ids = region.action_ids().collect::<Vec<_>>();
                let actions = ids
                    .iter()
                    .map(|id| catalog.action(id).ok_or_else(fail))
                    .collect::<Result<Vec<_>, _>>()?;
                let [
                    UnitLoanAction::Begin(loan),
                    UnitLoanAction::BorrowForwardHandoff(handoff),
                ] = actions.as_slice()
                else {
                    return Err(fail());
                };
                let tx = handoff.transaction();
                if region.live_handoff().as_ref() != ids.get(1)
                    || handoff.source_begin().as_ref() != ids.first()
                    || handoff.callable() != Some(callable)
                    || tx.source_loan() != loan
                    || tx.returned_expression() != call
                    || tx.point() != UnitDropPoint::ControlTransfer(call)
                    || loan.call() != call
                    || loan.kind() != FrontendLoanKind::Shared
                    || !matches!(loan.target(), UnitLoanTarget::Place(p) if p.is_root() && p.root() == root)
                {
                    return Err(fail());
                }
                let sources = inputs
                    .owned
                    .borrow_results()
                    .forwarded_source_loans()
                    .iter()
                    .filter(|s| s.call() == call)
                    .copied()
                    .collect::<Vec<_>>();
                let [source_loan] = sources.as_slice() else {
                    return Err(fail());
                };
                if source_loan.argument() != loan.argument()
                    || inputs
                        .owned
                        .loans()
                        .iter()
                        .filter(|l| l.call() == call)
                        .collect::<Vec<_>>()
                        != [loan]
                    || inputs
                        .owned
                        .borrow_forward_transports()
                        .iter()
                        .filter(|t| t.returned_expression() == call)
                        .collect::<Vec<_>>()
                        != [tx]
                {
                    return Err(fail());
                }
                let descriptor = inputs.typed.call(call).ok_or_else(fail)?;
                let expressions = inputs.parsed.ast().expressions();
                let Expression::Call {
                    callee: raw_callee,
                    arguments,
                    type_arguments,
                    ..
                } = expressions
                    .get(call.expression())
                    .map_err(|_| fail())?
                    .payload()
                else {
                    return Err(fail());
                };
                let [operand] = arguments.as_slice() else {
                    return Err(fail());
                };
                if !type_arguments.is_empty()
                    || operand.named_prefix.is_some()
                    || operand.mode_marker.is_some()
                    || UnitExpressionId::new(inputs.source_unit, operand.value) != loan.argument()
                    || !matches!(
                        expressions.get(*raw_callee).map_err(|_| fail())?.payload(),
                        Expression::Name
                    )
                {
                    return Err(fail());
                }
                let origins = inputs
                    .owned
                    .borrow_return_origins()
                    .iter()
                    .filter(|o| o.expression() == call)
                    .collect::<Vec<_>>();
                if !matches!(origins.as_slice(), [o] if o.origin() == loan.target()) {
                    return Err(fail());
                }
                let UnitCallTarget::Declaration(declaration) = descriptor.target() else {
                    return Err(fail());
                };
                let [argument] = descriptor.arguments() else {
                    return Err(fail());
                };
                let signature = inputs
                    .typed
                    .signatures()
                    .declaration(declaration)
                    .and_then(|d| d.callable())
                    .ok_or_else(fail)?;
                let [parameter] = signature.parameters() else {
                    return Err(fail());
                };
                if !signature.type_parameters().is_empty()
                    || signature.receiver().is_some()
                    || parameter.mode() != ParameterMode::Borrow
                    || parameter.ty() != inputs.return_type
                    || signature.return_type() != inputs.return_type
                    || signature.borrow_return().map(|r| r.origin())
                        != Some(BorrowReturnOrigin::Parameter(0))
                    || descriptor.result_source() != signature.result_source()
                {
                    return Err(fail());
                }
                let contracts = inputs
                    .owned
                    .call_argument_contracts()
                    .iter()
                    .filter(|c| c.call() == call)
                    .collect::<Vec<_>>();
                let [contract] = contracts.as_slice() else {
                    return Err(fail());
                };
                if contract.argument() != loan.argument()
                    || contract.kind() != UnitCallArgumentOwnershipKind::SharedLoan
                    || contract.parameter_index() != argument.parameter_index()
                    || contract.parameter_type() != argument.parameter_type()
                    || contract.category() != argument.category()
                    || contract.crosses_thread()
                    || contract.argument_span() != loan.begin_span()
                    || contract.loan_begin_span() != loan.begin_span()
                    || contract.call_span() != loan.end_span()
                    || contract.parameter_span() != loan.parameter_span()
                {
                    return Err(fail());
                }
                let node = inputs
                    .parsed
                    .ast()
                    .expressions()
                    .get(loan.argument().expression())
                    .map_err(|_| fail())?;
                if !matches!(node.payload(), Expression::Name)
                    || inputs
                        .references
                        .get(&(node.span().start(), node.span().end()))
                        != Some(&root)
                    || descriptor.expression() != call
                    || descriptor.receiver().is_some()
                    || descriptor.aborts()
                    || !descriptor.instance().type_arguments().is_empty()
                    || argument.argument_index() != 0
                    || argument.parameter_index() != 0
                    || argument.mode() != ParameterMode::Borrow
                    || argument.category() != ExpressionCategory::Place
                    || argument.crosses_thread()
                    || argument.parameter_type() != inputs.return_type
                    || inputs.typed.expression_type(loan.argument()) != Some(inputs.return_type)
                    || descriptor.return_type() != inputs.return_type
                    || descriptor.borrow_return().map(|r| r.origin())
                        != Some(BorrowReturnOrigin::Parameter(0))
                {
                    return Err(fail());
                }
                let site = inputs
                    .source_plan
                    .call_site(inputs.source_token, call)
                    .ok_or_else(fail)?;
                let key = site.dispatch_key().ok_or_else(fail)?;
                if !site.delegation().is_empty()
                    || key != site.key()
                    || key.target() != UnitCallableTarget::Declaration(declaration)
                    || key.type_arguments() != descriptor.instance().type_arguments()
                    || key.is_specialized()
                    || key.deinit_owner().is_some()
                {
                    return Err(fail());
                }
                let callee = *inputs.function_ids.get(key).ok_or_else(fail)?;
                let caller_key = inputs
                    .source_plan
                    .source(inputs.source_token)
                    .ok_or_else(fail)?;
                let UnitCallableTarget::Declaration(id) = caller_key.target() else {
                    return Err(fail());
                };
                let decl = inputs
                    .names
                    .names()
                    .index()
                    .declarations()
                    .iter()
                    .find(|d| d.id() == id)
                    .ok_or_else(fail)?;
                if UnitItemId::new(decl.source_unit(), decl.root()) != callable
                    || caller_key.is_specialized()
                    || caller_key.deinit_owner().is_some()
                {
                    return Err(fail());
                }
                let caller = *inputs.function_ids.get(caller_key).ok_or_else(fail)?;
                plans.entries.push(Plan {
                    ids,
                    callable,
                    caller,
                    call,
                    source_loan: *source_loan,
                    root,
                    callee,
                    source: None,
                    result: None,
                    block: None,
                    returned: false,
                    stage: 0,
                });
            }
        }
        Ok(plans)
    }
}
