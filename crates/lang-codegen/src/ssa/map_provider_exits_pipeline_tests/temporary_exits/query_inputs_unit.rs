//! unit 的 source-qualified 公共事实；不手造 SymbolId、owner 或 loan-end 表。
use super::super::ast::{Anchors, Call, Function, Input};
use super::{
    Failure, Inputs, anchor, callee_span, expected_result, failure, need, one, query_cleanup,
    query_guards, spans,
};
use lang_frontend::{
    ast::ExpressionId,
    name_resolution::{
        DeclarationId, Namespace, SourceUnitId, SymbolKind, UnitReferenceTarget, UnitSymbolId,
        ValidatedCompilationUnitNames,
    },
    ownership_checking::{
        CompilationUnitOwnership, IterationSource, LoanKind, MapIterationComponent,
        OwnershipBindingKind, UnitCallArgumentOwnershipKind, UnitDropTarget,
        UnitIterationSourceAccess, UnitLoanTarget,
    },
    parser::Expression,
    source::Span,
    type_checking::{
        BuiltinType, CallableResultSource, CompilationUnitTypes, ExpressionCategory,
        IntrinsicTypeConstructor, MapIterationKind, ParameterMode, UnitCallTarget,
        UnitCallableTarget, UnitExpressionId, UnitStatementId, UnitTypeKind,
    },
};

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
                && d.namespace() == Namespace::Value
                && d.name() == function.name
        }),
        "temporary exits FE unique actual qualified function declaration",
    )?
    .id())
}

pub(super) fn binding_at(
    names: &ValidatedCompilationUnitNames,
    id: UnitSymbolId,
    span: Span,
    kind: SymbolKind,
) -> Result<(), Failure> {
    let source = one(
        names
            .names()
            .source_units()
            .iter()
            .filter(|s| s.source_unit() == id.source_unit()),
        "temporary exits FE actual symbol source unit",
    )?;
    let symbol = one(
        source
            .resolution()
            .symbols()
            .iter()
            .filter(|s| s.id() == id.symbol()),
        "temporary exits FE actual qualified local symbol",
    )?;
    need(
        symbol.span() == span
            && symbol.kind() == kind
            && !symbol.is_synthetic()
            && symbol.namespace() == Namespace::Value,
        "temporary exits FE qualified symbol matches exact marker and kind",
        (id, symbol),
    )
}

pub(super) fn symbol(
    names: &ValidatedCompilationUnitNames,
    unit: SourceUnitId,
    span: Span,
    kind: SymbolKind,
) -> Result<UnitSymbolId, Failure> {
    let source = one(
        names
            .names()
            .source_units()
            .iter()
            .filter(|s| s.source_unit() == unit),
        "temporary exits FE actual unit resolution",
    )?;
    let actual = one(
        source
            .resolution()
            .symbols()
            .iter()
            .filter(|s| s.span() == span && !s.is_synthetic()),
        "temporary exits FE actual source marker symbol",
    )?;
    need(
        actual.kind() == kind && actual.namespace() == Namespace::Value,
        "temporary exits FE actual unit symbol kind",
        actual,
    )?;
    // qualified ID 仅取本次真实 reference；不调用 frontend 私有构造器。
    let mut found = Vec::new();
    for reference in names.names().references() {
        if let UnitReferenceTarget::Symbol(id) = reference.target()
            && id.source_unit() == unit
            && id.symbol() == actual.id()
            && !found.contains(id)
        {
            found.push(*id);
        }
    }
    one(
        found.into_iter(),
        "temporary exits FE unique actual referenced qualified symbol",
    )
}

fn reference(
    f: &Input,
    names: &ValidatedCompilationUnitNames,
    expression: ExpressionId,
) -> Result<UnitSymbolId, Failure> {
    let (value, span) = f.expression(expression)?;
    need(
        matches!(value, Expression::Name),
        "temporary exits FE direct qualified Name operand",
        value,
    )?;
    let reference = one(
        names
            .names()
            .references()
            .iter()
            .filter(|r| r.span() == span),
        "temporary exits FE unique actual qualified reference",
    )?;
    let UnitReferenceTarget::Symbol(id) = reference.target() else {
        return Err(failure(
            "temporary exits FE actual qualified Name target",
            reference,
        ));
    };
    let source = one(
        names.names().source_units().iter(),
        "temporary exits FE reference source unit",
    )?;
    need(
        reference.namespace() == Some(Namespace::Value)
            && reference.source_unit() == source.source_unit()
            && id.source_unit() == source.source_unit(),
        "temporary exits FE ordinary qualified reference source and Value namespace",
        reference,
    )?;
    Ok(*id)
}

pub(super) fn callee(
    f: &Input,
    names: &ValidatedCompilationUnitNames,
    expression: ExpressionId,
    selected: DeclarationId,
    namespace: Namespace,
) -> Result<(), Failure> {
    let span = callee_span(f, expression)?;
    let reference = one(
        names
            .names()
            .references()
            .iter()
            .filter(|r| r.span() == span),
        "temporary exits FE actual qualified callee reference",
    )?;
    let present = match reference.target() {
        UnitReferenceTarget::Declaration(id) => *id == selected,
        UnitReferenceTarget::OverloadSet(ids) => {
            namespace == Namespace::Value && ids.contains(&selected)
        }
        _ => false,
    };
    let source = one(
        names.names().source_units().iter(),
        "temporary exits FE callee source unit",
    )?;
    need(
        present
            && reference.namespace() == Some(namespace)
            && reference.source_unit() == source.source_unit(),
        "temporary exits FE selected declaration belongs to actual callee reference",
        reference,
    )
}

pub(super) fn collect(
    f: &Input,
    a: &Anchors,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
) -> Result<Inputs, Failure> {
    let source_names = one(
        names.names().source_units().iter(),
        "temporary exits FE actual sole source unit",
    )?;
    let unit = source_names.source_unit();
    let expression = |id| UnitExpressionId::new(unit, id);
    need(
        source_names.resolution().source_id() == f.parsed.source_id()
            && typed.map_iterations().len() == 2
            && owned.map_iterations().len() == 2,
        "temporary exits FE qualified source and exact two plans",
        (unit, typed.map_iterations(), owned.map_iterations()),
    )?;
    signatures(a, names, typed, owned, unit)?;
    let mut bindings = Vec::new();
    let mut component_loans = Vec::new();
    for site in &a.sites {
        let statement = UnitStatementId::new(unit, site.statement);
        let source = expression(site.source);
        let d = typed
            .map_iteration(statement)
            .ok_or_else(|| failure("temporary exits FE actual typed For", site))?;
        let p = owned
            .map_iteration(statement)
            .ok_or_else(|| failure("temporary exits FE actual owned For", site))?;
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
        need(
            d.statement() == statement
                && d.source() == source
                && p.descriptor() == d
                && typed.expression_type(source) == Some(d.source_type())
                && typed.sequential_iteration(statement).is_none()
                && owned.iteration(statement).is_none()
                && matches!(typed.types().get(d.source_type()), Some(UnitTypeKind::Intrinsic { constructor: actual, arguments })
                if *actual == constructor && d.kind() == kind && arguments.as_slice() == [d.key_type(), d.value_type()])
                && [d.key_type(), d.value_type()].iter().all(|ty| {
                    typed.types().get(*ty) == Some(&UnitTypeKind::Builtin(BuiltinType::String))
                })
                && typed.expression_category(source) == Some(ExpressionCategory::Temporary)
                && p.source_access() == UnitIterationSourceAccess::Temporary
                && matches!(p.source_kind(), IterationSource::BorrowedSource(UnitLoanTarget::Temporary(id)) if *id == source)
                && p.source() == Some(&UnitLoanTarget::Temporary(source)),
            "temporary exits FE exact owned Map/Mutable String pair and temporary Call source",
            (site, d, p),
        )?;
        let factory = anchor(a, site.factory)?;
        let target = declaration(names, unit, factory)?;
        let call = typed
            .call(source)
            .ok_or_else(|| failure("temporary exits FE actual zeroarg factory Call", site))?;
        need(
            call.expression() == source
                && call.target() == UnitCallTarget::Declaration(target)
                && call.receiver().is_none()
                && call.arguments().is_empty()
                && call.result_source() == CallableResultSource::Owned
                && call.return_type() == d.source_type()
                && !owned.loans().iter().any(|loan| loan.call() == source)
                && owned.call_argument_contracts_for(source).next().is_none()
                && !owned
                    .value_deliveries()
                    .iter()
                    .any(|delivery| delivery.call() == source),
            "temporary exits FE factory actual declaration Call and Owned result",
            call,
        )?;
        callee(f, names, site.source, target, Namespace::Value)?;
        for (i, component) in [MapIterationComponent::Key, MapIterationComponent::Value]
            .into_iter()
            .enumerate()
        {
            let binding = d.bindings()[i];
            let loan = p.loan(component);
            need(
                binding.source_unit() == unit
                    && binding == symbol(names, unit, site.bindings[i], SymbolKind::ForBinding)?
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
                    && loan.origin() == &UnitLoanTarget::Temporary(source)
                    && !owned
                        .drops()
                        .iter()
                        .any(|drop| drop.target() == UnitDropTarget::Named(binding)),
                "temporary exits FE exact component marker Shared type and source lineage",
                (site, binding, loan),
            )?;
            bindings.push(binding);
            component_loans.push(loan);
        }
        let consumer = Call {
            expression: site.read,
            span: site.read_span,
            target: site.consumer,
            arguments: site.arguments.to_vec(),
        };
        borrow_call(f, a, &consumer, names, (typed, owned), &d.bindings())?;
    }
    need(
        bindings.len() == 4
            && bindings
                .iter()
                .enumerate()
                .all(|(i, b)| !bindings[..i].contains(b))
            && component_loans.len() == 4
            && component_loans
                .iter()
                .enumerate()
                .all(|(i, l)| !component_loans[..i].contains(l)),
        "temporary exits FE all four component identities stay independent",
        (bindings, component_loans),
    )?;
    if let Some(read) = &a.operand {
        let roots = read
            .binding_markers
            .iter()
            .map(|marker| symbol(names, unit, *marker, SymbolKind::ValueParameter))
            .collect::<Result<Vec<_>, _>>()?;
        borrow_call(f, a, &read.call, names, (typed, owned), &roots)?;
    }
    callers(f, a, names, typed, owned, unit)?;
    let guards = query_guards::unit(f, a, names, typed, owned, unit)?;
    let drops = query_cleanup::unit(f, a, guards, owned, unit)?;
    spans(f, a, drops, Some(unit))
}

fn borrow_call(
    f: &Input,
    a: &Anchors,
    site: &Call,
    names: &ValidatedCompilationUnitNames,
    products: (&CompilationUnitTypes, &CompilationUnitOwnership),
    roots: &[UnitSymbolId],
) -> Result<(), Failure> {
    let (typed, owned) = products;
    let first = roots
        .first()
        .ok_or_else(|| failure("temporary exits FE nonempty qualified call roots", roots))?;
    let unit = first.source_unit();
    let expression = |id| UnitExpressionId::new(unit, id);
    let call_id = expression(site.expression);
    let target = declaration(names, unit, anchor(a, site.target)?)?;
    let signature = one(
        typed
            .signatures()
            .declarations()
            .iter()
            .filter(|s| s.declaration() == target)
            .filter_map(|s| s.callable()),
        "temporary exits FE actual qualified callee signature",
    )?;
    let call = typed
        .call(call_id)
        .ok_or_else(|| failure("temporary exits FE actual qualified typed call", site))?;
    need(
        call.expression() == call_id
            && call.target() == UnitCallTarget::Declaration(target)
            && call.receiver().is_none()
            && call.arguments().len() == roots.len()
            && site.arguments.len() == roots.len()
            && signature.parameters().len() == roots.len()
            && call.return_type() == signature.return_type()
            && typed.expression_type(call_id) == Some(call.return_type())
            && call.result_source() == CallableResultSource::Owned
            && typed.types().get(call.return_type())
                == Some(&UnitTypeKind::Builtin(expected_result(site.target))),
        "temporary exits FE actual qualified read/scan target signature and arity",
        (site, call, signature),
    )?;
    callee(f, names, site.expression, target, Namespace::Value)?;
    let loans = owned
        .loans()
        .iter()
        .filter(|l| l.call() == call_id)
        .collect::<Vec<_>>();
    let ending = owned.loans_ending_at(call_id).collect::<Vec<_>>();
    let contracts = owned
        .call_argument_contracts_for(call_id)
        .collect::<Vec<_>>();
    // unit getter 只按 call 筛同表，明确只用于一致性；终止遵循 UnitLoanFact 同步合同与 SSA BorrowEnd。
    need(
        loans.len() == roots.len()
            && ending == loans
            && contracts.len() == roots.len()
            && !owned.value_deliveries().iter().any(|d| d.call() == call_id),
        "temporary exits FE qualified exact SharedLoan contracts and same-table end accessor",
        (&loans, &ending, &contracts),
    )?;
    for (index, (argument, root)) in site.arguments.iter().zip(roots).enumerate() {
        let actual = &call.arguments()[index];
        let parameter = &signature.parameters()[index];
        let argument_span = f.expression(*argument)?.1;
        need(
            root.source_unit() == unit
                && actual.argument_index() == index
                && actual.parameter_index() == index
                && actual.mode() == ParameterMode::Borrow
                && parameter.mode() == ParameterMode::Borrow
                && actual.parameter_type() == parameter.ty()
                && typed.expression_type(expression(*argument)) == Some(actual.parameter_type())
                && typed.expression_category(expression(*argument))
                    == Some(ExpressionCategory::Place)
                && typed.symbol_type(*root) == Some(actual.parameter_type())
                && reference(f, names, *argument)? == *root,
            "temporary exits FE qualified argument parameter index and Borrow root exact",
            (site, index, actual, parameter, root),
        )?;
        let contract = one(
            contracts
                .iter()
                .copied()
                .filter(|c| c.argument() == expression(*argument)),
            "temporary exits FE unique actual qualified argument contract",
        )?;
        let loan = one(
            loans
                .iter()
                .copied()
                .filter(|l| l.argument() == expression(*argument)),
            "temporary exits FE unique actual qualified argument loan",
        )?;
        need(
            contract.call() == call_id
                && contract.parameter_index() == index
                && contract.parameter_type() == actual.parameter_type()
                && contract.kind() == UnitCallArgumentOwnershipKind::SharedLoan
                && contract.category() == ExpressionCategory::Place
                && !contract.crosses_thread()
                && contract.argument_span() == argument_span
                && contract.call_span() == site.span
                && contract.parameter_span() == Some(parameter.span())
                && contract.loan_begin_span() == argument_span
                && loan.kind() == LoanKind::Shared
                && matches!(loan.target(), UnitLoanTarget::Place(place) if place.is_root() && place.root() == *root)
                && loan.begin_span() == argument_span
                && loan.end_span() == site.span
                && loan.parameter_span() == contract.parameter_span(),
            "temporary exits FE qualified exact loan root contract and synchronous call spans",
            (contract, loan),
        )?;
    }
    Ok(())
}

fn signatures(
    a: &Anchors,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    unit: SourceUnitId,
) -> Result<(), Failure> {
    for function in &a.functions {
        let id = declaration(names, unit, function)?;
        let signature = one(
            typed
                .signatures()
                .declarations()
                .iter()
                .filter(|s| s.declaration() == id)
                .filter_map(|s| s.callable()),
            "temporary exits FE actual unique qualified function signature",
        )?;
        need(
            signature.name_span() == function.name_span
                && signature.target() == UnitCallableTarget::Declaration(id)
                && signature.receiver().is_none()
                && signature.type_parameters().is_empty()
                && signature.has_body()
                && signature.result_source() == CallableResultSource::Owned
                && signature.parameters().len() == function.parameters.len(),
            "temporary exits FE actual qualified function declaration and signature shape",
            (function, signature),
        )?;
        let source_type = typed.expression_type(UnitExpressionId::new(unit, a.sites[0].source));
        for (i, parameter) in signature.parameters().iter().enumerate() {
            let id = symbol(
                names,
                unit,
                function.parameters[i],
                SymbolKind::ValueParameter,
            )?;
            need(
                parameter.mode() == ParameterMode::Borrow
                    && parameter.symbol() == Some(id)
                    && typed.symbol_type(id) == Some(parameter.ty())
                    && owned.binding_kind(id) == Some(OwnershipBindingKind::Shared)
                    && typed.types().get(parameter.ty())
                        == Some(&UnitTypeKind::Builtin(BuiltinType::String)),
                "temporary exits FE actual ordered qualified Borrow parameter",
                (function, i, parameter, id),
            )?;
        }
        need(
            if a.sites.iter().any(|site| site.factory == function.name) {
                Some(signature.return_type()) == source_type
            } else {
                typed.types().get(signature.return_type())
                    == Some(&UnitTypeKind::Builtin(expected_result(function.name)))
            },
            "temporary exits FE actual qualified function return contract",
            (function, signature),
        )?;
    }
    Ok(())
}

fn callers(
    f: &Input,
    a: &Anchors,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    unit: SourceUnitId,
) -> Result<(), Failure> {
    let expression = |id| UnitExpressionId::new(unit, id);
    let expected = if f.exit == super::super::ast::Exit::Return {
        2
    } else {
        0
    };
    need(
        a.scans.len() == expected && a.return_checks.len() == expected,
        "temporary exits FE caller coverage before indexing",
        (&a.scans, &a.return_checks),
    )?;
    let mut locals = Vec::new();
    for (index, (scan, check)) in a.scans.iter().zip(&a.return_checks).enumerate() {
        let target = declaration(names, unit, anchor(a, scan.target)?)?;
        let call = typed
            .call(expression(scan.expression))
            .ok_or_else(|| failure("temporary exits FE actual scanner call", scan))?;
        need(
            scan.arguments.is_empty()
                && call.expression() == expression(scan.expression)
                && call.target() == UnitCallTarget::Declaration(target)
                && call.receiver().is_none()
                && call.arguments().is_empty()
                && call.result_source() == CallableResultSource::Owned
                && typed.types().get(call.return_type())
                    == Some(&UnitTypeKind::Builtin(BuiltinType::Int))
                && typed.expression_type(expression(scan.expression)) == Some(call.return_type())
                && !owned
                    .loans()
                    .iter()
                    .any(|loan| loan.call() == expression(scan.expression))
                && owned
                    .call_argument_contracts_for(expression(scan.expression))
                    .next()
                    .is_none()
                && !owned
                    .value_deliveries()
                    .iter()
                    .any(|delivery| delivery.call() == expression(scan.expression)),
            "temporary exits FE zeroarg scanner exact Int Call result",
            (scan, call),
        )?;
        callee(f, names, scan.expression, target, Namespace::Value)?;
        let local = symbol(names, unit, check.marker, SymbolKind::Variable)?;
        need(
            check.expected == if index == 0 { "7" } else { "0" }
                && check.initializer == scan.expression
                && reference(f, names, check.reference)? == local
                && owned.formed_owned_locals().contains(&local)
                && typed.symbol_type(local) == Some(call.return_type())
                && typed.expression_type(expression(check.reference)) == Some(call.return_type()),
            "temporary exits FE caller formed marker and exact scan-return reference",
            (check, local, call),
        )?;
        locals.push(local);
    }
    need(
        locals
            .iter()
            .enumerate()
            .all(|(i, v)| !locals[..i].contains(v)),
        "temporary exits FE caller local identities distinct",
        locals,
    )
}
