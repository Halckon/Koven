//! Independent typed/raw identity for ordinary external-source consumers between loops.
use super::super::RefineInputs;
use super::*;
use lang_frontend::{
    ast::StatementId,
    name_resolution::UnitSymbolId,
    ownership_checking::{LoanKind, UnitCallArgumentOwnershipKind, UnitLoanTarget},
    parser::{Expression, Statement},
    type_checking::{BuiltinType, ExpressionCategory, UnitCallTarget, UnitTypeKind},
};

pub(super) fn consumer(
    inputs: &RefineInputs<'_>,
    statement: StatementId,
    source: UnitSymbolId,
) -> Result<Option<SourceConsumer>, LoweringError> {
    let fail = || invalid(inputs.span);
    let ast = inputs.parsed.ast();
    let Statement::Expression { expression } = ast
        .statements()
        .get(statement)
        .map_err(|_| fail())?
        .payload()
    else {
        return Ok(None);
    };
    let node = ast.expressions().get(*expression).map_err(|_| fail())?;
    let Expression::Call {
        callee, arguments, ..
    } = node.payload()
    else {
        return Ok(None);
    };
    let [operand] = arguments.as_slice() else {
        return Ok(None);
    };
    let argument = ast.expressions().get(operand.value).map_err(|_| fail())?;
    if operand.named_prefix.is_some()
        || operand.mode_marker.is_some()
        || operand.span != argument.span()
        || !matches!(
            ast.expressions()
                .get(*callee)
                .map_err(|_| fail())?
                .payload(),
            Expression::Name
        )
        || !matches!(argument.payload(), Expression::Name)
        || inputs
            .references
            .get(&(argument.span().start(), argument.span().end()))
            != Some(&source)
    {
        return Ok(None);
    }
    let call = UnitExpressionId::new(inputs.source_unit, *expression);
    let argument_id = UnitExpressionId::new(inputs.source_unit, operand.value);
    let descriptor = inputs.typed.call(call).ok_or_else(fail)?;
    let [mapping] = descriptor.arguments() else {
        return Ok(None);
    };
    if descriptor.expression() != call
        || !matches!(descriptor.target(), UnitCallTarget::Declaration(_))
        || descriptor.receiver().is_some()
        || descriptor.aborts()
        || descriptor.prints_line()
        || descriptor.text_primitive().is_some()
        || descriptor.range_construction().is_some()
        || descriptor.borrow_return().is_some()
        || !descriptor.instance().type_arguments().is_empty()
        || inputs.typed.types().get(descriptor.return_type())
            != Some(&UnitTypeKind::Builtin(BuiltinType::Unit))
        || inputs.typed.expression_type(call) != Some(descriptor.return_type())
        || mapping.argument_index() != 0
        || mapping.parameter_index() != 0
        || mapping.mode() != ParameterMode::Borrow
        || mapping.category() != ExpressionCategory::Place
        || mapping.crosses_thread()
        || inputs.typed.types().get(mapping.parameter_type())
            != Some(&UnitTypeKind::Builtin(BuiltinType::String))
        || inputs.typed.symbol_type(source) != Some(mapping.parameter_type())
        || inputs.typed.expression_type(argument_id) != Some(mapping.parameter_type())
    {
        return Ok(None);
    }
    let mut loans = inputs
        .owned
        .loans()
        .iter()
        .filter(|loan| loan.call() == call);
    let loan = loans.next().ok_or_else(fail)?;
    if loans.next().is_some()
        || loan.argument() != argument_id
        || loan.kind() != LoanKind::Shared
        || !matches!(loan.target(), UnitLoanTarget::Place(place) if place.is_root() && place.root() == source)
        || loan.end_span() != node.span()
    {
        return Err(fail());
    }
    let mut contracts = inputs
        .owned
        .call_argument_contracts()
        .iter()
        .filter(|contract| contract.call() == call);
    let contract = contracts.next().ok_or_else(fail)?;
    if contracts.next().is_some()
        || contract.argument() != argument_id
        || contract.kind() != UnitCallArgumentOwnershipKind::SharedLoan
        || contract.parameter_index() != 0
        || contract.parameter_type() != mapping.parameter_type()
        || contract.category() != mapping.category()
        || contract.crosses_thread()
        || contract.argument_span() != loan.begin_span()
        || contract.loan_begin_span() != loan.begin_span()
        || contract.call_span() != loan.end_span()
        || contract.parameter_span() != loan.parameter_span()
    {
        return Err(fail());
    }
    Ok(Some(SourceConsumer {
        statement: UnitStatementId::new(inputs.source_unit, statement),
        call,
        argument: argument_id,
        callee: super::admission::callee(inputs, call)?,
        span: node.span(),
        argument_span: argument.span(),
    }))
}
