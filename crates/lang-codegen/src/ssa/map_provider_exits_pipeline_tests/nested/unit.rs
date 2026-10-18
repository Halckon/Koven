//! 同次 unit 的完整 qualified 身份；Map Borrow 独立于已交付 MutableMap mode。
use super::{
    super::Report,
    ast::{Anchors, Call, Function, Input},
};
use lang_frontend::{
    ast::ExpressionId,
    name_resolution::{
        DeclarationId, SourceUnitId, SymbolKind, UnitReferenceTarget, UnitSymbolId,
        ValidatedCompilationUnitNames,
    },
    ownership_checking::{
        CompilationUnitOwnership, IterationSource, LoanKind, MapIterationComponent,
        OwnershipBindingKind, UnitCallArgumentOwnershipKind, UnitDropTarget,
        UnitIterationSourceAccess, UnitLoanTarget,
    },
    source::Span,
    type_checking::{
        BuiltinType, CallableResultSource, CompilationUnitTypes, ExpressionCategory,
        IntrinsicCallable, IntrinsicTypeConstructor, MapIterationKind, ParameterMode,
        UnitCallTarget, UnitCallableTarget, UnitExpressionId, UnitStatementId, UnitTypeKind,
    },
};

fn declaration(
    names: &ValidatedCompilationUnitNames,
    function: &Function,
    unit: SourceUnitId,
) -> Option<DeclarationId> {
    let found = names
        .names()
        .index()
        .declarations()
        .iter()
        .filter(|d| {
            d.source_unit() == unit
                && d.root() == function.item
                && d.name_span() == function.name_span
                && d.kind() == SymbolKind::Function
                && d.name() == function.name
        })
        .collect::<Vec<_>>();
    match found.as_slice() {
        [d] => Some(d.id()),
        _ => None,
    }
}

fn symbol(
    names: &ValidatedCompilationUnitNames,
    unit: SourceUnitId,
    span: Span,
) -> Option<UnitSymbolId> {
    // UnitSymbolId 构造器是 frontend 私有；只取本次真实引用中的 qualified ID，再核声明 marker。
    let mut found = Vec::new();
    for reference in names.names().references() {
        let UnitReferenceTarget::Symbol(id) = reference.target() else {
            continue;
        };
        let actual = id.source_unit() == unit
            && names
                .names()
                .source_units()
                .iter()
                .filter(|source| source.source_unit() == unit)
                .any(|source| {
                    source.resolution().symbols().iter().any(|symbol| {
                        symbol.id() == id.symbol()
                            && symbol.span() == span
                            && !symbol.is_synthetic()
                    })
                });
        if actual && !found.contains(id) {
            found.push(*id);
        }
    }
    match found.as_slice() {
        [id] => Some(*id),
        _ => None,
    }
}

fn binding_at(
    names: &ValidatedCompilationUnitNames,
    id: UnitSymbolId,
    span: Span,
    kind: SymbolKind,
) -> bool {
    names
        .names()
        .source_units()
        .iter()
        .filter(|s| s.source_unit() == id.source_unit())
        .any(|source| {
            source.resolution().symbols().iter().any(|s| {
                s.id() == id.symbol() && s.span() == span && s.kind() == kind && !s.is_synthetic()
            })
        })
}

fn reference(
    f: &Input,
    names: &ValidatedCompilationUnitNames,
    expression: ExpressionId,
) -> Option<UnitSymbolId> {
    let (_, span) = f.expression(expression).ok()?;
    let found = names
        .names()
        .references()
        .iter()
        .filter(|r| r.span() == span)
        .collect::<Vec<_>>();
    match found.as_slice() {
        [r] => match r.target() {
            UnitReferenceTarget::Symbol(id) => Some(*id),
            _ => None,
        },
        _ => None,
    }
}

pub(super) fn observe(
    f: &Input,
    anchors: Option<&Anchors>,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    report: &mut Report,
) {
    // Debug 完整保全实际 owner/exit/cleanup；查询只读公开 API，不编造 unit 私有表接口。
    println!(
        "SPEC0331 NESTED UNIT REAL typed_maps={:#?} sizes={:#?} ownership={owned:#?}",
        typed.map_iterations(),
        typed.map_sizes()
    );
    let Some(a) = anchors else { return };
    let [source_names] = names.names().source_units() else {
        report.check(
            "unit nested exactly one actual source",
            false,
            names.names().source_units(),
        );
        return;
    };
    let unit = source_names.source_unit();
    let expression = |id| UnitExpressionId::new(unit, id);
    report.check(
        "unit nested original source and exact two typed/owned plans",
        source_names.resolution().source_id() == f.parsed.source_id()
            && typed.map_iterations().len() == 2
            && owned.map_iterations().len() == 2,
        (unit, typed.map_iterations(), owned.map_iterations()),
    );
    signatures(a, names, typed, owned, unit, report);
    let root = a.parameter().and_then(|span| symbol(names, unit, span));
    let mut bindings = Vec::new();
    let mut component_loans = Vec::new();
    for site in &a.sites {
        let statement = UnitStatementId::new(unit, site.statement);
        let source = expression(site.source);
        let (Some(d), Some(p)) = (
            typed.map_iteration(statement),
            owned.map_iteration(statement),
        ) else {
            report.check(
                "unit nested actual qualified site descriptor/plan",
                false,
                (statement, source, site),
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
        report.check("unit nested actual qualified For/source and intrinsic String K/V", d.statement() == statement
            && d.source() == source && p.descriptor() == d && typed.expression_type(source) == Some(d.source_type())
            && typed.sequential_iteration(statement).is_none() && owned.iteration(statement).is_none()
            && matches!(typed.types().get(d.source_type()), Some(UnitTypeKind::Intrinsic { constructor: actual, arguments })
                if *actual == constructor && d.kind() == kind && arguments.as_slice() == [d.key_type(), d.value_type()])
            && [d.key_type(), d.value_type()].iter().all(|t| typed.types().get(*t) == Some(&UnitTypeKind::Builtin(BuiltinType::String))), (site, d, p));
        let actual_root = reference(f, names, site.source);
        report.check("unit nested same actual qualified Borrow parameter root and Shared access", root.is_some() && actual_root == root
            && root.is_some_and(|id| id.source_unit() == unit && a.parameter().is_some_and(|span| binding_at(names, id, span, SymbolKind::ValueParameter))
                && owned.binding_kind(id) == Some(OwnershipBindingKind::Shared))
            && typed.expression_category(source) == Some(ExpressionCategory::Place) && p.source_access() == UnitIterationSourceAccess::Shared
            && matches!(p.source_kind(), IterationSource::BorrowedSource(UnitLoanTarget::Place(place)) if place.is_root() && Some(place.root()) == root)
            && matches!(p.source(), Some(UnitLoanTarget::Place(place)) if place.is_root() && Some(place.root()) == root),
            (statement, source, root, p.source_kind(), p.source_access()));
        bindings.extend(d.bindings());
        component_loans.extend(p.loans().iter());
        for (i, component) in [MapIterationComponent::Key, MapIterationComponent::Value]
            .into_iter()
            .enumerate()
        {
            let binding = d.bindings()[i];
            let loan = p.loan(component);
            report.check(
                "unit nested site-isolated qualified marker/type/Shared component loan",
                binding.source_unit() == unit
                    && binding_at(names, binding, site.bindings[i], SymbolKind::ForBinding)
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
                    && Some(loan.origin()) == p.source(),
                (statement, binding, loan),
            );
        }
        for exit in p.exits() {
            println!(
                "SPEC0331 NESTED UNIT SITE statement={statement:?} source={source:?} root={root:?} exit={exit:#?} actual_at_point={:#?}",
                owned.iteration_cleanup_at(exit.point())
            );
        }
    }
    report.check(
        "unit nested all four qualified bindings and component loan identities distinct",
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
        "unit nested borrowed scan parameter has no owner drop",
        root.is_some()
            && !owned
                .drops()
                .iter()
                .any(|d| matches!(d.target(), UnitDropTarget::Named(id) if Some(id) == root)),
        (root, owned.drops()),
    );
    for read in &a.reads {
        let site = &a.sites[read.binding_site];
        let Some(d) = typed.map_iteration(UnitStatementId::new(unit, site.statement)) else {
            continue;
        };
        borrow_call(
            f,
            a,
            &read.call,
            names,
            (typed, owned),
            &d.bindings(),
            report,
        );
    }
    for (index, call) in a.scans.iter().enumerate() {
        let (marker, initializer) = a.locals[index];
        let local = symbol(names, unit, marker);
        let initializer = expression(initializer);
        let initializer_type = typed.expression_type(initializer);
        let initializer_call = typed.call(initializer);
        let initializer_map = typed.map_construction(initializer);
        // AST 已核真实 Val/initializer；formed 只证明形成历史，不授予当前可用性。
        // 当前使用仍由下方真实 Borrow call/loan 与循环后的 size 引用独立核验。
        report.check(
            "unit nested actual qualified local formation and owned initializer",
            local.is_some_and(|id| {
                id.source_unit() == unit
                    && owned.formed_owned_locals().contains(&id)
                    && binding_at(names, id, marker, SymbolKind::Variable)
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
    let factory_id = a
        .function(a.factory.target)
        .and_then(|function| declaration(names, function, unit));
    let factory = typed.call(expression(a.factory.expression));
    report.check("unit nested actual zero-argument factory declaration and owned result", factory_id.is_some()
        && factory.is_some_and(|call| call.expression() == expression(a.factory.expression)
            && matches!(call.target(), UnitCallTarget::Declaration(id) if Some(id) == factory_id)
            && call.receiver().is_none() && call.arguments().is_empty() && call.result_source() == CallableResultSource::Owned
            && Some(call.return_type()) == typed.expression_type(expression(a.sites[0].source))), (&a.factory, factory_id, factory));
    let empty = typed.map_construction(expression(a.locals[1].1));
    report.check(
        "unit nested actual empty intrinsic owner",
        empty.is_some_and(|d| {
            d.expression() == expression(a.locals[1].1)
                && d.callable()
                    == if f.map {
                        IntrinsicCallable::MapOf
                    } else {
                        IntrinsicCallable::MutableMapOf
                    }
                && Some(d.map_type()) == typed.expression_type(expression(a.sites[0].source))
        }),
        empty,
    );
    for size in &a.sizes {
        let expected_root = symbol(names, unit, size.root_marker);
        let descriptor = typed.map_size(expression(size.expression));
        report.check(
            "unit nested size actual qualified receiver and original root identity",
            expected_root.is_some()
                && size.span.source_id() == f.parsed.source_id()
                && reference(f, names, size.receiver) == expected_root
                && descriptor.is_some_and(|d| {
                    d.expression() == expression(size.expression)
                        && d.receiver() == expression(size.receiver)
                        && Some(d.map_type()) == typed.expression_type(expression(size.receiver))
                        && typed
                            .expression_type(expression(size.expression))
                            .is_some_and(|t| {
                                typed.types().get(t)
                                    == Some(&UnitTypeKind::Builtin(BuiltinType::Int))
                            })
                }),
            (size, expected_root, descriptor),
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
        report.check("unit nested nonempty Borrow roots", false, roots);
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
        "unit nested actual read/scan declaration and ordered Borrow references",
        target.is_some() && site.arguments.len() == roots.len() && call.is_some_and(|call| {
            call.expression() == call_id
                && matches!(call.target(), UnitCallTarget::Declaration(id) if Some(id) == target)
                && call.receiver().is_none()
                && call.arguments().len() == roots.len()
                && typed.types().get(call.return_type())
                    == Some(&UnitTypeKind::Builtin(BuiltinType::Unit))
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
        "unit nested read/scan exact qualified SharedLoan contracts",
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
    report.check("unit nested read/scan loans retain qualified root and argument identity", loans.len() == roots.len()
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
        report.check("unit nested actual function declaration/callable", id.is_some()
            && matches!(signatures.as_slice(), [signature] if signature.name_span() == function.name_span
                && matches!(signature.target(), UnitCallableTarget::Declaration(target) if Some(target) == id)), (function, id, &signatures));
        let [signature] = signatures.as_slice() else {
            continue;
        };
        let source_type = typed.expression_type(UnitExpressionId::new(unit, a.sites[0].source));
        report.check(
            "unit nested exact unmarked Borrow signatures",
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
                            && if function.name == "scan" {
                                Some(parameter.ty()) == source_type
                            } else {
                                typed.types().get(parameter.ty())
                                    == Some(&UnitTypeKind::Builtin(BuiltinType::String))
                            }
                    })
                && if function.name == a.factory.target {
                    Some(signature.return_type()) == source_type
                } else {
                    typed.types().get(signature.return_type())
                        == Some(&UnitTypeKind::Builtin(BuiltinType::Unit))
                },
            (function, signature),
        );
    }
}
