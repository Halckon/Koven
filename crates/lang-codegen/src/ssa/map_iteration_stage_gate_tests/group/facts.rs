//! 全部四个真实 For 的只读身份合同；Group 集合排序独立于生产能力门。
use super::super::fixture::loop_sites;
use super::{
    Report, ast, cleanup,
    fixture::{Fixture, Location, Stopped, location, stopped},
};
use lang_frontend::{
    name_resolution::{
        SourceUnitKey, SymbolKind, UnitReferenceTarget, ValidatedCompilationUnitNames,
    },
    ownership_checking::{
        CompilationUnitOwnership, IterationSource, LoanKind, MapIterationComponent,
        OwnershipBindingKind, UnitIterationSourceAccess, UnitLoanTarget,
    },
    type_checking::{
        BuiltinType, CompilationUnitTypes, IntrinsicTypeConstructor, MapIterationKind,
        ParameterMode, UnitExpressionId, UnitStatementId, UnitTypeKind,
    },
};

pub(super) use super::ast::empty_entry;

fn kind(constructor: IntrinsicTypeConstructor) -> Option<MapIterationKind> {
    match constructor {
        IntrinsicTypeConstructor::Map => Some(MapIterationKind::Map),
        IntrinsicTypeConstructor::MutableMap => Some(MapIterationKind::MutableMap),
        _ => None,
    }
}

fn role_counts(roles: &[(bool, MapIterationKind)]) -> bool {
    roles.len() == 4
        && [false, true].into_iter().all(|grouped| {
            [MapIterationKind::Map, MapIterationKind::MutableMap]
                .into_iter()
                .all(|kind| {
                    roles
                        .iter()
                        .filter(|role| **role == (grouped, kind))
                        .count()
                        == 1
                })
        })
}

pub(super) fn unit(
    fixture: &Fixture,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    report: &mut Report,
) -> Result<Location, Stopped> {
    println!(
        "SPEC0331 GROUP UNIT FACTS typed={:#?} ownership={owned:#?}",
        typed.map_iterations()
    );
    report.check(
        "unit exact four typed/owned Map sites",
        typed.map_iterations().len() == 4 && owned.map_iterations().len() == 4,
        (typed.map_iterations(), owned.map_iterations()),
    );
    let memory = matches!(names.names().index().source_units(), [source]
        if source.key() == &SourceUnitKey::MemoryDocument);
    let mut all_sites = Vec::new();
    let mut groups = Vec::new();
    let mut roles = Vec::new();
    for source_names in names.names().source_units() {
        let source_unit = source_names.source_unit();
        let Some(file) = fixture
            .files
            .iter()
            .find(|file| file.parsed.source_id() == source_names.resolution().source_id())
        else {
            report.check(
                "unit source unit maps to actual parsed source",
                false,
                source_unit,
            );
            continue;
        };
        let sites = loop_sites(file);
        report.check("unit actual AST For query", sites.is_ok(), &sites);
        let Ok(sites) = sites else {
            continue;
        };
        report.check(
            "unit per-file loop count",
            sites.len()
                == if memory {
                    4
                } else {
                    usize::from(file.constructor.is_some())
                },
            (&file.path, &sites),
        );
        for site in sites {
            let statement = UnitStatementId::new(source_unit, site.statement);
            let source_expression = UnitExpressionId::new(source_unit, site.source);
            all_sites.push((statement, site.span));
            let shape = ast::source(fixture, file, &site);
            report.check(
                "unit actual Group or direct Name AST",
                shape.is_ok(),
                &shape,
            );
            let Ok(shape) = shape else {
                continue;
            };
            let inner = UnitExpressionId::new(source_unit, shape.name);
            let descriptor = typed.map_iteration(statement);
            let plan = owned.map_iteration(statement);
            let (Some(descriptor), Some(plan)) = (descriptor, plan) else {
                report.check(
                    "unit real Map descriptor/plan exist",
                    false,
                    (descriptor, plan),
                );
                continue;
            };
            roles.push((shape.grouped, descriptor.kind()));
            let identity = descriptor.statement() == statement
                && descriptor.source() == source_expression
                && Some(descriptor.source_type()) == typed.expression_type(source_expression)
                && typed.expression_type(inner) == typed.expression_type(source_expression)
                && plan.descriptor() == descriptor
                && typed.sequential_iteration(statement).is_none()
                && owned.iteration(statement).is_none();
            report.check(
                "unit qualified outer Group/inner Name descriptor identity",
                identity,
                (&shape, inner, descriptor, plan.descriptor()),
            );
            let source_type = typed.types().get(descriptor.source_type());
            report.check("unit intrinsic distinct Int/String K/V", matches!(source_type,
                Some(UnitTypeKind::Intrinsic { constructor, arguments })
                if kind(*constructor) == Some(descriptor.kind())
                    && file.constructor.is_none_or(|expected| expected == (*constructor, descriptor.kind()))
                    && arguments.as_slice() == [descriptor.key_type(), descriptor.value_type()]
                    && typed.types().get(descriptor.key_type()) == Some(&UnitTypeKind::Builtin(BuiltinType::Int))
                    && typed.types().get(descriptor.value_type()) == Some(&UnitTypeKind::Builtin(BuiltinType::String))), source_type);
            let references = names
                .names()
                .references()
                .iter()
                .filter_map(|reference| {
                    if reference.span() == shape.name_span
                        && let UnitReferenceTarget::Symbol(symbol) = reference.target()
                    {
                        Some(*symbol)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            let source = match references.as_slice() {
                [source] => Some(*source),
                _ => None,
            };
            let symbol = source.and_then(|id| {
                source_names
                    .resolution()
                    .symbols()
                    .iter()
                    .find(|s| s.id() == id.symbol())
            });
            let parameters = typed
                .signatures()
                .declarations()
                .iter()
                .filter_map(|s| s.callable())
                .flat_map(|c| c.parameters())
                .filter(|p| source.is_some() && p.symbol() == source)
                .collect::<Vec<_>>();
            let is_owned = !shape.grouped && descriptor.kind() == MapIterationKind::Map;
            let mode = if is_owned {
                ParameterMode::Value
            } else {
                ParameterMode::Borrow
            };
            let binding = if is_owned {
                OwnershipBindingKind::Owned
            } else {
                OwnershipBindingKind::Shared
            };
            let access = if is_owned {
                UnitIterationSourceAccess::Owned
            } else {
                UnitIterationSourceAccess::Shared
            };
            let parameter = symbol.is_some_and(|symbol| {
                symbol.kind() == SymbolKind::ValueParameter
                    && !symbol.is_synthetic()
                    && symbol.span().source_id() == file.parsed.source_id()
            }) && source.is_some_and(|source| {
                source.source_unit() == source_unit
                    && typed.symbol_type(source) == Some(descriptor.source_type())
                    && owned.binding_kind(source) == Some(binding)
            }) && matches!(parameters.as_slice(), [parameter] if parameter.mode() == mode
                    && parameter.ty() == descriptor.source_type());
            report.check(
                "unit inner Name unique qualified root/mode parameter",
                parameter,
                (&references, symbol, parameters, mode, binding),
            );
            let origin = matches!(plan.source(), Some(UnitLoanTarget::Place(place))
                    if Some(place.root()) == source && place.is_root())
                && matches!(plan.source_kind(), IterationSource::BorrowedSource(origin)
                    if Some(origin) == plan.source())
                && plan.source_access() == access;
            report.check(
                "unit actual qualified source-kind/root/access",
                origin,
                (plan.source(), plan.source_kind(), plan.source_access()),
            );
            report.check(
                "unit distinct component bindings/loans",
                descriptor.key_binding() != descriptor.value_binding()
                    && !source.is_some_and(|s| descriptor.bindings().contains(&s))
                    && plan.loans()[0] != plan.loans()[1],
                plan.loans(),
            );
            for (index, component) in [MapIterationComponent::Key, MapIterationComponent::Value]
                .into_iter()
                .enumerate()
            {
                let symbol = source_names.resolution().symbols().iter().find(|symbol| {
                    symbol.span() == site.bindings[index]
                        && !symbol.is_synthetic()
                        && symbol.kind() == SymbolKind::ForBinding
                });
                let binding = descriptor.bindings()[index];
                let component_type = [descriptor.key_type(), descriptor.value_type()][index];
                let loan = plan.loan(component);
                report.check(
                    "unit actual component qualified statement/source/binding/origin",
                    binding.source_unit() == source_unit
                        && symbol.is_some_and(|s| s.id() == binding.symbol())
                        && typed.symbol_type(binding) == Some(component_type)
                        && owned.binding_kind(binding) == Some(OwnershipBindingKind::Shared)
                        && plan.bindings()[index].symbol() == binding
                        && plan.bindings()[index].kind() == OwnershipBindingKind::Shared
                        && loan == &plan.loans()[index]
                        && loan.component() == component
                        && loan.statement() == statement
                        && loan.source() == source_expression
                        && loan.binding() == binding
                        && loan.kind() == LoanKind::Shared
                        && Some(loan.origin()) == plan.source(),
                    (symbol, loan),
                );
            }
            cleanup::unit(file, &site, plan, owned, is_owned, report);
            if shape.grouped {
                groups.push((descriptor.statement(), site.span));
                report.check(
                    "unit candidate Group source identity",
                    identity && parameter && origin,
                    (statement, source_expression, inner),
                );
            }
        }
    }
    report.check(
        "unit exact four actual AST sites and original source grouping",
        all_sites.len() == 4
            && all_sites.iter().enumerate().all(|(i, a)| {
                all_sites
                    .iter()
                    .skip(i + 1)
                    .all(|b| a.0 != b.0 && (a.0.source_unit() == b.0.source_unit()) == memory)
            }),
        &all_sites,
    );
    report.check(
        "unit one of each direct/Group Map kind",
        role_counts(&roles),
        &roles,
    );
    groups.sort_by_key(|(statement, _)| *statement);
    report.check(
        "unit two distinct Group sites sorted by actual UnitStatementId",
        groups.len() == 2 && groups[0].0 != groups[1].0 && groups[0].1 != groups[1].1,
        &groups,
    );
    let [(first, span), _] = groups.as_slice() else {
        return Err(stopped("unit exact Group oracle set", &groups));
    };
    let expected = location(&fixture.sources, *span)
        .ok_or_else(|| stopped("unit full Group For location", (first, span)))?;
    report.check(
        "unit first qualified Group full For/path/text",
        true,
        (first, &expected),
    );
    Ok(expected)
}
