//! 同次 single 分析的公开 temporary exits facts；复杂清理拓扑首轮完整记录，不预填。
use super::{
    super::Report,
    ast::{Anchors, Input},
};
use lang_frontend::{
    ast::ExpressionId,
    name_resolution::{NameResolution, ReferenceTarget, SymbolId, SymbolKind},
    ownership_checking::{
        DropTarget, IterationSource, LoanKind, LoanTarget, MapIterationComponent,
        MapIterationSourceAccess, OwnershipBindingKind, OwnershipCheckedFile,
    },
    source::Span,
    type_checking::{
        BuiltinType, ExpressionCategory, IntrinsicTypeConstructor, MapIterationKind, TypeKind,
        TypedFile,
    },
};

pub(super) fn symbol(names: &NameResolution, span: Span) -> Option<SymbolId> {
    let found = names
        .symbols()
        .iter()
        .filter(|s| s.span() == span && !s.is_synthetic())
        .collect::<Vec<_>>();
    match found.as_slice() {
        [s] => Some(s.id()),
        _ => None,
    }
}

pub(super) fn reference(
    f: &Input,
    names: &NameResolution,
    expression: ExpressionId,
) -> Option<SymbolId> {
    let (_, span) = f.expression(expression).ok()?;
    let found = names
        .references()
        .iter()
        .filter(|r| r.span() == span)
        .collect::<Vec<_>>();
    match found.as_slice() {
        [r] => match r.target() {
            ReferenceTarget::Symbol(id) => Some(*id),
            _ => None,
        },
        _ => None,
    }
}

pub(super) fn observe(
    f: &Input,
    anchors: Option<&Anchors>,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
    report: &mut Report,
) {
    // 在可选 AST/query 之前保全真实产物；每个 plan Debug 包含完整 exits 与 owner/phi 记录。
    // 先保全实际公开产物，再做任何可失败窄查询；不预言 owner/phi/cleanup 形状。
    println!(
        "SPEC0331 TEMPORARY EXITS SINGLE REAL source={:?} typed_maps={:#?} maps={:#?} drops={:#?} cleanup={:#?} conditions_with_owners={:#?} loans={:#?} loan_ends={:#?} formed_owned_locals={:#?}",
        typed.source_id(),
        typed.map_iterations(),
        owned.map_iterations(),
        owned.drops(),
        owned.cleanup_steps(),
        owned.cleanup_conditions(),
        owned.loans(),
        owned.loan_ends(),
        owned.formed_owned_locals()
    );
    let mut points = Vec::new();
    for plan in owned.map_iterations() {
        println!("SPEC0331 TEMPORARY EXITS SINGLE PLAN {plan:#?}");
        for exit in plan.exits() {
            if !points.contains(&exit.point()) {
                points.push(exit.point());
            }
        }
    }
    for point in points {
        let plans = owned
            .map_iterations()
            .iter()
            .filter_map(|plan| {
                plan.exits()
                    .iter()
                    .any(|exit| exit.point() == point)
                    .then_some(plan.descriptor().statement())
            })
            .collect::<Vec<_>>();
        let whole = owned
            .cleanup_steps()
            .iter()
            .filter(|(at, _)| *at == point)
            .collect::<Vec<_>>();
        println!(
            "SPEC0331 TEMPORARY EXITS SINGLE WHOLE POINT point={point:?} plans={plans:?} sequence={whole:#?}"
        );
    }
    let Some(a) = anchors else { return };
    report.check(
        "single temporary exits exact two typed/owned plans",
        typed.map_iterations().len() == 2 && owned.map_iterations().len() == 2,
        (typed.map_iterations(), owned.map_iterations()),
    );

    let mut owners = Vec::new();
    let mut bindings = Vec::new();
    for site in &a.sites {
        report.check(
            "single temporary exits actual source-qualified AST site",
            site.span.source_id() == f.parsed.source_id()
                && site.source_span.source_id() == f.parsed.source_id()
                && f.sources.source_name(site.span.source_id()) == Ok("provider.ko"),
            (
                site,
                site.body,
                super::super::fixture::location(&f.sources, site.source_span),
            ),
        );
        let drops = owned
            .drops()
            .iter()
            .filter(|drop| matches!(drop.target(), DropTarget::Temporary(id) if id == site.source))
            .collect::<Vec<_>>();
        println!(
            "SPEC0331 TEMPORARY EXITS SINGLE SOURCE statement={:?} source={:?} transfer={:?} drops={drops:#?}",
            site.statement, site.source, site.transfer
        );
        for drop in drops {
            println!(
                "SPEC0331 TEMPORARY EXITS SINGLE SOURCE DROP drop={drop:#?} owner={:#?} condition={:#?}",
                drop.owner()
                    .and_then(|id| owned.cleanup_conditions().owner_value(id)),
                drop.condition()
                    .and_then(|id| owned.cleanup_conditions().get(id))
            );
        }
        let (Some(d), Some(p)) = (
            typed.map_iteration(site.statement),
            owned.map_iteration(site.statement),
        ) else {
            report.check(
                "single temporary exits actual site descriptor/plan",
                false,
                (site, typed.map_iterations(), owned.map_iterations()),
            );
            continue;
        };
        let constructor = if f.map {
            IntrinsicTypeConstructor::Map
        } else {
            IntrinsicTypeConstructor::MutableMap
        };
        let kind = if f.map {
            MapIterationKind::Map
        } else {
            MapIterationKind::MutableMap
        };
        report.check(
            "single temporary exits actual For/source and intrinsic String K/V",
            d.statement() == site.statement && d.source() == site.source && p.descriptor() == d
                && typed.expression_type(site.source) == Some(d.source_type())
                && typed.sequential_iteration(site.statement).is_none() && owned.iteration(site.statement).is_none()
                && matches!(typed.types().get(d.source_type()), Some(TypeKind::Intrinsic { constructor: actual, arguments })
                    if *actual == constructor && d.kind() == kind && arguments.as_slice() == [d.key_type(), d.value_type()])
                && [d.key_type(), d.value_type()].iter().all(|t| typed.types().get(*t) == Some(&TypeKind::Builtin(BuiltinType::String))),
            (site.statement, d, p),
        );
        let owner = match p.source() {
            Some(LoanTarget::Temporary(id)) => Some(*id),
            _ => None,
        };
        report.check(
            "single temporary exits public source category and expression origin",
            typed.expression_category(site.source) == Some(ExpressionCategory::Temporary)
                && p.source_access() == MapIterationSourceAccess::Temporary
                && matches!(p.source_kind(), IterationSource::BorrowedSource(LoanTarget::Temporary(id)) if *id == site.source)
                && owner == Some(site.source),
            (site.statement, site.source, p.source_access(), p.source_kind()),
        );
        if let Some(owner) = owner {
            owners.push(owner);
        }
        bindings.push(d.bindings());
        report.check(
            "single temporary exits distinct Key/Value binding and loan identities",
            d.key_binding() != d.value_binding() && p.loans()[0] != p.loans()[1],
            (site.statement, d.bindings(), p.loans()),
        );
        for (i, component) in [MapIterationComponent::Key, MapIterationComponent::Value]
            .into_iter()
            .enumerate()
        {
            let binding = d.bindings()[i];
            let actual = names.symbols().iter().find(|s| s.id() == binding);
            let loan = p.loan(component);
            report.check(
                "single temporary exits actual component marker/type/Shared lineage",
                actual.is_some_and(|s| {
                    s.span() == site.bindings[i]
                        && s.kind() == SymbolKind::ForBinding
                        && !s.is_synthetic()
                }) && typed.symbol_type(binding) == Some([d.key_type(), d.value_type()][i])
                    && owned.binding_kind(binding) == Some(OwnershipBindingKind::Shared)
                    && p.bindings()[i].symbol() == binding
                    && p.bindings()[i].kind() == OwnershipBindingKind::Shared
                    && loan == &p.loans()[i]
                    && loan.statement() == site.statement
                    && loan.source() == site.source
                    && loan.binding() == binding
                    && loan.component() == component
                    && loan.kind() == LoanKind::Shared
                    && Some(loan.origin()) == p.source(),
                (site.statement, actual, loan),
            );
        }
    }
    report.check(
        "single temporary exits two sites keep independent origins and all four bindings",
        matches!(owners.as_slice(), [first, second] if first != second)
            && matches!(bindings.as_slice(), [first, second] if first.iter().all(|binding| !second.contains(binding))),
        (owners, bindings),
    );
    super::calls_single::observe(f, a, names, typed, owned, report);
    super::guards::single(f, a, names, typed, owned, report);
}
