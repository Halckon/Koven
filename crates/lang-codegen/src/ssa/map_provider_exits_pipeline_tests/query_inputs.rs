//! 同次公开 FE products 的身份桥；不调用生产 qualification，不构造或修改 facts。
use super::{
    fixture::{Exit, Failure, Fixture, LoopSite, failure, loop_site, need},
    identity,
};
use lang_frontend::{
    name_resolution::ValidatedCompilationUnitNames,
    ownership_checking::{
        CompilationUnitOwnership, UnitDropPoint, UnitDropTarget, UnitIterationCleanupAction as UA,
        UnitIterationExitKind as UE, UnitLoanTarget,
    },
    parser::{Item, NameMarker},
    source::Span,
    type_checking::{CompilationUnitTypes, UnitCallTarget, UnitExpressionId, UnitStatementId},
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
