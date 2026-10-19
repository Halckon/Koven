//! 同次公开 FE products 的身份桥；不调用生产 qualification，不构造或修改 facts。
use super::{
    fixture::{Exit, Failure, Fixture, LoopSite, failure, loop_site, need},
    identity,
};
use lang_frontend::{
    name_resolution::{NameResolution, ValidatedCompilationUnitNames},
    ownership_checking::{
        CleanupCondition, CleanupOwnerValue, CleanupSelection, CleanupSelectorSource,
        CompilationUnitOwnership, DropPoint, DropTarget, IterationCleanupAction as A,
        IterationExitKind as E, IterationPhiBoundary, IterationPhiIncomingKind as K, LoanTarget,
        OwnershipCheckedFile, UnitDropPoint, UnitDropTarget, UnitIterationCleanupAction as UA,
        UnitIterationExitKind as UE, UnitLoanTarget,
    },
    parser::{Item, NameMarker},
    source::Span,
    type_checking::{
        CallableTarget, CompilationUnitTypes, TypedFile, UnitCallTarget, UnitExpressionId,
        UnitStatementId,
    },
};

pub(in crate::ssa) fn one<T>(
    mut values: impl Iterator<Item = T>,
    stage: &'static str,
) -> Result<T, Failure> {
    let value = values
        .next()
        .ok_or_else(|| failure(stage, "missing entity"))?;
    need(values.next().is_none(), stage, "multiple entities")?;
    Ok(value)
}

#[derive(Debug)]
pub(super) struct Inputs {
    pub scan: Span,
    pub consumer: Span,
    pub deinit: Span,
    pub parameter: Span,
    pub parameter_slot: usize,
    pub source_drop: Option<Span>,
    pub site: LoopSite,
    pub owned: bool,
    pub exit: Exit,
}

fn function_span(f: &Fixture, name: Span) -> Result<Span, Failure> {
    one(f.parsed.roots().iter().filter_map(|root| {
        let node = f.parsed.ast().items().get(*root).ok()?;
        matches!(node.payload(), Item::Function { name: NameMarker::Present(n), .. } if *n == name)
            .then_some(node.span())
    }), "identity FE actual function root")
}

fn finish(
    f: &Fixture,
    scan: Span,
    consumer: Span,
    parameter: Span,
    parameter_slot: usize,
    source_drop: Option<Span>,
) -> Result<Inputs, Failure> {
    let functions = identity::source_functions(f)?;
    let scan = function_span(f, scan)?;
    let consumer = function_span(f, consumer)?;
    need(
        functions.contains(&("scan", scan)) && functions.contains(&(f.exit.callee(), consumer)),
        "identity FE source declarations agree with actual AST roots",
        &functions,
    )?;
    let deinit = one(
        functions
            .iter()
            .filter_map(|(n, s)| (*n == "Guard.deinit").then_some(*s)),
        "identity FE Guard deinit root",
    )?;
    Ok(Inputs {
        scan,
        consumer,
        deinit,
        parameter,
        parameter_slot,
        source_drop,
        site: loop_site(f)?,
        owned: f.map,
        exit: f.exit,
    })
}

pub(super) fn single(
    f: &Fixture,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
) -> Result<Inputs, Failure> {
    let s = loop_site(f)?;
    let d = typed
        .map_iteration(s.statement)
        .ok_or_else(|| failure("identity FE descriptor", &s))?;
    let p = owned
        .map_iteration(s.statement)
        .ok_or_else(|| failure("identity FE plan", &s))?;
    let Some(LoanTarget::Place(path)) = p.source() else {
        return Err(failure("identity FE named source", p));
    };
    let source = path.root();
    let parameter = one(
        names.symbols().iter().filter(|v| v.id() == source),
        "identity FE parameter",
    )?;
    let (signature, slot) = one(
        typed.callables().iter().flat_map(|c| {
            c.parameter_symbols()
                .iter()
                .enumerate()
                .filter_map(move |(i, v)| (*v == Some(source)).then_some((c, i)))
        }),
        "identity FE actual parameter slot",
    )?;
    need(
        path.is_root() && signature.receiver().is_none(),
        "identity FE direct parameter",
        signature,
    )?;
    let scan = one(
        names
            .symbols()
            .iter()
            .filter(|v| v.id() == signature.symbol()),
        "identity FE scan symbol",
    )?;
    let call = typed
        .call(s.call)
        .ok_or_else(|| failure("identity FE actual consumer call", s.call))?;
    let CallableTarget::Source(callee) = call.target() else {
        return Err(failure("identity FE source call target", call));
    };
    let consumer = one(
        names.symbols().iter().filter(|v| v.id() == callee),
        "identity FE consumer symbol",
    )?;
    let local = one(
        names.symbols().iter().filter(|v| v.span() == s.local),
        "identity FE Guard local",
    )?;
    let guard = *one(
        owned
            .drops()
            .iter()
            .filter(|v| v.target() == DropTarget::Named(local.id())),
        "identity FE Guard Drop",
    )?;
    need(guard.point() == DropPoint::ControlTransfer(s.transfer)
        && guard.value_origin() == s.local && guard.condition().is_none()
        && guard.owner().is_some_and(|owner| matches!(owned.cleanup_conditions().owner_value(owner),
            Some(CleanupOwnerValue::Expression { expression, origin }) if *expression == s.guard && *origin == s.guard_span)),
        "identity FE exact Guard construction/drop", guard)?;
    let drops = owned
        .drops()
        .iter()
        .filter(|v| v.target() == DropTarget::Named(source))
        .collect::<Vec<_>>();
    let expected_count = usize::from(f.map) * if f.exit == Exit::Continue { 1 } else { 2 };
    need(
        drops.len() == expected_count && drops.iter().all(|v| v.value_origin() == parameter.span()),
        "identity FE source Drop count/origin",
        &drops,
    )?;
    let transfer_kind = match f.exit {
        Exit::Continue => E::Continue(s.transfer),
        Exit::Break => E::Break(s.transfer),
        Exit::Return => E::Return(s.transfer),
    };
    need(
        p.exits().len() == 2,
        "identity FE two actual exits",
        p.exits(),
    )?;
    for (kind, point, transfer) in [
        (transfer_kind, DropPoint::ControlTransfer(s.transfer), true),
        (E::Exhaustion, DropPoint::LoopExit(s.statement), false),
    ] {
        let exit = one(
            p.exits()
                .iter()
                .filter(|v| v.kind() == kind && v.point() == point),
            "identity FE exact exit",
        )?;
        let mut expected = if transfer {
            vec![
                A::Drop(guard),
                A::EndBinding {
                    statement: s.statement,
                    symbol: d.value_binding(),
                },
                A::EndBinding {
                    statement: s.statement,
                    symbol: d.key_binding(),
                },
            ]
        } else {
            vec![]
        };
        if !transfer || f.exit != Exit::Continue {
            expected.extend([A::FinishProvider(s.statement), A::EndSource(s.statement)]);
            if f.map {
                expected.push(A::Drop(*one(
                    drops.iter().copied().filter(|v| v.point() == point),
                    "identity FE per-path source Drop",
                )?));
            }
        }
        let full = owned
            .cleanup_steps()
            .iter()
            .filter_map(|(at, action)| (*at == point).then_some(*action))
            .collect::<Vec<_>>();
        need(
            exit.actions() == expected && full == expected,
            "identity FE exact complete cleanup order",
            (exit, full, expected),
        )?;
    }
    if f.map {
        let header = one(
            p.closure_phis()
                .iter()
                .filter(|v| v.symbol() == source && v.boundary() == IterationPhiBoundary::Header),
            "identity FE source header owner",
        )?;
        let exit = one(
            p.closure_phis()
                .iter()
                .filter(|v| v.symbol() == source && v.boundary() == IterationPhiBoundary::Exit),
            "identity FE source exit owner",
        )?;
        let conditions = owned.cleanup_conditions();
        need(
            p.closure_phis().len() == 2
                && header.owner() != exit.owner()
                && header.availability_selector() != exit.availability_selector()
                && drops.iter().all(|v| {
                    v.owner() == Some(header.owner())
                        && v.condition() == Some(header.availability_condition())
                }),
            "identity FE each terminating Drop has current header owner",
            (p.closure_phis(), &drops),
        )?;
        for phi in [header, exit] {
            let owner = conditions.owner_value(phi.owner());
            need(
                matches!(owner, Some(CleanupOwnerValue::IterationPhi { statement, boundary, symbol, origin })
                if *statement == s.statement && *boundary == phi.boundary()
                    && *symbol == source && *origin == s.span),
                "identity FE actual source owner phi statement/boundary/origin",
                owner,
            )?;
            let selector = conditions
                .selector(phi.availability_selector())
                .ok_or_else(|| failure("identity FE source availability selector", phi))?;
            need(
                matches!(selector.source(), CleanupSelectorSource::IterationPhi { statement, boundary, .. }
                if statement == s.statement && boundary == phi.boundary())
                    && selector.selection() == CleanupSelection::IterationPresence
                    && selector.origin() == s.span
                    && selector.branch_count() == 2,
                "identity FE source selector actual boundary and presence domain",
                selector,
            )?;
            let Some(CleanupCondition::Choice { selector, branches }) =
                conditions.get(phi.availability_condition())
            else {
                return Err(failure("identity FE source availability is Choice", phi));
            };
            need(
                *selector == phi.availability_selector()
                    && branches.len() == 2
                    && conditions.get(branches[0]) == Some(&CleanupCondition::Never)
                    && conditions.get(branches[1]) == Some(&CleanupCondition::Always),
                "identity FE source Choice exact selector and Never/Always branches",
                phi,
            )?;
        }
        let mut kinds = vec![K::Entry, K::Exhaustion];
        match f.exit {
            Exit::Continue => kinds.push(K::Continue(s.transfer)),
            Exit::Break => kinds.push(K::Break(s.transfer)),
            Exit::Return => {}
        }
        need(
            p.closure_phi_incomings().len() == kinds.len(),
            "identity FE full owner incoming set",
            p.closure_phi_incomings(),
        )?;
        for kind in kinds {
            let incoming = one(
                p.closure_phi_incomings()
                    .iter()
                    .filter(|v| v.kind() == kind),
                "identity FE owner incoming kind",
            )?;
            let binding = one(
                incoming.bindings().iter(),
                "identity FE exact source binding",
            )?;
            let entry = kind == K::Entry;
            let back = matches!(kind, K::Continue(_));
            let point = if entry {
                DropPoint::AfterExpression(s.source)
            } else if kind == K::Exhaustion {
                DropPoint::LoopExit(s.statement)
            } else {
                DropPoint::ControlTransfer(s.transfer)
            };
            need(
                incoming.point() == point
                    && incoming.boundary()
                        == if entry || back {
                            IterationPhiBoundary::Header
                        } else {
                            IterationPhiBoundary::Exit
                        }
                    && binding.target()
                        == if entry || back {
                            header.owner()
                        } else {
                            exit.owner()
                        },
                "identity FE edge target and control point",
                incoming,
            )?;
            if entry || back {
                let value = one(binding.values().iter(), "identity FE owner incoming value")?;
                need(
                    value.condition() == binding.available_when()
                        && if entry {
                            conditions.get(binding.available_when())
                                == Some(&CleanupCondition::Always)
                                && matches!(conditions.owner_value(value.source()), Some(CleanupOwnerValue::Parameter { symbol, origin }) if *symbol == source && *origin == parameter.span())
                        } else {
                            value.source() == header.owner()
                                && binding.available_when() == header.availability_condition()
                        },
                    "identity FE actual seed/current owner incoming",
                    incoming,
                )?;
            } else {
                need(
                    binding.values().is_empty()
                        && conditions.get(binding.available_when())
                            == Some(&CleanupCondition::Never),
                    "identity FE terminated owner cannot enter merge",
                    incoming,
                )?;
            }
        }
    } else {
        need(
            p.closure_phis().is_empty()
                && p.closure_phi_incomings()
                    .iter()
                    .all(|v| v.bindings().is_empty()),
            "identity FE Borrow has no owner phi bindings",
            p,
        )?;
    }
    finish(
        f,
        scan.span(),
        consumer.span(),
        parameter.span(),
        slot,
        f.map.then_some(parameter.span()),
    )
}

pub(super) fn unit(
    f: &Fixture,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
) -> Result<Inputs, Failure> {
    let s = loop_site(f)?;
    let source_unit = one(
        names.names().source_units().iter(),
        "identity FE original unit",
    )?;
    let unit = source_unit.source_unit();
    let statement = UnitStatementId::new(unit, s.statement);
    let transfer = UnitExpressionId::new(unit, s.transfer);
    let d = typed
        .map_iteration(statement)
        .ok_or_else(|| failure("identity FE unit descriptor", statement))?;
    let p = owned
        .map_iteration(statement)
        .ok_or_else(|| failure("identity FE unit plan", statement))?;
    let Some(UnitLoanTarget::Place(path)) = p.source() else {
        return Err(failure("identity FE unit root", p));
    };
    let source = path.root();
    need(
        path.is_root() && source.source_unit() == unit,
        "identity FE unit source identity",
        source,
    )?;
    let parameter = one(
        source_unit
            .resolution()
            .symbols()
            .iter()
            .filter(|v| v.id() == source.symbol()),
        "identity FE unit parameter",
    )?;
    let (signature, slot) = one(
        typed
            .signatures()
            .declarations()
            .iter()
            .filter_map(|v| v.callable())
            .flat_map(|c| {
                c.parameters()
                    .iter()
                    .enumerate()
                    .filter_map(move |(i, p)| (p.symbol() == Some(source)).then_some((c, i)))
            }),
        "identity FE unit parameter slot",
    )?;
    need(
        signature.receiver().is_none(),
        "identity FE unit no receiver",
        signature,
    )?;
    let call = typed
        .call(UnitExpressionId::new(unit, s.call))
        .ok_or_else(|| failure("identity FE unit call", s.call))?;
    let UnitCallTarget::Declaration(callee) = call.target() else {
        return Err(failure("identity FE unit source callee", call));
    };
    let consumer = one(
        names
            .names()
            .index()
            .declarations()
            .iter()
            .filter(|v| v.id() == callee && v.source_unit() == unit),
        "identity FE unit consumer",
    )?;
    let local = one(
        source_unit
            .resolution()
            .symbols()
            .iter()
            .filter(|v| v.span() == s.local),
        "identity FE unit Guard local",
    )?;
    let guard = *one(owned.drops().iter().filter(|v| matches!(v.target(), UnitDropTarget::Named(n) if n.source_unit() == unit && n.symbol() == local.id())), "identity FE unit Guard Drop")?;
    need(
        guard.point() == UnitDropPoint::ControlTransfer(transfer)
            && guard.value_origin() == s.local,
        "identity FE unit Guard Drop identity",
        guard,
    )?;
    let drops = owned
        .drops()
        .iter()
        .filter(|v| v.target() == UnitDropTarget::Named(source))
        .collect::<Vec<_>>();
    let expected_count = usize::from(f.map) * if f.exit == Exit::Continue { 1 } else { 2 };
    need(
        drops.len() == expected_count && drops.iter().all(|v| v.value_origin() == parameter.span()),
        "identity FE unit all source Drop origins",
        &drops,
    )?;
    let transfer_kind = match f.exit {
        Exit::Continue => UE::Continue(transfer),
        Exit::Break => UE::Break(transfer),
        Exit::Return => UE::Return(transfer),
    };
    need(
        p.exits().len() == 2,
        "identity FE unit actual two exits",
        p.exits(),
    )?;
    for (kind, point, is_transfer) in [
        (
            transfer_kind,
            UnitDropPoint::ControlTransfer(transfer),
            true,
        ),
        (UE::Exhaustion, UnitDropPoint::LoopExit(statement), false),
    ] {
        let exit = one(
            p.exits()
                .iter()
                .filter(|v| v.kind() == kind && v.point() == point),
            "identity FE unit exact exit",
        )?;
        let mut expected = if is_transfer {
            vec![
                UA::Drop(guard),
                UA::EndBinding {
                    statement,
                    symbol: d.value_binding(),
                },
                UA::EndBinding {
                    statement,
                    symbol: d.key_binding(),
                },
            ]
        } else {
            vec![]
        };
        if !is_transfer || f.exit != Exit::Continue {
            expected.extend([UA::FinishProvider(statement), UA::EndSource(statement)]);
            if f.map {
                expected.push(UA::Drop(*one(
                    drops.iter().copied().filter(|v| v.point() == point),
                    "identity FE unit per-path source Drop",
                )?));
            }
        }
        need(
            exit.actions() == expected
                && owned.iteration_cleanup_at(point) == Some(expected.as_slice()),
            "identity FE unit full cleanup sequence",
            (exit, expected),
        )?;
    }
    finish(
        f,
        signature.name_span(),
        consumer.name_span(),
        parameter.span(),
        slot,
        f.map.then_some(parameter.span()),
    )
}
