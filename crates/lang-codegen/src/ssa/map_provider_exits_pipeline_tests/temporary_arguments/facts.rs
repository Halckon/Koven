//! Actual selected overload, Borrow argument delivery and the provider's independent owner facts.
use super::super::{
    fixture::{Failure, failure, need},
    query_inputs::one,
};
use super::{Case, ast};
use lang_frontend::{
    name_resolution::{UnitReferenceTarget, ValidatedCompilationUnitNames},
    ownership_checking::{
        CompilationUnitOwnership, LoanKind, OwnershipBindingKind, UnitDropPoint, UnitDropTarget,
        UnitIterationCleanupAction as UA, UnitIterationExitKind as UE, UnitIterationSourceAccess,
        UnitLoanTarget,
    },
    parser::ParsedFile,
    source::{SourceMap, Span},
    type_checking::{
        BuiltinType, CallableResultSource, CompilationUnitTypes, ExpressionCategory,
        IntrinsicTypeConstructor, ParameterMode, UnitCallTarget, UnitExpressionId, UnitStatementId,
        UnitTypeKind,
    },
};

pub(in crate::ssa::map_provider_exits_pipeline_tests) fn unit(
    case: &Case,
    sources: &SourceMap,
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    control: bool,
) -> Result<(), Failure> {
    let input = ast::input(sources, parsed, control)?;
    let resolution = one(
        names.names().source_units().iter(),
        "argument original source unit",
    )?;
    let unit = resolution.source_unit();
    let expression = |id| UnitExpressionId::new(unit, id);
    let declaration = |name: Span| {
        one(
            names
                .names()
                .index()
                .declarations()
                .iter()
                .filter(|d| d.source_unit() == unit && d.name_span() == name)
                .map(|d| d.id()),
            "argument actual qualified declaration",
        )
    };
    let factory = declaration(input.factory.name)?;
    let signature = typed
        .signatures()
        .declaration(factory)
        .and_then(|d| d.callable())
        .ok_or_else(|| failure("argument actual qualified signature", factory))?;
    let call = typed
        .call(expression(input.produce))
        .ok_or_else(|| failure("argument actual qualified factory Call", input.produce))?;
    let value = typed
        .call(expression(input.value))
        .ok_or_else(|| failure("argument actual qualified argument Call", input.value))?;
    need(
        call.target() == UnitCallTarget::Declaration(factory)
            && factory != declaration(input.rejected.name)?
            && signature.parameters().len() == 1
            && signature.parameters()[0].mode() == ParameterMode::Borrow
            && typed.types().get(signature.parameters()[0].ty())
                == Some(&UnitTypeKind::Builtin(BuiltinType::Int))
            && signature.parameters()[0].symbol().is_some_and(|p| {
                p.source_unit() == unit
                    && resolution
                        .resolution()
                        .symbols()
                        .get(p.symbol().index())
                        .is_some_and(|s| s.span() == input.factory.parameter.unwrap())
            })
            && value.target() == UnitCallTarget::Declaration(declaration(input.argument.name)?)
            && value.arguments().is_empty()
            && value.receiver().is_none()
            && typed.types().get(value.return_type())
                == Some(&UnitTypeKind::Builtin(BuiltinType::Int))
            && typed.expression_type(expression(input.value)) == Some(value.return_type())
            && typed.expression_category(expression(input.value))
                == Some(ExpressionCategory::Temporary)
            && matches!(call.arguments(), [argument] if argument.argument_index() == 0 && argument.parameter_index() == 0
            && argument.mode() == ParameterMode::Borrow && argument.category() == ExpressionCategory::Temporary
            && argument.parameter_type() == value.return_type())
            && call.return_type() == signature.return_type()
            && call.result_source() == CallableResultSource::Owned
            && call.receiver().is_none()
            && typed.expression_type(expression(input.produce)) == Some(call.return_type())
            && typed.expression_category(expression(input.produce))
                == Some(ExpressionCategory::Temporary),
        "argument actual qualified Int overload/signature/Borrow mapping",
        (call, value, signature),
    )?;
    let constructor = if case.mutable {
        IntrinsicTypeConstructor::MutableMap
    } else {
        IntrinsicTypeConstructor::Map
    };
    need(
        matches!(typed.types().get(call.return_type()), Some(UnitTypeKind::Intrinsic {constructor:c, arguments})
        if *c == constructor && arguments.len() == 2 && arguments.iter().all(|ty| typed.types().get(*ty) == Some(&UnitTypeKind::Builtin(BuiltinType::String)))),
        "argument qualified exact represented factory return",
        call,
    )?;
    let loan = one(
        owned
            .loans()
            .iter()
            .filter(|loan| loan.call() == expression(input.produce)),
        "argument qualified actual argument loan",
    )?;
    need(
        loan.argument() == expression(input.value)
            && loan.kind() == LoanKind::Shared
            && loan.target() == &UnitLoanTarget::Temporary(expression(input.value))
            && loan.begin_span() == ast::span(parsed, input.value)
            && loan.end_span() == ast::span(parsed, input.produce)
            && !owned
                .value_deliveries()
                .iter()
                .any(|delivery| delivery.call() == expression(input.produce)),
        "argument qualified actual temporary Borrow and no Value delivery",
        loan,
    )?;
    need(
        typed.map_iterations().len() == usize::from(!control)
            && owned.map_iterations().len() == usize::from(!control),
        "argument qualified actual provider/control plans",
        typed.map_iterations(),
    )?;
    let Some(site) = &input.loop_site else {
        return Ok(());
    };
    let statement = UnitStatementId::new(unit, site.statement);
    let p = owned
        .map_iteration(statement)
        .ok_or_else(|| failure("argument qualified actual temporary plan", site))?;
    let d = p.descriptor();
    need(
        typed.map_iteration(statement) == Some(d)
            && d.source() == expression(input.produce)
            && d.source_type() == call.return_type()
            && p.source_access() == UnitIterationSourceAccess::Temporary
            && p.source() == Some(&UnitLoanTarget::Temporary(expression(input.produce))),
        "argument qualified factory Call is this provider's hidden owner",
        p,
    )?;
    let read = typed
        .call(expression(site.read))
        .ok_or_else(|| failure("argument actual qualified reader", site.read))?;
    need(
        read.target() == UnitCallTarget::Declaration(declaration(input.reader.name)?)
            && read.arguments().len() == 2
            && read.receiver().is_none(),
        "argument qualified actual reader selection",
        read,
    )?;
    for (index, component) in p.loans().iter().enumerate() {
        let binding = d.bindings()[index];
        let reference = one(
            names
                .names()
                .references()
                .iter()
                .filter(|r| r.span() == ast::span(parsed, site.arguments[index])),
            "argument qualified component reference",
        )?;
        need(
            binding.source_unit() == unit
                && resolution
                    .resolution()
                    .symbols()
                    .get(binding.symbol().index())
                    .is_some_and(|s| s.span() == site.bindings[index])
                && owned.binding_kind(binding) == Some(OwnershipBindingKind::Shared)
                && component.statement() == statement
                && component.source() == expression(input.produce)
                && component.binding() == binding
                && component.origin() == &UnitLoanTarget::Temporary(expression(input.produce))
                && component.kind() == LoanKind::Shared
                && matches!(reference.target(), UnitReferenceTarget::Symbol(root) if *root == binding)
                && read.arguments()[index].mode() == ParameterMode::Borrow
                && read.arguments()[index].argument_index() == index
                && read.arguments()[index].parameter_index() == index
                && read.arguments()[index].parameter_type()
                    == [d.key_type(), d.value_type()][index],
            "argument qualified K/V original roots and Borrow mapping",
            (component, reference),
        )?;
    }
    // Unit has no old cleanup-owner/Always/closure-phi table. The paired graph
    // query proves the original Call birth, every directed current-role edge,
    // exact retirement and complete owner/lease/place definition inventory.
    let drop = one(
        owned
            .drops()
            .iter()
            .filter(|drop| drop.target() == UnitDropTarget::Temporary(expression(input.produce))),
        "argument qualified unique hidden owner Drop",
    )?;
    need(
        drop.point() == UnitDropPoint::LoopExit(statement)
            && drop.value_origin() == ast::span(parsed, input.produce)
            && p.exits().len() == 2,
        "argument qualified Call-origin owner normal Drop",
        drop,
    )?;
    for (kind, point, actions) in [
        (
            UE::Fallthrough,
            UnitDropPoint::AfterStatement(UnitStatementId::new(unit, site.body)),
            vec![
                UA::EndBinding {
                    statement,
                    symbol: d.value_binding(),
                },
                UA::EndBinding {
                    statement,
                    symbol: d.key_binding(),
                },
            ],
        ),
        (
            UE::Exhaustion,
            UnitDropPoint::LoopExit(statement),
            vec![
                UA::FinishProvider(statement),
                UA::EndSource(statement),
                UA::Drop(*drop),
            ],
        ),
    ] {
        let exit = one(
            p.exits().iter().filter(|e| e.kind() == kind),
            "argument qualified actual normal exit",
        )?;
        need(
            exit.point() == point
                && exit.actions() == actions
                && owned.iteration_cleanup_at(point) == Some(actions.as_slice()),
            "argument qualified complete same-point normal cleanup",
            exit,
        )?;
    }
    Ok(())
}
