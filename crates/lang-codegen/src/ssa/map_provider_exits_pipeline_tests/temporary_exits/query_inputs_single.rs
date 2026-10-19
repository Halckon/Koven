//! single 公共 names/typed/owned 的 fail-closed 身份核验。
use super::super::ast::{Anchors, Call, Input};
use super::Failure;
use super::{
    Inputs, anchor, callee_span, expected_result, failure, need, one, query_cleanup, query_guards,
    spans,
};
use lang_frontend::{
    ast::ExpressionId,
    name_resolution::{NameResolution, Namespace, ReferenceTarget, SymbolId, SymbolKind},
    ownership_checking::{
        DropTarget, IterationSource, LoanEndPoint, LoanKind, LoanTarget, MapIterationComponent,
        MapIterationSourceAccess, OwnershipBindingKind, OwnershipCheckedFile,
    },
    parser::Expression,
    source::Span,
    type_checking::{
        BuiltinType, CallableResultSource, CallableTarget, ExpressionCategory,
        IntrinsicTypeConstructor, MapIterationKind, ParameterMode, TypeKind, TypedFile,
    },
};

pub(super) fn symbol(
    names: &NameResolution,
    span: Span,
    kind: SymbolKind,
) -> Result<SymbolId, Failure> {
    let symbol = one(
        names
            .symbols()
            .iter()
            .filter(|s| s.span() == span && !s.is_synthetic()),
        "temporary exits FE unique actual single symbol",
    )?;
    need(
        symbol.kind() == kind
            && symbol.namespace()
                == if kind == SymbolKind::Classifier {
                    Namespace::Type
                } else {
                    Namespace::Value
                },
        "temporary exits FE actual symbol kind",
        symbol,
    )?;
    Ok(symbol.id())
}

fn reference(
    f: &Input,
    names: &NameResolution,
    expression: ExpressionId,
) -> Result<SymbolId, Failure> {
    let (value, span) = f.expression(expression)?;
    need(
        matches!(value, Expression::Name),
        "temporary exits FE direct Name operand",
        value,
    )?;
    let reference = one(
        names.references().iter().filter(|r| r.span() == span),
        "temporary exits FE unique actual single reference",
    )?;
    let ReferenceTarget::Symbol(id) = reference.target() else {
        return Err(failure(
            "temporary exits FE actual Name symbol target",
            reference,
        ));
    };
    need(
        reference.namespace() == Namespace::Value,
        "temporary exits FE ordinary reference Value namespace",
        reference,
    )?;
    Ok(*id)
}

pub(super) fn callee(
    f: &Input,
    names: &NameResolution,
    expression: ExpressionId,
    selected: SymbolId,
    namespace: Namespace,
) -> Result<(), Failure> {
    let span = callee_span(f, expression)?;
    let reference = one(
        names.references().iter().filter(|r| r.span() == span),
        "temporary exits FE actual single callee reference",
    )?;
    let present = match reference.target() {
        ReferenceTarget::Symbol(id) => *id == selected,
        ReferenceTarget::OverloadSet(ids) => {
            namespace == Namespace::Value && ids.contains(&selected)
        }
        _ => false,
    };
    need(
        present && reference.namespace() == namespace,
        "temporary exits FE selected target belongs to actual callee reference",
        reference,
    )
}

pub(super) fn collect(
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
        "temporary exits FE same source and exact two plans",
        (typed.map_iterations(), owned.map_iterations()),
    )?;
    signatures(a, names, typed, owned)?;
    let mut bindings = Vec::new();
    let mut component_loans = Vec::new();
    for site in &a.sites {
        let d = typed
            .map_iteration(site.statement)
            .ok_or_else(|| failure("temporary exits FE actual typed For", site))?;
        let p = owned
            .map_iteration(site.statement)
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
            d.statement() == site.statement
                && d.source() == site.source
                && p.descriptor() == d
                && typed.expression_type(site.source) == Some(d.source_type())
                && typed.sequential_iteration(site.statement).is_none()
                && owned.iteration(site.statement).is_none()
                && matches!(typed.types().get(d.source_type()), Some(TypeKind::Intrinsic { constructor: actual, arguments })
                if *actual == constructor && d.kind() == kind && arguments.as_slice() == [d.key_type(), d.value_type()])
                && [d.key_type(), d.value_type()].iter().all(|ty| {
                    typed.types().get(*ty) == Some(&TypeKind::Builtin(BuiltinType::String))
                })
                && typed.expression_category(site.source) == Some(ExpressionCategory::Temporary)
                && p.source_access() == MapIterationSourceAccess::Temporary
                && matches!(p.source_kind(), IterationSource::BorrowedSource(LoanTarget::Temporary(id)) if *id == site.source)
                && p.source() == Some(&LoanTarget::Temporary(site.source)),
            "temporary exits FE exact owned Map/Mutable String pair and temporary Call source",
            (site, d, p),
        )?;
        let factory = anchor(a, site.factory)?;
        let target = symbol(names, factory.name_span, SymbolKind::Function)?;
        let call = typed
            .call(site.source)
            .ok_or_else(|| failure("temporary exits FE actual zeroarg factory Call", site))?;
        need(
            call.expression() == site.source
                && call.target() == CallableTarget::Source(target)
                && call.receiver().is_none()
                && call.arguments().is_empty()
                && call.result_source() == CallableResultSource::Owned
                && call.return_type() == d.source_type()
                && !owned.loans().iter().any(|loan| loan.call() == site.source)
                && !owned
                    .loan_ends()
                    .iter()
                    .any(|end| end.call() == site.source),
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
                binding == symbol(names, site.bindings[i], SymbolKind::ForBinding)?
                    && typed.symbol_type(binding) == Some([d.key_type(), d.value_type()][i])
                    && owned.binding_kind(binding) == Some(OwnershipBindingKind::Shared)
                    && p.bindings()[i].symbol() == binding
                    && p.bindings()[i].kind() == OwnershipBindingKind::Shared
                    && loan == &p.loans()[i]
                    && loan.statement() == site.statement
                    && loan.source() == site.source
                    && loan.binding() == binding
                    && loan.component() == component
                    && loan.kind() == LoanKind::Shared
                    && loan.origin() == &LoanTarget::Temporary(site.source)
                    && !owned
                        .drops()
                        .iter()
                        .any(|drop| drop.target() == DropTarget::Named(binding)),
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
            .map(|marker| symbol(names, *marker, SymbolKind::ValueParameter))
            .collect::<Result<Vec<_>, _>>()?;
        borrow_call(f, a, &read.call, names, (typed, owned), &roots)?;
    }
    callers(f, a, names, typed, owned)?;
    let guards = query_guards::single(f, a, names, typed, owned)?;
    let drops = query_cleanup::single(f, a, guards, owned)?;
    spans(f, a, drops, None)
}

fn borrow_call(
    f: &Input,
    a: &Anchors,
    site: &Call,
    names: &NameResolution,
    products: (&TypedFile, &OwnershipCheckedFile),
    roots: &[SymbolId],
) -> Result<(), Failure> {
    let (typed, owned) = products;
    let function = anchor(a, site.target)?;
    let target = symbol(names, function.name_span, SymbolKind::Function)?;
    let signature = one(
        typed.callables().iter().filter(|c| c.symbol() == target),
        "temporary exits FE actual callee callable",
    )?;
    let call = typed
        .call(site.expression)
        .ok_or_else(|| failure("temporary exits FE actual read/scan typed call", site))?;
    need(
        call.expression() == site.expression
            && call.target() == CallableTarget::Source(target)
            && call.receiver().is_none()
            && call.arguments().len() == roots.len()
            && site.arguments.len() == roots.len()
            && signature.parameters().len() == roots.len()
            && call.return_type() == signature.return_type()
            && typed.expression_type(site.expression) == Some(call.return_type())
            && call.result_source() == CallableResultSource::Owned
            && typed.types().get(call.return_type())
                == Some(&TypeKind::Builtin(expected_result(site.target))),
        "temporary exits FE actual read/scan target signature and exact arity",
        (site, call, signature),
    )?;
    callee(f, names, site.expression, target, Namespace::Value)?;
    let loans = owned
        .loans()
        .iter()
        .filter(|l| l.call() == site.expression)
        .collect::<Vec<_>>();
    let ends = owned
        .loan_ends()
        .iter()
        .filter(|e| e.call() == site.expression)
        .collect::<Vec<_>>();
    need(
        loans.len() == roots.len() && ends.len() == roots.len(),
        "temporary exits FE exact argument loan and end coverage",
        (&loans, &ends),
    )?;
    for (index, (argument, root)) in site.arguments.iter().zip(roots).enumerate() {
        let actual = &call.arguments()[index];
        let parameter = &signature.parameters()[index];
        need(
            actual.argument_index() == index
                && actual.parameter_index() == index
                && actual.mode() == ParameterMode::Borrow
                && parameter.mode == ParameterMode::Borrow
                && actual.parameter_type() == parameter.ty
                && typed.expression_type(*argument) == Some(actual.parameter_type())
                && typed.expression_category(*argument) == Some(ExpressionCategory::Place)
                && typed.symbol_type(*root) == Some(actual.parameter_type())
                && reference(f, names, *argument)? == *root,
            "temporary exits FE argument parameter index and Borrow root are exact",
            (site, index, actual, parameter, root),
        )?;
        let loan = one(
            loans.iter().copied().filter(|l| l.argument() == *argument),
            "temporary exits FE unique actual argument loan",
        )?;
        let end = one(
            ends.iter().copied().filter(|e| e.argument() == *argument),
            "temporary exits FE unique actual argument loan end",
        )?;
        need(
            loan.kind() == LoanKind::Shared
                && matches!(loan.target(), LoanTarget::Place(place) if place.is_root() && place.root() == *root)
                && loan.begin_span() == f.expression(*argument)?.1
                && loan.end_span() == site.span
                && !loan.is_receiver_reservation()
                && loan.activation_point().is_none()
                && end.call() == site.expression
                && end.argument() == loan.argument()
                && end.point() == LoanEndPoint::CallReturn(site.expression),
            "temporary exits FE exact loan lineage span and synchronous CallReturn end",
            (loan, end),
        )?;
    }
    Ok(())
}

fn signatures(
    a: &Anchors,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
) -> Result<(), Failure> {
    for function in &a.functions {
        let id = symbol(names, function.name_span, SymbolKind::Function)?;
        let source = one(
            names.symbols().iter().filter(|s| s.id() == id),
            "temporary exits FE function source symbol",
        )?;
        let callable = one(
            typed.callables().iter().filter(|c| c.symbol() == id),
            "temporary exits FE actual unique function callable",
        )?;
        need(
            source.name() == function.name
                && callable.receiver().is_none()
                && callable.type_parameters().is_empty()
                && callable.result_source() == CallableResultSource::Owned
                && callable.parameters().len() == function.parameters.len()
                && callable.parameter_symbols().len() == function.parameters.len(),
            "temporary exits FE actual function name and unmarked signature shape",
            (function, callable),
        )?;
        let source_type = typed.expression_type(a.sites[0].source);
        for (i, parameter) in callable.parameters().iter().enumerate() {
            let id = symbol(names, function.parameters[i], SymbolKind::ValueParameter)?;
            need(
                parameter.mode == ParameterMode::Borrow
                    && callable.parameter_symbols()[i] == Some(id)
                    && typed.symbol_type(id) == Some(parameter.ty)
                    && owned.binding_kind(id) == Some(OwnershipBindingKind::Shared)
                    && typed.types().get(parameter.ty)
                        == Some(&TypeKind::Builtin(BuiltinType::String)),
                "temporary exits FE actual ordered Borrow parameter identity",
                (function, i, parameter, id),
            )?;
        }
        need(
            if a.sites.iter().any(|site| site.factory == function.name) {
                Some(callable.return_type()) == source_type
            } else {
                typed.types().get(callable.return_type())
                    == Some(&TypeKind::Builtin(expected_result(function.name)))
            },
            "temporary exits FE actual function return contract",
            (function, callable),
        )?;
    }
    Ok(())
}

fn callers(
    f: &Input,
    a: &Anchors,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
) -> Result<(), Failure> {
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
        let target = symbol(
            names,
            anchor(a, scan.target)?.name_span,
            SymbolKind::Function,
        )?;
        let call = typed
            .call(scan.expression)
            .ok_or_else(|| failure("temporary exits FE actual scanner call", scan))?;
        need(
            scan.arguments.is_empty()
                && call.expression() == scan.expression
                && call.target() == CallableTarget::Source(target)
                && call.receiver().is_none()
                && call.arguments().is_empty()
                && call.result_source() == CallableResultSource::Owned
                && typed.types().get(call.return_type())
                    == Some(&TypeKind::Builtin(BuiltinType::Int))
                && typed.expression_type(scan.expression) == Some(call.return_type())
                && !owned
                    .loans()
                    .iter()
                    .any(|loan| loan.call() == scan.expression)
                && !owned
                    .loan_ends()
                    .iter()
                    .any(|end| end.call() == scan.expression),
            "temporary exits FE zeroarg scanner exact Int Call result",
            (scan, call),
        )?;
        callee(f, names, scan.expression, target, Namespace::Value)?;
        let local = symbol(names, check.marker, SymbolKind::Variable)?;
        need(
            check.expected == if index == 0 { "7" } else { "0" }
                && check.initializer == scan.expression
                && reference(f, names, check.reference)? == local
                && owned.formed_owned_locals().contains(&local)
                && typed.symbol_type(local) == Some(call.return_type())
                && typed.expression_type(check.reference) == Some(call.return_type()),
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
