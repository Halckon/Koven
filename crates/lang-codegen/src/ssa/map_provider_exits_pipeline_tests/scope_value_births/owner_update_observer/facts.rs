//! Read-only original-analysis facts; Unit never borrows Single cleanup tables.
mod deep;
mod provider_contract;
mod unit;
use super::super::super::{
    fixture::{Failure, failure, need},
    query_inputs::one,
};
use super::{
    Case,
    ast::{Input, inside, span},
};
pub(super) use deep::Proof;
use lang_frontend::{
    name_resolution::{NameResolution, ReferenceTarget, SymbolId},
    ownership_checking::{
        DropTarget, LoanEndPoint, LoanTarget, MapIterationSourceAccess, OwnershipCheckedFile,
    },
    parser::{Expression, ParsedFile},
    type_checking::{
        BuiltinType, CallableResultSource, CallableTarget, ExpressionCategory,
        IntrinsicTypeConstructor, ParameterMode, TypeKind, TypedFile,
    },
};
pub(super) use unit::check as unit;
fn local(n: &NameResolution, marker: lang_frontend::source::Span) -> Result<SymbolId, Failure> {
    one(
        n.symbols()
            .iter()
            .filter(|s| s.span() == marker && !s.is_synthetic())
            .map(|s| s.id()),
        "update actual original local symbol",
    )
}
fn reference(
    p: &ParsedFile,
    n: &NameResolution,
    e: lang_frontend::ast::ExpressionId,
) -> Result<SymbolId, Failure> {
    one(
        n.references()
            .iter()
            .filter(|r| r.span() == span(p, e))
            .filter_map(|r| match r.target() {
                ReferenceTarget::Symbol(s) => Some(*s),
                _ => None,
            }),
        "update actual original Name reference",
    )
}
pub(super) fn single(
    c: &Case,
    p: &ParsedFile,
    i: &Input,
    n: &NameResolution,
    t: &TypedFile,
    o: &OwnershipCheckedFile,
) -> Result<Proof, Failure> {
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
        "SCOPE C UPDATE FE label={} unit=false root={root:?} locals={locals:?} formed={:?} scan_drops={drops:#?} all_drops={:#?} conditions={:#?} loans={:#?} loan_ends={:#?} map_puts={:#?} string_effects={:#?}",
        c.label,
        o.formed_owned_locals(),
        o.drops(),
        o.cleanup_conditions(),
        o.loans(),
        o.loan_ends(),
        t.map_puts(),
        o.string_effects()
    );
    for d in &drops {
        println!(
            "SCOPE C UPDATE DROP label={} unit=false target={:?} point={:?} origin={:?} owner={:?} condition={:?} owner_value={:?} condition_node={:?}",
            c.label,
            d.target(),
            d.point(),
            d.value_origin(),
            d.owner(),
            d.condition(),
            d.owner()
                .and_then(|v| o.cleanup_conditions().owner_value(v)),
            d.condition().and_then(|v| o.cleanup_conditions().get(v))
        );
    }
    for b in &i.births {
        println!(
            "SCOPE C UPDATE BIRTH label={} unit=false expression={:?} span={:?} type={:?} category={:?} call={:#?}",
            c.label,
            b.expression,
            span(p, b.expression),
            t.expression_type(b.expression),
            t.expression_category(b.expression),
            t.call(b.expression)
        );
    }
    if let Some((statement, source)) = i.site {
        println!(
            "SCOPE C UPDATE PLAN label={} unit=false source={source:?} descriptor={:#?} plan={:#?}",
            c.label,
            t.map_iteration(statement),
            o.map_iteration(statement)
        );
    }
    if let Some((assignment, target, rhs)) = i.assignment {
        println!(
            "SCOPE C UPDATE ASSIGN label={} unit=false expression={assignment:?} target={target:?} rhs={rhs:?} target_type={:?} rhs_type={:?} rhs_category={:?} target_ref={:?} rhs_reference={:?}",
            c.label,
            t.expression_type(target),
            t.expression_type(rhs),
            t.expression_category(rhs),
            n.references()
                .iter()
                .filter(|r| r.span() == span(p, target))
                .collect::<Vec<_>>(),
            n.references()
                .iter()
                .filter(|r| r.span() == span(p, rhs))
                .collect::<Vec<_>>()
        );
    }
    for l in &i.locals {
        println!(
            "SCOPE C UPDATE LOCAL label={} unit=false marker={:?} initializer={:?} type={:?} category={:?} reference={:?}",
            c.label,
            l.marker,
            l.initializer,
            t.expression_type(l.initializer),
            t.expression_category(l.initializer),
            n.references()
                .iter()
                .filter(|r| r.span() == span(p, l.initializer))
                .collect::<Vec<_>>()
        );
    }
    let ty = t
        .symbol_type(root)
        .ok_or_else(|| failure("update actual source storage type", root))?;
    need(
        matches!(t.types().get(ty),Some(TypeKind::Intrinsic{constructor,arguments})if *constructor==if c.code==9{IntrinsicTypeConstructor::MutableMap}else{IntrinsicTypeConstructor::Map}&&arguments.len()==2)
            && locals.iter().all(|s| o.formed_owned_locals().contains(s)),
        "update real formed locals and concrete Map storage",
        (ty, &locals),
    )?;
    for b in &i.births {
        let call = t
            .call(b.expression)
            .ok_or_else(|| failure("update actual factory typed Call", b.expression))?;
        let target = local(n, b.name)?;
        need(
            call.target() == CallableTarget::Source(target)
                && call.result_source() == CallableResultSource::Owned
                && call.arguments().is_empty()
                && call.receiver().is_none()
                && t.expression_type(b.expression) == Some(ty)
                && t.expression_category(b.expression) == Some(ExpressionCategory::Temporary),
            "update exact distinct original factory Owned Calls",
            call,
        )?;
    }
    if let Some((_, target, rhs)) = i.assignment {
        need(
            reference(p, n, target)? == root
                && t.expression_type(target) == Some(ty)
                && t.expression_type(rhs) == Some(ty),
            "update actual assignment target/RHS storage",
            (target, rhs),
        )?;
        if c.code == 11 {
            let expected = local(n, i.locals[0].marker)?;
            need(
                reference(p, n, rhs)? == expected
                    && matches!(
                        p.ast().expressions().get(rhs).unwrap().payload(),
                        Expression::Name
                    ),
                "update future MoveName is the actual next binding",
                rhs,
            )?;
        }
    }
    if c.code == 10 {
        let source = i.locals.iter().find(|l| l.marker == i.source).unwrap();
        need(
            reference(p, n, source.initializer)? == local(n, i.locals[0].marker)?,
            "update source initializer moves actual old binding",
            source,
        )?;
    }
    if let Some((_, condition)) = i.conditional {
        let flag = local(n, i.flag.unwrap())?;
        need(
            reference(p, n, condition)? == flag
                && t.expression_type(condition).is_some_and(|ty| {
                    t.types().get(ty) == Some(&TypeKind::Builtin(BuiltinType::Boolean))
                }),
            "update original condition references its actual Boolean flag",
            condition,
        )?;
    }
    let clone_receivers = super::ast::clone_receivers(p, i)?;
    need(
        drops
            .iter()
            .any(|d| matches!(d.target(),DropTarget::Named(s)if locals.contains(&s)))
            && drops.iter().all(|d| match d.target() {
                DropTarget::Named(s) => locals.contains(&s),
                DropTarget::Temporary(e) => clone_receivers.contains(&e),
                _ => false,
            }),
        "update nonzero real owner epochs and exact clone receiver cleanup targets",
        &drops,
    )?;
    let observe_target = one(
        n.symbols()
            .iter()
            .filter(|s| {
                inside(i.observer, s.span())
                    && s.kind() == lang_frontend::name_resolution::SymbolKind::Function
            })
            .map(|s| s.id()),
        "update actual original observe symbol",
    )?;
    for (call, arg) in &i.observes {
        let typed = t
            .call(*call)
            .ok_or_else(|| failure("update actual observe Call", call))?;
        let loan = one(
            o.loans()
                .iter()
                .filter(|l| l.call() == *call && l.argument() == *arg),
            "update original observe loan",
        )?;
        let end = one(
            o.loan_ends()
                .iter()
                .filter(|l| l.call() == *call && l.argument() == *arg),
            "update independent observe CallReturn endpoint",
        )?;
        println!(
            "SCOPE C UPDATE OBSERVE label={} unit=false call={typed:#?} loan={loan:#?} end={end:#?}",
            c.label
        );
        need(
            typed.target() == CallableTarget::Source(observe_target)
                && reference(p, n, *arg)? == root
                && matches!(typed.arguments(),[a]if a.argument_index()==0&&a.parameter_index()==0&&a.mode()==ParameterMode::Borrow&&a.parameter_type()==ty)
                && matches!(loan.target(),LoanTarget::Place(path)if path.is_root()&&path.root()==root)
                && loan.begin_span() == span(p, *arg)
                && loan.end_span() == span(p, *call)
                && end.point() == LoanEndPoint::CallReturn(*call),
            "update actual current source observe target/contract/loan/end",
            (typed, loan, end),
        )?;
    }
    need(
        t.map_iterations().len() == usize::from(c.provider)
            && o.map_iterations().len() == usize::from(c.provider),
        "update exact real acquired For/control set",
        o.map_iterations(),
    )?;
    if let Some((statement, source)) = i.site {
        let plan = o
            .map_iteration(statement)
            .ok_or_else(|| failure("update original source provider plan", statement))?;
        need(
            t.map_iteration(statement) == Some(plan.descriptor())
                && plan.descriptor().source() == source
                && plan.descriptor().source_type() == ty
                && reference(p, n, source)? == root
                && matches!(plan.source(),Some(LoanTarget::Place(path))if path.is_root()&&path.root()==root)
                && plan.source_access() == MapIterationSourceAccess::Owned
                && plan.loans().len() == 2
                && !plan.exits().is_empty(),
            "update canonical original source/nonzero complete plan",
            plan,
        )?;
    }
    if let Some(put) = i.put {
        let d = t
            .map_put(put)
            .ok_or_else(|| failure("update actual conditional put descriptor", put))?;
        need(
            reference(p, n, d.receiver())? == root
                && [d.key(), d.value()]
                    .iter()
                    .all(|e| o.string_effect(*e).is_some()),
            "update actual put receiver and independent approved String clone owners",
            d,
        )?;
    }
    if c.provider {
        provider_contract::single(c, p, i, n, t, o)?;
    }
    deep::single(c, p, i, n, o, root)
}
