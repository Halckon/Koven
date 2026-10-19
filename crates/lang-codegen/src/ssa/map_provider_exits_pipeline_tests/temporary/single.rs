//! 同次 single 分析的公开 temporary facts；复杂清理拓扑首轮完整记录，不预填。
use super::{
    super::Report,
    ast::{Anchors, Input},
};
use lang_frontend::{
    ast::ExpressionId,
    name_resolution::{NameResolution, ReferenceTarget, SymbolId, SymbolKind},
    ownership_checking::{
        IterationSource, LoanKind, LoanTarget, MapIterationComponent, MapIterationSourceAccess,
        OwnershipBindingKind, OwnershipCheckedFile,
    },
    source::Span,
    type_checking::{
        BuiltinType, CallableResultSource, CallableTarget, ExpressionCategory,
        IntrinsicTypeConstructor, MapIterationKind, ParameterMode, TypeKind, TypedFile,
    },
};

fn symbol(names: &NameResolution, span: Span) -> Option<SymbolId> {
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
    // 在可选 AST/query 之前保全真实产物；每个 plan Debug 包含完整 exits 与 owner/phi 记录。
    println!(
        "SPEC0331 TEMPORARY SINGLE REAL source={:?} typed_maps={:#?} maps={:#?} drops={:#?} cleanup={:#?} conditions={:#?} loans={:#?} loan_ends={:#?}",
        typed.source_id(),
        typed.map_iterations(),
        owned.map_iterations(),
        owned.drops(),
        owned.cleanup_steps(),
        owned.cleanup_conditions(),
        owned.loans(),
        owned.loan_ends()
    );
    let Some(a) = anchors else { return };
    report.check(
        "single temporary exact two typed/owned plans",
        typed.map_iterations().len() == 2 && owned.map_iterations().len() == 2,
        (typed.map_iterations(), owned.map_iterations()),
    );
    signatures(a, names, typed, owned, report);
    let mut owners = Vec::new();
    let mut bindings = Vec::new();
    for site in &a.sites {
        report.check(
            "single temporary actual source-qualified AST site",
            site.span.source_id() == f.parsed.source_id()
                && site.source_span.source_id() == f.parsed.source_id()
                && f.sources.source_name(site.span.source_id()) == Ok("provider.ko"),
            (
                site,
                site.body,
                super::super::fixture::location(&f.sources, site.source_span),
            ),
        );
        let (Some(d), Some(p)) = (
            typed.map_iteration(site.statement),
            owned.map_iteration(site.statement),
        ) else {
            report.check(
                "single temporary actual site descriptor/plan",
                false,
                (site, typed.map_iterations(), owned.map_iterations()),
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
        report.check(
            "single temporary actual For/source and intrinsic String K/V",
            d.statement() == site.statement && d.source() == site.source && p.descriptor() == d
                && typed.expression_type(site.source) == Some(d.source_type())
                && typed.sequential_iteration(site.statement).is_none() && owned.iteration(site.statement).is_none()
                && matches!(typed.types().get(d.source_type()), Some(TypeKind::Intrinsic { constructor: actual, arguments })
                    if *actual == constructor && d.kind() == kind && arguments.as_slice() == [d.key_type(), d.value_type()])
                && [d.key_type(), d.value_type()].iter().all(|t| typed.types().get(*t) == Some(&TypeKind::Builtin(BuiltinType::String))),
            (site.statement, d, p),
        );
        let owner = match p.source() {
            Some(LoanTarget::Temporary(id)) => Some(*id),
            _ => None,
        };
        report.check(
            "single temporary public source category and expression origin",
            typed.expression_category(site.source) == Some(ExpressionCategory::Temporary)
                && p.source_access() == MapIterationSourceAccess::Temporary
                && matches!(p.source_kind(), IterationSource::BorrowedSource(LoanTarget::Temporary(id)) if *id == site.source)
                && owner == Some(site.source),
            (site.statement, site.source, p.source_access(), p.source_kind()),
        );
        if let Some(owner) = owner {
            owners.push(owner);
        }
        bindings.push(d.bindings());
        let factory = a
            .function(site.factory)
            .and_then(|function| symbol(names, function.name_span));
        let call = typed.call(site.source);
        report.check(
            "single temporary own real zero-argument factory and owned result",
            factory.is_some() && call.is_some_and(|call| {
                call.expression() == site.source
                    && matches!(call.target(), CallableTarget::Source(id) if Some(id) == factory)
                    && call.receiver().is_none()
                    && call.arguments().is_empty()
                    && call.return_type() == d.source_type()
                    && call.result_source() == CallableResultSource::Owned
            }),
            (site.statement, site.factory, factory, call),
        );
        report.check(
            "single temporary distinct Key/Value binding and loan identities",
            d.key_binding() != d.value_binding() && p.loans()[0] != p.loans()[1],
            (site.statement, d.bindings(), p.loans()),
        );
        for (i, component) in [MapIterationComponent::Key, MapIterationComponent::Value]
            .into_iter()
            .enumerate()
        {
            let binding = d.bindings()[i];
            let actual = names.symbols().iter().find(|s| s.id() == binding);
            let loan = p.loan(component);
            report.check(
                "single temporary actual component marker/type/Shared lineage",
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
        let read = a
            .function("read")
            .and_then(|function| symbol(names, function.name_span));
        let call = typed.call(site.read);
        report.check(
            "single temporary own read call and two actual Borrow arguments",
            read.is_some()
                && call.is_some_and(|call| {
                    call.expression() == site.read
                        && matches!(call.target(), CallableTarget::Source(id) if Some(id) == read)
                        && call.receiver().is_none()
                        && call.arguments().len() == 2
                        && typed.types().get(call.return_type())
                            == Some(&TypeKind::Builtin(BuiltinType::Unit))
                        && typed.expression_type(site.read) == Some(call.return_type())
                        && call.arguments().iter().enumerate().all(|(i, argument)| {
                            argument.argument_index() == i
                                && argument.parameter_index() == i
                                && argument.mode() == ParameterMode::Borrow
                                && argument.parameter_type() == [d.key_type(), d.value_type()][i]
                                && typed.expression_type(site.arguments[i])
                                    == Some(argument.parameter_type())
                                && reference(f, names, site.arguments[i]) == Some(d.bindings()[i])
                        })
                }),
            (site.statement, site.read, read, call),
        );
        let call_loans = owned
            .loans()
            .iter()
            .filter(|loan| loan.call() == site.read)
            .collect::<Vec<_>>();
        report.check(
            "single temporary read loans refer to this site's two bindings",
            call_loans.len() == 2 && site.arguments.iter().enumerate().all(|(i, argument)| {
                call_loans.iter().filter(|loan| loan.argument() == *argument && loan.kind() == LoanKind::Shared
                    && matches!(loan.target(), LoanTarget::Place(place) if place.is_root() && place.root() == d.bindings()[i])).count() == 1
            }),
            (site.statement, &call_loans),
        );
        // 先记录完整真实退出/全局同点动作与条件；首轮不假设未来 owner/CFG 清理拓扑。
        for exit in p.exits() {
            let at_point = owned
                .cleanup_steps()
                .iter()
                .filter(|(point, _)| *point == exit.point())
                .collect::<Vec<_>>();
            println!(
                "SPEC0331 TEMPORARY SINGLE SITE statement={:?} source={:?} owner={owner:?} factory={} exit={exit:#?} actual_at_point={at_point:#?} condition={:#?}",
                site.statement,
                site.source,
                site.factory,
                owned.cleanup_conditions().get(exit.condition())
            );
        }
    }
    report.check(
        "single temporary two sites keep independent origins and all four bindings",
        matches!(owners.as_slice(), [first, second] if first != second)
            && matches!(bindings.as_slice(), [first, second] if first.iter().all(|binding| !second.contains(binding))),
        (owners, bindings),
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
        let id = symbol(names, function.name_span);
        let source = id.and_then(|id| names.symbols().iter().find(|s| s.id() == id));
        let callables = typed
            .callables()
            .iter()
            .filter(|c| Some(c.symbol()) == id)
            .collect::<Vec<_>>();
        report.check(
            "single temporary actual function symbol and callable",
            source.is_some_and(|s| s.kind() == SymbolKind::Function && s.name() == function.name)
                && matches!(callables.as_slice(), [_]),
            (function, source, &callables),
        );
        let [callable] = callables.as_slice() else {
            continue;
        };
        let return_type = a
            .sites
            .iter()
            .find(|site| site.factory == function.name)
            .and_then(|site| typed.map_iteration(site.statement))
            .map(|d| d.source_type());
        report.check(
            "single temporary callable result and exact Borrow/value signature",
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
                        && if function.name == "entry" {
                            typed.types().get(callable.return_type())
                                == Some(&TypeKind::Builtin(BuiltinType::Unit))
                        } else {
                            Some(callable.return_type()) == return_type
                        }
                },
            (function, callable, return_type),
        );
    }
}
