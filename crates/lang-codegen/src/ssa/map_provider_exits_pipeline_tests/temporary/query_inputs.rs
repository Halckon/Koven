//! 同次公开 typed/ownership 产品到真实 source spans 的桥；不制造 facts 或资格结果。
use super::{
    super::{
        fixture::{Failure, failure, need},
        query_inputs::one,
    },
    ast::{Anchors, Function, Input, Site},
    query_cleanup,
};
use lang_frontend::{
    ast::ExpressionId,
    name_resolution::{
        DeclarationId, NameResolution, ReferenceTarget, SourceUnitId, SymbolId, SymbolKind,
        UnitReferenceTarget, ValidatedCompilationUnitNames,
    },
    ownership_checking::{
        CompilationUnitOwnership, IterationSource, LoanKind, LoanTarget, MapIterationComponent,
        MapIterationSourceAccess, OwnershipBindingKind, OwnershipCheckedFile,
        UnitIterationSourceAccess, UnitLoanTarget,
    },
    parser::Expression,
    source::Span,
    type_checking::{
        CallableResultSource, CallableTarget, CompilationUnitTypes, ExpressionCategory,
        ParameterMode, TypedFile, UnitCallTarget, UnitExpressionId, UnitStatementId,
    },
};

#[derive(Debug)]
pub(super) struct SiteInput {
    pub span: Span,
    pub source: Span,
    pub factory: Span,
    pub read_call: Span,
    pub arguments: [Span; 2],
    pub drop: Span,
}

#[derive(Debug)]
pub(super) struct Inputs {
    pub entry: Span,
    pub read: Span,
    pub sites: Vec<SiteInput>,
}

fn anchor<'a>(a: &'a Anchors, name: &str) -> Result<&'a Function, Failure> {
    one(
        a.functions.iter().filter(|v| v.name == name),
        "temporary FE unique actual function AST",
    )
}

fn spans(f: &Input, a: &Anchors, site: &Site, drop: Span) -> Result<SiteInput, Failure> {
    need(
        drop == site.source_span,
        "temporary FE source and Drop original Call span",
        (site, drop),
    )?;
    Ok(SiteInput {
        span: site.span,
        source: site.source_span,
        factory: anchor(a, site.factory)?.span,
        read_call: f.expression(site.read)?.1,
        arguments: [
            f.expression(site.arguments[0])?.1,
            f.expression(site.arguments[1])?.1,
        ],
        drop,
    })
}

fn symbol(names: &NameResolution, span: Span) -> Result<SymbolId, Failure> {
    Ok(one(
        names
            .symbols()
            .iter()
            .filter(|s| s.span() == span && !s.is_synthetic()),
        "temporary FE unique actual source symbol",
    )?
    .id())
}

fn callee_span(f: &Input, expression: ExpressionId) -> Result<Span, Failure> {
    let (value, _) = f.expression(expression)?;
    let Expression::Call { callee, .. } = value else {
        return Err(failure("temporary FE actual Call expression", value));
    };
    Ok(f.expression(*callee)?.1)
}

fn single_callee(
    f: &Input,
    names: &NameResolution,
    expression: ExpressionId,
    selected: SymbolId,
) -> Result<(), Failure> {
    let span = callee_span(f, expression)?;
    let reference = one(
        names.references().iter().filter(|r| r.span() == span),
        "temporary FE actual callee reference",
    )?;
    let selected_present = match reference.target() {
        ReferenceTarget::Symbol(id) => *id == selected,
        ReferenceTarget::OverloadSet(ids) => ids.contains(&selected),
        _ => false,
    };
    need(
        selected_present,
        "temporary FE typed-selected source target belongs to real callee reference",
        reference,
    )
}

pub(super) fn single(
    f: &Input,
    a: &Anchors,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
) -> Result<Inputs, Failure> {
    need(
        names.source_id() == f.parsed.source_id()
            && typed.source_id() == f.parsed.source_id()
            && owned.source_id() == f.parsed.source_id()
            && typed.map_iterations().len() == 2
            && owned.map_iterations().len() == 2,
        "temporary FE same source exact two actual plans",
        (typed.map_iterations(), owned.map_iterations()),
    )?;
    let read = anchor(a, "read")?;
    let read_id = symbol(names, read.name_span)?;
    let mut sites = Vec::new();
    let mut owners = Vec::new();
    let mut bindings = Vec::new();
    for site in &a.sites {
        let d = typed
            .map_iteration(site.statement)
            .ok_or_else(|| failure("temporary FE exact typed For", site))?;
        let p = owned
            .map_iteration(site.statement)
            .ok_or_else(|| failure("temporary FE exact owned For", site))?;
        need(
            d.statement() == site.statement
                && d.source() == site.source
                && p.descriptor() == d
                && typed.expression_type(site.source) == Some(d.source_type())
                && typed.expression_category(site.source) == Some(ExpressionCategory::Temporary)
                && p.source_access() == MapIterationSourceAccess::Temporary
                && matches!(p.source_kind(), IterationSource::BorrowedSource(LoanTarget::Temporary(id)) if *id == site.source),
            "temporary FE actual factory expression is this provider owner",
            (site, d, p),
        )?;
        let factory = anchor(a, site.factory)?;
        let factory_id = symbol(names, factory.name_span)?;
        let factory_symbol = one(
            names.symbols().iter().filter(|s| s.id() == factory_id),
            "temporary FE actual factory symbol",
        )?;
        need(
            factory_symbol.kind() == SymbolKind::Function && factory_symbol.name() == site.factory,
            "temporary FE factory symbol matches actual function",
            factory_symbol,
        )?;
        let call = typed
            .call(site.source)
            .ok_or_else(|| failure("temporary FE actual factory typed Call", site.source))?;
        need(
            call.expression() == site.source
                && call.target() == CallableTarget::Source(factory_id)
                && call.receiver().is_none()
                && call.arguments().is_empty()
                && call.result_source() == CallableResultSource::Owned
                && call.return_type() == d.source_type(),
            "temporary FE own zero-argument factory Call selected result",
            call,
        )?;
        single_callee(f, names, site.source, factory_id)?;
        for (index, component) in [MapIterationComponent::Key, MapIterationComponent::Value]
            .into_iter()
            .enumerate()
        {
            let binding = d.bindings()[index];
            let loan = p.loan(component);
            need(
                binding == symbol(names, site.bindings[index])?
                    && typed.symbol_type(binding) == Some([d.key_type(), d.value_type()][index])
                    && owned.binding_kind(binding) == Some(OwnershipBindingKind::Shared)
                    && p.bindings()[index].symbol() == binding
                    && p.bindings()[index].kind() == OwnershipBindingKind::Shared
                    && loan.statement() == site.statement
                    && loan.source() == site.source
                    && loan.binding() == binding
                    && loan.component() == component
                    && loan.kind() == LoanKind::Shared
                    && loan.origin() == &LoanTarget::Temporary(site.source),
                "temporary FE actual component binding and source lineage",
                (site, binding, loan),
            )?;
        }
        let call = typed
            .call(site.read)
            .ok_or_else(|| failure("temporary FE actual read typed Call", site.read))?;
        need(
            call.expression() == site.read
                && call.target() == CallableTarget::Source(read_id)
                && call.receiver().is_none()
                && call.arguments().len() == 2,
            "temporary FE actual read target and arity before indexing",
            call,
        )?;
        single_callee(f, names, site.read, read_id)?;
        for (index, argument) in call.arguments().iter().enumerate() {
            let span = f.expression(site.arguments[index])?.1;
            let reference = one(
                names.references().iter().filter(|r| r.span() == span),
                "temporary FE actual component reference",
            )?;
            need(
                argument.argument_index() == index
                    && argument.parameter_index() == index
                    && argument.mode() == ParameterMode::Borrow
                    && argument.parameter_type() == [d.key_type(), d.value_type()][index]
                    && matches!(reference.target(), ReferenceTarget::Symbol(id) if *id == d.bindings()[index]),
                "temporary FE read Borrow delivery belongs to actual K/V",
                (argument, reference),
            )?;
        }
        let (drop, owner) = query_cleanup::single(site, owned)?;
        owners.push(owner);
        bindings.extend(d.bindings());
        sites.push(spans(f, a, site, drop)?);
    }
    need(
        owners.len() == 2
            && owners[0] != owners[1]
            && bindings
                .iter()
                .enumerate()
                .all(|(i, b)| !bindings[..i].contains(b)),
        "temporary FE distinct source owners and all four bindings",
        (owners, bindings),
    )?;
    Ok(Inputs {
        entry: anchor(a, "entry")?.span,
        read: read.span,
        sites,
    })
}

fn declaration(
    names: &ValidatedCompilationUnitNames,
    unit: SourceUnitId,
    function: &Function,
) -> Result<DeclarationId, Failure> {
    Ok(one(
        names.names().index().declarations().iter().filter(|d| {
            d.source_unit() == unit
                && d.root() == function.item
                && d.name_span() == function.name_span
                && d.kind() == SymbolKind::Function
                && d.name() == function.name
        }),
        "temporary FE actual qualified function declaration",
    )?
    .id())
}

fn unit_callee(
    f: &Input,
    names: &ValidatedCompilationUnitNames,
    expression: ExpressionId,
    selected: DeclarationId,
) -> Result<(), Failure> {
    let span = callee_span(f, expression)?;
    let reference = one(
        names
            .names()
            .references()
            .iter()
            .filter(|r| r.span() == span),
        "temporary FE qualified actual callee reference",
    )?;
    let selected_present = match reference.target() {
        UnitReferenceTarget::Declaration(id) => *id == selected,
        UnitReferenceTarget::OverloadSet(ids) => ids.contains(&selected),
        _ => false,
    };
    need(
        selected_present,
        "temporary FE qualified typed-selected target belongs to actual reference",
        reference,
    )
}

pub(super) fn unit(
    f: &Input,
    a: &Anchors,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
) -> Result<Inputs, Failure> {
    let source_names = one(
        names.names().source_units().iter(),
        "temporary FE actual sole source unit",
    )?;
    let unit = source_names.source_unit();
    let expression = |id| UnitExpressionId::new(unit, id);
    need(
        source_names.resolution().source_id() == f.parsed.source_id()
            && typed.map_iterations().len() == 2
            && owned.map_iterations().len() == 2,
        "temporary FE actual qualified source and two plans",
        (unit, typed.map_iterations(), owned.map_iterations()),
    )?;
    let read = anchor(a, "read")?;
    let read_id = declaration(names, unit, read)?;
    let mut sites = Vec::new();
    let mut bindings = Vec::new();
    for site in &a.sites {
        let statement = UnitStatementId::new(unit, site.statement);
        let source = expression(site.source);
        let d = typed
            .map_iteration(statement)
            .ok_or_else(|| failure("temporary FE qualified typed For", statement))?;
        let p = owned
            .map_iteration(statement)
            .ok_or_else(|| failure("temporary FE qualified owned For", statement))?;
        need(
            d.statement() == statement
                && d.source() == source
                && p.descriptor() == d
                && typed.expression_type(source) == Some(d.source_type())
                && typed.expression_category(source) == Some(ExpressionCategory::Temporary)
                && p.source_access() == UnitIterationSourceAccess::Temporary
                && matches!(p.source_kind(), IterationSource::BorrowedSource(UnitLoanTarget::Temporary(id)) if *id == source),
            "temporary FE actual qualified source expression identity",
            (site, d, p),
        )?;
        let factory_id = declaration(names, unit, anchor(a, site.factory)?)?;
        let call = typed
            .call(source)
            .ok_or_else(|| failure("temporary FE qualified actual factory Call", source))?;
        need(
            call.expression() == source
                && call.target() == UnitCallTarget::Declaration(factory_id)
                && call.receiver().is_none()
                && call.arguments().is_empty()
                && call.result_source() == CallableResultSource::Owned
                && call.return_type() == d.source_type(),
            "temporary FE actual qualified factory and Owned result",
            call,
        )?;
        unit_callee(f, names, site.source, factory_id)?;
        for (index, component) in [MapIterationComponent::Key, MapIterationComponent::Value]
            .into_iter()
            .enumerate()
        {
            let binding = d.bindings()[index];
            let loan = p.loan(component);
            need(
                binding.source_unit() == unit
                    && binding.symbol() == symbol(source_names.resolution(), site.bindings[index])?
                    && typed.symbol_type(binding) == Some([d.key_type(), d.value_type()][index])
                    && owned.binding_kind(binding) == Some(OwnershipBindingKind::Shared)
                    && p.bindings()[index].symbol() == binding
                    && p.bindings()[index].kind() == OwnershipBindingKind::Shared
                    && loan.statement() == statement
                    && loan.source() == source
                    && loan.binding() == binding
                    && loan.component() == component
                    && loan.kind() == LoanKind::Shared
                    && loan.origin() == &UnitLoanTarget::Temporary(source),
                "temporary FE qualified component source and marker lineage",
                (statement, binding, loan),
            )?;
        }
        let read_call = expression(site.read);
        let call = typed
            .call(read_call)
            .ok_or_else(|| failure("temporary FE qualified actual read Call", read_call))?;
        need(
            call.expression() == read_call
                && call.target() == UnitCallTarget::Declaration(read_id)
                && call.receiver().is_none()
                && call.arguments().len() == 2,
            "temporary FE qualified read target and arity",
            call,
        )?;
        unit_callee(f, names, site.read, read_id)?;
        for (index, argument) in call.arguments().iter().enumerate() {
            let span = f.expression(site.arguments[index])?.1;
            let reference = one(
                names
                    .names()
                    .references()
                    .iter()
                    .filter(|r| r.span() == span),
                "temporary FE qualified actual component reference",
            )?;
            need(
                argument.argument_index() == index
                    && argument.parameter_index() == index
                    && argument.mode() == ParameterMode::Borrow
                    && argument.parameter_type() == [d.key_type(), d.value_type()][index]
                    && matches!(reference.target(), UnitReferenceTarget::Symbol(id) if *id == d.bindings()[index]),
                "temporary FE qualified Borrow argument is this component",
                (argument, reference),
            )?;
        }
        let drop = query_cleanup::unit(site, statement, source, owned)?;
        bindings.extend(d.bindings());
        sites.push(spans(f, a, site, drop)?);
    }
    need(
        bindings.len() == 4
            && bindings
                .iter()
                .enumerate()
                .all(|(i, b)| !bindings[..i].contains(b)),
        "temporary FE four qualified component bindings stay independent",
        bindings,
    )?;
    Ok(Inputs {
        entry: anchor(a, "entry")?.span,
        read: read.span,
        sites,
    })
}
