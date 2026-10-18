//! 同次 unit 分析逐完整 qualified For/source/binding 身份查询；不读取私有 acquired 表。
use super::{
    super::Report,
    ast::{Anchors, Function, Input},
};
use lang_frontend::{
    ast::ExpressionId,
    name_resolution::{
        DeclarationId, SourceUnitId, SymbolKind, UnitReferenceTarget, UnitSymbolId,
        ValidatedCompilationUnitNames,
    },
    ownership_checking::{
        CompilationUnitOwnership, IterationSource, LoanKind, MapIterationComponent,
        OwnershipBindingKind, UnitDropTarget, UnitIterationSourceAccess, UnitLoanTarget,
    },
    source::Span,
    type_checking::{
        BuiltinType, CompilationUnitTypes, ExpressionCategory, IntrinsicTypeConstructor,
        MapIterationKind, UnitExpressionId, UnitStatementId, UnitTypeKind,
    },
};

pub(super) fn declaration(
    names: &ValidatedCompilationUnitNames,
    function: &Function,
    unit: SourceUnitId,
) -> Option<DeclarationId> {
    let found = names
        .names()
        .index()
        .declarations()
        .iter()
        .filter(|d| {
            d.source_unit() == unit
                && d.root() == function.item
                && d.name_span() == function.name_span
                && d.kind() == SymbolKind::Function
                && d.name() == function.name
        })
        .collect::<Vec<_>>();
    match found.as_slice() {
        [d] => Some(d.id()),
        _ => None,
    }
}

pub(super) fn reference(
    f: &Input,
    names: &ValidatedCompilationUnitNames,
    expression: ExpressionId,
) -> Option<UnitSymbolId> {
    let (_, span) = f.expression(expression).ok()?;
    let found = names
        .names()
        .references()
        .iter()
        .filter(|r| r.span() == span)
        .collect::<Vec<_>>();
    match found.as_slice() {
        [r] => match r.target() {
            UnitReferenceTarget::Symbol(id) => Some(*id),
            _ => None,
        },
        _ => None,
    }
}

pub(super) fn symbol(
    names: &ValidatedCompilationUnitNames,
    unit: SourceUnitId,
    span: Span,
) -> Option<UnitSymbolId> {
    // UnitSymbolId 构造器是 frontend 私有；只取本次真实引用中的 qualified ID，再核声明 marker。
    let mut found = Vec::new();
    for reference in names.names().references() {
        let UnitReferenceTarget::Symbol(id) = reference.target() else {
            continue;
        };
        let actual = id.source_unit() == unit
            && names
                .names()
                .source_units()
                .iter()
                .filter(|source| source.source_unit() == unit)
                .any(|source| {
                    source.resolution().symbols().iter().any(|symbol| {
                        symbol.id() == id.symbol()
                            && symbol.span() == span
                            && !symbol.is_synthetic()
                    })
                });
        if actual && !found.contains(id) {
            found.push(*id);
        }
    }
    match found.as_slice() {
        [id] => Some(*id),
        _ => None,
    }
}

pub(super) fn binding_at(
    names: &ValidatedCompilationUnitNames,
    id: UnitSymbolId,
    span: Span,
    kind: SymbolKind,
) -> bool {
    names
        .names()
        .source_units()
        .iter()
        .filter(|s| s.source_unit() == id.source_unit())
        .any(|source| {
            source.resolution().symbols().iter().any(|s| {
                s.id() == id.symbol() && s.span() == span && s.kind() == kind && !s.is_synthetic()
            })
        })
}

pub(super) fn observe(
    f: &Input,
    anchors: Option<&Anchors>,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    report: &mut Report,
) {
    // 先保全实际公开产物，再做任何可失败窄查询；不预言 owner/phi/cleanup 形状。
    println!(
        "SPEC0331 TEMPORARY EXITS UNIT REAL typed_maps={:#?} maps={:#?} drops={:#?} conditional_drops={:#?} call_argument_contracts={:#?} loans={:#?} value_deliveries={:#?} formed_owned_locals={:#?}",
        typed.map_iterations(),
        owned.map_iterations(),
        owned.drops(),
        owned.conditional_receiver_drops(),
        owned.call_argument_contracts(),
        owned.loans(),
        owned.value_deliveries(),
        owned.formed_owned_locals()
    );
    let mut points = Vec::new();
    for plan in owned.map_iterations() {
        println!("SPEC0331 TEMPORARY EXITS UNIT PLAN {plan:#?}");
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
        let whole = owned.iteration_cleanup_at(point);
        println!(
            "SPEC0331 TEMPORARY EXITS UNIT WHOLE POINT point={point:?} plans={plans:?} sequence={whole:#?}"
        );
    }
    let Some(a) = anchors else { return };
    let [source_names] = names.names().source_units() else {
        report.check(
            "unit temporary exits exactly one actual source",
            false,
            names.names().source_units(),
        );
        return;
    };
    let unit = source_names.source_unit();
    let expression = |id| UnitExpressionId::new(unit, id);
    report.check(
        "unit temporary exits original source and exact two typed/owned plans",
        source_names.resolution().source_id() == f.parsed.source_id()
            && typed.map_iterations().len() == 2
            && owned.map_iterations().len() == 2,
        (unit, typed.map_iterations(), owned.map_iterations()),
    );

    let mut owners = Vec::new();
    let mut bindings = Vec::new();
    for site in &a.sites {
        let statement = UnitStatementId::new(unit, site.statement);
        let source = expression(site.source);
        report.check(
            "unit temporary exits actual qualified AST site",
            site.span.source_id() == f.parsed.source_id()
                && site.source_span.source_id() == f.parsed.source_id()
                && f.sources.source_name(site.span.source_id()) == Ok("provider.ko"),
            (
                statement,
                source,
                site.body,
                super::super::fixture::location(&f.sources, site.source_span),
            ),
        );
        let drops = owned
            .drops()
            .iter()
            .filter(|drop| matches!(drop.target(), UnitDropTarget::Temporary(id) if id == source))
            .collect::<Vec<_>>();
        println!(
            "SPEC0331 TEMPORARY EXITS UNIT SOURCE statement={statement:?} source={source:?} transfer={:?} drops={drops:#?}",
            expression(site.transfer)
        );
        let (Some(d), Some(p)) = (
            typed.map_iteration(statement),
            owned.map_iteration(statement),
        ) else {
            report.check(
                "unit temporary exits actual qualified site descriptor/plan",
                false,
                (
                    statement,
                    source,
                    typed.map_iterations(),
                    owned.map_iterations(),
                ),
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
            "unit temporary exits exact qualified For/source and intrinsic String K/V",
            d.statement() == statement && d.source() == source && p.descriptor() == d
                && typed.expression_type(source) == Some(d.source_type())
                && typed.sequential_iteration(statement).is_none() && owned.iteration(statement).is_none()
                && matches!(typed.types().get(d.source_type()), Some(UnitTypeKind::Intrinsic { constructor: actual, arguments })
                    if *actual == constructor && d.kind() == kind && arguments.as_slice() == [d.key_type(), d.value_type()])
                && [d.key_type(), d.value_type()].iter().all(|t| typed.types().get(*t) == Some(&UnitTypeKind::Builtin(BuiltinType::String))),
            (statement, source, d, p),
        );
        let owner = match p.source() {
            Some(UnitLoanTarget::Temporary(id)) => Some(*id),
            _ => None,
        };
        report.check(
            "unit temporary exits public category and qualified expression origin",
            typed.expression_category(source) == Some(ExpressionCategory::Temporary)
                && p.source_access() == UnitIterationSourceAccess::Temporary
                && matches!(p.source_kind(), IterationSource::BorrowedSource(UnitLoanTarget::Temporary(id)) if *id == source)
                && owner == Some(source),
            (statement, source, p.source_access(), p.source_kind()),
        );
        if let Some(owner) = owner {
            owners.push(owner);
        }
        bindings.push(d.bindings());
        report.check(
            "unit temporary exits distinct qualified Key/Value bindings and loans",
            d.key_binding() != d.value_binding() && p.loans()[0] != p.loans()[1],
            (statement, d.bindings(), p.loans()),
        );
        for (i, component) in [MapIterationComponent::Key, MapIterationComponent::Value]
            .into_iter()
            .enumerate()
        {
            let binding = d.bindings()[i];
            let loan = p.loan(component);
            report.check(
                "unit temporary exits actual marker/type/Shared component lineage",
                binding.source_unit() == unit
                    && binding_at(names, binding, site.bindings[i], SymbolKind::ForBinding)
                    && typed.symbol_type(binding) == Some([d.key_type(), d.value_type()][i])
                    && owned.binding_kind(binding) == Some(OwnershipBindingKind::Shared)
                    && p.bindings()[i].symbol() == binding
                    && p.bindings()[i].kind() == OwnershipBindingKind::Shared
                    && loan == &p.loans()[i]
                    && loan.statement() == statement
                    && loan.source() == source
                    && loan.binding() == binding
                    && loan.component() == component
                    && loan.kind() == LoanKind::Shared
                    && Some(loan.origin()) == p.source(),
                (statement, binding, loan),
            );
        }
    }
    report.check(
        "unit temporary exits two qualified sites retain independent origins and bindings",
        matches!(owners.as_slice(), [first, second] if first != second && first.source_unit() == unit && second.source_unit() == unit)
            && matches!(bindings.as_slice(), [first, second] if first.iter().all(|binding| !second.contains(binding))),
        (owners, bindings),
    );
    super::calls_unit::observe(f, a, names, typed, owned, report);
    super::guards::unit(f, a, names, typed, owned, report);
}
