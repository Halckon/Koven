//! Consume only same-catalog certified paths and their original typed call mappings.
use super::super::{RefineBody, RefineInputs};
use super::*;
use lang_frontend::{
    ownership_checking::{
        LoanKind, UnitCallArgumentOwnershipKind, UnitDropPoint, UnitLoanAction,
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
            for region in catalog.branch_forward_regions().iter().filter(|r| {
                r.callable() == callable && r.coverage() == UnitLoanActionCoverage::Complete
            }) {
                let [root, flag] = inputs.parameters.as_slice() else {
                    return Err(fail());
                };
                let (root, flag) = (*root, *flag);
                if plans.plan.is_some()
                    || catalog.source() != inputs.source_unit
                    || !catalog.owns_branch_forward_region(region)
                    || !matches!(inputs.body, RefineBody::Block(_))
                    || root != region.source_parameter()
                    || root == flag
                    || inputs.modes != [ParameterMode::Borrow, ParameterMode::Borrow]
                    || inputs.receiver.is_some()
                    || !inputs.substitutions.is_empty()
                    || inputs.static_self.is_some()
                    || inputs.borrow_return != Some(0)
                    || inputs.typed.symbol_type(root) != Some(inputs.return_type)
                    || inputs.typed.types().get(inputs.return_type)
                        != Some(&UnitTypeKind::Builtin(BuiltinType::String))
                    || inputs
                        .typed
                        .symbol_type(flag)
                        .and_then(|ty| inputs.typed.types().get(ty))
                        != Some(&UnitTypeKind::Builtin(BuiltinType::Boolean))
                {
                    return Err(fail());
                }
                let condition = inputs
                    .parsed
                    .ast()
                    .expressions()
                    .get(region.condition().expression())
                    .map_err(|_| fail())?;
                if !matches!(condition.payload(), Expression::Name)
                    || inputs
                        .references
                        .get(&(condition.span().start(), condition.span().end()))
                        != Some(&flag)
                {
                    return Err(fail());
                }
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
                    .find(|d| d.id() == declaration)
                    .ok_or_else(fail)?;
                if UnitItemId::new(declaration.source_unit(), declaration.root()) != callable
                    || caller_key.is_specialized()
                    || caller_key.deinit_owner().is_some()
                {
                    return Err(fail());
                }
                let caller = *inputs.function_ids.get(caller_key).ok_or_else(fail)?;
                let mut paths = Vec::new();
                for path in region.paths() {
                    let branch = path.branch();
                    let call = branch.value();
                    let ids = path.action_ids().collect::<Vec<_>>();
                    if ids != [path.source_begin(), path.live_handoff()]
                        || branch.callable() != callable
                        || branch.split() != region.split()
                        || branch.condition() != region.condition()
                    {
                        return Err(fail());
                    }
                    let Some(UnitLoanAction::Begin(loan)) = catalog.action(&ids[0]) else {
                        return Err(fail());
                    };
                    let Some(UnitLoanAction::BorrowForwardHandoff(handoff)) =
                        catalog.action(&ids[1])
                    else {
                        return Err(fail());
                    };
                    let tx = handoff.transaction();
                    if handoff.branch() != Some(branch)
                        || handoff.callable() != Some(callable)
                        || handoff.source_begin().as_ref() != ids.first()
                        || tx.source_loan() != loan
                        || tx.returned_expression() != call
                        || tx.point() != UnitDropPoint::ControlTransfer(branch.control())
                        || loan.call() != call
                        || loan.kind() != LoanKind::Shared
                        || !matches!(loan.target(),UnitLoanTarget::Place(p) if p.is_root() && p.root()==root)
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
                    let origins = inputs
                        .owned
                        .borrow_return_origins()
                        .iter()
                        .filter(|o| o.expression() == call)
                        .collect::<Vec<_>>();
                    if !matches!(origins.as_slice(),[o] if o.origin()==loan.target()) {
                        return Err(fail());
                    }
                    let descriptor = inputs.typed.call(call).ok_or_else(fail)?;
                    let UnitCallTarget::Declaration(target) = descriptor.target() else {
                        return Err(fail());
                    };
                    let [argument] = descriptor.arguments() else {
                        return Err(fail());
                    };
                    let signature = inputs
                        .typed
                        .signatures()
                        .declaration(target)
                        .and_then(|d| d.callable())
                        .ok_or_else(fail)?;
                    let [parameter] = signature.parameters() else {
                        return Err(fail());
                    };
                    if descriptor.expression() != call
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
                        || !signature.type_parameters().is_empty()
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
                        || contract.parameter_index() != 0
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
                        || key.target() != UnitCallableTarget::Declaration(target)
                        || key.type_arguments() != descriptor.instance().type_arguments()
                        || key.is_specialized()
                        || key.deinit_owner().is_some()
                    {
                        return Err(fail());
                    }
                    let callee = *inputs.function_ids.get(key).ok_or_else(fail)?;
                    paths.push(PathPlan {
                        ids,
                        branch,
                        source_loan: *source_loan,
                        callee,
                        source: None,
                        result: None,
                        block: None,
                        stage: 0,
                    });
                }
                paths.sort_by_key(|p| !p.branch.then_arm());
                if paths.len() != 2
                    || !paths[0].branch.then_arm()
                    || paths[1].branch.then_arm()
                    || paths[0].branch.control() == paths[1].branch.control()
                    || paths[0].branch.value() == paths[1].branch.value()
                    || paths[0].callee != paths[1].callee
                    || paths[0].ids.iter().any(|id| paths[1].ids.contains(id))
                {
                    return Err(fail());
                }
                plans.plan = Some(Plan {
                    callable,
                    caller,
                    root,
                    flag,
                    split: region.split(),
                    condition: region.condition(),
                    paths,
                    entry: None,
                    source: None,
                    flag_slot: None,
                    condition_slot: None,
                    arms: None,
                    join: None,
                    split_done: false,
                    goto_done: false,
                });
            }
        }
        Ok(plans)
    }
}
