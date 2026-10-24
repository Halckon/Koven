//! Same-analysis public Call contracts and the independently named current owner.
mod route;
use super::super::{
    fixture::{Failure, failure, need},
    query_inputs::one,
};
use super::{Case, ast};
use lang_frontend::{
    name_resolution::ValidatedCompilationUnitNames,
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

fn unit_owning(kind: &UnitTypeKind) -> bool {
    !matches!(
        kind,
        UnitTypeKind::Builtin(BuiltinType::Int | BuiltinType::Boolean | BuiltinType::Unit)
    )
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
