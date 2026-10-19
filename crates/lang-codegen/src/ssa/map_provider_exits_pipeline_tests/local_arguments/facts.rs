//! Same-analysis public Call contracts and the independently named current owner.
mod owner;
mod route;
use super::super::{
    fixture::{Failure, failure, need},
    query_inputs::one,
};
use super::{Case, ast};
use lang_frontend::{
    name_resolution::{NameResolution, ReferenceTarget, ValidatedCompilationUnitNames},
    ownership_checking::*,
    parser::{Expression, ParsedFile},
    source::{SourceMap, Span},
    type_checking::*,
};
#[derive(Debug)]
pub(super) struct Proof {
    pub implementation: Span,
    pub receiver: Option<route::Receiver>,
}
fn owning(kind: &TypeKind) -> bool {
    !matches!(
        kind,
        TypeKind::Builtin(BuiltinType::Int | BuiltinType::Boolean | BuiltinType::Unit)
    )
}
fn unit_owning(kind: &UnitTypeKind) -> bool {
    !matches!(
        kind,
        UnitTypeKind::Builtin(BuiltinType::Int | BuiltinType::Boolean | BuiltinType::Unit)
    )
}

pub(super) fn single(
    case: &Case,
    _sources: &SourceMap,
    parsed: &ParsedFile,
    input: &ast::Input,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
) -> Result<Proof, Failure> {
    let symbol = |marker| {
        one(
            names
                .symbols()
                .iter()
                .filter(|s| s.span() == marker && !s.is_synthetic()),
            "local argument actual declared symbol",
        )
        .map(|s| s.id())
    };
    let factory = symbol(input.factory.name)?;
    let signature = one(
        typed.callables().iter().filter(|c| c.symbol() == factory),
        "local argument selected source signature",
    )?;
    let call = typed
        .call(input.call)
        .ok_or_else(|| failure("local argument actual typed Call", input.call))?;
    need(
        !case.route
            && call.target() == CallableTarget::Source(factory)
            && call.receiver().is_none()
            && call.return_type() == signature.return_type()
            && call.result_source() == CallableResultSource::Owned
            && call.arguments().len() == input.arguments.len()
            && signature.parameters().len() == input.arguments.len(),
        "local argument exact nonzero overload target/signature/owned result",
        (call, signature),
    )?;
    let Expression::Call { type_arguments, .. } = parsed
        .ast()
        .expressions()
        .get(input.call)
        .unwrap()
        .payload()
    else {
        unreachable!()
    };
    need(
        call.instance().type_arguments().len() == type_arguments.len()
            && (type_arguments.is_empty()
                || matches!(call.instance().type_arguments(),[ty] if typed.types().get(*ty)==Some(&TypeKind::Builtin(BuiltinType::Int)) && _sources.slice(parsed.ast().type_refs().get(type_arguments[0]).unwrap().span())==Ok("Int"))),
        "local argument actual generic instance matches original explicit Int type argument",
        call.instance(),
    )?;
    for (i, a) in input.arguments.iter().enumerate() {
        let d = call.arguments()[i];
        let p = &input.factory.parameters[a.parameter];
        let formal = signature.parameters()[a.parameter];
        need(
            d.argument_index() == i
                && d.parameter_index() == a.parameter
                && d.mode() == p.mode
                && formal.mode == p.mode
                && d.parameter_type() == formal.ty
                && typed.expression_type(a.expression) == Some(formal.ty)
                && d.category() == ExpressionCategory::Temporary
                && typed.expression_category(a.expression) == Some(ExpressionCategory::Temporary)
                && signature.parameter_symbols()[a.parameter] == Some(symbol(p.name)?),
            "local argument exact source-order to declared ABI slot/mode/type/category",
            d,
        )?;
        if let Expression::Call { callee, .. } = parsed
            .ast()
            .expressions()
            .get(a.expression)
            .unwrap()
            .payload()
        {
            let evaluator = typed
                .call(a.expression)
                .ok_or_else(|| failure("local argument evaluator Call", a.expression))?;
            let name = ast::span(parsed, *callee);
            let expected = one(
                names.symbols().iter().filter(|s| {
                    s.span().source_id() == name.source_id()
                        && _sources.slice(s.span()) == _sources.slice(name)
                        && s.kind() == lang_frontend::name_resolution::SymbolKind::Function
                }),
                "local argument source evaluator symbol",
            )?;
            need(
                evaluator.target() == CallableTarget::Source(expected.id())
                    && evaluator.arguments().is_empty()
                    && evaluator.return_type() == formal.ty,
                "local argument actual nested evaluator target",
                evaluator,
            )?;
        }
        let loans = owned
            .loans()
            .iter()
            .filter(|l| l.call() == input.call && l.argument() == a.expression)
            .collect::<Vec<_>>();
        let drops = owned
            .drops()
            .iter()
            .filter(|d| d.target() == DropTarget::Temporary(a.expression))
            .collect::<Vec<_>>();
        if p.mode == ParameterMode::Borrow {
            let end = one(
                owned
                    .loan_ends()
                    .iter()
                    .filter(|e| e.call() == input.call && e.argument() == a.expression),
                "local argument unique original synchronous loan endpoint",
            )?;
            need(
                end.point() == LoanEndPoint::CallReturn(input.call),
                "local argument exact CallReturn loan endpoint",
                end,
            )?;
            need(
                matches!(loans.as_slice(),[l] if l.kind()==lang_frontend::ownership_checking::LoanKind::Shared
                &&l.target()==&LoanTarget::Temporary(a.expression)&&l.begin_span()==ast::span(parsed,a.expression)&&l.end_span()==ast::span(parsed,input.call))
                    && if owning(typed.types().get(formal.ty).unwrap()) {
                        matches!(drops.as_slice(),[d] if d.point()==DropPoint::CallReturn(input.call)&&d.value_origin()==ast::span(parsed,a.expression))
                    } else {
                        drops.is_empty()
                    },
                "local argument exact synchronous Borrow loan and original CallReturn Drop",
                (loans, drops),
            )?;
        } else {
            need(
                loans.is_empty() && drops.is_empty(),
                "local argument owned delivery has no caller loan/Drop",
                (loans, drops),
            )?;
        }
    }
    let p = owned
        .map_iteration(input.statement)
        .ok_or_else(|| failure("local argument actual provider", input.statement))?;
    let d = p.descriptor();
    need(
        typed.map_iterations().len() == 1
            && owned.map_iterations().len() == 1
            && typed.map_iteration(input.statement) == Some(d)
            && d.source() == input.source
            && d.source_type() == call.return_type()
            && typed.is_resource_type(call.return_type()) == Some(case.resource),
        "local argument same actual map descriptor/type",
        p,
    )?;
    let marker = input
        .root
        .ok_or_else(|| failure("local argument Single local marker", input))?;
    let root = symbol(marker)?;
    let reference = one(
        names
            .references()
            .iter()
            .filter(|r| r.span() == ast::span(parsed, input.source)),
        "local argument original For Name resolution",
    )?;
    need(
        reference.target() == &ReferenceTarget::Symbol(root)
            && owned.formed_owned_locals().contains(&root)
            && typed.symbol_type(root) == Some(call.return_type())
            && typed.expression_category(input.source) == Some(ExpressionCategory::Place)
            && p.source_access() == MapIterationSourceAccess::Owned
            && matches!(p.source(),Some(LoanTarget::Place(path)) if path.is_root()&&path.root()==root)
            && !owned
                .drops()
                .iter()
                .any(|drop| drop.target() == DropTarget::Temporary(input.call)),
        "local argument actual formed root owns Call birth and For Place",
        (root, p),
    )?;
    let drop = *one(
        owned
            .drops()
            .iter()
            .filter(|d| d.target() == DropTarget::Named(root)),
        "local argument unique named Drop",
    )?;
    need(
        drop.value_origin() == marker
            && drop.point()
                == if case.resource {
                    DropPoint::AfterStatement(input.scope.body)
                } else {
                    DropPoint::LoopExit(input.statement)
                },
        "local argument Resource lexical or Pure last-use Drop",
        drop,
    )?;
    owner::check(input, root, p, owned, &drop)?;
    for i in 0..2 {
        let binding = symbol(input.bindings[i])?;
        let l = &p.loans()[i];
        need(
            d.bindings()[i] == binding
                && owned.binding_kind(binding) == Some(OwnershipBindingKind::Shared)
                && l.statement() == input.statement
                && l.source() == input.source
                && l.binding() == binding
                && Some(l.origin()) == p.source()
                && l.kind() == lang_frontend::ownership_checking::LoanKind::Shared,
            "local argument original ordered Shared K/V lineage",
            l,
        )?;
    }
    cleanup_single(input, owned, p)?;
    Ok(Proof {
        implementation: input.factory.span,
        receiver: None,
    })
}
fn cleanup_single(
    input: &ast::Input,
    owned: &OwnershipCheckedFile,
    p: &MapIterationOwnershipPlan,
) -> Result<(), Failure> {
    need(
        p.exits().len() == 2,
        "local argument complete normal exits",
        p.exits(),
    )?;
    for (kind, point, mut actions) in [
        (
            IterationExitKind::Fallthrough,
            DropPoint::AfterStatement(input.body),
            vec![
                IterationCleanupAction::EndBinding {
                    statement: input.statement,
                    symbol: p.descriptor().value_binding(),
                },
                IterationCleanupAction::EndBinding {
                    statement: input.statement,
                    symbol: p.descriptor().key_binding(),
                },
            ],
        ),
        (
            IterationExitKind::Exhaustion,
            DropPoint::LoopExit(input.statement),
            vec![
                IterationCleanupAction::FinishProvider(input.statement),
                IterationCleanupAction::EndSource(input.statement),
            ],
        ),
    ] {
        actions.extend(
            owned
                .drops()
                .iter()
                .filter(|d| d.point() == point)
                .copied()
                .map(IterationCleanupAction::Drop),
        );
        let exit = one(
            p.exits().iter().filter(|e| e.kind() == kind),
            "local argument exact normal exit",
        )?;
        need(
            exit.point() == point
                && exit.actions() == actions
                && owned
                    .cleanup_steps()
                    .iter()
                    .filter_map(|(at, a)| (*at == point).then_some(*a))
                    .collect::<Vec<_>>()
                    == actions,
            "local argument complete ordered exit cleanup",
            exit,
        )?;
    }
    Ok(())
}

pub(super) fn unit(
    case: &Case,
    sources: &SourceMap,
    parsed: &ParsedFile,
    input: &ast::Input,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
) -> Result<Proof, Failure> {
    let resolution = one(
        names.names().source_units().iter(),
        "local argument original source unit",
    )?;
    let unit = resolution.source_unit();
    let expression = |id| UnitExpressionId::new(unit, id);
    let symbol = |marker| {
        one(
            resolution
                .resolution()
                .symbols()
                .iter()
                .filter(|s| s.span() == marker && !s.is_synthetic()),
            "local argument qualified source symbol",
        )
        .and_then(|s| {
            one(
                names
                    .names()
                    .references()
                    .iter()
                    .filter_map(|r| match r.target() {
                        lang_frontend::name_resolution::UnitReferenceTarget::Symbol(id)
                            if id.source_unit() == unit && id.symbol() == s.id() =>
                        {
                            Some(*id)
                        }
                        _ => None,
                    })
                    .collect::<std::collections::BTreeSet<_>>()
                    .into_iter(),
                "local argument original qualified root",
            )
        })
    };
    let declaration = |marker| {
        one(
            names
                .names()
                .index()
                .declarations()
                .iter()
                .filter(|d| d.source_unit() == unit && d.name_span() == marker)
                .map(|d| d.id()),
            "local argument actual qualified declaration",
        )
    };
    let call = typed
        .call(expression(input.call))
        .ok_or_else(|| failure("local argument qualified actual Call", input.call))?;
    let receiver = if case.route {
        Some(route::check(
            case, sources, parsed, input, names, typed, owned, call,
        )?)
    } else {
        None
    };
    let signature = if case.route {
        let UnitCallTarget::Symbol(target) = call.target() else {
            unreachable!()
        };
        one(
            typed
                .signatures()
                .declarations()
                .iter()
                .flat_map(|d| {
                    d.callable().into_iter().chain(
                        d.nominal()
                            .into_iter()
                            .flat_map(|n| n.members().iter().chain(n.companion_members())),
                    )
                })
                .filter(|signature| signature.target() == UnitCallableTarget::Symbol(target)),
            "local argument actual Symbol-selected member/forwarder signature",
        )?
    } else {
        let target = declaration(input.factory.name)?;
        need(
            call.target() == UnitCallTarget::Declaration(target) && call.receiver().is_none(),
            "local argument qualified exact selected overload",
            call,
        )?;
        typed
            .signatures()
            .declaration(target)
            .and_then(|d| d.callable())
            .ok_or_else(|| failure("local argument qualified signature", target))?
    };
    need(
        call.return_type() == signature.return_type()
            && call.result_source() == CallableResultSource::Owned
            && call.arguments().len() == input.arguments.len()
            && signature.parameters().len() == input.arguments.len(),
        "local argument qualified exact arity/owned return",
        call,
    )?;
    let Expression::Call { type_arguments, .. } = parsed
        .ast()
        .expressions()
        .get(input.call)
        .unwrap()
        .payload()
    else {
        unreachable!()
    };
    need(
        call.instance().type_arguments().len() == type_arguments.len()
            && (type_arguments.is_empty()
                || matches!(call.instance().type_arguments(),[ty] if typed.types().get(*ty)==Some(&UnitTypeKind::Builtin(BuiltinType::Int)) && sources.slice(parsed.ast().type_refs().get(type_arguments[0]).unwrap().span())==Ok("Int"))),
        "local argument qualified generic instance matches original explicit Int type argument",
        call.instance(),
    )?;
    for (i, a) in input.arguments.iter().enumerate() {
        let d = call.arguments()[i];
        let p = &input.factory.parameters[a.parameter];
        let formal = &signature.parameters()[a.parameter];
        need(
            d.argument_index() == i
                && d.parameter_index() == a.parameter
                && d.mode() == p.mode
                && formal.mode() == p.mode
                && d.parameter_type() == formal.ty()
                && formal.name() == sources.slice(p.name).ok()
                && typed.expression_type(expression(a.expression)) == Some(formal.ty())
                && d.category() == ExpressionCategory::Temporary
                && typed.expression_category(expression(a.expression))
                    == Some(ExpressionCategory::Temporary),
            "local argument qualified exact argument slot/mode/type/category",
            d,
        )?;
        if let Expression::Call { callee, .. } = parsed
            .ast()
            .expressions()
            .get(a.expression)
            .unwrap()
            .payload()
        {
            let evaluator = typed
                .call(expression(a.expression))
                .ok_or_else(|| failure("local argument qualified evaluator", a.expression))?;
            let target = one(
                names
                    .names()
                    .index()
                    .declarations()
                    .iter()
                    .filter(|d| {
                        d.source_unit() == unit
                            && sources.slice(d.name_span())
                                == sources.slice(ast::span(parsed, *callee))
                    })
                    .map(|d| d.id()),
                "local argument actual evaluator declaration",
            )?;
            need(
                evaluator.target() == UnitCallTarget::Declaration(target)
                    && evaluator.arguments().is_empty()
                    && evaluator.return_type() == formal.ty(),
                "local argument selected qualified evaluator",
                evaluator,
            )?;
        }
        let contract = one(
            owned
                .call_argument_contracts_for(expression(input.call))
                .filter(|c| c.argument() == expression(a.expression)),
            "local argument original qualified ownership contract",
        )?;
        need(
            contract.parameter_index() == a.parameter
                && contract.parameter_type() == formal.ty()
                && contract.category() == ExpressionCategory::Temporary
                && contract.argument_span() == ast::span(parsed, a.expression)
                && contract.call_span() == ast::span(parsed, input.call)
                && contract.parameter_span() == Some(formal.span())
                && contract.kind()
                    == if p.mode == ParameterMode::Value {
                        UnitCallArgumentOwnershipKind::Value
                    } else {
                        UnitCallArgumentOwnershipKind::SharedLoan
                    },
            "local argument exact qualified parameter ownership delivery",
            contract,
        )?;
        let loans = owned
            .loans()
            .iter()
            .filter(|l| {
                l.call() == expression(input.call) && l.argument() == expression(a.expression)
            })
            .collect::<Vec<_>>();
        let drops = owned
            .drops()
            .iter()
            .filter(|d| d.target() == UnitDropTarget::Temporary(expression(a.expression)))
            .collect::<Vec<_>>();
        let deliveries = owned
            .value_deliveries()
            .iter()
            .filter(|d| {
                d.call() == expression(input.call) && d.argument() == expression(a.expression)
            })
            .collect::<Vec<_>>();
        if p.mode == ParameterMode::Borrow {
            need(
                matches!(loans.as_slice(),[l]if l.kind()==lang_frontend::ownership_checking::LoanKind::Shared&&l.target()==&UnitLoanTarget::Temporary(expression(a.expression))&&l.begin_span()==ast::span(parsed,a.expression)&&l.end_span()==ast::span(parsed,input.call))
                    && deliveries.is_empty()
                    && if unit_owning(typed.types().get(formal.ty()).unwrap()) {
                        matches!(drops.as_slice(),[d]if d.point()==UnitDropPoint::CallReturn(expression(input.call))&&d.value_origin()==ast::span(parsed,a.expression))
                    } else {
                        drops.is_empty()
                    },
                "local argument qualified CallReturn loan/Drop and no transfer",
                (loans, drops, deliveries),
            )?;
        } else {
            need(
                loans.is_empty()
                    && drops.is_empty()
                    && matches!(deliveries.as_slice(),[d]if d.kind()==UnitValueDeliveryKind::Temporary&&d.source()==&UnitValueDeliverySource::Temporary(expression(a.expression))&&d.parameter_span()==Some(formal.span())),
                "local argument actual temporary owned transfer",
                deliveries,
            )?;
        }
    }
    let statement = UnitStatementId::new(unit, input.statement);
    let p = owned
        .map_iteration(statement)
        .ok_or_else(|| failure("local argument qualified actual provider", statement))?;
    let d = p.descriptor();
    need(
        typed.map_iterations().len() == 1
            && owned.map_iterations().len() == 1
            && typed.map_iteration(statement) == Some(d)
            && d.source() == expression(input.source)
            && d.source_type() == call.return_type()
            && typed.is_resource_type(call.return_type()) == Some(case.resource),
        "local argument same qualified For/map identity",
        p,
    )?;
    let target = if let Some(marker) = input.root {
        let root = symbol(marker)?;
        let reference = one(
            names
                .names()
                .references()
                .iter()
                .filter(|r| r.span() == ast::span(parsed, input.source)),
            "local argument qualified For Name",
        )?;
        need(
            reference.target()
                == &lang_frontend::name_resolution::UnitReferenceTarget::Symbol(root)
                && owned.formed_owned_locals().contains(&root)
                && typed.symbol_type(root) == Some(call.return_type())
                && typed.expression_category(expression(input.source))
                    == Some(ExpressionCategory::Place)
                && p.source_access() == UnitIterationSourceAccess::Owned
                && matches!(p.source(),Some(UnitLoanTarget::Place(path))if path.is_root()&&path.root()==root)
                && !owned
                    .drops()
                    .iter()
                    .any(|d| d.target() == UnitDropTarget::Temporary(expression(input.call))),
            "local argument qualified actual formed birth/root/For identity",
            (root, p),
        )?;
        UnitDropTarget::Named(root)
    } else {
        need(
            case.direct
                && p.source_access() == UnitIterationSourceAccess::Temporary
                && p.source() == Some(&UnitLoanTarget::Temporary(expression(input.call))),
            "local argument original direct temporary control",
            p,
        )?;
        UnitDropTarget::Temporary(expression(input.call))
    };
    let drop = *one(
        owned.drops().iter().filter(|d| d.target() == target),
        "local argument qualified unique current Drop",
    )?;
    need(
        drop.value_origin() == input.root.unwrap_or(ast::span(parsed, input.call))
            && drop.point()
                == if case.resource && !case.direct {
                    UnitDropPoint::AfterStatement(UnitStatementId::new(unit, input.scope.body))
                } else {
                    UnitDropPoint::LoopExit(statement)
                },
        "local argument qualified actual lexical/loop cleanup",
        drop,
    )?;
    for i in 0..2 {
        let binding = d.bindings()[i];
        let l = &p.loans()[i];
        need(
            resolution
                .resolution()
                .symbols()
                .get(binding.symbol().index())
                .is_some_and(|s| s.span() == input.bindings[i])
                && owned.binding_kind(binding) == Some(OwnershipBindingKind::Shared)
                && l.statement() == statement
                && l.source() == expression(input.source)
                && l.binding() == binding
                && Some(l.origin()) == p.source(),
            "local argument qualified real K/V Shared lineage",
            l,
        )?;
    }
    need(
        p.exits().len() == 2,
        "local argument qualified full exit set",
        p.exits(),
    )?;
    for (kind, point, mut actions) in [
        (
            UnitIterationExitKind::Fallthrough,
            UnitDropPoint::AfterStatement(UnitStatementId::new(unit, input.body)),
            vec![
                UnitIterationCleanupAction::EndBinding {
                    statement,
                    symbol: d.value_binding(),
                },
                UnitIterationCleanupAction::EndBinding {
                    statement,
                    symbol: d.key_binding(),
                },
            ],
        ),
        (
            UnitIterationExitKind::Exhaustion,
            UnitDropPoint::LoopExit(statement),
            vec![
                UnitIterationCleanupAction::FinishProvider(statement),
                UnitIterationCleanupAction::EndSource(statement),
            ],
        ),
    ] {
        actions.extend(
            owned
                .drops()
                .iter()
                .filter(|d| d.point() == point)
                .copied()
                .map(UnitIterationCleanupAction::Drop),
        );
        let exit = one(
            p.exits().iter().filter(|e| e.kind() == kind),
            "local argument qualified exact exit",
        )?;
        need(
            exit.point() == point
                && exit.actions() == actions
                && owned.iteration_cleanup_at(point) == Some(actions.as_slice()),
            "local argument qualified complete ordered cleanup",
            exit,
        )?;
    }
    Ok(Proof {
        implementation: input.factory.span,
        receiver,
    })
}
