//! Actual selected overload, Borrow argument delivery and the provider's independent owner facts.
use super::super::{
    fixture::{Failure, failure, need},
    query_inputs::one,
};
use super::{Case, ast};
use lang_frontend::{
    name_resolution::{
        NameResolution, ReferenceTarget, UnitReferenceTarget, ValidatedCompilationUnitNames,
    },
    ownership_checking::{
        CleanupCondition as C, CleanupOwnerValue as O, CompilationUnitOwnership, DropPoint,
        DropTarget, IterationCleanupAction as A, IterationExitKind as E, IterationPhiBoundary as B,
        IterationPhiIncomingKind as K, LoanKind, LoanTarget, MapIterationSourceAccess,
        OwnershipBindingKind, OwnershipCheckedFile, UnitDropPoint, UnitDropTarget,
        UnitIterationCleanupAction as UA, UnitIterationExitKind as UE, UnitIterationSourceAccess,
        UnitLoanTarget,
    },
    parser::ParsedFile,
    source::{SourceMap, Span},
    type_checking::{
        BuiltinType, CallableResultSource, CallableTarget, CompilationUnitTypes,
        ExpressionCategory, IntrinsicTypeConstructor, ParameterMode, TypeKind, TypedFile,
        UnitCallTarget, UnitExpressionId, UnitStatementId, UnitTypeKind,
    },
};

pub(in crate::ssa::map_provider_exits_pipeline_tests) fn single(
    case: &Case,
    sources: &SourceMap,
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
    control: bool,
) -> Result<(), Failure> {
    let input = ast::input(sources, parsed, control)?;
    let symbol = |marker| {
        one(
            names
                .symbols()
                .iter()
                .filter(|s| s.span() == marker && !s.is_synthetic()),
            "argument actual source symbol",
        )
        .map(|s| s.id())
    };
    let factory = symbol(input.factory.name)?;
    let signature = one(
        typed.callables().iter().filter(|c| c.symbol() == factory),
        "argument actual selected callable signature",
    )?;
    let call = typed
        .call(input.produce)
        .ok_or_else(|| failure("argument actual typed factory Call", input.produce))?;
    let value = typed
        .call(input.value)
        .ok_or_else(|| failure("argument actual typed argument Call", input.value))?;
    need(
        call.target() == CallableTarget::Source(factory)
            && factory != symbol(input.rejected.name)?
            && signature.parameters().len() == 1
            && signature.parameters()[0].mode == ParameterMode::Borrow
            && typed.types().get(signature.parameters()[0].ty)
                == Some(&TypeKind::Builtin(BuiltinType::Int))
            && signature.parameter_symbols() == [Some(symbol(input.factory.parameter.unwrap())?)]
            && value.target() == CallableTarget::Source(symbol(input.argument.name)?)
            && value.arguments().is_empty()
            && value.receiver().is_none()
            && typed.types().get(value.return_type()) == Some(&TypeKind::Builtin(BuiltinType::Int))
            && typed.expression_type(input.value) == Some(value.return_type())
            && typed.expression_category(input.value) == Some(ExpressionCategory::Temporary)
            && matches!(call.arguments(), [argument] if argument.argument_index() == 0 && argument.parameter_index() == 0
            && argument.mode() == ParameterMode::Borrow && argument.category() == ExpressionCategory::Temporary
            && argument.parameter_type() == value.return_type())
            && call.return_type() == signature.return_type()
            && call.result_source() == CallableResultSource::Owned
            && call.receiver().is_none()
            && typed.expression_type(input.produce) == Some(call.return_type())
            && typed.expression_category(input.produce) == Some(ExpressionCategory::Temporary),
        "argument actual Int overload and precise Borrow mapping/category/signature",
        (call, signature, value),
    )?;
    let constructor = if case.mutable {
        IntrinsicTypeConstructor::MutableMap
    } else {
        IntrinsicTypeConstructor::Map
    };
    need(
        matches!(typed.types().get(call.return_type()), Some(TypeKind::Intrinsic {constructor:c, arguments})
        if *c == constructor && arguments.len() == 2 && arguments.iter().all(|ty| typed.types().get(*ty) == Some(&TypeKind::Builtin(BuiltinType::String)))),
        "argument factory returns original represented String/String map",
        call,
    )?;
    let loan = one(
        owned
            .loans()
            .iter()
            .filter(|loan| loan.call() == input.produce),
        "argument actual ownership loan",
    )?;
    need(
        loan.argument() == input.value
            && loan.kind() == LoanKind::Shared
            && loan.target() == &LoanTarget::Temporary(input.value)
            && loan.begin_span() == ast::span(parsed, input.value)
            && loan.end_span() == ast::span(parsed, input.produce),
        "argument actual temporary loan source and synchronous bounds",
        loan,
    )?;
    need(
        typed.map_iterations().len() == usize::from(!control)
            && owned.map_iterations().len() == usize::from(!control),
        "argument actual provider/control plan count",
        typed.map_iterations(),
    )?;
    let Some(site) = &input.loop_site else {
        return Ok(());
    };
    let p = owned
        .map_iteration(site.statement)
        .ok_or_else(|| failure("argument actual temporary provider plan", site))?;
    let d = p.descriptor();
    need(
        typed.map_iteration(site.statement) == Some(d)
            && d.source() == input.produce
            && d.source_type() == call.return_type()
            && p.source_access() == MapIterationSourceAccess::Temporary
            && p.source() == Some(&LoanTarget::Temporary(input.produce)),
        "argument actual source Call owns this provider",
        p,
    )?;
    let read = typed
        .call(site.read)
        .ok_or_else(|| failure("argument original reader Call", site.read))?;
    need(
        read.target() == CallableTarget::Source(symbol(input.reader.name)?)
            && read.arguments().len() == 2
            && read.receiver().is_none(),
        "argument actual reader selection",
        read,
    )?;
    for (index, component) in p.loans().iter().enumerate() {
        let binding = d.bindings()[index];
        let reference = one(
            names
                .references()
                .iter()
                .filter(|r| r.span() == ast::span(parsed, site.arguments[index])),
            "argument actual component reference",
        )?;
        need(
            binding == symbol(site.bindings[index])?
                && owned.binding_kind(binding) == Some(OwnershipBindingKind::Shared)
                && component.statement() == site.statement
                && component.source() == input.produce
                && component.binding() == binding
                && component.origin() == &LoanTarget::Temporary(input.produce)
                && component.kind() == LoanKind::Shared
                && matches!(reference.target(), ReferenceTarget::Symbol(root) if *root == binding)
                && read.arguments()[index].mode() == ParameterMode::Borrow
                && read.arguments()[index].argument_index() == index
                && read.arguments()[index].parameter_index() == index
                && read.arguments()[index].parameter_type()
                    == [d.key_type(), d.value_type()][index],
            "argument real K/V Shared loan and ordered Borrow argument",
            (component, reference),
        )?;
    }
    let drop = one(
        owned
            .drops()
            .iter()
            .filter(|drop| drop.target() == DropTarget::Temporary(input.produce)),
        "argument unique hidden-owner Drop",
    )?;
    let table = owned.cleanup_conditions();
    need(
        drop.point() == DropPoint::LoopExit(site.statement)
            && drop.value_origin() == ast::span(parsed, input.produce)
            && drop.condition().is_none()
            && drop.capture_slot().is_none()
            && drop.instance_address().is_none()
            && matches!(drop.owner().and_then(|owner| table.owner_value(owner)), Some(O::Expression {expression, origin})
            if *expression == input.produce && *origin == ast::span(parsed, input.produce))
            && p.exits().len() == 2
            && p.closure_phis().is_empty()
            && p.closure_phi_incomings().len() == 3,
        "argument exact Call-origin hidden owner and full normal incoming facts",
        (drop, p),
    )?;
    for (kind, point, actions) in [
        (
            E::Fallthrough,
            DropPoint::AfterStatement(site.body),
            vec![
                A::EndBinding {
                    statement: site.statement,
                    symbol: d.value_binding(),
                },
                A::EndBinding {
                    statement: site.statement,
                    symbol: d.key_binding(),
                },
            ],
        ),
        (
            E::Exhaustion,
            DropPoint::LoopExit(site.statement),
            vec![
                A::FinishProvider(site.statement),
                A::EndSource(site.statement),
                A::Drop(*drop),
            ],
        ),
    ] {
        let exit = one(
            p.exits().iter().filter(|e| e.kind() == kind),
            "argument original normal cleanup exit",
        )?;
        need(
            exit.point() == point
                && exit.actions() == actions
                && table.get(exit.condition()) == Some(&C::Always)
                && owned
                    .cleanup_steps()
                    .iter()
                    .filter_map(|(at, action)| (*at == point).then_some(*action))
                    .collect::<Vec<_>>()
                    == actions,
            "argument full ordered same-point cleanup",
            exit,
        )?;
    }
    for (kind, point, boundary) in [
        (
            K::Entry,
            DropPoint::AfterExpression(input.produce),
            B::Header,
        ),
        (
            K::Fallthrough,
            DropPoint::AfterStatement(site.body),
            B::Header,
        ),
        (K::Exhaustion, DropPoint::LoopExit(site.statement), B::Exit),
    ] {
        let edge = one(
            p.closure_phi_incomings()
                .iter()
                .filter(|e| e.kind() == kind),
            "argument actual incoming kind",
        )?;
        need(
            edge.point() == point
                && edge.boundary() == boundary
                && edge.bindings().is_empty()
                && table.get(edge.condition()) == Some(&C::Always),
            "argument incoming exact point/Always/empty bindings",
            edge,
        )?;
    }
    Ok(())
}

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
