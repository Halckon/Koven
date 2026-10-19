//! 同次 unit 的 qualified local/source/call 身份；不使用 single 条件或 owner 表。
use super::{
    super::Report,
    ast::{Anchors, Call, Function, Input},
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
        UnitIterationCleanupAction as Action, UnitIterationExitKind, UnitIterationSourceAccess,
        UnitLoanTarget,
    },
    source::Span,
    type_checking::{
        BuiltinType, CallableResultSource, CompilationUnitTypes, ExpressionCategory,
        IntrinsicTypeConstructor, MapIterationKind, ParameterMode, UnitCallTarget,
        UnitCallableTarget, UnitExpressionId, UnitStatementId, UnitTypeId, UnitTypeKind,
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

fn reference(
    f: &Input,
    names: &ValidatedCompilationUnitNames,
    unit: SourceUnitId,
    expression: ExpressionId,
) -> Option<UnitSymbolId> {
    let (_, span) = f.expression(expression).ok()?;
    let found = names
        .names()
        .references()
        .iter()
        .filter(|r| {
            r.source_unit() == unit && r.span() == span && r.namespace() == Some(Namespace::Value)
        })
        .collect::<Vec<_>>();
    match found.as_slice() {
        [r] => match r.target() {
            UnitReferenceTarget::Symbol(id) if id.source_unit() == unit => Some(*id),
            _ => None,
        },
        _ => None,
    }
}

fn binding_at(
    names: &ValidatedCompilationUnitNames,
    id: UnitSymbolId,
    span: Span,
    kind: SymbolKind,
) -> bool {
    let found = names
        .names()
        .source_units()
        .iter()
        .filter(|s| s.source_unit() == id.source_unit())
        .flat_map(|s| s.resolution().symbols().iter())
        .filter(|s| s.id() == id.symbol())
        .collect::<Vec<_>>();
    matches!(found.as_slice(), [s] if s.span() == span && s.kind() == kind && !s.is_synthetic())
}

fn callee(
    f: &Input,
    names: &ValidatedCompilationUnitNames,
    unit: SourceUnitId,
    call: &Call,
    target: DeclarationId,
) -> bool {
    let Ok((_, span)) = f.expression(call.callee) else {
        return false;
    };
    let found = names
        .names()
        .references()
        .iter()
        .filter(|r| {
            r.source_unit() == unit && r.span() == span && r.namespace() == Some(Namespace::Value)
        })
        .collect::<Vec<_>>();
    matches!(found.as_slice(), [r] if match r.target() {
        UnitReferenceTarget::Declaration(id) => *id == target,
        UnitReferenceTarget::OverloadSet(ids) => ids.contains(&target),
        _ => false,
    })
}

pub(super) fn observe(
    f: &Input,
    anchors: Option<&Anchors>,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    report: &mut Report,
) {
    println!(
        "SPEC0331 LOCAL UNIT REAL typed_maps={:#?} maps={:#?} formed={:#?} drops={:#?} conditional_drops={:#?} contracts={:#?} loans={:#?} deliveries={:#?}",
        typed.map_iterations(),
        owned.map_iterations(),
        owned.formed_owned_locals(),
        owned.drops(),
        owned.conditional_receiver_drops(),
        owned.call_argument_contracts(),
        owned.loans(),
        owned.value_deliveries()
    );
    let Some(a) = anchors else { return };
    let [source_names] = names.names().source_units() else {
        report.check(
            "unit local one actual source unit",
            false,
            names.names().source_units(),
        );
        return;
    };
    let unit = source_names.source_unit();
    let expression = |id| UnitExpressionId::new(unit, id);
    report.check(
        "unit local original source and eight qualified sites and size count",
        source_names.resolution().source_id() == f.parsed.source_id()
            && typed.map_iterations().len() == 8
            && owned.map_iterations().len() == 8
            && typed.map_sizes().len() == if f.later { 8 } else { 0 },
        (
            unit,
            typed.map_iterations(),
            owned.map_iterations(),
            typed.map_sizes(),
        ),
    );
    signatures(a, names, typed, owned, unit, report);
    for call in &a.scans {
        zero_call(f, a, call, names, typed, (unit, None), report);
    }
    let mut roots = Vec::new();
    let mut bindings = Vec::new();
    for site in &a.sites {
        let statement = UnitStatementId::new(unit, site.statement);
        let source = expression(site.source);
        report.check(
            "unit local qualified AST scanner/declaration/site identities",
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
                expression(site.local.initializer),
                statement,
                site.body,
                source,
            ),
        );
        let (Some(d), Some(p)) = (
            typed.map_iteration(statement),
            owned.map_iteration(statement),
        ) else {
            report.check(
                "unit local actual qualified site descriptor/plan",
                false,
                (
                    statement,
                    source,
                    typed.map_iterations(),
                    owned.map_iterations(),
                ),
            );
            continue;
        };
        // qualified root 只取实际 source reference；不从裸 symbol 人工拼 UnitSymbolId。
        let root = reference(f, names, unit, site.source);
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
        report.check("unit local exact qualified source and intrinsic String/String descriptor",
            d.statement() == statement && d.source() == source && p.descriptor() == d
                && typed.expression_type(source) == Some(d.source_type())
                && typed.sequential_iteration(statement).is_none() && owned.iteration(statement).is_none()
                && matches!(typed.types().get(d.source_type()), Some(UnitTypeKind::Intrinsic { constructor: actual, arguments })
                    if *actual == constructor && d.kind() == kind && arguments.as_slice() == [d.key_type(), d.value_type()])
                && [d.key_type(), d.value_type()].iter().all(|t| typed.types().get(*t) == Some(&UnitTypeKind::Builtin(BuiltinType::String))),
            (statement, source, d, p));
        report.check("unit local formation history and actual Owned root Place lineage",
            root.is_some_and(|root| {
                binding_at(names, root, site.local.marker, SymbolKind::Variable)
                    && root.source_unit() == unit && typed.symbol_type(root) == Some(d.source_type())
                    && owned.formed_owned_locals().contains(&root)
                    && typed.expression_type(expression(site.local.initializer)) == Some(d.source_type())
                    && typed.expression_category(expression(site.local.initializer)) == Some(ExpressionCategory::Temporary)
                    && typed.expression_category(source) == Some(ExpressionCategory::Place)
                    && p.source_access() == UnitIterationSourceAccess::Owned
                    && matches!(p.source_kind(), IterationSource::BorrowedSource(UnitLoanTarget::Place(place))
                        if place.is_root() && place.root() == root)
                    && matches!(p.source(), Some(UnitLoanTarget::Place(place)) if place.is_root() && place.root() == root)
            }), (statement, root, p.source_kind(), p.source_access()));
        if let Some(root) = root {
            roots.push(root);
        }
        zero_call(
            f,
            a,
            &site.factory,
            names,
            typed,
            (unit, Some(d.source_type())),
            report,
        );
        report.check(
            "unit local distinct qualified components and own loan rows",
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
                "unit local actual qualified component marker/type/Shared lineage",
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
            bindings.push(binding);
        }
        let read_target = a.function("read").and_then(|v| declaration(names, v, unit));
        let call_id = expression(site.read.expression);
        let call = typed.call(call_id);
        report.check("unit local actual read declaration and two Borrow arguments",
            read_target.is_some_and(|target| callee(f, names, unit, &site.read, target))
                && call.is_some_and(|call| {
                    call.expression() == call_id
                        && matches!(call.target(), UnitCallTarget::Declaration(id) if Some(id) == read_target)
                        && call.receiver().is_none() && call.arguments().len() == 2
                        && call.result_source() == CallableResultSource::Owned
                        && typed.types().get(call.return_type()) == Some(&UnitTypeKind::Builtin(BuiltinType::Unit))
                        && typed.expression_type(call_id) == Some(call.return_type())
                        && call.arguments().iter().enumerate().all(|(i, arg)| {
                            arg.argument_index() == i && arg.parameter_index() == i && arg.mode() == ParameterMode::Borrow
                                && arg.parameter_type() == [d.key_type(), d.value_type()][i]
                                && typed.expression_type(expression(site.read.arguments[i])) == Some(arg.parameter_type())
                                && reference(f, names, unit, site.read.arguments[i]) == Some(d.bindings()[i])
                        })
                }), (statement, &site.read, read_target, call));
        let contracts = owned
            .call_argument_contracts_for(call_id)
            .collect::<Vec<_>>();
        report.check(
            "unit local read exact qualified Borrow contracts and no owned delivery",
            contracts.len() == 2
                && site.read.arguments.iter().enumerate().all(|(i, argument)| {
                    contracts
                        .iter()
                        .filter(|c| {
                            c.call() == call_id
                                && c.argument() == expression(*argument)
                                && c.parameter_index() == i
                                && c.parameter_type() == [d.key_type(), d.value_type()][i]
                                && c.kind() == UnitCallArgumentOwnershipKind::SharedLoan
                        })
                        .count()
                        == 1
                })
                && !owned
                    .value_deliveries()
                    .iter()
                    .any(|delivery| delivery.call() == call_id),
            (statement, &contracts),
        );
        let loans = owned
            .loans()
            .iter()
            .filter(|l| l.call() == call_id)
            .collect::<Vec<_>>();
        report.check("unit local read loans retain exact qualified component roots",
            loans.len() == 2 && site.read.arguments.iter().enumerate().all(|(i, argument)| {
                loans.iter().filter(|l| l.argument() == expression(*argument) && l.kind() == LoanKind::Shared
                    && matches!(l.target(), UnitLoanTarget::Place(place) if place.is_root() && place.root() == d.bindings()[i])).count() == 1
            }), (statement, &loans));
        if let Some(size) = &site.size {
            let descriptor = typed.map_size(expression(size.expression));
            report.check(
                "unit local later size retains this qualified named root",
                root.is_some()
                    && reference(f, names, unit, size.receiver) == root
                    && descriptor.is_some_and(|s| {
                        s.expression() == expression(size.expression)
                            && s.receiver() == expression(size.receiver)
                            && s.map_type() == d.source_type()
                    })
                    && typed.expression_type(expression(size.receiver)) == Some(d.source_type())
                    && typed
                        .expression_type(expression(size.expression))
                        .is_some_and(|t| {
                            typed.types().get(t) == Some(&UnitTypeKind::Builtin(BuiltinType::Int))
                        })
                    && typed
                        .expression_type(expression(size.comparison))
                        .is_some_and(|t| {
                            typed.types().get(t)
                                == Some(&UnitTypeKind::Builtin(BuiltinType::Boolean))
                        })
                    && site.span.end() <= size.span.start(),
                (
                    statement,
                    root,
                    expression(size.condition),
                    size,
                    descriptor,
                ),
            );
        }
        let drops = owned
            .drops()
            .iter()
            .filter(|d| matches!(d.target(), UnitDropTarget::Named(id) if Some(id) == root))
            .collect::<Vec<_>>();
        println!(
            "SPEC0331 LOCAL UNIT OWNER site={statement:?} root={root:?} initializer={:?} drops={drops:#?} plan={p:#?}",
            expression(site.local.initializer)
        );
        for exit in p.exits() {
            let actual = owned.iteration_cleanup_at(exit.point());
            println!(
                "SPEC0331 LOCAL UNIT EXIT site={statement:?} root={root:?} exit={exit:#?} actual_at_point={actual:#?}"
            );
            // 这个 getter 是同计划表一致性，不声称第二份 unit cleanup 存储或 single 条件模型。
            report.check(
                "unit local same-table full cleanup consistency",
                actual == Some(exit.actions()),
                (statement, exit, actual),
            );
            let map_actions = exit
                .actions()
                .iter()
                .filter(|a| !matches!(a, Action::Drop(_)))
                .cloned()
                .collect::<Vec<_>>();
            let expected = match exit.kind() {
                UnitIterationExitKind::Fallthrough => Some(vec![
                    Action::EndBinding {
                        statement,
                        symbol: d.value_binding(),
                    },
                    Action::EndBinding {
                        statement,
                        symbol: d.key_binding(),
                    },
                ]),
                UnitIterationExitKind::Exhaustion => Some(vec![
                    Action::FinishProvider(statement),
                    Action::EndSource(statement),
                ]),
                _ => None,
            };
            report.check(
                "unit local normal component/provider action order",
                expected.as_ref().is_some_and(|e| e == &map_actions),
                (statement, exit.kind(), &map_actions),
            );
        }
        report.check(
            "unit local exact normal exit kinds",
            p.exits().len() == 2
                && [
                    UnitIterationExitKind::Fallthrough,
                    UnitIterationExitKind::Exhaustion,
                ]
                .iter()
                .all(|kind| p.exits().iter().filter(|e| e.kind() == *kind).count() == 1),
            p.exits(),
        );
    }
    report.check(
        "unit local eight qualified roots and sixteen component symbols remain distinct",
        roots.len() == 8
            && roots.iter().all(|r| r.source_unit() == unit)
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
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    expected: (SourceUnitId, Option<UnitTypeId>),
    report: &mut Report,
) {
    let (unit, result) = expected;
    let target = a
        .function(site.target)
        .and_then(|v| declaration(names, v, unit));
    let call_id = UnitExpressionId::new(unit, site.expression);
    let call = typed.call(call_id);
    report.check(
        "unit local selected qualified zero-argument scanner/factory identity",
        target.is_some_and(|t| callee(f, names, unit, site, t)) && call.is_some_and(|call| {
            call.expression() == call_id
                && matches!(call.target(), UnitCallTarget::Declaration(id) if Some(id) == target)
                && call.receiver().is_none()
                && call.arguments().is_empty()
                && call.result_source() == CallableResultSource::Owned
                && !call.aborts()
                && typed.expression_type(call_id) == Some(call.return_type())
                && result.map_or_else(
                    || {
                        typed.types().get(call.return_type())
                            == Some(&UnitTypeKind::Builtin(BuiltinType::Unit))
                    },
                    |ty| call.return_type() == ty,
                )
        }),
        (site, target, call, result),
    );
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
        report.check("unit local real function declaration and callable",
            id.is_some() && matches!(signatures.as_slice(), [s] if s.name_span() == function.name_span
                && matches!(s.target(), UnitCallableTarget::Declaration(target) if Some(target) == id)),
            (function, id, &signatures));
        let [signature] = signatures.as_slice() else {
            continue;
        };
        let factory_result = a
            .sites
            .iter()
            .find(|s| s.factory.target == function.name)
            .and_then(|s| typed.map_iteration(UnitStatementId::new(unit, s.statement)))
            .map(|d| d.source_type());
        report.check(
            "unit local exact callable signature/result source",
            signature.receiver().is_none()
                && signature.type_parameters().is_empty()
                && signature.has_body()
                && signature.result_source() == CallableResultSource::Owned
                && if function.name == "read" {
                    typed.types().get(signature.return_type())
                        == Some(&UnitTypeKind::Builtin(BuiltinType::Unit))
                        && signature.parameters().len() == 2
                        && signature.parameters().iter().all(|p| {
                            p.mode() == ParameterMode::Borrow
                                && typed.types().get(p.ty())
                                    == Some(&UnitTypeKind::Builtin(BuiltinType::String))
                                && p.symbol().is_some_and(|id| {
                                    id.source_unit() == unit
                                        && owned.binding_kind(id)
                                            == Some(OwnershipBindingKind::Shared)
                                })
                        })
                } else {
                    signature.parameters().is_empty()
                        && factory_result.map_or_else(
                            || {
                                typed.types().get(signature.return_type())
                                    == Some(&UnitTypeKind::Builtin(BuiltinType::Unit))
                            },
                            |ty| signature.return_type() == ty,
                        )
                },
            (function, signature, factory_result),
        );
    }
}
