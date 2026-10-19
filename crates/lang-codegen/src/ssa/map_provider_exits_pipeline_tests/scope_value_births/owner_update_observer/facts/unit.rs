//! Source-qualified effects and cleanup from this Unit analysis only.
use super::*;
use lang_frontend::{
    name_resolution::{UnitReferenceTarget, UnitSymbolId, ValidatedCompilationUnitNames},
    ownership_checking::{
        CompilationUnitOwnership, UnitCallArgumentOwnershipKind, UnitDropTarget,
        UnitIterationSourceAccess, UnitLoanTarget,
    },
    type_checking::{
        CompilationUnitTypes, UnitCallTarget, UnitExpressionId, UnitStatementId, UnitTypeKind,
    },
};
fn local(
    n: &ValidatedCompilationUnitNames,
    marker: lang_frontend::source::Span,
) -> Result<UnitSymbolId, Failure> {
    let (u, id) = one(
        n.names().source_units().iter().flat_map(|u| {
            u.resolution()
                .symbols()
                .iter()
                .filter(move |s| s.span() == marker && !s.is_synthetic())
                .map(move |s| (u.source_unit(), s.id()))
        }),
        "update actual qualified local symbol",
    )?;
    one(
        n.names()
            .references()
            .iter()
            .filter_map(|r| match r.target() {
                UnitReferenceTarget::Symbol(s) if s.source_unit() == u && s.symbol() == id => {
                    Some(*s)
                }
                _ => None,
            })
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter(),
        "update actual qualified referenced local",
    )
}
fn reference(
    p: &ParsedFile,
    n: &ValidatedCompilationUnitNames,
    e: lang_frontend::ast::ExpressionId,
) -> Result<UnitSymbolId, Failure> {
    one(
        n.names()
            .references()
            .iter()
            .filter(|r| r.span() == span(p, e))
            .filter_map(|r| match r.target() {
                UnitReferenceTarget::Symbol(s) => Some(*s),
                _ => None,
            }),
        "update actual qualified Name reference",
    )
}
pub(in crate::ssa::map_provider_exits_pipeline_tests::scope_value_births::owner_update_observer) fn check(
    c: &Case,
    p: &ParsedFile,
    i: &Input,
    n: &ValidatedCompilationUnitNames,
    t: &CompilationUnitTypes,
    o: &CompilationUnitOwnership,
) -> Result<Proof, Failure> {
    let u = one(n.names().source_units().iter(), "update real source unit")?.source_unit();
    let e = |id| UnitExpressionId::new(u, id);
    let root = local(n, i.source)?;
    let locals = i
        .locals
        .iter()
        .map(|l| local(n, l.marker))
        .collect::<Result<Vec<_>, _>>()?;
    let drops = o
        .drops()
        .iter()
        .filter(|d| inside(i.scan, d.value_origin()))
        .collect::<Vec<_>>();
    println!(
        "SCOPE C UPDATE FE label={} unit=true root={root:?} locals={locals:?} formed={:?} scan_drops={drops:#?} all_drops={:#?} loans={:#?} contracts={:#?} deliveries={:#?} assignments={:#?} map_puts={:#?} string_effects={:#?}",
        c.label,
        o.formed_owned_locals(),
        o.drops(),
        o.loans(),
        o.call_argument_contracts(),
        o.value_deliveries(),
        t.assignments(),
        t.map_puts(),
        o.string_effects()
    );
    for d in &drops {
        println!(
            "SCOPE C UPDATE DROP label={} unit=true target={:?} point={:?} origin={:?}",
            c.label,
            d.target(),
            d.point(),
            d.value_origin()
        );
    }
    for b in &i.births {
        println!(
            "SCOPE C UPDATE BIRTH label={} unit=true expression={:?} span={:?} type={:?} category={:?} call={:#?}",
            c.label,
            e(b.expression),
            span(p, b.expression),
            t.expression_type(e(b.expression)),
            t.expression_category(e(b.expression)),
            t.call(e(b.expression))
        );
    }
    if let Some((statement, source)) = i.site {
        let statement = UnitStatementId::new(u, statement);
        println!(
            "SCOPE C UPDATE PLAN label={} unit=true source={:?} descriptor={:#?} plan={:#?}",
            c.label,
            e(source),
            t.map_iteration(statement),
            o.map_iteration(statement)
        );
        if let Some(plan) = o.map_iteration(statement) {
            for exit in plan.exits() {
                println!(
                    "SCOPE C UPDATE EXIT label={} unit=true exit={exit:#?} published_actions={:#?}",
                    c.label,
                    o.iteration_cleanup_at(exit.point())
                );
            }
        }
    }
    for l in &i.locals {
        println!(
            "SCOPE C UPDATE LOCAL label={} unit=true marker={:?} initializer={:?} type={:?} category={:?} reference={:?}",
            c.label,
            l.marker,
            e(l.initializer),
            t.expression_type(e(l.initializer)),
            t.expression_category(e(l.initializer)),
            n.names()
                .references()
                .iter()
                .filter(|r| r.span() == span(p, l.initializer))
                .collect::<Vec<_>>()
        );
    }
    let ty = t
        .symbol_type(root)
        .ok_or_else(|| failure("update qualified source storage type", root))?;
    need(
        matches!(t.types().get(ty),Some(UnitTypeKind::Intrinsic{constructor,arguments})if *constructor==if c.code==9{IntrinsicTypeConstructor::MutableMap}else{IntrinsicTypeConstructor::Map}&&arguments.len()==2)
            && locals.iter().all(|s| o.formed_owned_locals().contains(s)),
        "update qualified actual formed Map locals",
        (ty, &locals),
    )?;
    for b in &i.births {
        let target = one(
            n.names()
                .index()
                .declarations()
                .iter()
                .filter(|d| d.source_unit() == u && d.name_span() == b.name)
                .map(|d| d.id()),
            "update actual qualified factory declaration",
        )?;
        let call = t
            .call(e(b.expression))
            .ok_or_else(|| failure("update actual qualified factory Call", e(b.expression)))?;
        need(
            call.target() == UnitCallTarget::Declaration(target)
                && call.result_source() == CallableResultSource::Owned
                && call.arguments().is_empty()
                && call.receiver().is_none()
                && t.expression_type(e(b.expression)) == Some(ty)
                && t.expression_category(e(b.expression)) == Some(ExpressionCategory::Temporary),
            "update actual qualified distinct Owned factory instances",
            call,
        )?;
    }
    if let Some((assignment, target, rhs)) = i.assignment {
        let d = t.assignment(e(assignment)).ok_or_else(|| {
            failure(
                "update original qualified assignment descriptor",
                e(assignment),
            )
        })?;
        println!(
            "SCOPE C UPDATE ASSIGN label={} unit=true descriptor={d:#?} target_reference={:?} rhs_reference={:?}",
            c.label,
            n.names()
                .references()
                .iter()
                .filter(|r| r.span() == span(p, target))
                .collect::<Vec<_>>(),
            n.names()
                .references()
                .iter()
                .filter(|r| r.span() == span(p, rhs))
                .collect::<Vec<_>>()
        );
        need(
            d.target() == e(target)
                && d.value() == e(rhs)
                && d.target_type() == ty
                && d.falls_through()
                && reference(p, n, target)? == root
                && t.expression_type(e(rhs)) == Some(ty),
            "update exact qualified assignment target/RHS/current storage",
            d,
        )?;
        if c.code == 11 {
            need(
                reference(p, n, rhs)? == local(n, i.locals[0].marker)?,
                "update future Name is exact qualified next binding",
                rhs,
            )?;
        }
    }
    if c.code == 10 {
        let source = i.locals.iter().find(|l| l.marker == i.source).unwrap();
        need(
            reference(p, n, source.initializer)? == local(n, i.locals[0].marker)?,
            "update prior Name moves exact qualified old binding",
            source,
        )?;
    }
    if let Some((_, condition)) = i.conditional {
        let flag = local(n, i.flag.unwrap())?;
        need(
            reference(p, n, condition)? == flag
                && t.expression_type(e(condition)).is_some_and(|ty| {
                    t.types().get(ty) == Some(&UnitTypeKind::Builtin(BuiltinType::Boolean))
                }),
            "update qualified condition references actual Boolean flag",
            condition,
        )?;
    }
    let clone_receivers = super::super::ast::clone_receivers(p, i)?;
    need(
        drops
            .iter()
            .any(|d| matches!(d.target(),UnitDropTarget::Named(s)if locals.contains(&s)))
            && drops.iter().all(|d| match d.target() {
                UnitDropTarget::Named(s) => locals.contains(&s),
                UnitDropTarget::Temporary(e) => clone_receivers.contains(&e.expression()),
                _ => false,
            }),
        "update nonzero qualified epochs and exact clone receiver cleanup targets",
        &drops,
    )?;
    let observe_target = one(
        n.names()
            .index()
            .declarations()
            .iter()
            .filter(|d| d.source_unit() == u && inside(i.observer, d.name_span()))
            .map(|d| d.id()),
        "update exact qualified observe declaration",
    )?;
    for &(call, arg) in &i.observes {
        let typed = t
            .call(e(call))
            .ok_or_else(|| failure("update actual qualified observe Call", e(call)))?;
        let loan = one(
            o.loans()
                .iter()
                .filter(|l| l.call() == e(call) && l.argument() == e(arg)),
            "update actual qualified observe loan",
        )?;
        let contract = one(
            o.call_argument_contracts_for(e(call))
                .filter(|v| v.argument() == e(arg)),
            "update actual qualified observe contract",
        )?;
        println!(
            "SCOPE C UPDATE OBSERVE label={} unit=true call={typed:#?} loan={loan:#?} contract={contract:#?}",
            c.label
        );
        need(
            typed.target() == UnitCallTarget::Declaration(observe_target)
                && reference(p, n, arg)? == root
                && matches!(typed.arguments(),[a]if a.argument_index()==0&&a.parameter_index()==0&&a.mode()==ParameterMode::Borrow&&a.parameter_type()==ty)
                && matches!(loan.target(),UnitLoanTarget::Place(path)if path.is_root()&&path.root()==root)
                && loan.begin_span() == span(p, arg)
                && loan.end_span() == span(p, call)
                && contract.kind() == UnitCallArgumentOwnershipKind::SharedLoan
                && contract.parameter_index() == 0
                && contract.parameter_type() == ty,
            "update exact qualified observe target/current source loan/bounds",
            (typed, loan, contract),
        )?;
    }
    need(
        t.map_iterations().len() == usize::from(c.provider)
            && o.map_iterations().len() == usize::from(c.provider),
        "update actual qualified For/control set",
        o.map_iterations(),
    )?;
    if let Some((statement, source)) = i.site {
        let statement = UnitStatementId::new(u, statement);
        let plan = o
            .map_iteration(statement)
            .ok_or_else(|| failure("update actual qualified provider plan", statement))?;
        need(
            t.map_iteration(statement) == Some(plan.descriptor())
                && plan.descriptor().source() == e(source)
                && plan.descriptor().source_type() == ty
                && reference(p, n, source)? == root
                && matches!(plan.source(),Some(UnitLoanTarget::Place(path))if path.is_root()&&path.root()==root)
                && plan.source_access() == UnitIterationSourceAccess::Owned
                && plan.loans().len() == 2
                && !plan.exits().is_empty(),
            "update qualified canonical source/full nonempty plan",
            plan,
        )?;
    }
    if let Some(put) = i.put {
        let d = t
            .map_put(e(put))
            .ok_or_else(|| failure("update qualified conditional put", e(put)))?;
        need(
            reference(p, n, d.receiver().expression())? == root
                && [d.key(), d.value()]
                    .iter()
                    .all(|e| o.string_effect(*e).is_some()),
            "update qualified put receiver and true independent clones",
            d,
        )?;
    }
    if c.provider {
        super::provider_contract::unit(c, p, i, n, t, o)?;
    }
    super::deep::unit(c, p, i, o, root)
}
