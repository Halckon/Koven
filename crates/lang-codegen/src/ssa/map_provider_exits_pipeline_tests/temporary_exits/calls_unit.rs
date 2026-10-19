//! source-qualified factory/scanner/consumer 与真实 Return caller 链；不手造 local ID。
use super::{
    super::Report,
    ast::{Anchors, Call, Input},
    unit::{binding_at, declaration, reference, symbol},
};
use lang_frontend::{
    ast::ExpressionId,
    name_resolution::{
        DeclarationId, Namespace, SourceUnitId, SymbolKind, UnitReferenceTarget, UnitSymbolId,
        ValidatedCompilationUnitNames,
    },
    ownership_checking::{
        CompilationUnitOwnership, LoanKind, OwnershipBindingKind, UnitCallArgumentOwnershipKind,
        UnitLoanTarget,
    },
    parser::Expression,
    type_checking::{
        BuiltinType, CallableResultSource, CompilationUnitTypes, ParameterMode, UnitCallTarget,
        UnitCallableTarget, UnitExpressionId, UnitTypeKind,
    },
};

pub(super) fn callee(
    f: &Input,
    names: &ValidatedCompilationUnitNames,
    expression: ExpressionId,
    selected: DeclarationId,
    namespace: Namespace,
) -> bool {
    let [source] = names.names().source_units() else {
        return false;
    };
    let Ok((Expression::Call { callee, .. }, _)) = f.expression(expression) else {
        return false;
    };
    let Ok((Expression::Name, span)) = f.expression(*callee) else {
        return false;
    };
    let references = names
        .names()
        .references()
        .iter()
        .filter(|r| r.span() == span)
        .collect::<Vec<_>>();
    matches!(references.as_slice(), [reference] if reference.source_unit() == source.source_unit()
    && reference.namespace() == Some(namespace) && match reference.target() {
        UnitReferenceTarget::Declaration(id) => *id == selected,
        UnitReferenceTarget::OverloadSet(ids) => ids.contains(&selected),
        _ => false,
    })
}

pub(super) fn observe(
    f: &Input,
    a: &Anchors,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    report: &mut Report,
) {
    let [source] = names.names().source_units() else {
        report.check(
            "unit temporary exits callable unique source",
            false,
            names.names().source_units(),
        );
        return;
    };
    let unit = source.source_unit();
    let expression = |id| UnitExpressionId::new(unit, id);
    signatures(a, names, typed, owned, unit, report);
    for site in &a.sites {
        let factory = a
            .function(site.factory)
            .and_then(|function| declaration(names, function, unit));
        let call = typed.call(expression(site.source));
        report.check("unit temporary exits real qualified factory Call, selected callee and Owned result",
            factory.is_some_and(|id| callee(f, names, site.source, id, Namespace::Value))
                && call.is_some_and(|call| call.expression() == expression(site.source)
                    && matches!(call.target(), UnitCallTarget::Declaration(id) if Some(id) == factory)
                    && call.receiver().is_none() && call.arguments().is_empty()
                    && call.result_source() == CallableResultSource::Owned
                    && Some(call.return_type()) == typed.expression_type(expression(site.source))), (site, factory, call));
        let consumer = Call {
            expression: site.read,
            span: site.read_span,
            target: site.consumer,
            arguments: site.arguments.to_vec(),
        };
        let roots = site
            .bindings
            .iter()
            .map(|span| symbol(names, unit, *span))
            .collect::<Option<Vec<_>>>();
        report.check(
            "unit temporary exits real qualified consumer component roots",
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
            .map(|span| symbol(names, unit, *span))
            .collect::<Option<Vec<_>>>();
        report.check(
            "unit temporary exits operand to read qualified parameter roots",
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
            .and_then(|function| declaration(names, function, unit));
        let call = typed.call(expression(scan.expression));
        report.check("unit temporary exits real qualified zero-argument scanner and Int result", target.is_some_and(|id| callee(f, names, scan.expression, id, Namespace::Value))
            && f.expression(scan.expression).is_ok_and(|(_, span)| span == scan.span) && scan.arguments.is_empty() && call.is_some_and(|call| call.expression() == expression(scan.expression)
                && matches!(call.target(), UnitCallTarget::Declaration(id) if Some(id) == target)
                && call.receiver().is_none() && call.arguments().is_empty()
                && call.result_source() == CallableResultSource::Owned
                && typed.types().get(call.return_type()) == Some(&UnitTypeKind::Builtin(BuiltinType::Int))
                && typed.expression_type(expression(scan.expression)) == Some(call.return_type()))
            && !owned.loans().iter().any(|loan| loan.call() == expression(scan.expression))
            && owned.call_argument_contracts_for(expression(scan.expression)).next().is_none()
            && !owned.value_deliveries().iter().any(|v| v.call() == expression(scan.expression)), (scan, target, call));
    }
    for (index, check) in a.return_checks.iter().enumerate() {
        let root = symbol(names, unit, check.marker);
        let initializer = UnitExpressionId::new(unit, check.initializer);
        let reference_id = UnitExpressionId::new(unit, check.reference);
        report.check(
            "unit temporary exits caller exact 7/0 result reference to actual scan initializer",
            root.is_some_and(|id| {
                id.source_unit() == unit
                    && binding_at(names, id, check.marker, SymbolKind::Variable)
            }) && check.expected == if index == 0 { "7" } else { "0" }
                && reference(f, names, check.reference) == root
                && a.scans
                    .iter()
                    .any(|call| call.expression == check.initializer)
                && typed.call(initializer).is_some_and(|call| {
                    call.expression() == initializer
                        && typed.types().get(call.return_type())
                            == Some(&UnitTypeKind::Builtin(BuiltinType::Int))
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
    names: &ValidatedCompilationUnitNames,
    products: (&CompilationUnitTypes, &CompilationUnitOwnership),
    roots: &[UnitSymbolId],
    report: &mut Report,
) {
    let (typed, owned) = products;
    let Some(first) = roots.first() else {
        report.check("unit temporary exits nonempty Borrow roots", false, roots);
        return;
    };
    let unit = first.source_unit();
    let expression = |id| UnitExpressionId::new(unit, id);
    let call_id = expression(site.expression);
    let target = a
        .function(site.target)
        .and_then(|function| declaration(names, function, unit));
    let call = typed.call(call_id);
    report.check(
        "unit temporary exits actual read/scan declaration and ordered Borrow references",
        target.is_some()
            && f.expression(site.expression)
                .is_ok_and(|(_, span)| span == site.span)
            && site.arguments.len() == roots.len() && call.is_some_and(|call| {
            call.expression() == call_id
                && matches!(call.target(), UnitCallTarget::Declaration(id) if Some(id) == target)
                && target.is_some_and(|id| callee(f, names, site.expression, id, Namespace::Value))
                && call.receiver().is_none()
                && call.result_source() == CallableResultSource::Owned
                && typed.expression_type(call_id) == Some(call.return_type())
                && call.arguments().len() == roots.len()
                && typed.types().get(call.return_type())
                    == Some(&UnitTypeKind::Builtin(expected_result(site.target)))
                && call.arguments().iter().enumerate().all(|(i, arg)| {
                    arg.argument_index() == i
                        && arg.parameter_index() == i
                        && arg.mode() == ParameterMode::Borrow
                        && roots[i].source_unit() == unit
                        && typed.expression_type(expression(site.arguments[i]))
                            == Some(arg.parameter_type())
                        && typed.symbol_type(roots[i]) == Some(arg.parameter_type())
                        && reference(f, names, site.arguments[i]) == Some(roots[i])
                })
        }),
        (site, roots, target, call),
    );
    let contracts = owned
        .call_argument_contracts_for(call_id)
        .collect::<Vec<_>>();
    report.check(
        "unit temporary exits read/scan exact qualified SharedLoan contracts",
        contracts.len() == roots.len()
            && site
                .arguments
                .iter()
                .zip(roots)
                .enumerate()
                .all(|(i, (argument, root))| {
                    contracts
                        .iter()
                        .filter(|c| {
                            c.call() == call_id
                                && c.argument() == expression(*argument)
                                && c.parameter_index() == i
                                && typed.symbol_type(*root) == Some(c.parameter_type())
                                && c.kind() == UnitCallArgumentOwnershipKind::SharedLoan
                        })
                        .count()
                        == 1
                })
            && !owned
                .value_deliveries()
                .iter()
                .any(|delivery| delivery.call() == call_id),
        (site, &contracts),
    );
    let loans = owned
        .loans()
        .iter()
        .filter(|loan| loan.call() == call_id)
        .collect::<Vec<_>>();
    report.check("unit temporary exits read/scan loans retain qualified root and argument identity", loans.len() == roots.len()
        && site.arguments.iter().zip(roots).all(|(argument, root)| loans.iter().filter(|loan|
            loan.argument() == expression(*argument) && loan.kind() == LoanKind::Shared
                && matches!(loan.target(), UnitLoanTarget::Place(place) if place.is_root() && place.root() == *root)).count() == 1), (site, roots, loans));
}

fn signatures(
    a: &Anchors,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    unit: SourceUnitId,
    report: &mut Report,
) {
    for function in &a.functions {
        let id = declaration(names, function, unit);
        let signatures = typed
            .signatures()
            .declarations()
            .iter()
            .filter(|s| Some(s.declaration()) == id)
            .filter_map(|s| s.callable())
            .collect::<Vec<_>>();
        report.check("unit temporary exits actual function declaration/callable", id.is_some()
            && matches!(signatures.as_slice(), [signature] if signature.name_span() == function.name_span
                && matches!(signature.target(), UnitCallableTarget::Declaration(target) if Some(target) == id)), (function, id, &signatures));
        let [signature] = signatures.as_slice() else {
            continue;
        };
        let source_type = typed.expression_type(UnitExpressionId::new(unit, a.sites[0].source));
        report.check(
            "unit temporary exits exact unmarked Borrow signatures",
            signature.receiver().is_none()
                && signature.type_parameters().is_empty()
                && signature.has_body()
                && signature.result_source() == CallableResultSource::Owned
                && signature.parameters().len() == function.parameters.len()
                && signature
                    .parameters()
                    .iter()
                    .enumerate()
                    .all(|(i, parameter)| {
                        parameter.mode() == ParameterMode::Borrow
                            && parameter.symbol() == symbol(names, unit, function.parameters[i])
                            && parameter.symbol().is_some_and(|id| {
                                owned.binding_kind(id) == Some(OwnershipBindingKind::Shared)
                            })
                            && typed.types().get(parameter.ty())
                                == Some(&UnitTypeKind::Builtin(BuiltinType::String))
                    })
                && if a.sites.iter().any(|site| site.factory == function.name) {
                    Some(signature.return_type()) == source_type
                } else {
                    typed.types().get(signature.return_type())
                        == Some(&UnitTypeKind::Builtin(expected_result(function.name)))
                },
            (function, signature),
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
