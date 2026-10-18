//! 同次 single 首证身份查询；复杂出口事实保全，不套用正常双层出口拓扑。
use super::{
    super::Report,
    ast::{Anchors, Call, Exit, Input},
};
use lang_frontend::{
    ast::ExpressionId,
    name_resolution::{NameResolution, ReferenceTarget, SymbolId, SymbolKind},
    ownership_checking::{
        DropTarget, IterationSource, LoanKind, LoanTarget, MapIterationComponent,
        MapIterationSourceAccess, OwnershipBindingKind, OwnershipCheckedFile,
    },
    source::Span,
    type_checking::{
        BuiltinType, CallableResultSource, CallableTarget, ExpressionCategory, IntrinsicCallable,
        IntrinsicTypeConstructor, MapIterationKind, ParameterMode, TypeKind, TypedFile,
    },
};

pub(super) fn symbol(names: &NameResolution, span: Span) -> Option<SymbolId> {
    let found = names
        .symbols()
        .iter()
        .filter(|s| s.span() == span && !s.is_synthetic())
        .collect::<Vec<_>>();
    match found.as_slice() {
        [s] => Some(s.id()),
        _ => None,
    }
}

fn reference(f: &Input, names: &NameResolution, expression: ExpressionId) -> Option<SymbolId> {
    let (_, span) = f.expression(expression).ok()?;
    let found = names
        .references()
        .iter()
        .filter(|r| r.span() == span)
        .collect::<Vec<_>>();
    match found.as_slice() {
        [r] => match r.target() {
            ReferenceTarget::Symbol(id) => Some(*id),
            _ => None,
        },
        _ => None,
    }
}

pub(super) fn observe(
    f: &Input,
    anchors: Option<&Anchors>,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
    report: &mut Report,
) {
    // 完整真实 ownership Debug 在可选查询前保全，包含 owner/exit/cleanup/condition/loan。
    // 不预填任何清理序列，不把相同 source root 当作相同 provider lease。
    println!(
        "SPEC0331 NESTED EXITS SINGLE REAL source={:?} typed_maps={:#?} sizes={:#?} ownership={owned:#?}",
        typed.source_id(),
        typed.map_iterations(),
        typed.map_sizes()
    );
    let mut points = Vec::new();
    for plan in owned.map_iterations() {
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
        let whole = owned
            .cleanup_steps()
            .iter()
            .filter(|(at, _)| *at == point)
            .collect::<Vec<_>>();
        println!(
            "SPEC0331 NESTED EXITS SINGLE WHOLE POINT point={point:?} plans={plans:?} sequence={whole:#?}"
        );
    }
    let Some(a) = anchors else { return };
    report.check(
        "single nested exits exact two typed/owned plans",
        typed.map_iterations().len() == 2 && owned.map_iterations().len() == 2,
        (typed.map_iterations(), owned.map_iterations()),
    );
    signatures(f, a, names, typed, owned, report);
    let root = a.parameter().and_then(|span| symbol(names, span));
    let mut bindings = Vec::new();
    let mut component_loans = Vec::new();
    for site in &a.sites {
        let (Some(d), Some(p)) = (
            typed.map_iteration(site.statement),
            owned.map_iteration(site.statement),
        ) else {
            report.check(
                "single nested exits actual site descriptor/plan",
                false,
                site,
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
        report.check("single nested exits actual For/source and intrinsic String K/V", d.statement() == site.statement
            && d.source() == site.source && p.descriptor() == d
            && typed.expression_type(site.source) == Some(d.source_type())
            && typed.sequential_iteration(site.statement).is_none() && owned.iteration(site.statement).is_none()
            && matches!(typed.types().get(d.source_type()), Some(TypeKind::Intrinsic { constructor: actual, arguments })
                if *actual == constructor && d.kind() == kind && arguments.as_slice() == [d.key_type(), d.value_type()])
            && [d.key_type(), d.value_type()].iter().all(|t| typed.types().get(*t) == Some(&TypeKind::Builtin(BuiltinType::String))),
            (site, d, p));
        let actual_root = reference(f, names, site.source);
        let root_symbol = actual_root.and_then(|id| names.symbols().iter().find(|s| s.id() == id));
        report.check("single nested exits same actual Borrow parameter root with Shared access", root.is_some() && actual_root == root
            && root_symbol.is_some_and(|s| s.kind() == SymbolKind::ValueParameter && !s.is_synthetic() && Some(s.span()) == a.parameter())
            && root.is_some_and(|id| owned.binding_kind(id) == Some(OwnershipBindingKind::Shared))
            && typed.expression_category(site.source) == Some(ExpressionCategory::Place)
            && p.source_access() == MapIterationSourceAccess::Shared
            && matches!(p.source_kind(), IterationSource::BorrowedSource(LoanTarget::Place(place)) if place.is_root() && Some(place.root()) == root)
            && matches!(p.source(), Some(LoanTarget::Place(place)) if place.is_root() && Some(place.root()) == root),
            (site.statement, site.source, root_symbol, p.source_kind(), p.source_access()));
        bindings.extend(d.bindings());
        component_loans.extend(p.loans().iter());
        for (i, component) in [MapIterationComponent::Key, MapIterationComponent::Value]
            .into_iter()
            .enumerate()
        {
            let binding = d.bindings()[i];
            let actual = names.symbols().iter().find(|s| s.id() == binding);
            let loan = p.loan(component);
            report.check(
                "single nested exits site-isolated marker/type/Shared component loan",
                actual.is_some_and(|s| {
                    s.span() == site.bindings[i]
                        && s.kind() == SymbolKind::ForBinding
                        && !s.is_synthetic()
                }) && typed.symbol_type(binding) == Some([d.key_type(), d.value_type()][i])
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
                (site.statement, actual, loan),
            );
        }
    }

    report.check(
        "single nested exits all four bindings and four component loan identities distinct",
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
        (bindings, component_loans),
    );
    report.check(
        "single nested exits borrowed scan parameter has no owner drop",
        root.is_some()
            && !owned
                .drops()
                .iter()
                .any(|d| matches!(d.target(), DropTarget::Named(id) if Some(id) == root)),
        (root, owned.drops()),
    );
    super::guards::single(a, names, typed, owned, report);
    for read in &a.reads {
        let roots = read
            .binding_markers
            .iter()
            .map(|span| symbol(names, *span))
            .collect::<Option<Vec<_>>>();
        report.check(
            "single nested exits actual read argument root markers",
            roots.is_some(),
            (&read.call, &roots),
        );
        if let Some(roots) = roots {
            borrow_call(f, a, &read.call, names, (typed, owned), &roots, report);
        }
    }
    if let Some(call) = &a.operand {
        let roots = a
            .sites
            .iter()
            .flat_map(|site| site.bindings)
            .map(|span| symbol(names, span))
            .collect::<Option<Vec<_>>>();
        report.check(
            "single nested exits return operand four actual component roots",
            roots.as_ref().is_some_and(|roots| roots.len() == 4),
            &roots,
        );
        if let Some(roots) = roots {
            borrow_call(f, a, call, names, (typed, owned), &roots, report);
        }
    }
    for (index, call) in a.scans.iter().enumerate() {
        let (marker, initializer) = a.locals[index];
        let local = symbol(names, marker);
        let initializer_type = typed.expression_type(initializer);
        let initializer_call = typed.call(initializer);
        let initializer_map = typed.map_construction(initializer);
        // AST 已核真实 Val/initializer；formed 只证明形成历史，不授予当前可用性。
        // 当前使用仍由下方真实 Borrow call/loan 与循环后的 size 引用独立核验。
        report.check(
            "single nested exits actual caller local formation and owned initializer",
            local.is_some_and(|id| {
                owned.formed_owned_locals().contains(&id)
                    && names.symbols().iter().any(|symbol| {
                        symbol.id() == id
                            && symbol.span() == marker
                            && symbol.kind() == SymbolKind::Variable
                            && !symbol.is_synthetic()
                    })
                    && initializer_type.is_some_and(|ty| {
                        typed.symbol_type(id) == Some(ty)
                            && (initializer_call.is_some_and(|call| {
                                call.expression() == initializer
                                    && call.return_type() == ty
                                    && call.result_source() == CallableResultSource::Owned
                            }) || initializer_map.is_some_and(|map| {
                                map.expression() == initializer && map.map_type() == ty
                            }))
                    })
            }),
            (
                marker,
                initializer,
                local,
                initializer_type,
                initializer_call,
                initializer_map,
                owned.formed_owned_locals(),
            ),
        );
        if let Some(local) = local {
            borrow_call(f, a, call, names, (typed, owned), &[local], report);
        }
    }
    let factory_symbol = a
        .function(a.factory.target)
        .and_then(|function| symbol(names, function.name_span));
    let factory = typed.call(a.factory.expression);
    report.check(
        "single nested exits actual zero-argument factory owned result",
        factory_symbol.is_some() && factory.is_some_and(|call| {
            call.expression() == a.factory.expression
                && matches!(call.target(), CallableTarget::Source(id) if Some(id) == factory_symbol)
                && call.receiver().is_none()
                && call.result_source() == CallableResultSource::Owned
                && call.arguments().is_empty()
                && Some(call.return_type()) == typed.expression_type(a.sites[0].source)
        }),
        (&a.factory, factory_symbol, factory),
    );
    let empty = typed.map_construction(a.locals[1].1);
    report.check(
        "single nested exits actual empty intrinsic owner",
        empty.is_some_and(|d| {
            d.expression() == a.locals[1].1
                && d.callable()
                    == if f.map {
                        IntrinsicCallable::MapOf
                    } else {
                        IntrinsicCallable::MutableMapOf
                    }
                && Some(d.map_type()) == typed.expression_type(a.sites[0].source)
        }),
        empty,
    );
    for size in &a.sizes {
        let expected_root = symbol(names, size.root_marker);
        let descriptor = typed.map_size(size.expression);
        report.check(
            "single nested exits size actual receiver and original root identity",
            expected_root.is_some()
                && size.span.source_id() == f.parsed.source_id()
                && reference(f, names, size.receiver) == expected_root
                && descriptor.is_some_and(|d| {
                    d.expression() == size.expression
                        && d.receiver() == size.receiver
                        && Some(d.map_type()) == typed.expression_type(size.receiver)
                        && typed.expression_type(size.expression).is_some_and(|t| {
                            typed.types().get(t) == Some(&TypeKind::Builtin(BuiltinType::Int))
                        })
                }),
            (size, expected_root, descriptor),
        );
    }
    for (index, check) in a.return_checks.iter().enumerate() {
        let root = symbol(names, check.marker);
        let initializer = check.initializer;
        let reference_id = check.reference;
        report.check(
            "single nested exits caller exact 7/0 result reference to actual scan initializer",
            root.is_some()
                && check.expected == if index == 0 { "7" } else { "0" }
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
        "single nested exits actual read/scan call target and ordered Borrow references",
        target.is_some()
            && site.arguments.len() == roots.len()
            && call.is_some_and(|call| {
                call.expression() == site.expression
                    && matches!(call.target(), CallableTarget::Source(id) if Some(id) == target)
                    && call.receiver().is_none()
                    && call.result_source() == CallableResultSource::Owned
                    && call.arguments().len() == roots.len()
                    && typed.types().get(call.return_type())
                        == Some(&TypeKind::Builtin(expected_result(f, site.target)))
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
    report.check("single nested exits read/scan loans retain exact root and argument identity", loans.len() == roots.len()
        && site.arguments.iter().zip(roots).all(|(argument, root)| loans.iter().filter(|loan|
            loan.argument() == *argument && loan.kind() == LoanKind::Shared
                && matches!(loan.target(), LoanTarget::Place(place) if place.is_root() && place.root() == *root)).count() == 1), (site, roots, loans));
}

fn signatures(
    f: &Input,
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
            "single nested exits actual function symbol/callable",
            source.is_some_and(|s| s.kind() == SymbolKind::Function && s.name() == function.name)
                && matches!(callables.as_slice(), [_]),
            (function, source, &callables),
        );
        let [callable] = callables.as_slice() else {
            continue;
        };
        let source_type = typed.expression_type(a.sites[0].source);
        report.check(
            "single nested exits exact unmarked Borrow signatures",
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
                            && if function.name == "scan" {
                                Some(parameter.ty) == source_type
                            } else {
                                typed.types().get(parameter.ty)
                                    == Some(&TypeKind::Builtin(BuiltinType::String))
                            }
                    })
                && if function.name == a.factory.target {
                    Some(callable.return_type()) == source_type
                } else {
                    typed.types().get(callable.return_type())
                        == Some(&TypeKind::Builtin(expected_result(f, function.name)))
                },
            (function, callable),
        );
    }
}

fn expected_result(f: &Input, name: &str) -> BuiltinType {
    if name == "operand" || (name == "scan" && f.exit == Exit::Return) {
        BuiltinType::Int
    } else {
        BuiltinType::Unit
    }
}
