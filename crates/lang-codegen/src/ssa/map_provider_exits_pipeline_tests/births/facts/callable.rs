//! Unit FunctionValue resolves the actual noncapturing lambda and selected Int overload.
use super::*;
use crate::ssa::map_provider_exits_pipeline_tests::temporary_arguments;
pub(super) fn unit(
    input: Input<'_>,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
) -> Result<UnitDropFact, Failure> {
    let (_, sources, parsed, i) = input;
    let unit = one(
        names.names().source_units().iter(),
        "birth callable original unit",
    )?
    .source_unit();
    let e = |id| UnitExpressionId::new(unit, id);
    let lambda = i.lambda.unwrap();
    let callee = i.callee.unwrap();
    let factory = unit_root(names, i.factory.unwrap())?;
    let descriptor = owned
        .closure(e(lambda))
        .ok_or_else(|| failure("birth original qualified closure", lambda))?;
    let origin = owned
        .callable_origin(e(callee))
        .ok_or_else(|| failure("birth actual callable source provenance", callee))?;
    let call = typed
        .call(e(i.birth))
        .ok_or_else(|| failure("birth actual FunctionValue descriptor", i.birth))?;
    let reference = one(
        names
            .names()
            .references()
            .iter()
            .filter(|r| r.span() == ast::span(parsed, callee)),
        "birth original callable Name reference",
    )?;
    let fun = typed
        .expression_type(e(callee))
        .ok_or_else(|| failure("birth original function type", callee))?;
    need(
        descriptor.expression() == e(lambda)
            && !descriptor.move_owned()
            && owned.captures_of(e(lambda)).next().is_none()
            && origin.expression() == e(callee)
            && origin.span() == ast::span(parsed, callee)
            && origin.origin() == UnitCallableOrigin::Lambda(e(lambda))
            && reference.target() == &UnitReferenceTarget::Symbol(factory)
            && typed.symbol_type(factory) == Some(fun)
            && matches!(typed.types().get(fun),Some(UnitTypeKind::Function{move_only:false,parameters,return_type})if parameters.is_empty()&&*return_type==call.return_type())
            && call.target() == UnitCallTarget::FunctionValue
            && call.receiver().is_none()
            && call.arguments().is_empty()
            && call.instance().type_arguments().is_empty()
            && call.result_source() == CallableResultSource::Owned
            && call.borrow_return().is_none()
            && typed.call_borrow_return(e(i.birth)).is_none(),
        "birth exact Fn0 noncapturing mode/original lambda/owned Map return",
        (call, descriptor, origin),
    )?;
    let selected = temporary_arguments::ast::function(sources, parsed, "produce", Some("Int"))?;
    let rejected = temporary_arguments::ast::function(sources, parsed, "produce", Some("Boolean"))?;
    let evaluator = temporary_arguments::ast::function(sources, parsed, "argument", None)?;
    let declaration = |marker| {
        one(
            names
                .names()
                .index()
                .declarations()
                .iter()
                .filter(|d| d.source_unit() == unit && d.name_span() == marker)
                .map(|d| d.id()),
            "birth lambda actual callable declaration",
        )
    };
    let produce = i.produce.unwrap();
    let argument = i.argument.unwrap();
    let inner = typed
        .call(e(produce))
        .ok_or_else(|| failure("birth lambda actual producer Call", produce))?;
    let evaluate = typed
        .call(e(argument))
        .ok_or_else(|| failure("birth lambda actual argument Call", argument))?;
    let signature = typed
        .signatures()
        .declaration(declaration(selected.name)?)
        .and_then(|d| d.callable())
        .ok_or_else(|| failure("birth lambda original Int signature", selected.name))?;
    need(
        inner.target() == UnitCallTarget::Declaration(declaration(selected.name)?)
            && inner.target() != UnitCallTarget::Declaration(declaration(rejected.name)?)
            && inner.result_source() == CallableResultSource::Owned
            && inner.borrow_return().is_none()
            && inner.return_type() == call.return_type()
            && signature.return_type() == call.return_type()
            && signature.parameters().len() == 1
            && signature.parameters()[0].mode() == ParameterMode::Borrow
            && typed.types().get(signature.parameters()[0].ty())
                == Some(&UnitTypeKind::Builtin(BuiltinType::Int))
            && evaluate.target() == UnitCallTarget::Declaration(declaration(evaluator.name)?)
            && evaluate.arguments().is_empty()
            && evaluate.receiver().is_none()
            && evaluate.return_type() == signature.parameters()[0].ty()
            && matches!(inner.arguments(),[a]if a.argument_index()==0&&a.parameter_index()==0&&a.mode()==ParameterMode::Borrow&&a.category()==ExpressionCategory::Temporary&&a.parameter_type()==evaluate.return_type()),
        "birth actual lambda selected Int overload and ordered Borrow evaluator",
        (inner, evaluate, signature),
    )?;
    let loan = one(
        owned
            .loans()
            .iter()
            .filter(|l| l.call() == e(produce) && l.argument() == e(argument)),
        "birth lambda actual Borrow loan",
    )?;
    let contract = one(
        owned
            .call_argument_contracts_for(e(produce))
            .filter(|a| a.argument() == e(argument)),
        "birth lambda actual argument contract",
    )?;
    need(
        loan.kind() == LoanKind::Shared
            && loan.target() == &UnitLoanTarget::Temporary(e(argument))
            && loan.begin_span() == ast::span(parsed, argument)
            && loan.end_span() == ast::span(parsed, produce)
            && contract.parameter_index() == 0
            && contract.parameter_type() == evaluate.return_type()
            && contract.parameter_span() == Some(signature.parameters()[0].span())
            && contract.kind() == UnitCallArgumentOwnershipKind::SharedLoan
            && !owned
                .drops()
                .iter()
                .any(|d| d.target() == UnitDropTarget::Temporary(e(produce))),
        "birth lambda synchronous loan and transferred owned tail",
        (loan, contract),
    )?;
    let drop = *one(
        owned
            .drops()
            .iter()
            .filter(|d| d.target() == UnitDropTarget::Named(factory)),
        "birth unique actual callable binding Drop",
    )?;
    need(
        drop.value_origin() == i.factory.unwrap()
            && drop.point() == UnitDropPoint::CallReturn(e(i.birth))
            && !owned
                .drops()
                .iter()
                .any(|d| d.target() == UnitDropTarget::Temporary(e(lambda))),
        "birth original callable last-use endpoint and no duplicate lambda cleanup",
        drop,
    )?;
    Ok(drop)
}
