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
        BuiltinType, CallableResultSource, CallableTarget, ExpressionCategory, IntrinsicCallable,
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
        "nested exits FE unique actual single symbol",
    )?;
    need(
        symbol.kind() == kind
            && symbol.namespace()
                == if kind == SymbolKind::Classifier {
                    Namespace::Type
                } else {
                    Namespace::Value
                },
        "nested exits FE actual symbol kind",
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
        "nested exits FE direct Name operand",
        value,
    )?;
    let reference = one(
        names.references().iter().filter(|r| r.span() == span),
        "nested exits FE unique actual single reference",
    )?;
    let ReferenceTarget::Symbol(id) = reference.target() else {
        return Err(failure(
            "nested exits FE actual Name symbol target",
            reference,
        ));
    };
    need(
        reference.namespace() == Namespace::Value,
        "nested exits FE ordinary reference Value namespace",
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
        "nested exits FE actual single callee reference",
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
        "nested exits FE selected target belongs to actual callee reference",
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
            && owned.map_iterations().len() == 2
            && typed.map_sizes().len() == a.sizes.len(),
        "nested exits FE single same source and exact plan/size coverage",
        (
            typed.map_iterations(),
            owned.map_iterations(),
            typed.map_sizes(),
        ),
    )?;
    signatures(f, a, names, typed, owned)?;
    let parameter = a
        .parameter()
        .ok_or_else(|| failure("nested exits FE scan parameter", a))?;
    let root = symbol(names, parameter, SymbolKind::ValueParameter)?;
    let mut bindings = Vec::new();
    let mut component_loans = Vec::new();
    for site in &a.sites {
        let d = typed
            .map_iteration(site.statement)
            .ok_or_else(|| failure("nested exits FE actual typed For", site))?;
        let p = owned
            .map_iteration(site.statement)
            .ok_or_else(|| failure("nested exits FE actual owned For", site))?;
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
                && typed.symbol_type(root) == Some(d.source_type())
                && typed.sequential_iteration(site.statement).is_none()
                && owned.iteration(site.statement).is_none()
                && matches!(typed.types().get(d.source_type()), Some(TypeKind::Intrinsic {
                    constructor: actual, arguments
                }) if *actual == constructor && d.kind() == kind
                    && arguments.as_slice() == [d.key_type(), d.value_type()])
                && [d.key_type(), d.value_type()].iter().all(|ty| {
                    typed.types().get(*ty) == Some(&TypeKind::Builtin(BuiltinType::String))
                }),
            "nested exits FE exact typed descriptor and String K/V intrinsic",
            (site, d, p),
        )?;
        need(
            reference(f, names, site.source)? == root
                && owned.binding_kind(root) == Some(OwnershipBindingKind::Shared)
                && typed.expression_category(site.source) == Some(ExpressionCategory::Place)
                && p.source_access() == MapIterationSourceAccess::Shared
                && matches!(p.source_kind(), IterationSource::BorrowedSource(LoanTarget::Place(place))
                    if place.is_root() && place.root() == root)
                && matches!(p.source(), Some(LoanTarget::Place(place))
                    if place.is_root() && place.root() == root),
            "nested exits FE both real direct sources are same Borrow parameter root",
            (site, root, p),
        )?;
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
                    && Some(loan.origin()) == p.source(),
                "nested exits FE exact site-specific marker component and loan lineage",
                (site, binding, loan),
            )?;
            bindings.push(binding);
            component_loans.push(loan);
        }
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
                .all(|(i, l)| !component_loans[..i].contains(l))
            && !owned
                .drops()
                .iter()
                .any(|d| d.target() == DropTarget::Named(root)),
        "nested exits FE all component identities distinct and Borrow root has no Drop",
        (bindings, component_loans, root),
    )?;
    let guards = query_guards::single(f, a, names, typed, owned)?;
    query_cleanup::single(f, a, guards, owned)?;
    for read in &a.reads {
        let roots = read
            .binding_markers
            .iter()
            .map(|marker| {
                let kind = if a.sites.iter().any(|site| site.bindings.contains(marker)) {
                    SymbolKind::ForBinding
                } else {
                    SymbolKind::ValueParameter
                };
                symbol(names, *marker, kind)
            })
            .collect::<Result<Vec<_>, _>>()?;
        borrow_call(f, a, &read.call, names, (typed, owned), &roots)?;
    }
    if let Some(call) = &a.operand {
        let roots = a
            .sites
            .iter()
            .flat_map(|site| site.bindings)
            .map(|marker| symbol(names, marker, SymbolKind::ForBinding))
            .collect::<Result<Vec<_>, _>>()?;
        borrow_call(f, a, call, names, (typed, owned), &roots)?;
    }
    let mut locals = Vec::new();
    let mut owners = Vec::new();
    let mut drops = Vec::new();
    for (index, call) in a.scans.iter().enumerate() {
        let (marker, initializer) = a.locals[index];
        let local = symbol(names, marker, SymbolKind::Variable)?;
        let ty = typed
            .expression_type(initializer)
            .ok_or_else(|| failure("nested exits FE caller initializer type", initializer))?;
        // formed 只证历史；活性由真实 Borrow/size/Drop，再由 SSA use 证明。
        need(
            owned.formed_owned_locals().contains(&local)
                && typed.symbol_type(local) == Some(ty)
                && Some(ty) == typed.expression_type(a.sites[0].source)
                && typed.expression_category(initializer) == Some(ExpressionCategory::Temporary),
            "nested exits FE actual caller local owned formation and source type",
            (local, initializer, ty),
        )?;
        borrow_call(f, a, call, names, (typed, owned), &[local])?;
        let size = one(
            a.sizes.iter().filter(|s| s.root_marker == marker),
            "nested exits FE caller actual unique size",
        )?;
        let (drop, owner) =
            query_cleanup::single_caller(f, local, marker, initializer, size, owned)?;
        locals.push(local);
        owners.push(owner);
        drops.push(drop);
    }
    need(
        locals.len() == 2 && locals[0] != locals[1] && owners.len() == 2 && owners[0] != owners[1],
        "nested exits FE two caller locals and initializer owner IDs stay distinct",
        (&locals, &owners),
    )?;
    factory(f, a, names, typed)?;
    for size in &a.sizes {
        let kind = if size.root_marker == parameter {
            SymbolKind::ValueParameter
        } else {
            SymbolKind::Variable
        };
        let expected_root = symbol(names, size.root_marker, kind)?;
        let descriptor = typed
            .map_size(size.expression)
            .ok_or_else(|| failure("nested exits FE actual size descriptor", size))?;
        need(
            size.span.source_id() == f.parsed.source_id()
                && reference(f, names, size.receiver)? == expected_root
                && descriptor.expression() == size.expression
                && descriptor.receiver() == size.receiver
                && Some(descriptor.map_type()) == typed.expression_type(size.receiver)
                && typed.symbol_type(expected_root) == Some(descriptor.map_type())
                && Some(descriptor.map_type()) == typed.expression_type(a.sites[0].source)
                && typed.expression_type(size.expression).is_some_and(|ty| {
                    typed.types().get(ty) == Some(&TypeKind::Builtin(BuiltinType::Int))
                }),
            "nested exits FE each size receiver retains exact original root",
            (size, expected_root, descriptor),
        )?;
    }
    let drops: [Span; 2] = drops
        .try_into()
        .map_err(|v| failure("nested exits FE two actual caller Drops", v))?;
    return_checks(f, a, names, typed)?;
    spans(f, a, drops, false)
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
        "nested exits FE actual callee callable",
    )?;
    let call = typed
        .call(site.expression)
        .ok_or_else(|| failure("nested exits FE actual read/scan typed call", site))?;
    need(
        call.expression() == site.expression
            && call.target() == CallableTarget::Source(target)
            && call.receiver().is_none()
            && call.arguments().len() == roots.len()
            && site.arguments.len() == roots.len()
            && signature.parameters().len() == roots.len()
            && call.return_type() == signature.return_type()
            && call.result_source() == CallableResultSource::Owned
            && typed.types().get(call.return_type())
                == Some(&TypeKind::Builtin(expected_result(f, site.target))),
        "nested exits FE actual read/scan target signature and exact arity",
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
        "nested exits FE exact argument loan and end coverage",
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
            "nested exits FE argument parameter index and Borrow root are exact",
            (site, index, actual, parameter, root),
        )?;
        let loan = one(
            loans.iter().copied().filter(|l| l.argument() == *argument),
            "nested exits FE unique actual argument loan",
        )?;
        let end = one(
            ends.iter().copied().filter(|e| e.argument() == *argument),
            "nested exits FE unique actual argument loan end",
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
            "nested exits FE exact loan lineage span and synchronous CallReturn end",
            (loan, end),
        )?;
    }
    Ok(())
}

fn factory(
    f: &Input,
    a: &Anchors,
    names: &NameResolution,
    typed: &TypedFile,
) -> Result<(), Failure> {
    let id = symbol(
        names,
        anchor(a, a.factory.target)?.name_span,
        SymbolKind::Function,
    )?;
    let call = typed
        .call(a.factory.expression)
        .ok_or_else(|| failure("nested exits FE actual owned factory Call", &a.factory))?;
    need(
        a.locals[0].1 == a.factory.expression
            && call.expression() == a.factory.expression
            && call.target() == CallableTarget::Source(id)
            && call.receiver().is_none()
            && call.arguments().is_empty()
            && call.result_source() == CallableResultSource::Owned
            && Some(call.return_type()) == typed.expression_type(a.sites[0].source),
        "nested exits FE first owner is actual zero-argument owned factory result",
        call,
    )?;
    callee(f, names, a.factory.expression, id, Namespace::Value)?;
    let empty = typed
        .map_construction(a.locals[1].1)
        .ok_or_else(|| failure("nested exits FE actual empty map initializer", a.locals[1]))?;
    let (expression, _) = f.expression(a.locals[1].1)?;
    need(
        matches!(expression, Expression::Call { arguments, .. } if arguments.is_empty())
            && empty.expression() == a.locals[1].1
            && empty.callable()
                == if f.map {
                    IntrinsicCallable::MapOf
                } else {
                    IntrinsicCallable::MutableMapOf
                }
            && Some(empty.map_type()) == typed.expression_type(a.sites[0].source),
        "nested exits FE second owner is actual empty intrinsic construction",
        empty,
    )
}

fn signatures(
    f: &Input,
    a: &Anchors,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
) -> Result<(), Failure> {
    for function in &a.functions {
        let id = symbol(names, function.name_span, SymbolKind::Function)?;
        let source = one(
            names.symbols().iter().filter(|s| s.id() == id),
            "nested exits FE function source symbol",
        )?;
        let callable = one(
            typed.callables().iter().filter(|c| c.symbol() == id),
            "nested exits FE actual unique function callable",
        )?;
        need(
            source.name() == function.name
                && callable.receiver().is_none()
                && callable.type_parameters().is_empty()
                && callable.result_source() == CallableResultSource::Owned
                && callable.parameters().len() == function.parameters.len()
                && callable.parameter_symbols().len() == function.parameters.len(),
            "nested exits FE actual function name and unmarked signature shape",
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
                    && if function.name == "scan" {
                        Some(parameter.ty) == source_type
                    } else {
                        typed.types().get(parameter.ty)
                            == Some(&TypeKind::Builtin(BuiltinType::String))
                    },
                "nested exits FE actual ordered Borrow parameter identity",
                (function, i, parameter, id),
            )?;
        }
        need(
            if function.name == a.factory.target {
                Some(callable.return_type()) == source_type
            } else {
                typed.types().get(callable.return_type())
                    == Some(&TypeKind::Builtin(expected_result(f, function.name)))
            },
            "nested exits FE actual function return contract",
            (function, callable),
        )?;
    }
    Ok(())
}

fn return_checks(
    f: &Input,
    a: &Anchors,
    names: &NameResolution,
    typed: &TypedFile,
) -> Result<(), Failure> {
    need(
        a.return_checks.len()
            == if f.exit == super::super::ast::Exit::Return {
                2
            } else {
                0
            },
        "nested exits FE exact result-reference coverage before indexing",
        &a.return_checks,
    )?;
    for (index, check) in a.return_checks.iter().enumerate() {
        let root = symbol(names, check.marker, SymbolKind::Variable)?;
        need(
            check.expected == if index == 0 { "7" } else { "0" }
                && check.initializer == a.scans[index].expression
                && reference(f, names, check.reference)? == root
                && typed.symbol_type(root) == typed.expression_type(check.initializer)
                && typed.expression_type(check.reference)
                    == typed.expression_type(check.initializer)
                && typed.expression_type(check.initializer).is_some_and(|ty| {
                    typed.types().get(ty) == Some(&TypeKind::Builtin(BuiltinType::Int))
                }),
            "nested exits FE actual result initializer/reference Int identity",
            (check, root),
        )?;
    }
    Ok(())
}
