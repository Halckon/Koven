//! Same-analysis public facts, including the complete cleanup facts before lowering.
mod provider;
use super::super::super::{
    fixture::{Failure, failure, need},
    query_inputs::one,
};
use super::{
    Case,
    ast::{Input, inside, span},
};
use crate::ssa::unit_plan;
use lang_frontend::{
    name_resolution::{UnitReferenceTarget, UnitSymbolId, ValidatedCompilationUnitNames},
    ownership_checking::{
        CompilationUnitOwnership, IterationSource, UnitCallArgumentOwnershipKind, UnitDropFact,
        UnitDropPoint, UnitDropTarget, UnitIterationSourceAccess, UnitLoanTarget,
    },
    parser::{Expression, ParsedFile},
    source::Span,
    type_checking::{
        BuiltinType, CallableResultSource, CompilationUnitTypes, ExpressionCategory,
        IntrinsicTypeConstructor, ParameterMode, UnitCallTarget, UnitCallableTarget,
        UnitExpressionId, UnitStatementId, UnitTypeKind,
    },
};
#[derive(Debug)]
pub(super) struct Proof {
    pub drop: UnitDropFact,
}
fn root(n: &ValidatedCompilationUnitNames, at: Span) -> Result<UnitSymbolId, Failure> {
    let (u, symbol) = one(
        n.names().source_units().iter().flat_map(|u| {
            u.resolution()
                .symbols()
                .iter()
                .filter(move |s| s.span() == at && !s.is_synthetic())
                .map(move |s| (u.source_unit(), s.id()))
        }),
        "If original source-qualified symbol",
    )?;
    one(
        n.names()
            .references()
            .iter()
            .filter_map(|r| match r.target() {
                UnitReferenceTarget::Symbol(s) if s.source_unit() == u && s.symbol() == symbol => {
                    Some(*s)
                }
                _ => None,
            })
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter(),
        "If original referenced qualified symbol",
    )
}
pub(super) fn check(
    c: &Case,
    p: &ParsedFile,
    i: &Input,
    n: &ValidatedCompilationUnitNames,
    t: &CompilationUnitTypes,
    o: &CompilationUnitOwnership,
) -> Result<Proof, Failure> {
    let unit = one(n.names().source_units().iter(), "If original source unit")?.source_unit();
    let e = |id| UnitExpressionId::new(unit, id);
    let decl = |at: Span| {
        one(
            n.names()
                .index()
                .declarations()
                .iter()
                .filter(|d| d.source_unit() == unit && inside(at, d.name_span()))
                .map(|d| d.id()),
            "If exact original callable declaration",
        )
    };
    let make = one(
        n.names()
            .index()
            .declarations()
            .iter()
            .filter(|d| d.source_unit() == unit && d.name_span() == i.make_name)
            .map(|d| d.id()),
        "If actual source-qualified make declaration",
    )?;
    let local = i.local.map(|at| root(n, at)).transpose()?;
    let drops = o
        .drops()
        .iter()
        .copied()
        .filter(|d| inside(i.scan, d.value_origin()))
        .collect::<Vec<_>>();
    println!(
        "SCOPE B IF FE label={} unit=true conditional={:?} condition={:?} arms={:?} if_type={:?} if_category={:?} condition_type={:?} local={local:?} local_type={:?} formed={:?} scan_drops={drops:#?} all_drops={:#?}",
        c.label,
        e(i.conditional),
        e(i.condition),
        i.arms,
        t.expression_type(e(i.conditional)),
        t.expression_category(e(i.conditional)),
        t.expression_type(e(i.condition)),
        local.and_then(|s| t.symbol_type(s)),
        o.formed_owned_locals(),
        o.drops()
    );
    for drop in &drops {
        println!(
            "SCOPE B IF DROP label={} unit=true target={:?} point={:?} origin={:?}",
            c.label,
            drop.target(),
            drop.point(),
            drop.value_origin()
        );
    }
    for birth in i.births {
        println!(
            "SCOPE B IF BIRTH label={} unit=true expression={:?} span={:?} type={:?} category={:?} call={:#?}",
            c.label,
            e(birth),
            span(p, birth),
            t.expression_type(e(birth)),
            t.expression_category(e(birth)),
            t.call(e(birth))
        );
    }
    if let Some((statement, source)) = i.site {
        let statement = UnitStatementId::new(unit, statement);
        println!(
            "SCOPE B IF PLAN label={} unit=true statement={statement:?} surface={:?} surface_type={:?} surface_category={:?} descriptor={:#?} ownership={:#?}",
            c.label,
            e(source),
            t.expression_type(e(source)),
            t.expression_category(e(source)),
            t.map_iteration(statement),
            o.map_iteration(statement)
        );
        if let Some(plan) = o.map_iteration(statement) {
            println!(
                "SCOPE B IF CANONICAL label={} unit=true source_kind={:?} source={:?} access={:?} bindings={:?} loans={:#?} exits={:#?}",
                c.label,
                plan.source_kind(),
                plan.source(),
                plan.source_access(),
                plan.bindings(),
                plan.loans(),
                plan.exits()
            );
            for exit in plan.exits() {
                println!(
                    "SCOPE B IF EXIT label={} unit=true exit={exit:#?} published_actions={:#?}",
                    c.label,
                    o.iteration_cleanup_at(exit.point())
                );
            }
        }
    }
    for &(call, arg) in &i.observes {
        println!(
            "SCOPE B IF OBSERVE label={} unit=true expression={:?} argument={:?} typed={:#?} contracts={:#?} loans={:#?}",
            c.label,
            e(call),
            e(arg),
            t.call(e(call)),
            o.call_argument_contracts_for(e(call)).collect::<Vec<_>>(),
            o.loans()
                .iter()
                .filter(|l| l.call() == e(call))
                .collect::<Vec<_>>()
        );
    }
    let ty = t
        .expression_type(e(i.conditional))
        .ok_or_else(|| failure("If actual result type", e(i.conditional)))?;
    need(
        matches!(t.types().get(ty),Some(UnitTypeKind::Intrinsic{constructor:IntrinsicTypeConstructor::Map,arguments})if arguments.len()==2&&arguments.iter().all(|v|t.types().get(*v)==Some(&UnitTypeKind::Builtin(BuiltinType::String))))
            && t.expression_category(e(i.conditional)).is_some()
            && t.expression_type(e(i.condition)).is_some_and(|ty| {
                t.types().get(ty) == Some(&UnitTypeKind::Builtin(BuiltinType::Boolean))
            }),
        "If real concrete Map result and Boolean condition",
        ty,
    )?;
    let flag = root(n, i.flag)?;
    let reference = one(
        n.names()
            .references()
            .iter()
            .filter(|r| r.span() == span(p, i.condition)),
        "If original actual flag reference",
    )?;
    need(
        reference.target() == &UnitReferenceTarget::Symbol(flag),
        "If condition refers to actual scanner flag",
        reference,
    )?;
    for birth in i.births {
        let call = t
            .call(e(birth))
            .ok_or_else(|| failure("If actual arm Call descriptor", e(birth)))?;
        let target = UnitCallableTarget::Declaration(make);
        let resolved = unit_plan::resolve_unit_call_instance(
            t,
            o,
            target,
            call.instance().type_arguments().to_vec(),
            call.receiver().map(|r| r.ty()),
            span(p, birth),
        )
        .map_err(|err| failure("If actual make Call instance", err))?;
        println!(
            "SCOPE B IF INSTANCE label={} unit=true expression={:?} instance={:?} resolved={:?} delegation={:?}",
            c.label,
            e(birth),
            call.instance(),
            resolved.key(),
            resolved.delegation()
        );
        need(
            call.expression() == e(birth)
                && call.target() == UnitCallTarget::Declaration(make)
                && call.arguments().is_empty()
                && call.receiver().is_none()
                && call.result_source() == CallableResultSource::Owned
                && call.return_type() == ty
                && t.expression_type(e(birth)) == Some(ty)
                && t.expression_category(e(birth)) == Some(ExpressionCategory::Temporary)
                && resolved.key().target() == target
                && resolved.delegation().is_empty(),
            "If both original calls select actual make Owned result/instance",
            call,
        )?;
    }
    if let Some(root) = local {
        need(
            t.symbol_type(root) == Some(ty) && o.formed_owned_locals().contains(&root),
            "If original local forms from concrete If result",
            (root, o.formed_owned_locals()),
        )?;
        need(
            drops
                .iter()
                .any(|d| d.target() == UnitDropTarget::Named(root)),
            "If actual local has published named cleanup",
            &drops,
        )?;
    }
    need(
        t.map_iterations().len() == usize::from(c.provider)
            && o.map_iterations().len() == usize::from(c.provider),
        "If actual acquired For/control plan set",
        o.map_iterations(),
    )?;
    if let Some((statement, source)) = i.site {
        let statement = UnitStatementId::new(unit, statement);
        let plan = o
            .map_iteration(statement)
            .ok_or_else(|| failure("If actual acquired ownership plan", statement))?;
        need(
            t.map_iteration(statement) == Some(plan.descriptor())
                && plan.descriptor().source() == e(source)
                && plan.descriptor().source_type() == ty
                && plan.loans().len() == 2
                && !plan.exits().is_empty(),
            "If actual surface descriptor and complete nonempty published plan",
            plan,
        )?;
        if c.temporary {
            need(
                plan.source_kind()
                    == &IterationSource::BorrowedSource(UnitLoanTarget::Temporary(
                        e(i.conditional),
                    ))
                    && plan.source_access() == UnitIterationSourceAccess::Temporary
                    && drops
                        .iter()
                        .any(|d| d.target() == UnitDropTarget::Temporary(e(i.conditional))),
                "If full conditional is the original temporary owner/source/drop",
                plan,
            )?;
        } else {
            let local = local.ok_or_else(|| failure("If actual named For source", i))?;
            let reference = one(
                n.names()
                    .references()
                    .iter()
                    .filter(|r| r.span() == span(p, source)),
                "If actual source Name reference",
            )?;
            need(
                matches!(
                    p.ast().expressions().get(source).unwrap().payload(),
                    Expression::Name
                ) && reference.target() == &UnitReferenceTarget::Symbol(local)
                    && matches!(plan.source(),Some(UnitLoanTarget::Place(path))if path.is_root()&&path.root()==local)
                    && plan.source_access() == UnitIterationSourceAccess::Owned,
                "If actual named surface source and canonical local owner",
                plan,
            )?;
        }
    }
    for &(call, arg) in &i.observes {
        let call_descriptor = t
            .call(e(call))
            .ok_or_else(|| failure("If real observe descriptor", e(call)))?;
        let contract = one(
            o.call_argument_contracts_for(e(call))
                .filter(|v| v.argument() == e(arg)),
            "If actual observe contract",
        )?;
        let loan = one(
            o.loans()
                .iter()
                .filter(|v| v.call() == e(call) && v.argument() == e(arg)),
            "If actual observe loan",
        )?;
        let local = local.ok_or_else(|| failure("If observe actual local owner", i))?;
        need(
            call_descriptor.target() == UnitCallTarget::Declaration(decl(i.observe)?)
                && matches!(call_descriptor.arguments(),[a]if a.argument_index()==0&&a.parameter_index()==0&&a.mode()==ParameterMode::Borrow&&a.parameter_type()==ty)
                && contract.kind() == UnitCallArgumentOwnershipKind::SharedLoan
                && contract.parameter_index() == 0
                && contract.parameter_type() == ty
                && matches!(loan.target(),UnitLoanTarget::Place(path)if path.is_root()&&path.root()==local)
                && loan.begin_span() == span(p, arg)
                && loan.end_span() == span(p, call),
            "If observe borrows the same original local through actual call completion",
            (call_descriptor, contract, loan),
        )?;
    }
    let drop = *one(drops.iter(), "If exact sole scanner owner cleanup fact")?;
    let (target, point, origin) = if c.temporary {
        let (statement, _) = i
            .site
            .ok_or_else(|| failure("If temporary actual For", i))?;
        (
            UnitDropTarget::Temporary(e(i.conditional)),
            UnitDropPoint::LoopExit(UnitStatementId::new(unit, statement)),
            span(p, i.conditional),
        )
    } else {
        let local = local.ok_or_else(|| failure("If actual named owner", i))?;
        let (call, _) = i
            .observes
            .last()
            .ok_or_else(|| failure("If actual final observe", i))?;
        (
            UnitDropTarget::Named(local),
            UnitDropPoint::CallReturn(e(*call)),
            i.local.unwrap(),
        )
    };
    need(
        drop.target() == target && drop.point() == point && drop.value_origin() == origin,
        "If exact original owner Drop target/final public point/value origin",
        drop,
    )?;
    if c.provider {
        provider::check(p, i, n, t, o, drop)?;
    }
    Ok(Proof { drop })
}
