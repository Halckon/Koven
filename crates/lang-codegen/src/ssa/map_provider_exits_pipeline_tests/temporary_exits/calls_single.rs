//! 实际函数签名、零参 factory/scanner、两 Borrow consumer 与 Return caller 链。
use super::{
    super::Report,
    ast::{Anchors, Call, Input},
    single::{reference, symbol},
};
use lang_frontend::{
    ast::ExpressionId,
    name_resolution::{NameResolution, Namespace, ReferenceTarget, SymbolId, SymbolKind},
    ownership_checking::{LoanKind, LoanTarget, OwnershipBindingKind, OwnershipCheckedFile},
    parser::Expression,
    type_checking::{
        BuiltinType, CallableResultSource, CallableTarget, ParameterMode, TypeKind, TypedFile,
    },
};

pub(super) fn callee(
    f: &Input,
    names: &NameResolution,
    expression: ExpressionId,
    selected: SymbolId,
    namespace: Namespace,
) -> bool {
    let Ok((Expression::Call { callee, .. }, _)) = f.expression(expression) else {
        return false;
    };
    let Ok((Expression::Name, span)) = f.expression(*callee) else {
        return false;
    };
    let references = names
        .references()
        .iter()
        .filter(|r| r.span() == span)
        .collect::<Vec<_>>();
    matches!(references.as_slice(), [reference] if reference.namespace() == namespace && match reference.target() {
        ReferenceTarget::Symbol(id) => *id == selected,
        ReferenceTarget::OverloadSet(ids) => ids.contains(&selected),
        _ => false,
    })
}

pub(super) fn observe(
    f: &Input,
    a: &Anchors,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
    report: &mut Report,
) {
    signatures(a, names, typed, owned, report);
    for site in &a.sites {
        let factory = a
            .function(site.factory)
            .and_then(|function| symbol(names, function.name_span));
        let call = typed.call(site.source);
        report.check("single temporary exits real factory Call, selected callee and Owned result",
            factory.is_some_and(|id| callee(f, names, site.source, id, Namespace::Value))
                && call.is_some_and(|call| call.expression() == site.source
                    && matches!(call.target(), CallableTarget::Source(id) if Some(id) == factory)
                    && call.receiver().is_none() && call.arguments().is_empty()
                    && call.result_source() == CallableResultSource::Owned
                    && Some(call.return_type()) == typed.expression_type(site.source)), (site, factory, call));
        let consumer = Call {
            expression: site.read,
            span: site.read_span,
            target: site.consumer,
            arguments: site.arguments.to_vec(),
        };
        let roots = site
            .bindings
            .iter()
            .map(|span| symbol(names, *span))
            .collect::<Option<Vec<_>>>();
        report.check(
            "single temporary exits real consumer component roots",
            roots.is_some(),
            (&consumer, &roots),
        );
        if let Some(roots) = roots {
            borrow_call(f, a, &consumer, names, (typed, owned), &roots, report);
        }
    }
    if let Some(read) = &a.operand {
        let roots = read
            .binding_markers
            .iter()
            .map(|span| symbol(names, *span))
            .collect::<Option<Vec<_>>>();
        report.check(
            "single temporary exits operand to read actual parameter roots",
            roots.is_some(),
            (read, &roots),
        );
        if let Some(roots) = roots {
            borrow_call(f, a, &read.call, names, (typed, owned), &roots, report);
        }
    }
    for scan in &a.scans {
        let target = a
            .function(scan.target)
            .and_then(|function| symbol(names, function.name_span));
        let call = typed.call(scan.expression);
        report.check(
            "single temporary exits real zero-argument scanner and Int result",
            target.is_some_and(|id| callee(f, names, scan.expression, id, Namespace::Value))
                && f.expression(scan.expression)
                    .is_ok_and(|(_, span)| span == scan.span)
                && scan.arguments.is_empty()
                && call.is_some_and(|call| {
                    call.expression() == scan.expression
                        && matches!(call.target(), CallableTarget::Source(id) if Some(id) == target)
                        && call.receiver().is_none()
                        && call.arguments().is_empty()
                        && call.result_source() == CallableResultSource::Owned
                        && typed.types().get(call.return_type())
                            == Some(&TypeKind::Builtin(BuiltinType::Int))
                        && typed.expression_type(scan.expression) == Some(call.return_type())
                })
                && !owned
                    .loans()
                    .iter()
                    .any(|loan| loan.call() == scan.expression),
            (scan, target, call),
        );
    }
    for (index, check) in a.return_checks.iter().enumerate() {
        let root = symbol(names, check.marker);
        let initializer = check.initializer;
        let reference_id = check.reference;
        report.check(
            "single temporary exits caller exact 7/0 result reference to actual scan initializer",
            root.is_some_and(|id| {
                names.symbols().iter().any(|s| {
                    s.id() == id
                        && s.kind() == SymbolKind::Variable
                        && s.span() == check.marker
                        && !s.is_synthetic()
                })
            }) && check.expected == if index == 0 { "7" } else { "0" }
                && reference(f, names, check.reference) == root
                && a.scans
                    .iter()
                    .any(|call| call.expression == check.initializer)
                && typed.call(initializer).is_some_and(|call| {
                    call.expression() == initializer
                        && typed.types().get(call.return_type())
                            == Some(&TypeKind::Builtin(BuiltinType::Int))
                })
                && root
                    .is_some_and(|id| typed.symbol_type(id) == typed.expression_type(initializer))
                && typed.expression_type(reference_id) == typed.expression_type(initializer),
            (check, root, typed.call(initializer)),
        );
    }
}

fn borrow_call(
    f: &Input,
    a: &Anchors,
    site: &Call,
    names: &NameResolution,
    products: (&TypedFile, &OwnershipCheckedFile),
    roots: &[SymbolId],
    report: &mut Report,
) {
    let (typed, owned) = products;
    let target = a
        .function(site.target)
        .and_then(|function| symbol(names, function.name_span));
    let call = typed.call(site.expression);
    report.check(
        "single temporary exits actual read/scan call target and ordered Borrow references",
        target.is_some()
            && f.expression(site.expression)
                .is_ok_and(|(_, span)| span == site.span)
            && site.arguments.len() == roots.len()
            && call.is_some_and(|call| {
                call.expression() == site.expression
                    && matches!(call.target(), CallableTarget::Source(id) if Some(id) == target)
                    && target
                        .is_some_and(|id| callee(f, names, site.expression, id, Namespace::Value))
                    && call.receiver().is_none()
                    && call.result_source() == CallableResultSource::Owned
                    && typed.expression_type(site.expression) == Some(call.return_type())
                    && call.arguments().len() == roots.len()
                    && typed.types().get(call.return_type())
                        == Some(&TypeKind::Builtin(expected_result(site.target)))
                    && call.arguments().iter().enumerate().all(|(i, arg)| {
                        arg.argument_index() == i
                            && arg.parameter_index() == i
                            && arg.mode() == ParameterMode::Borrow
                            && typed.expression_type(site.arguments[i])
                                == Some(arg.parameter_type())
                            && typed.symbol_type(roots[i]) == Some(arg.parameter_type())
                            && reference(f, names, site.arguments[i]) == Some(roots[i])
                    })
            }),
        (site, roots, target, call),
    );
    let loans = owned
        .loans()
        .iter()
        .filter(|loan| loan.call() == site.expression)
        .collect::<Vec<_>>();
    report.check("single temporary exits read/scan loans retain exact root and argument identity", loans.len() == roots.len()
        && site.arguments.iter().zip(roots).all(|(argument, root)| loans.iter().filter(|loan|
            loan.argument() == *argument && loan.kind() == LoanKind::Shared
                && matches!(loan.target(), LoanTarget::Place(place) if place.is_root() && place.root() == *root)).count() == 1), (site, roots, loans));
}

fn signatures(
    a: &Anchors,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
    report: &mut Report,
) {
    for function in &a.functions {
        let id = symbol(names, function.name_span);
        let source = id.and_then(|id| names.symbols().iter().find(|s| s.id() == id));
        let callables = typed
            .callables()
            .iter()
            .filter(|c| Some(c.symbol()) == id)
            .collect::<Vec<_>>();
        report.check(
            "single temporary exits actual function symbol/callable",
            source.is_some_and(|s| s.kind() == SymbolKind::Function && s.name() == function.name)
                && matches!(callables.as_slice(), [_]),
            (function, source, &callables),
        );
        let [callable] = callables.as_slice() else {
            continue;
        };
        let source_type = typed.expression_type(a.sites[0].source);
        report.check(
            "single temporary exits exact unmarked Borrow signatures",
            callable.receiver().is_none()
                && callable.type_parameters().is_empty()
                && callable.result_source() == CallableResultSource::Owned
                && callable.parameters().len() == function.parameters.len()
                && callable.parameter_symbols().len() == function.parameters.len()
                && callable
                    .parameters()
                    .iter()
                    .enumerate()
                    .all(|(i, parameter)| {
                        parameter.mode == ParameterMode::Borrow
                            && callable.parameter_symbols()[i]
                                == symbol(names, function.parameters[i])
                            && callable.parameter_symbols()[i].is_some_and(|id| {
                                owned.binding_kind(id) == Some(OwnershipBindingKind::Shared)
                            })
                            && typed.types().get(parameter.ty)
                                == Some(&TypeKind::Builtin(BuiltinType::String))
                    })
                && if a.sites.iter().any(|site| site.factory == function.name) {
                    Some(callable.return_type()) == source_type
                } else {
                    typed.types().get(callable.return_type())
                        == Some(&TypeKind::Builtin(expected_result(function.name)))
                },
            (function, callable),
        );
    }
}

fn expected_result(name: &str) -> BuiltinType {
    if matches!(name, "operand" | "scanNonempty" | "scanEmpty") {
        BuiltinType::Int
    } else {
        BuiltinType::Unit
    }
}
