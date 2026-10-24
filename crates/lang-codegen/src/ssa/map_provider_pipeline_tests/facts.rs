//! 只读实际 For/source/K/V/双 Shared 绑定与来源模式；不构造 facts，不决定 lower 结果。
use super::{
    Report,
    fixture::{FOR_TEXT, Fixture, LoopSite, loop_site},
};
use lang_frontend::{
    name_resolution::{SymbolKind, UnitReferenceTarget, ValidatedCompilationUnitNames},
    ownership_checking::{
        CompilationUnitOwnership, IterationSource, LoanKind, MapIterationComponent,
        OwnershipBindingKind, UnitDropTarget, UnitIterationSourceAccess, UnitLoanTarget,
    },
    type_checking::{
        BuiltinType, CompilationUnitTypes, IntrinsicTypeConstructor, MapIterationKind,
        ParameterMode, UnitExpressionId, UnitStatementId, UnitTypeKind,
    },
};

fn expected(
    fixture: &Fixture,
) -> (
    IntrinsicTypeConstructor,
    MapIterationKind,
    OwnershipBindingKind,
) {
    if fixture.map {
        (
            IntrinsicTypeConstructor::Map,
            MapIterationKind::Map,
            OwnershipBindingKind::Owned,
        )
    } else {
        (
            IntrinsicTypeConstructor::MutableMap,
            MapIterationKind::MutableMap,
            OwnershipBindingKind::Shared,
        )
    }
}

fn site(fixture: &Fixture, report: &mut Report) -> Option<LoopSite> {
    let site = loop_site(fixture);
    report.query("exactly one actual For with direct Name source", &site);
    let site = site.ok()?;
    report.check(
        "actual For/source/binding source text and path",
        fixture.sources.slice(site.span) == Ok(FOR_TEXT)
            && fixture.sources.slice(site.source_span) == Ok("source")
            && fixture.sources.slice(site.bindings[0]) == Ok("key")
            && fixture.sources.slice(site.bindings[1]) == Ok("value")
            && fixture.sources.source_name(site.span.source_id()) == Ok(fixture.path)
            && site.span.source_id() == fixture.parsed.source_id(),
        &site,
    );
    Some(site)
}

pub(super) fn unit(
    fixture: &Fixture,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    report: &mut Report,
) {
    let Some(site) = site(fixture, report) else {
        return;
    };
    let [source_names] = names.names().source_units() else {
        report.check(
            "unit exactly one real source",
            false,
            names.names().source_units(),
        );
        return;
    };
    let source_unit = source_names.source_unit();
    report.check(
        "unit source names belong to original parsed file",
        source_names.resolution().source_id() == fixture.parsed.source_id(),
        (source_unit, fixture.parsed.source_id()),
    );
    report.check(
        "unit exact typed/owned Map counts",
        typed.map_iterations().len() == 1 && owned.map_iterations().len() == 1,
        (typed.map_iterations(), owned.map_iterations()),
    );
    let statement = UnitStatementId::new(source_unit, site.statement);
    let source_expression = UnitExpressionId::new(source_unit, site.source);
    let descriptor = typed.map_iteration(statement);
    let plan = owned.map_iteration(statement);
    let (Some(descriptor), Some(plan)) = (descriptor, plan) else {
        report.check("unit real Map facts exist", false, (descriptor, plan));
        return;
    };
    report.check(
        "unit actual source-qualified For/source and same typed descriptor",
        descriptor.statement() == statement
            && descriptor.source() == source_expression
            && Some(descriptor.source_type()) == typed.expression_type(source_expression)
            && plan.descriptor() == descriptor
            && typed.sequential_iteration(statement).is_none()
            && owned.iteration(statement).is_none(),
        (descriptor, plan.descriptor()),
    );
    let (constructor, kind, binding_kind) = expected(fixture);
    let source_type = typed.types().get(descriptor.source_type());
    report.check(
        "unit real intrinsic Map K/V both String",
        matches!(source_type,
            Some(UnitTypeKind::Intrinsic { constructor: actual, arguments })
            if *actual == constructor && descriptor.kind() == kind
                && arguments.as_slice() == [descriptor.key_type(), descriptor.value_type()]
                && typed.types().get(descriptor.key_type()) == Some(&UnitTypeKind::Builtin(BuiltinType::String))
                && typed.types().get(descriptor.value_type()) == Some(&UnitTypeKind::Builtin(BuiltinType::String))),
        source_type,
    );
    let references = names
        .names()
        .references()
        .iter()
        .filter_map(|reference| {
            if reference.span() == site.source_span
                && let UnitReferenceTarget::Symbol(symbol) = reference.target()
            {
                Some(*symbol)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let source = if let [source] = references.as_slice() {
        Some(*source)
    } else {
        None
    };
    let source_symbol = source.and_then(|id| {
        source_names
            .resolution()
            .symbols()
            .iter()
            .find(|symbol| symbol.id() == id.symbol())
    });
    let access = if fixture.map {
        UnitIterationSourceAccess::Owned
    } else {
        UnitIterationSourceAccess::Shared
    };
    report.check(
        "unit real named parameter source and original capability",
        matches!(plan.source(), Some(UnitLoanTarget::Place(place)) if Some(place.root()) == source && place.is_root())
            && matches!(plan.source_kind(), IterationSource::BorrowedSource(_))
            && plan.source_access() == access
            && source.is_some_and(|source| source.source_unit() == source_unit && owned.binding_kind(source) == Some(binding_kind))
            && source_symbol.is_some_and(|symbol| {
                symbol.kind() == SymbolKind::ValueParameter && !symbol.is_synthetic()
                    && symbol.name() == "source" && symbol.span().source_id() == fixture.parsed.source_id()
            }),
        (references, source_symbol, plan.source_kind(), plan.source_access()),
    );
    let mode = if fixture.map {
        ParameterMode::Value
    } else {
        ParameterMode::Borrow
    };
    for name in ["scan", "read"] {
        let callables = typed
            .signatures()
            .declarations()
            .iter()
            .filter_map(|signature| {
                let symbol = signature.symbol();
                let matches = symbol.source_unit() == source_unit
                    && source_names.resolution().symbols().iter().any(|local| {
                        local.id() == symbol.symbol()
                            && local.name() == name
                            && local.kind() == SymbolKind::Function
                            && !local.is_synthetic()
                            && local.span().source_id() == fixture.parsed.source_id()
                    });
                if matches { signature.callable() } else { None }
            })
            .collect::<Vec<_>>();
        let passed = matches!(callables.as_slice(), [callable] if {
            if name == "scan" {
                source.is_some() && matches!(callable.parameters(), [parameter]
                    if parameter.symbol() == source && parameter.mode() == mode
                        && parameter.ty() == descriptor.source_type())
            } else {
                callable.parameters().len() == 2 && callable.parameters().iter().all(|parameter| {
                    parameter.mode() == ParameterMode::Borrow
                        && typed.types().get(parameter.ty()) == Some(&UnitTypeKind::Builtin(BuiltinType::String))
                        && parameter.symbol().is_some_and(|symbol| {
                            symbol.source_unit() == source_unit && owned.binding_kind(symbol) == Some(OwnershipBindingKind::Shared)
                        })
                })
            }
        });
        report.check(
            "unit actual scan/read callable parameter modes",
            passed,
            (name, callables),
        );
    }
    report.check(
        "unit distinct K/V symbols and loans",
        descriptor.key_binding() != descriptor.value_binding()
            && plan.loans()[0] != plan.loans()[1],
        plan.loans(),
    );
    for (index, component) in [MapIterationComponent::Key, MapIterationComponent::Value]
        .into_iter()
        .enumerate()
    {
        let symbols = source_names
            .resolution()
            .symbols()
            .iter()
            .filter(|symbol| {
                symbol.span() == site.bindings[index]
                    && !symbol.is_synthetic()
                    && symbol.kind() == SymbolKind::ForBinding
            })
            .collect::<Vec<_>>();
        let binding = descriptor.bindings()[index];
        let component_type = [descriptor.key_type(), descriptor.value_type()][index];
        let loan = plan.loan(component);
        report.check(
            "unit actual component symbol/type/Shared loan lineage",
            binding.source_unit() == source_unit
                && matches!(symbols.as_slice(), [symbol] if symbol.id() == binding.symbol())
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
            (symbols, loan),
        );
    }
    if report.identity_requested {
        report.fe_identity = Some(super::query_inputs::unit(fixture, names, typed, owned));
    }
    // unit 只读取其真实公开 plan/drop；不伪造不存在的 closure phi 产品。
    let drops = owned
        .drops()
        .iter()
        .filter(
            |drop| matches!(drop.target(), UnitDropTarget::Named(symbol) if Some(symbol) == source),
        )
        .map(|drop| {
            (
                drop,
                super::fixture::location(&fixture.sources, drop.value_origin()),
            )
        })
        .collect::<Vec<_>>();
    println!(
        "SPEC0331 PROVIDER REAL UNIT OWNER source={source:?} plan={plan:#?} source_drops={drops:#?}"
    );
}
