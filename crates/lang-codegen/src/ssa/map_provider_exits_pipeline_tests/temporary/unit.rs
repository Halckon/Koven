//! 同次 unit 分析逐完整 qualified For/source/binding 身份查询；不读取私有 acquired 表。
use super::{
    super::Report,
    ast::{Anchors, Function, Input},
};
use lang_frontend::{
    ast::ExpressionId,
    name_resolution::{
        DeclarationId, SourceUnitId, SymbolKind, UnitReferenceTarget, UnitSymbolId,
        ValidatedCompilationUnitNames,
    },
    ownership_checking::{
        CompilationUnitOwnership, IterationSource, LoanKind, MapIterationComponent,
        OwnershipBindingKind, UnitCallArgumentOwnershipKind, UnitIterationSourceAccess,
        UnitLoanTarget,
    },
    source::Span,
    type_checking::{
        BuiltinType, CallableResultSource, CompilationUnitTypes, ExpressionCategory,
        IntrinsicTypeConstructor, MapIterationKind, ParameterMode, UnitCallTarget,
        UnitCallableTarget, UnitExpressionId, UnitStatementId, UnitTypeKind,
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

fn binding_at(names: &ValidatedCompilationUnitNames, id: UnitSymbolId, span: Span) -> bool {
    names
        .names()
        .source_units()
        .iter()
        .filter(|s| s.source_unit() == id.source_unit())
        .any(|source| {
            source.resolution().symbols().iter().any(|s| {
                s.id() == id.symbol()
                    && s.span() == span
                    && s.kind() == SymbolKind::ForBinding
                    && !s.is_synthetic()
            })
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
        "SPEC0331 TEMPORARY UNIT REAL typed_maps={:#?} maps={:#?} drops={:#?} conditional_drops={:#?} contracts={:#?} loans={:#?} deliveries={:#?}",
        typed.map_iterations(),
        owned.map_iterations(),
        owned.drops(),
        owned.conditional_receiver_drops(),
        owned.call_argument_contracts(),
        owned.loans(),
        owned.value_deliveries()
    );
    let Some(a) = anchors else { return };
    let [source_names] = names.names().source_units() else {
        report.check(
            "unit temporary exactly one actual source",
            false,
            names.names().source_units(),
        );
        return;
    };
    let unit = source_names.source_unit();
    let expression = |id| UnitExpressionId::new(unit, id);
    report.check(
        "unit temporary original source and exact two typed/owned plans",
        source_names.resolution().source_id() == f.parsed.source_id()
            && typed.map_iterations().len() == 2
            && owned.map_iterations().len() == 2,
        (unit, typed.map_iterations(), owned.map_iterations()),
    );
    signatures(a, names, typed, owned, unit, report);
    let mut owners = Vec::new();
    let mut bindings = Vec::new();
    for site in &a.sites {
        let statement = UnitStatementId::new(unit, site.statement);
        let source = expression(site.source);
        report.check(
            "unit temporary actual qualified AST site",
            site.span.source_id() == f.parsed.source_id()
                && site.source_span.source_id() == f.parsed.source_id()
                && f.sources.source_name(site.span.source_id()) == Ok("provider.ko"),
            (
                statement,
                source,
                site.body,
                super::super::fixture::location(&f.sources, site.source_span),
            ),
        );
        let (Some(d), Some(p)) = (
            typed.map_iteration(statement),
            owned.map_iteration(statement),
        ) else {
            report.check(
                "unit temporary actual qualified site descriptor/plan",
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
            "unit temporary exact qualified For/source and intrinsic String K/V",
            d.statement() == statement && d.source() == source && p.descriptor() == d
                && typed.expression_type(source) == Some(d.source_type())
                && typed.sequential_iteration(statement).is_none() && owned.iteration(statement).is_none()
                && matches!(typed.types().get(d.source_type()), Some(UnitTypeKind::Intrinsic { constructor: actual, arguments })
                    if *actual == constructor && d.kind() == kind && arguments.as_slice() == [d.key_type(), d.value_type()])
                && [d.key_type(), d.value_type()].iter().all(|t| typed.types().get(*t) == Some(&UnitTypeKind::Builtin(BuiltinType::String))),
            (statement, source, d, p),
        );
        let owner = match p.source() {
            Some(UnitLoanTarget::Temporary(id)) => Some(*id),
            _ => None,
        };
        report.check(
            "unit temporary public category and qualified expression origin",
            typed.expression_category(source) == Some(ExpressionCategory::Temporary)
                && p.source_access() == UnitIterationSourceAccess::Temporary
                && matches!(p.source_kind(), IterationSource::BorrowedSource(UnitLoanTarget::Temporary(id)) if *id == source)
                && owner == Some(source),
            (statement, source, p.source_access(), p.source_kind()),
        );
        if let Some(owner) = owner {
            owners.push(owner);
        }
        bindings.push(d.bindings());
        let factory = a
            .function(site.factory)
            .and_then(|function| declaration(names, function, unit));
        let call = typed.call(source);
        report.check(
            "unit temporary own real factory declaration and owned result",
            factory.is_some() && call.is_some_and(|call| {
                call.expression() == source
                    && matches!(call.target(), UnitCallTarget::Declaration(id) if Some(id) == factory)
                    && call.receiver().is_none() && call.arguments().is_empty()
                    && call.return_type() == d.source_type()
                    && call.result_source() == CallableResultSource::Owned
            }),
            (statement, source, site.factory, factory, call),
        );
        report.check(
            "unit temporary distinct qualified Key/Value bindings and loans",
            d.key_binding() != d.value_binding() && p.loans()[0] != p.loans()[1],
            (statement, d.bindings(), p.loans()),
        );
        for (i, component) in [MapIterationComponent::Key, MapIterationComponent::Value]
            .into_iter()
            .enumerate()
        {
            let binding = d.bindings()[i];
            let loan = p.loan(component);
            report.check(
                "unit temporary actual marker/type/Shared component lineage",
                binding.source_unit() == unit
                    && binding_at(names, binding, site.bindings[i])
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
        let read = a
            .function("read")
            .and_then(|function| declaration(names, function, unit));
        let call_id = expression(site.read);
        let call = typed.call(call_id);
        report.check(
            "unit temporary own read declaration and two actual Borrow arguments",
            read.is_some() && call.is_some_and(|call| {
                call.expression() == call_id
                    && matches!(call.target(), UnitCallTarget::Declaration(id) if Some(id) == read)
                    && call.receiver().is_none()
                    && call.arguments().len() == 2
                    && typed.types().get(call.return_type())
                        == Some(&UnitTypeKind::Builtin(BuiltinType::Unit))
                    && typed.expression_type(call_id) == Some(call.return_type())
                    && call.arguments().iter().enumerate().all(|(i, argument)| {
                        argument.argument_index() == i
                            && argument.parameter_index() == i
                            && argument.mode() == ParameterMode::Borrow
                            && argument.parameter_type() == [d.key_type(), d.value_type()][i]
                            && typed.expression_type(expression(site.arguments[i]))
                                == Some(argument.parameter_type())
                            && reference(f, names, site.arguments[i]) == Some(d.bindings()[i])
                    })
            }),
            (statement, call_id, read, call),
        );
        let contracts = owned
            .call_argument_contracts_for(call_id)
            .collect::<Vec<_>>();
        report.check(
            "unit temporary read exact qualified Borrow contracts",
            contracts.len() == 2
                && site.arguments.iter().enumerate().all(|(i, argument)| {
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
        let call_loans = owned
            .loans()
            .iter()
            .filter(|loan| loan.call() == call_id)
            .collect::<Vec<_>>();
        report.check(
            "unit temporary read loans retain this site's qualified bindings",
            call_loans.len() == 2 && site.arguments.iter().enumerate().all(|(i, argument)| {
                call_loans.iter().filter(|loan| loan.argument() == expression(*argument) && loan.kind() == LoanKind::Shared
                    && matches!(loan.target(), UnitLoanTarget::Place(place) if place.is_root() && place.root() == d.bindings()[i])).count() == 1
            }),
            (statement, &call_loans),
        );
        // unit 仅公开退出计划及同点完整动作；不虚构 cleanup_steps/condition/owner API。
        for exit in p.exits() {
            println!(
                "SPEC0331 TEMPORARY UNIT SITE statement={statement:?} source={source:?} owner={owner:?} factory={} exit={exit:#?} actual_at_point={:#?}",
                site.factory,
                owned.iteration_cleanup_at(exit.point())
            );
        }
    }
    report.check(
        "unit temporary two qualified sites retain independent origins and bindings",
        matches!(owners.as_slice(), [first, second] if first != second && first.source_unit() == unit && second.source_unit() == unit)
            && matches!(bindings.as_slice(), [first, second] if first.iter().all(|binding| !second.contains(binding))),
        (owners, bindings),
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
            .filter(|signature| Some(signature.declaration()) == id)
            .filter_map(|signature| signature.callable())
            .collect::<Vec<_>>();
        report.check(
            "unit temporary actual function declaration and callable",
            id.is_some() && matches!(signatures.as_slice(), [signature] if signature.name_span() == function.name_span
                && matches!(signature.target(), UnitCallableTarget::Declaration(target) if Some(target) == id)),
            (function, id, &signatures),
        );
        let [signature] = signatures.as_slice() else {
            continue;
        };
        let return_type = a
            .sites
            .iter()
            .find(|site| site.factory == function.name)
            .and_then(|site| typed.map_iteration(UnitStatementId::new(unit, site.statement)))
            .map(|d| d.source_type());
        report.check(
            "unit temporary callable result and exact Borrow/value signature",
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
                        && if function.name == "entry" {
                            typed.types().get(signature.return_type())
                                == Some(&UnitTypeKind::Builtin(BuiltinType::Unit))
                        } else {
                            Some(signature.return_type()) == return_type
                        }
                },
            (function, signature, return_type),
        );
    }
}
