//! 同次 single 的本地形成历史、来源与调用；当前 owner/Drop/phi 首轮完整采集。
use super::{
    super::Report,
    ast::{Anchors, Call, Function, Input},
};
use lang_frontend::{
    ast::ExpressionId,
    name_resolution::{NameResolution, Namespace, ReferenceTarget, SymbolId, SymbolKind},
    ownership_checking::{
        DropTarget, IterationCleanupAction as Action, IterationExitKind, IterationSource, LoanKind,
        LoanTarget, MapIterationComponent, MapIterationSourceAccess, OwnershipBindingKind,
        OwnershipCheckedFile,
    },
    source::Span,
    type_checking::{
        BuiltinType, CallableResultSource, CallableTarget, ExpressionCategory,
        IntrinsicTypeConstructor, MapIterationKind, ParameterMode, TypeKind, TypedFile,
    },
};

fn symbol(names: &NameResolution, span: Span, kind: SymbolKind) -> Option<SymbolId> {
    let found = names
        .symbols()
        .iter()
        .filter(|s| s.span() == span && s.kind() == kind && !s.is_synthetic())
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
        .filter(|r| r.span() == span && r.namespace() == Namespace::Value)
        .collect::<Vec<_>>();
    match found.as_slice() {
        [r] => match r.target() {
            ReferenceTarget::Symbol(id) => Some(*id),
            _ => None,
        },
        _ => None,
    }
}

fn callee(f: &Input, names: &NameResolution, call: &Call, target: SymbolId) -> bool {
    let Ok((_, span)) = f.expression(call.callee) else {
        return false;
    };
    let found = names
        .references()
        .iter()
        .filter(|r| r.span() == span && r.namespace() == Namespace::Value)
        .collect::<Vec<_>>();
    matches!(found.as_slice(), [r] if match r.target() {
        ReferenceTarget::Symbol(id) => *id == target,
        ReferenceTarget::OverloadSet(ids) => ids.contains(&target),
        _ => false,
    })
}

fn function_symbol(names: &NameResolution, f: &Function) -> Option<SymbolId> {
    symbol(names, f.name_span, SymbolKind::Function)
}

pub(super) fn observe(
    f: &Input,
    anchors: Option<&Anchors>,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
    report: &mut Report,
) {
    // 先完整记录真实产物。formed 只证历史，Drop/conditions/incomings 没有预填形状。
    println!(
        "SPEC0331 LOCAL SINGLE REAL source={:?} typed_maps={:#?} maps={:#?} formed={:#?} drops={:#?} cleanup={:#?} conditions={:#?} loans={:#?} loan_ends={:#?}",
        typed.source_id(),
        typed.map_iterations(),
        owned.map_iterations(),
        owned.formed_owned_locals(),
        owned.drops(),
        owned.cleanup_steps(),
        owned.cleanup_conditions(),
        owned.loans(),
        owned.loan_ends()
    );
    let Some(a) = anchors else { return };
    report.check(
        "single local exact eight typed/owned sites and size count",
        typed.map_iterations().len() == 8
            && owned.map_iterations().len() == 8
            && typed.map_sizes().len() == if f.later { 8 } else { 0 },
        (
            typed.map_iterations(),
            owned.map_iterations(),
            typed.map_sizes(),
        ),
    );
    signatures(a, names, typed, owned, report);
    for call in &a.scans {
        zero_call(f, a, call, names, typed, None, report);
    }
    let mut roots = Vec::new();
    let mut bindings = Vec::new();
    for site in &a.sites {
        report.check(
            "single local source AST scanner/declaration/site identities",
            site.span.source_id() == f.parsed.source_id()
                && site.source_span.source_id() == f.parsed.source_id()
                && f.sources.source_name(site.span.source_id()) == Ok("provider.ko")
                && a.function(site.scanner)
                    .is_some_and(|f| f.span == site.scanner_span && f.body == site.scanner_body),
            (
                site.scanner,
                site.scanner_span,
                site.scanner_body,
                site.local.statement,
                site.local.item,
                site.local.kind,
                site.local.marker,
                site.local.initializer,
                site.statement,
                site.body,
                site.source_span,
            ),
        );
        let (Some(d), Some(p)) = (
            typed.map_iteration(site.statement),
            owned.map_iteration(site.statement),
        ) else {
            report.check(
                "single local site has actual descriptor/plan",
                false,
                (site, typed.map_iterations(), owned.map_iterations()),
            );
            continue;
        };
        let root = symbol(names, site.local.marker, SymbolKind::Variable);
        let constructor = if site.map {
            IntrinsicTypeConstructor::Map
        } else {
            IntrinsicTypeConstructor::MutableMap
        };
        let kind = if site.map {
            MapIterationKind::Map
        } else {
            MapIterationKind::MutableMap
        };
        report.check("single local exact source and intrinsic String/String descriptor",
            d.statement() == site.statement && d.source() == site.source && p.descriptor() == d
                && typed.expression_type(site.source) == Some(d.source_type())
                && typed.sequential_iteration(site.statement).is_none() && owned.iteration(site.statement).is_none()
                && matches!(typed.types().get(d.source_type()), Some(TypeKind::Intrinsic { constructor: actual, arguments })
                    if *actual == constructor && d.kind() == kind && arguments.as_slice() == [d.key_type(), d.value_type()])
                && [d.key_type(), d.value_type()].iter().all(|t| typed.types().get(*t) == Some(&TypeKind::Builtin(BuiltinType::String))),
            (site.statement, d, p));
        report.check("single local actual root formation history and Owned Place lineage",
            root.is_some_and(|root| {
                reference(f, names, site.source) == Some(root)
                    && typed.symbol_type(root) == Some(d.source_type())
                    && owned.formed_owned_locals().contains(&root)
                    && typed.expression_type(site.local.initializer) == Some(d.source_type())
                    && typed.expression_category(site.local.initializer) == Some(ExpressionCategory::Temporary)
                    && typed.expression_category(site.source) == Some(ExpressionCategory::Place)
                    && p.source_access() == MapIterationSourceAccess::Owned
                    && matches!(p.source_kind(), IterationSource::BorrowedSource(LoanTarget::Place(place))
                        if place.is_root() && place.root() == root)
                    && matches!(p.source(), Some(LoanTarget::Place(place)) if place.is_root() && place.root() == root)
            }), (site, root, p.source_kind(), p.source_access()));
        if let Some(root) = root {
            roots.push(root);
        }
        zero_call(
            f,
            a,
            &site.factory,
            names,
            typed,
            Some(d.source_type()),
            report,
        );
        report.check(
            "single local two distinct components and own loan rows",
            d.key_binding() != d.value_binding() && p.loans()[0] != p.loans()[1],
            (d.bindings(), p.loans()),
        );
        for (i, component) in [MapIterationComponent::Key, MapIterationComponent::Value]
            .into_iter()
            .enumerate()
        {
            let binding = d.bindings()[i];
            let loan = p.loan(component);
            report.check(
                "single local actual component symbol/type and Shared lineage",
                Some(binding) == symbol(names, site.bindings[i], SymbolKind::ForBinding)
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
                (site.statement, binding, loan),
            );
            bindings.push(binding);
        }
        let read_target = a.function("read").and_then(|v| function_symbol(names, v));
        let call = typed.call(site.read.expression);
        report.check("single local actual read target and two Borrow arguments",
            read_target.is_some_and(|target| callee(f, names, &site.read, target))
                && call.is_some_and(|call| {
                    call.expression() == site.read.expression
                        && matches!(call.target(), CallableTarget::Source(id) if Some(id) == read_target)
                        && call.receiver().is_none() && call.arguments().len() == 2
                        && call.result_source() == CallableResultSource::Owned
                        && typed.types().get(call.return_type()) == Some(&TypeKind::Builtin(BuiltinType::Unit))
                        && typed.expression_type(site.read.expression) == Some(call.return_type())
                        && call.arguments().iter().enumerate().all(|(i, arg)| {
                            arg.argument_index() == i && arg.parameter_index() == i && arg.mode() == ParameterMode::Borrow
                                && arg.parameter_type() == [d.key_type(), d.value_type()][i]
                                && typed.expression_type(site.read.arguments[i]) == Some(arg.parameter_type())
                                && reference(f, names, site.read.arguments[i]) == Some(d.bindings()[i])
                        })
                }), (site.statement, &site.read, read_target, call));
        let loans = owned
            .loans()
            .iter()
            .filter(|l| l.call() == site.read.expression)
            .collect::<Vec<_>>();
        report.check("single local read loans retain exact component roots",
            loans.len() == 2 && site.read.arguments.iter().enumerate().all(|(i, argument)| {
                loans.iter().filter(|l| l.argument() == *argument && l.kind() == LoanKind::Shared
                    && matches!(l.target(), LoanTarget::Place(place) if place.is_root() && place.root() == d.bindings()[i])).count() == 1
            }), (site.statement, &loans));
        if let Some(size) = &site.size {
            let descriptor = typed.map_size(size.expression);
            report.check(
                "single local later size uses this exact named source root",
                root.is_some()
                    && reference(f, names, size.receiver) == root
                    && descriptor.is_some_and(|s| {
                        s.expression() == size.expression
                            && s.receiver() == size.receiver
                            && s.map_type() == d.source_type()
                    })
                    && typed.expression_type(size.receiver) == Some(d.source_type())
                    && typed.expression_type(size.expression).is_some_and(|t| {
                        typed.types().get(t) == Some(&TypeKind::Builtin(BuiltinType::Int))
                    })
                    && typed.expression_type(size.comparison).is_some_and(|t| {
                        typed.types().get(t) == Some(&TypeKind::Builtin(BuiltinType::Boolean))
                    })
                    && site.span.end() <= size.span.start(),
                (site.statement, root, size.condition, size, descriptor),
            );
        }
        let drops = owned
            .drops()
            .iter()
            .filter(|d| matches!(d.target(), DropTarget::Named(id) if Some(id) == root))
            .collect::<Vec<_>>();
        println!(
            "SPEC0331 LOCAL SINGLE OWNER site={:?} root={root:?} initializer={:?} drops={drops:#?} phis={:#?} incomings={:#?} flow={:#?}",
            site.statement,
            site.local.initializer,
            p.closure_phis(),
            p.closure_phi_incomings(),
            p.closure_flow()
        );
        for exit in p.exits() {
            let actual = owned
                .cleanup_steps()
                .iter()
                .filter_map(|(point, action)| (*point == exit.point()).then_some(*action))
                .collect::<Vec<_>>();
            println!(
                "SPEC0331 LOCAL SINGLE EXIT site={:?} root={root:?} exit={exit:#?} actual_at_point={actual:#?} condition={:#?}",
                site.statement,
                owned.cleanup_conditions().get(exit.condition())
            );
            report.check(
                "single local full same-point cleanup consistency",
                actual == exit.actions(),
                (site.statement, exit, &actual),
            );
            let map_actions = exit
                .actions()
                .iter()
                .filter(|a| !matches!(a, Action::Drop(_)))
                .copied()
                .collect::<Vec<_>>();
            let expected = match exit.kind() {
                IterationExitKind::Fallthrough => Some(vec![
                    Action::EndBinding {
                        statement: site.statement,
                        symbol: d.value_binding(),
                    },
                    Action::EndBinding {
                        statement: site.statement,
                        symbol: d.key_binding(),
                    },
                ]),
                IterationExitKind::Exhaustion => Some(vec![
                    Action::FinishProvider(site.statement),
                    Action::EndSource(site.statement),
                ]),
                _ => None,
            };
            report.check(
                "single local normal component/provider action order",
                expected.as_ref().is_some_and(|e| e == &map_actions),
                (site.statement, exit.kind(), &map_actions),
            );
        }
        report.check(
            "single local exact fallthrough and exhaustion exit kinds",
            p.exits().len() == 2
                && [
                    IterationExitKind::Fallthrough,
                    IterationExitKind::Exhaustion,
                ]
                .iter()
                .all(|kind| p.exits().iter().filter(|e| e.kind() == *kind).count() == 1),
            p.exits(),
        );
    }
    report.check(
        "single local all eight roots and sixteen components stay distinct",
        roots.len() == 8
            && roots
                .iter()
                .enumerate()
                .all(|(i, r)| !roots[..i].contains(r))
            && bindings.len() == 16
            && bindings
                .iter()
                .enumerate()
                .all(|(i, b)| !bindings[..i].contains(b))
            && roots.iter().all(|r| !bindings.contains(r)),
        (&roots, &bindings),
    );
}

fn zero_call(
    f: &Input,
    a: &Anchors,
    site: &Call,
    names: &NameResolution,
    typed: &TypedFile,
    result: Option<lang_frontend::type_checking::TypeId>,
    report: &mut Report,
) {
    let target = a
        .function(site.target)
        .and_then(|v| function_symbol(names, v));
    let call = typed.call(site.expression);
    report.check(
        "single local selected zero-argument scanner/factory call identity",
        target.is_some_and(|t| callee(f, names, site, t))
            && call.is_some_and(|call| {
                call.expression() == site.expression
                    && matches!(call.target(), CallableTarget::Source(id) if Some(id) == target)
                    && call.receiver().is_none()
                    && call.arguments().is_empty()
                    && call.result_source() == CallableResultSource::Owned
                    && !call.aborts()
                    && call.instance().type_arguments().is_empty()
                    && typed.expression_type(site.expression) == Some(call.return_type())
                    && result.map_or_else(
                        || {
                            typed.types().get(call.return_type())
                                == Some(&TypeKind::Builtin(BuiltinType::Unit))
                        },
                        |ty| call.return_type() == ty,
                    )
            }),
        (site, target, call, result),
    );
}

fn signatures(
    a: &Anchors,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
    report: &mut Report,
) {
    for function in &a.functions {
        let id = function_symbol(names, function);
        let callables = typed
            .callables()
            .iter()
            .filter(|c| Some(c.symbol()) == id)
            .collect::<Vec<_>>();
        report.check(
            "single local exact callable for real source function",
            id.is_some() && matches!(callables.as_slice(), [_]),
            (function, id, &callables),
        );
        let [callable] = callables.as_slice() else {
            continue;
        };
        let factory_result = a
            .sites
            .iter()
            .find(|s| s.factory.target == function.name)
            .and_then(|s| typed.map_iteration(s.statement))
            .map(|d| d.source_type());
        report.check(
            "single local source callable signature and result source",
            callable.receiver().is_none()
                && callable.type_parameters().is_empty()
                && callable.result_source() == CallableResultSource::Owned
                && if function.name == "read" {
                    typed.types().get(callable.return_type())
                        == Some(&TypeKind::Builtin(BuiltinType::Unit))
                        && callable.parameters().len() == 2
                        && callable.parameter_symbols().len() == 2
                        && callable.parameters().iter().all(|p| {
                            p.mode == ParameterMode::Borrow
                                && typed.types().get(p.ty)
                                    == Some(&TypeKind::Builtin(BuiltinType::String))
                        })
                        && callable.parameter_symbols().iter().all(|p| {
                            p.is_some_and(|id| {
                                owned.binding_kind(id) == Some(OwnershipBindingKind::Shared)
                            })
                        })
                } else {
                    callable.parameters().is_empty()
                        && callable.parameter_symbols().is_empty()
                        && factory_result.map_or_else(
                            || {
                                typed.types().get(callable.return_type())
                                    == Some(&TypeKind::Builtin(BuiltinType::Unit))
                            },
                            |ty| callable.return_type() == ty,
                        )
                },
            (function, callable, factory_result),
        );
    }
}
