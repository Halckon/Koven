//! Independent Unit-only admission of bounded, fully terminal borrowing trees.
//! Raw checked facts preserve observer-off lowering; only a separate Complete region authorizes receipts.
use lang_frontend::{
    ast::{ExpressionId, StatementId},
    name_resolution::{UnitReferenceTarget, UnitSymbolId, ValidatedCompilationUnitNames},
    ownership_checking::{
        CompilationUnitOwnership, LoanKind, UnitCallArgumentOwnershipKind, UnitDropPoint,
        UnitLoanTarget,
    },
    parser::{Expression, FunctionBody, FunctionForm, Item, ParsedFile, Statement},
    type_checking::{
        BorrowReturnOrigin, BuiltinType, CompilationUnitTypes, ExpressionCategory, ParameterMode,
        UnitCallTarget, UnitCallableSignature, UnitCallableTarget, UnitExpressionId, UnitTypeKind,
    },
};

pub(super) fn allows(
    file: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    callable: &UnitCallableSignature,
) -> bool {
    admitted(file, names, typed, owned, callable).is_some()
}

fn admitted(
    file: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    callable: &UnitCallableSignature,
) -> Option<()> {
    let UnitCallableTarget::Declaration(id) = callable.target() else {
        return None;
    };
    let declaration = names
        .names()
        .index()
        .declarations()
        .iter()
        .find(|d| d.id() == id)?;
    let unit = declaration.source_unit();
    // Match exact declaration root, not a containing marker span or function name.
    let node = file.ast().items().get(declaration.root()).ok()?;
    let contract = callable.borrow_return()?;
    if node.span().source_id() != file.source_id()
        || contract.marker_span().source_id() != file.source_id()
        || callable.receiver().is_some()
        || !callable.type_parameters().is_empty()
        || contract.origin() != BorrowReturnOrigin::Parameter(0)
    {
        return None;
    }
    let [source, outer, inner_flags @ ..] = callable.parameters() else {
        return None;
    };
    if source.mode() != ParameterMode::Borrow
        || outer.mode() != ParameterMode::Borrow
        || inner_flags.len() > 6
        || inner_flags
            .iter()
            .any(|inner| inner.mode() != ParameterMode::Borrow || inner.ty() != outer.ty())
        || typed.types().get(source.ty()) != Some(&UnitTypeKind::Builtin(BuiltinType::String))
        || typed.types().get(outer.ty()) != Some(&UnitTypeKind::Builtin(BuiltinType::Boolean))
        || callable.return_type() != source.ty()
    {
        return None;
    }
    let root = source.symbol()?;
    let mut flags = vec![outer.symbol()?];
    flags.extend(
        inner_flags
            .iter()
            .map(|inner| inner.symbol())
            .collect::<Option<Vec<_>>>()?,
    );
    if root.source_unit() != unit
        || flags.iter().any(|flag| flag.source_unit() != unit)
        || !file.roots().contains(&declaration.root())
    {
        return None;
    }
    let Item::Function {
        type_parameters,
        extension_receiver,
        form:
            FunctionForm::Explicit {
                body: FunctionBody::Block(block),
                ..
            },
        ..
    } = node.payload()
    else {
        return None;
    };
    if !type_parameters.is_empty() || extension_receiver.is_some() {
        return None;
    }
    let Statement::Block { elements } = file.ast().statements().get(*block).ok()?.payload() else {
        return None;
    };
    let [statement] = elements.as_slice() else {
        return None;
    };
    let mut splits = Vec::new();
    let mut conditions = Vec::new();
    let mut returns = Vec::new();
    terminal_tree(
        file,
        *statement,
        0,
        &mut splits,
        &mut conditions,
        &mut returns,
    )?;
    if splits.is_empty()
        || splits.len() != flags.len()
        || returns.len() != splits.len() + 1
        || flags.contains(&root)
        || flags
            .iter()
            .enumerate()
            .any(|(i, f)| flags[i + 1..].contains(f))
    {
        return None;
    }
    for (condition, flag) in conditions.into_iter().zip(flags) {
        if !is_symbol(file, names, condition, flag)
            || typed.expression_type(UnitExpressionId::new(unit, condition)) != Some(outer.ty())
        {
            return None;
        }
    }
    if returns
        .iter()
        .enumerate()
        .any(|(i, left)| returns[i + 1..].contains(left))
    {
        return None;
    }
    let expected_returns = returns.len();
    let mut calls = Vec::new();
    for returned in returns {
        let Expression::Return {
            value: Some(value), ..
        } = file.ast().expressions().get(returned).ok()?.payload()
        else {
            return None;
        };
        let Expression::Call {
            callee,
            arguments,
            type_arguments,
            ..
        } = file.ast().expressions().get(*value).ok()?.payload()
        else {
            return None;
        };
        let [operand] = arguments.as_slice() else {
            return None;
        };
        if !type_arguments.is_empty()
            || operand.named_prefix.is_some()
            || operand.mode_marker.is_some()
            || !matches!(
                file.ast().expressions().get(*callee).ok()?.payload(),
                Expression::Name
            )
            || !is_symbol(file, names, operand.value, root)
        {
            return None;
        }
        let call = UnitExpressionId::new(unit, *value);
        let argument = UnitExpressionId::new(unit, operand.value);
        let control = UnitExpressionId::new(unit, returned);
        let descriptor = typed.call(call)?;
        let UnitCallTarget::Declaration(target) = descriptor.target() else {
            return None;
        };
        let signature = typed.signatures().declaration(target)?.callable()?;
        let [parameter] = signature.parameters() else {
            return None;
        };
        let [actual] = descriptor.arguments() else {
            return None;
        };
        if descriptor.expression() != call
            || descriptor.receiver().is_some()
            || descriptor.aborts()
            || !descriptor.instance().type_arguments().is_empty()
            || signature.receiver().is_some()
            || !signature.type_parameters().is_empty()
            || parameter.mode() != ParameterMode::Borrow
            || parameter.ty() != source.ty()
            || signature.return_type() != source.ty()
            || signature.borrow_return()?.origin() != BorrowReturnOrigin::Parameter(0)
            || descriptor.result_source() != signature.result_source()
            || descriptor.return_type() != source.ty()
            || actual.argument_index() != 0
            || actual.parameter_index() != 0
            || actual.mode() != ParameterMode::Borrow
            || actual.category() != ExpressionCategory::Place
            || actual.crosses_thread()
            || actual.parameter_type() != source.ty()
            || typed.expression_type(argument) != Some(source.ty())
        {
            return None;
        }
        let loans = owned
            .loans()
            .iter()
            .filter(|l| l.call() == call)
            .collect::<Vec<_>>();
        let [loan] = loans.as_slice() else {
            return None;
        };
        if loan.argument() != argument
            || loan.kind() != LoanKind::Shared
            || !matches!(loan.target(), UnitLoanTarget::Place(p) if p.is_root() && p.root() == root)
        {
            return None;
        }
        let sources = owned
            .borrow_results()
            .forwarded_source_loans()
            .iter()
            .filter(|s| s.call() == call)
            .collect::<Vec<_>>();
        if !matches!(sources.as_slice(), [s] if s.argument() == argument) {
            return None;
        }
        let transports = owned
            .borrow_forward_transports()
            .iter()
            .filter(|t| t.point() == UnitDropPoint::ControlTransfer(control))
            .collect::<Vec<_>>();
        if !matches!(transports.as_slice(), [t] if t.source_loan() == *loan
            && t.returned_expression() == call && t.handoff_point() == UnitDropPoint::CallReturn(call))
        {
            return None;
        }
        let origins = owned
            .borrow_return_origins()
            .iter()
            .filter(|o| o.declaration_span() == contract.marker_span() && o.expression() == call)
            .collect::<Vec<_>>();
        if !matches!(origins.as_slice(), [o] if o.origin() == loan.target()) {
            return None;
        }
        let contracts = owned
            .call_argument_contracts()
            .iter()
            .filter(|c| c.call() == call)
            .collect::<Vec<_>>();
        let [mapping] = contracts.as_slice() else {
            return None;
        };
        if mapping.argument() != argument
            || mapping.kind() != UnitCallArgumentOwnershipKind::SharedLoan
            || mapping.parameter_index() != 0
            || mapping.parameter_type() != source.ty()
            || mapping.category() != ExpressionCategory::Place
            || mapping.crosses_thread()
            || mapping.argument_span() != loan.begin_span()
            || mapping.loan_begin_span() != loan.begin_span()
            || mapping.call_span() != loan.end_span()
            || mapping.parameter_span() != loan.parameter_span()
        {
            return None;
        }
        calls.push(call);
    }
    if calls
        .iter()
        .enumerate()
        .any(|(i, left)| calls[i + 1..].contains(left))
        || owned
            .borrow_return_origins()
            .iter()
            .filter(|o| o.declaration_span() == contract.marker_span())
            .count()
            != expected_returns
    {
        return None;
    }
    Some(())
}

fn expression(file: &ParsedFile, statement: StatementId) -> Option<ExpressionId> {
    let Statement::Expression { expression } =
        file.ast().statements().get(statement).ok()?.payload()
    else {
        return None;
    };
    Some(*expression)
}

fn is_symbol(
    file: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    expression: ExpressionId,
    symbol: UnitSymbolId,
) -> bool {
    let Ok(node) = file.ast().expressions().get(expression) else {
        return false;
    };
    if !matches!(node.payload(), Expression::Name) {
        return false;
    }
    let references = names
        .names()
        .references()
        .iter()
        .filter(|r| r.source_unit() == symbol.source_unit() && r.span() == node.span())
        .collect::<Vec<_>>();
    matches!(references.as_slice(), [r] if r.target() == &UnitReferenceTarget::Symbol(symbol))
}

// Exact tree admission only. These AST identities never stand in for actual entered edges.
fn split(
    file: &ParsedFile,
    statement: StatementId,
) -> Option<(ExpressionId, ExpressionId, StatementId, StatementId)> {
    let id = expression(file, statement)?;
    let Expression::If {
        condition,
        then_branch,
        else_branch: Some(other),
        ..
    } = file.ast().expressions().get(id).ok()?.payload()
    else {
        return None;
    };
    let sole = |body| {
        let (Statement::Block { elements } | Statement::ControlBody { elements }) =
            file.ast().statements().get(body).ok()?.payload()
        else {
            return None;
        };
        let [statement] = elements.as_slice() else {
            return None;
        };
        Some(*statement)
    };
    Some((id, *condition, sole(*then_branch)?, sole(*other)?))
}

// Bounded declaration traversal supplies structure only, never observed execution evidence.
fn terminal_tree(
    file: &ParsedFile,
    statement: StatementId,
    depth: usize,
    splits: &mut Vec<ExpressionId>,
    conditions: &mut Vec<ExpressionId>,
    returns: &mut Vec<ExpressionId>,
) -> Option<()> {
    if depth > 3 {
        return None;
    }
    let id = expression(file, statement)?;
    if let Some((split, condition, left, right)) = split(file, statement) {
        if depth >= 3
            || splits.len() >= 7
            || splits.contains(&split)
            || conditions.contains(&condition)
        {
            return None;
        }
        splits.push(split);
        conditions.push(condition);
        terminal_tree(file, left, depth + 1, splits, conditions, returns)?;
        terminal_tree(file, right, depth + 1, splits, conditions, returns)?;
    } else {
        if depth == 0
            || returns.len() >= 8
            || !matches!(
                file.ast().expressions().get(id).ok()?.payload(),
                Expression::Return { value: Some(_), .. }
            )
        {
            return None;
        }
        returns.push(id);
    }
    Some(())
}
