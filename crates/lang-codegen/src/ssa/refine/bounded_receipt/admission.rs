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
            for region in catalog.bounded_forward_regions().iter().filter(|r| {
                r.callable() == callable && r.coverage() == UnitLoanActionCoverage::Complete
            }) {
                let [root, flags @ ..] = inputs.parameters.as_slice() else {
                    return Err(fail());
                };
                let (root, flags) = (*root, flags.to_vec());
                if plans.plan.is_some()
                    || catalog.source() != inputs.source_unit
                    || !catalog.owns_bounded_forward_region(region)
                    || root != region.source_parameter()
                    || !(1..=7).contains(&flags.len())
                    || region.split_count() != flags.len()
                    || region.paths().len() != flags.len() + 1
                    || flags.contains(&root)
                    || flags
                        .iter()
                        .collect::<std::collections::BTreeSet<_>>()
                        .len()
                        != flags.len()
                    || inputs.modes.len() != flags.len() + 1
                    || inputs.modes.iter().any(|m| *m != ParameterMode::Borrow)
                    || inputs.receiver.is_some()
                    || !inputs.substitutions.is_empty()
                    || inputs.static_self.is_some()
                    || inputs.borrow_return != Some(0)
                    || inputs.typed.symbol_type(root) != Some(inputs.return_type)
                    || inputs.typed.types().get(inputs.return_type)
                        != Some(&UnitTypeKind::Builtin(BuiltinType::String))
                    || flags.iter().any(|f| {
                        inputs
                            .typed
                            .symbol_type(*f)
                            .and_then(|t| inputs.typed.types().get(t))
                            != Some(&UnitTypeKind::Builtin(BuiltinType::Boolean))
                    })
                {
                    return Err(fail());
                }
                // Independently recover the bounded declaration, then compare every observed edge.
                // This tree is never used as evidence of actual execution or closure.
                let RefineBody::Block(body) = inputs.body else {
                    return Err(fail());
                };
                let mut splits = Vec::new();
                let mut leaves = Vec::new();
                declaration_tree(inputs, body, &[], &flags, &mut splits, &mut leaves)?;
                if splits.len() != flags.len()
                    || leaves.len() != region.paths().len()
                    || splits.iter().enumerate().any(|(i, s)| {
                        splits[i + 1..]
                            .iter()
                            .any(|r| s.split == r.split || s.condition == r.condition)
                    })
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
                for (path, expected) in region.paths().iter().zip(&leaves) {
                    let call = path.value();
                    let ids = vec![path.source_begin(), path.handoff()];
                    if path.callable() != callable
                        || path.source_parameter() != root
                        || path.coverage() != UnitLoanActionCoverage::Partial
                        || path.control() != expected.control
                        || call != expected.value
                        || path.edges().len() != expected.route.len()
                        || path
                            .edges()
                            .iter()
                            .zip(&expected.route)
                            .any(|(edge, (index, arm))| {
                                splits.get(*index).is_none_or(|split| {
                                    edge.split() != split.split
                                        || edge.condition() != split.condition
                                        || edge.then_arm() != (*arm == 0)
                                })
                            })
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
                        route: expected.route.clone(),
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
                for (path, leaf) in paths.iter().zip(&leaves) {
                    if path.source_loan.argument() != leaf.argument {
                        return Err(fail());
                    }
                }
                plans.plan = Some(Plan {
                    callable,
                    caller,
                    root,
                    flags,
                    splits,
                    paths,
                    source: None,
                    cursor: 0,
                });
            }
        }
        Ok(plans)
    }
}

struct DeclaredLeaf {
    control: UnitExpressionId,
    value: UnitExpressionId,
    argument: UnitExpressionId,
    route: Vec<(usize, usize)>,
}

fn declaration_tree(
    inputs: &RefineInputs<'_>,
    body: lang_frontend::ast::StatementId,
    route: &[(usize, usize)],
    flags: &[UnitSymbolId],
    splits: &mut Vec<SplitPlan>,
    leaves: &mut Vec<DeclaredLeaf>,
) -> Result<(), LoweringError> {
    let fail = || error(LoweringErrorKind::MissingFact, inputs.span);
    if route.len() > 3 {
        return Err(fail());
    }
    let ast = inputs.parsed.ast();
    let (Statement::Block { elements } | Statement::ControlBody { elements }) =
        ast.statements().get(body).map_err(|_| fail())?.payload()
    else {
        return Err(fail());
    };
    let [one] = elements.as_slice() else {
        return Err(fail());
    };
    let Statement::Expression { expression } =
        ast.statements().get(*one).map_err(|_| fail())?.payload()
    else {
        return Err(fail());
    };
    let node = ast.expressions().get(*expression).map_err(|_| fail())?;
    match node.payload() {
        Expression::If {
            condition,
            then_branch,
            else_branch: Some(other),
            ..
        } => {
            let index = splits.len();
            if index >= 7 || route.len() >= 3 {
                return Err(fail());
            }
            let flag = flags.get(index).ok_or_else(fail)?;
            let condition_node = ast.expressions().get(*condition).map_err(|_| fail())?;
            let condition = UnitExpressionId::new(inputs.source_unit, *condition);
            if !matches!(condition_node.payload(), Expression::Name)
                || inputs
                    .references
                    .get(&(condition_node.span().start(), condition_node.span().end()))
                    != Some(flag)
                || inputs.typed.expression_type(condition) != inputs.typed.symbol_type(*flag)
            {
                return Err(fail());
            }
            splits.push(SplitPlan {
                split: UnitExpressionId::new(inputs.source_unit, *expression),
                condition,
                parent: route.last().copied(),
                entry: None,
                arms: None,
                flag: None,
                value: None,
                done: false,
                join: None,
            });
            for (arm, child) in [*then_branch, *other].into_iter().enumerate() {
                let mut child_route = route.to_vec();
                child_route.push((index, arm));
                declaration_tree(inputs, child, &child_route, flags, splits, leaves)?;
            }
        }
        Expression::Return {
            value: Some(value), ..
        } => {
            if route.is_empty() || leaves.len() >= 8 {
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
            {
                return Err(fail());
            }
            leaves.push(DeclaredLeaf {
                control: UnitExpressionId::new(inputs.source_unit, *expression),
                value: UnitExpressionId::new(inputs.source_unit, *value),
                argument: UnitExpressionId::new(inputs.source_unit, argument.value),
                route: route.to_vec(),
            });
        }
        _ => return Err(fail()),
    }
    Ok(())
}
