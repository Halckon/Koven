//! Same-analysis source selection, represented component types and independent public cleanup.
use super::super::{
    fixture::{Failure, failure, need},
    query_inputs::one,
    query_temporary_cleanup as cleanup,
};
use super::{Case, ast};
use lang_frontend::{
    name_resolution::{
        NameResolution, ReferenceTarget, SymbolId, UnitReferenceTarget,
        ValidatedCompilationUnitNames,
    },
    ownership_checking::{
        CompilationUnitOwnership, IterationSource, LoanKind, LoanTarget, MapIterationComponent,
        MapIterationSourceAccess, OwnershipBindingKind, OwnershipCheckedFile,
        UnitIterationSourceAccess, UnitLoanTarget,
    },
    parser::ParsedFile,
    source::{SourceMap, Span},
    type_checking::{
        BuiltinType, CallableResultSource, CallableTarget, CompilationUnitTypes,
        ExpressionCategory, IntrinsicTypeConstructor, MapIterationKind, ParameterMode, TypeId,
        TypeKind, TypedFile, UnitCallTarget, UnitExpressionId, UnitStatementId, UnitTypeId,
        UnitTypeKind,
    },
};

fn single_symbol(names: &NameResolution, marker: Span) -> Result<SymbolId, Failure> {
    Ok(one(
        names
            .symbols()
            .iter()
            .filter(|symbol| symbol.span() == marker && !symbol.is_synthetic()),
        "temporary forms actual source symbol",
    )?
    .id())
}

fn single_value(
    case: &Case,
    sources: &SourceMap,
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    key: TypeId,
    value: TypeId,
) -> Result<(), Failure> {
    let expected = if matches!(case.representation, 2 | 4) {
        BuiltinType::Int
    } else {
        BuiltinType::String
    };
    need(
        typed.types().get(key) == Some(&TypeKind::Builtin(expected)),
        "temporary forms exact key type",
        key,
    )?;
    if case.representation == 4 {
        return need(
            typed.types().get(value) == Some(&TypeKind::Builtin(BuiltinType::String)),
            "temporary forms Pure String value",
            value,
        );
    }
    let value = if case.representation == 3 {
        let Some(TypeKind::Nullable(inner)) = typed.types().get(value) else {
            return Err(failure("temporary forms actual nullable value", value));
        };
        *inner
    } else {
        value
    };
    let name = if case.representation == 1 {
        "Payload"
    } else {
        "Resource"
    };
    need(
        matches!(typed.types().get(value), Some(TypeKind::Nominal {nominal, arguments})
        if arguments.is_empty() && names.symbols().get(nominal.symbol().index()).is_some_and(|symbol|
            symbol.span() == ast::nominal(sources, parsed, name).unwrap().1)),
        "temporary forms actual represented nominal value",
        value,
    )
}

pub(super) fn single(
    case: &Case,
    sources: &SourceMap,
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
) -> Result<(), Failure> {
    let input = ast::input(sources, parsed)?;
    need(
        typed.map_iterations().len() == 2
            && owned.map_iterations().len() == 2
            && owned.is_compatible_with(names, typed),
        "temporary forms two same-analysis single plans",
        typed.map_iterations(),
    )?;
    let mut owners = Vec::new();
    let mut bindings = Vec::new();
    for site in &input.sites {
        let d = typed
            .map_iteration(site.statement)
            .ok_or_else(|| failure("temporary forms actual typed For", site))?;
        let p = owned
            .map_iteration(site.statement)
            .ok_or_else(|| failure("temporary forms actual owned For", site))?;
        let factory = typed
            .call(site.source)
            .ok_or_else(|| failure("temporary forms original typed source Call", site))?;
        let constructor = if case.mutable {
            IntrinsicTypeConstructor::MutableMap
        } else {
            IntrinsicTypeConstructor::Map
        };
        need(
            p.descriptor() == d
                && d.source() == site.source
                && d.kind()
                    == if case.mutable {
                        MapIterationKind::MutableMap
                    } else {
                        MapIterationKind::Map
                    }
                && typed.expression_type(site.source) == Some(d.source_type())
                && typed.expression_category(site.source) == Some(ExpressionCategory::Temporary)
                && typed.is_resource_type(d.source_type()) == Some(case.representation != 4)
                && matches!(typed.types().get(d.source_type()), Some(TypeKind::Intrinsic {constructor:c, arguments}) if *c == constructor && arguments.as_slice() == [d.key_type(), d.value_type()])
                && p.source_access() == MapIterationSourceAccess::Temporary
                && matches!(p.source_kind(), IterationSource::BorrowedSource(LoanTarget::Temporary(source)) if *source == site.source),
            "temporary forms actual source type/category/classification/temporary provenance",
            p,
        )?;
        single_value(
            case,
            sources,
            parsed,
            names,
            typed,
            d.key_type(),
            d.value_type(),
        )?;
        need(
            factory.target() == CallableTarget::Source(single_symbol(names, site.factory.name)?)
                && factory.expression() == site.source
                && factory.receiver().is_none()
                && factory.arguments().is_empty()
                && factory.return_type() == d.source_type()
                && factory.result_source() == CallableResultSource::Owned,
            "temporary forms exact selected zeroarg factory and Owned result",
            factory,
        )?;
        let read = typed
            .call(site.read)
            .ok_or_else(|| failure("temporary forms actual reader Call", site))?;
        need(
            read.target() == CallableTarget::Source(single_symbol(names, input.reader.name)?)
                && read.arguments().len() == 2
                && read.receiver().is_none(),
            "temporary forms actual reader target/arity",
            read,
        )?;
        for (index, component) in [MapIterationComponent::Key, MapIterationComponent::Value]
            .into_iter()
            .enumerate()
        {
            let binding = d.bindings()[index];
            let loan = p.loan(component);
            let argument = read.arguments()[index];
            let reference = one(
                names
                    .references()
                    .iter()
                    .filter(|r| r.span() == ast::expression(parsed, site.arguments[index])),
                "temporary forms original reader argument root",
            )?;
            need(
                binding == single_symbol(names, site.bindings[index])?
                    && typed.symbol_type(binding) == Some([d.key_type(), d.value_type()][index])
                    && owned.binding_kind(binding) == Some(OwnershipBindingKind::Shared)
                    && p.bindings()[index].symbol() == binding
                    && p.bindings()[index].kind() == OwnershipBindingKind::Shared
                    && loan.statement() == site.statement
                    && loan.source() == site.source
                    && loan.binding() == binding
                    && loan.component() == component
                    && loan.kind() == LoanKind::Shared
                    && loan.origin() == &LoanTarget::Temporary(site.source)
                    && argument.argument_index() == index
                    && argument.parameter_index() == index
                    && argument.mode() == ParameterMode::Borrow
                    && argument.parameter_type() == [d.key_type(), d.value_type()][index]
                    && matches!(reference.target(), ReferenceTarget::Symbol(root) if *root == binding),
                "temporary forms exact Shared component and real ordered Borrow argument",
                (loan, argument, reference),
            )?;
            bindings.push(binding);
        }
        owners.push(cleanup::single(
            site.statement,
            site.source,
            site.body,
            site.call,
            owned,
        )?);
    }
    need(
        owners[0] != owners[1]
            && bindings
                .iter()
                .enumerate()
                .all(|(index, value)| !bindings[..index].contains(value)),
        "temporary forms disjoint owners and all four binding identities",
        (owners, bindings),
    )
}

fn unit_value(
    case: &Case,
    sources: &SourceMap,
    parsed: &ParsedFile,
    typed: &CompilationUnitTypes,
    names: &ValidatedCompilationUnitNames,
    key: UnitTypeId,
    value: UnitTypeId,
) -> Result<(), Failure> {
    let expected = if matches!(case.representation, 2 | 4) {
        BuiltinType::Int
    } else {
        BuiltinType::String
    };
    need(
        typed.types().get(key) == Some(&UnitTypeKind::Builtin(expected)),
        "temporary forms exact qualified key",
        key,
    )?;
    if case.representation == 4 {
        return need(
            typed.types().get(value) == Some(&UnitTypeKind::Builtin(BuiltinType::String)),
            "temporary forms qualified Pure String",
            value,
        );
    }
    let value = if case.representation == 3 {
        let Some(UnitTypeKind::Nullable(inner)) = typed.types().get(value) else {
            return Err(failure("temporary forms actual qualified nullable", value));
        };
        *inner
    } else {
        value
    };
    let wanted = ast::nominal(
        sources,
        parsed,
        if case.representation == 1 {
            "Payload"
        } else {
            "Resource"
        },
    )?
    .1;
    need(
        matches!(typed.types().get(value), Some(UnitTypeKind::Nominal {declaration, arguments})
        if arguments.is_empty() && typed.signatures().declaration(*declaration).and_then(|d| d.nominal()).is_some_and(|nominal|
            names.names().source_units().iter().any(|unit| unit.source_unit() == nominal.symbol().source_unit()
                && unit.resolution().symbols().get(nominal.symbol().symbol().index()).is_some_and(|symbol| symbol.span() == wanted)))),
        "temporary forms original qualified represented nominal",
        value,
    )
}

pub(super) fn unit(
    case: &Case,
    sources: &SourceMap,
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
) -> Result<(), Failure> {
    let input = ast::input(sources, parsed)?;
    let resolution = one(
        names.names().source_units().iter(),
        "temporary forms original source unit",
    )?;
    let unit = resolution.source_unit();
    let expression = |id| UnitExpressionId::new(unit, id);
    need(
        typed.map_iterations().len() == 2 && owned.map_iterations().len() == 2,
        "temporary forms two actual qualified plans",
        typed.map_iterations(),
    )?;
    let target = |function: &ast::Function| {
        one(
            names
                .names()
                .index()
                .declarations()
                .iter()
                .filter(|d| {
                    d.source_unit() == unit
                        && d.root() == function.item
                        && d.name_span() == function.name
                })
                .map(|d| d.id()),
            "temporary forms original qualified function declaration",
        )
    };
    let mut bindings = Vec::new();
    for site in &input.sites {
        let statement = UnitStatementId::new(unit, site.statement);
        let source = expression(site.source);
        let d = typed
            .map_iteration(statement)
            .ok_or_else(|| failure("temporary forms actual qualified typed For", site))?;
        let p = owned
            .map_iteration(statement)
            .ok_or_else(|| failure("temporary forms actual qualified owned For", site))?;
        let factory = typed
            .call(source)
            .ok_or_else(|| failure("temporary forms actual qualified factory Call", source))?;
        let constructor = if case.mutable {
            IntrinsicTypeConstructor::MutableMap
        } else {
            IntrinsicTypeConstructor::Map
        };
        need(
            p.descriptor() == d
                && d.source() == source
                && d.kind()
                    == if case.mutable {
                        MapIterationKind::MutableMap
                    } else {
                        MapIterationKind::Map
                    }
                && typed.expression_type(source) == Some(d.source_type())
                && typed.expression_category(source) == Some(ExpressionCategory::Temporary)
                && typed.is_resource_type(d.source_type()) == Some(case.representation != 4)
                && matches!(typed.types().get(d.source_type()), Some(UnitTypeKind::Intrinsic {constructor:c, arguments}) if *c == constructor && arguments.as_slice() == [d.key_type(), d.value_type()])
                && p.source_access() == UnitIterationSourceAccess::Temporary
                && matches!(p.source_kind(), IterationSource::BorrowedSource(UnitLoanTarget::Temporary(id)) if *id == source),
            "temporary forms original qualified temporary and represented classification",
            p,
        )?;
        unit_value(
            case,
            sources,
            parsed,
            typed,
            names,
            d.key_type(),
            d.value_type(),
        )?;
        need(
            factory.target() == UnitCallTarget::Declaration(target(&site.factory)?)
                && factory.expression() == source
                && factory.arguments().is_empty()
                && factory.receiver().is_none()
                && factory.return_type() == d.source_type()
                && factory.result_source() == CallableResultSource::Owned,
            "temporary forms exact qualified zeroarg factory Owned result",
            factory,
        )?;
        let read = typed
            .call(expression(site.read))
            .ok_or_else(|| failure("temporary forms actual qualified reader", site.read))?;
        need(
            read.target() == UnitCallTarget::Declaration(target(&input.reader)?)
                && read.arguments().len() == 2
                && read.receiver().is_none(),
            "temporary forms actual qualified reader target/arity",
            read,
        )?;
        for (index, component) in [MapIterationComponent::Key, MapIterationComponent::Value]
            .into_iter()
            .enumerate()
        {
            let binding = d.bindings()[index];
            let loan = p.loan(component);
            let argument = read.arguments()[index];
            let reference = one(
                names
                    .names()
                    .references()
                    .iter()
                    .filter(|r| r.span() == ast::expression(parsed, site.arguments[index])),
                "temporary forms qualified actual reader root",
            )?;
            need(
                binding.source_unit() == unit
                    && binding.symbol()
                        == single_symbol(resolution.resolution(), site.bindings[index])?
                    && typed.symbol_type(binding) == Some([d.key_type(), d.value_type()][index])
                    && owned.binding_kind(binding) == Some(OwnershipBindingKind::Shared)
                    && p.bindings()[index].symbol() == binding
                    && p.bindings()[index].kind() == OwnershipBindingKind::Shared
                    && loan.statement() == statement
                    && loan.source() == source
                    && loan.binding() == binding
                    && loan.component() == component
                    && loan.kind() == LoanKind::Shared
                    && loan.origin() == &UnitLoanTarget::Temporary(source)
                    && argument.argument_index() == index
                    && argument.parameter_index() == index
                    && argument.mode() == ParameterMode::Borrow
                    && argument.parameter_type() == [d.key_type(), d.value_type()][index]
                    && matches!(reference.target(), UnitReferenceTarget::Symbol(root) if *root == binding),
                "temporary forms qualified ordered Shared component and Borrow argument",
                (loan, argument, reference),
            )?;
            bindings.push(binding);
        }
        cleanup::unit(
            UnitStatementId::new(unit, site.body),
            site.call,
            statement,
            source,
            owned,
        )?;
    }
    need(
        bindings
            .iter()
            .enumerate()
            .all(|(index, value)| !bindings[..index].contains(value)),
        "temporary forms four qualified component identities stay separate",
        bindings,
    )
}
