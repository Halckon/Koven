//! Consume only same-catalog certified paths and their original typed call mappings.
use super::super::{RefineBody, RefineInputs};
use super::*;
use lang_frontend::{
    ownership_checking::{
        LoanKind, UnitCallArgumentOwnershipKind, UnitDropPoint, UnitLoanAction,
        UnitLoanActionCoverage, UnitLoanTarget,
    },
    parser::{Expression, Statement},
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
            for region in catalog.nested_forward_regions().iter().filter(|r| {
                r.callable() == callable && r.coverage() == UnitLoanActionCoverage::Complete
            }) {
                let [root, outer, inner] = inputs.parameters.as_slice() else {
                    return Err(fail());
                };
                let (root, flags) = (*root, [*outer, *inner]);
                if plans.plan.is_some()
                    || catalog.source() != inputs.source_unit
                    || !catalog.owns_nested_forward_region(region)
                    || !matches!(inputs.body, RefineBody::Block(_))
                    || root != region.source_parameter()
                    || root == flags[0]
                    || root == flags[1]
                    || flags[0] == flags[1]
                    || inputs.modes
                        != [
                            ParameterMode::Borrow,
                            ParameterMode::Borrow,
                            ParameterMode::Borrow,
                        ]
                    || inputs.receiver.is_some()
                    || !inputs.substitutions.is_empty()
                    || inputs.static_self.is_some()
                    || inputs.borrow_return != Some(0)
                    || inputs.typed.symbol_type(root) != Some(inputs.return_type)
                    || inputs.typed.types().get(inputs.return_type)
                        != Some(&UnitTypeKind::Builtin(BuiltinType::String))
                    || region.paths().len() != 3
                    || flags.iter().any(|flag| {
                        inputs
                            .typed
                            .symbol_type(*flag)
                            .and_then(|ty| inputs.typed.types().get(ty))
                            != Some(&UnitTypeKind::Builtin(BuiltinType::Boolean))
                    })
                {
                    return Err(fail());
                }
                let nested_leaf = match region.topology() {
                    UnitBorrowNestedForwardTopology::NestedThen => 0,
                    UnitBorrowNestedForwardTopology::NestedElse => 1,
                };
                let first_edges = region.paths()[nested_leaf].edges();
                let [outer_edge, inner_edge] = first_edges else {
                    return Err(fail());
                };
                if outer_edge.split() == inner_edge.split() {
                    return Err(fail());
                }
                let mut splits = Vec::new();
                for (edge, flag) in first_edges.iter().zip(flags) {
                    let condition = inputs
                        .parsed
                        .ast()
                        .expressions()
                        .get(edge.condition().expression())
                        .map_err(|_| fail())?;
                    if edge.split().source_unit() != inputs.source_unit
                        || edge.condition().source_unit() != inputs.source_unit
                        || !matches!(condition.payload(), Expression::Name)
                        || inputs
                            .references
                            .get(&(condition.span().start(), condition.span().end()))
                            != Some(&flag)
                        || inputs.typed.expression_type(edge.condition())
                            != inputs.typed.symbol_type(flag)
                    {
                        return Err(fail());
                    }
                    splits.push(SplitPlan {
                        split: edge.split(),
                        condition: edge.condition(),
                        entry: None,
                        arms: None,
                        flag: None,
                        value: None,
                        done: false,
                        join: None,
                    });
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
                for (leaf, path) in region.paths().iter().enumerate() {
                    let call = path.value();
                    let ids = vec![path.source_begin(), path.handoff()];
                    let expected: &[bool] = match (region.topology(), leaf) {
                        (UnitBorrowNestedForwardTopology::NestedThen, 0) => &[true, true],
                        (UnitBorrowNestedForwardTopology::NestedThen, 1) => &[true, false],
                        (UnitBorrowNestedForwardTopology::NestedThen, _) => &[false],
                        (UnitBorrowNestedForwardTopology::NestedElse, 0) => &[true],
                        (UnitBorrowNestedForwardTopology::NestedElse, 1) => &[false, true],
                        (UnitBorrowNestedForwardTopology::NestedElse, _) => &[false, false],
                    };
                    if path.topology() != region.topology()
                        || !catalog.owns_nested_forward_lineage(path)
                        || path.callable() != callable
                        || path.source_parameter() != root
                        || path.coverage() != UnitLoanActionCoverage::Partial
                        || path.edges().len() != expected.len()
                        || path
                            .edges()
                            .iter()
                            .zip(expected)
                            .enumerate()
                            .any(|(i, (edge, arm))| {
                                edge.then_arm() != *arm
                                    || edge.split() != splits[i].split
                                    || edge.condition() != splits[i].condition
                            })
                        || [path.value(), path.control()]
                            .iter()
                            .any(|e| e.source_unit() != inputs.source_unit)
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
                    if handoff.branch().is_some()
                        || tx.source_loan() != loan
                        || tx.returned_expression() != call
                        || tx.point() != UnitDropPoint::ControlTransfer(path.control())
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
                        control: path.control(),
                        value: path.value(),
                        source_loan: *source_loan,
                        callee,
                        source: None,
                        result: None,
                        block: None,
                        stage: 0,
                    });
                }
                if paths.iter().enumerate().any(|(i, left)| {
                    paths[i + 1..].iter().any(|right| {
                        left.control == right.control
                            || left.value == right.value
                            || left.ids.iter().any(|id| right.ids.contains(id))
                    })
                }) {
                    return Err(fail());
                }
                validate_topology(inputs, region.topology(), &splits, &paths)?;
                plans.plan = Some(Plan {
                    topology: region.topology(),
                    callable,
                    caller,
                    root,
                    flags,
                    splits,
                    paths,
                    source: None,
                });
            }
        }
        Ok(plans)
    }
}

// Require the original fixed declaration tree, not three unrelated certified leaves.
fn validate_topology(
    inputs: &RefineInputs<'_>,
    topology: UnitBorrowNestedForwardTopology,
    splits: &[SplitPlan],
    paths: &[PathPlan],
) -> Result<(), LoweringError> {
    let fail = || error(LoweringErrorKind::MissingFact, inputs.span);
    let RefineBody::Block(body) = inputs.body else {
        return Err(fail());
    };
    let ast = inputs.parsed.ast();
    let expression = |statement| match ast.statements().get(statement).ok()?.payload() {
        Statement::Expression { expression } => Some(*expression),
        _ => None,
    };
    let single = |statement| match ast.statements().get(statement).ok()?.payload() {
        Statement::Block { elements } | Statement::ControlBody { elements } => {
            match elements.as_slice() {
                [s] => expression(*s),
                _ => None,
            }
        }
        _ => None,
    };
    let outer = single(body).ok_or_else(fail)?;
    if UnitExpressionId::new(inputs.source_unit, outer) != splits[0].split {
        return Err(fail());
    }
    let Expression::If {
        condition,
        then_branch,
        else_branch: Some(other),
        ..
    } = ast.expressions().get(outer).map_err(|_| fail())?.payload()
    else {
        return Err(fail());
    };
    if UnitExpressionId::new(inputs.source_unit, *condition) != splits[0].condition {
        return Err(fail());
    }
    let (nested, plain) = match topology {
        UnitBorrowNestedForwardTopology::NestedThen => (*then_branch, *other),
        UnitBorrowNestedForwardTopology::NestedElse => (*other, *then_branch),
    };
    let inner = single(nested).ok_or_else(fail)?;
    let plain = single(plain).ok_or_else(fail)?;
    if UnitExpressionId::new(inputs.source_unit, inner) != splits[1].split {
        return Err(fail());
    }
    let Expression::If {
        condition,
        then_branch,
        else_branch: Some(other),
        ..
    } = ast.expressions().get(inner).map_err(|_| fail())?.payload()
    else {
        return Err(fail());
    };
    if UnitExpressionId::new(inputs.source_unit, *condition) != splits[1].condition {
        return Err(fail());
    }
    let left = single(*then_branch).ok_or_else(fail)?;
    let right = single(*other).ok_or_else(fail)?;
    let returns = match topology {
        UnitBorrowNestedForwardTopology::NestedThen => [left, right, plain],
        UnitBorrowNestedForwardTopology::NestedElse => [plain, left, right],
    };
    for (returned, path) in returns.into_iter().zip(paths) {
        let Expression::Return {
            value: Some(value), ..
        } = ast
            .expressions()
            .get(returned)
            .map_err(|_| fail())?
            .payload()
        else {
            return Err(fail());
        };
        if path.control != UnitExpressionId::new(inputs.source_unit, returned)
            || path.value != UnitExpressionId::new(inputs.source_unit, *value)
        {
            return Err(fail());
        }
        let Expression::Call {
            callee,
            arguments,
            type_arguments,
            ..
        } = ast.expressions().get(*value).map_err(|_| fail())?.payload()
        else {
            return Err(fail());
        };
        let [argument] = arguments.as_slice() else {
            return Err(fail());
        };
        if !type_arguments.is_empty()
            || argument.named_prefix.is_some()
            || argument.mode_marker.is_some()
            || !matches!(
                ast.expressions()
                    .get(*callee)
                    .map_err(|_| fail())?
                    .payload(),
                Expression::Name
            )
            || UnitExpressionId::new(inputs.source_unit, argument.value)
                != path.source_loan.argument()
        {
            return Err(fail());
        }
    }
    Ok(())
}
