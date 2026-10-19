//! Same-analysis unit facts; every Unit ID comes from real products or wraps an actual AST node.
use super::{
    super::Report,
    ast::{Anchors, Input},
};
use lang_frontend::{
    ast::ExpressionId,
    name_resolution::{
        SymbolKind, UnitReferenceTarget, UnitSymbolId, ValidatedCompilationUnitNames,
    },
    ownership_checking::{
        CompilationUnitOwnership, IterationSource, LoanKind, MapIterationComponent,
        OwnershipBindingKind, UnitCallArgumentOwnershipKind, UnitDropTarget,
        UnitIterationSourceAccess, UnitLoanTarget, UnitValueDeliveryKind,
    },
    source::Span,
    type_checking::{
        BuiltinType, CompilationUnitTypes, IntrinsicTypeConstructor, MapIterationKind,
        ParameterMode, UnitCallTarget, UnitConstructionTarget, UnitExpressionId, UnitStatementId,
        UnitTypeKind,
    },
};

fn reference(
    f: &Input,
    names: &ValidatedCompilationUnitNames,
    expression: ExpressionId,
) -> Option<UnitSymbolId> {
    let (_, span) = f.expression(expression).ok()?;
    let refs = names
        .names()
        .references()
        .iter()
        .filter(|r| r.span() == span)
        .collect::<Vec<_>>();
    match refs.as_slice() {
        [r] => match r.target() {
            UnitReferenceTarget::Symbol(id) => Some(*id),
            _ => None,
        },
        _ => None,
    }
}

fn has_span(
    names: &ValidatedCompilationUnitNames,
    id: UnitSymbolId,
    span: Span,
    kind: Option<SymbolKind>,
) -> bool {
    names
        .names()
        .source_units()
        .iter()
        .filter(|s| s.source_unit() == id.source_unit())
        .any(|s| {
            s.resolution().symbols().iter().any(|s| {
                s.id() == id.symbol()
                    && s.span() == span
                    && !s.is_synthetic()
                    && kind.is_none_or(|k| k == s.kind())
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
        "SPEC0331 CONDITIONAL UNIT REAL maps={:#?} drops={:#?} contracts={:#?} loans={:#?} deliveries={:#?}",
        owned.map_iterations(),
        owned.drops(),
        owned.call_argument_contracts(),
        owned.loans(),
        owned.value_deliveries()
    );
    let Some(a) = anchors else { return };
    report.check(
        "conditional actual For source location",
        a.span.source_id() == f.parsed.source_id()
            && f.sources.source_name(a.span.source_id()) == Ok("provider.ko"),
        (a.statement, a.body, a.span),
    );
    let [source_names] = names.names().source_units() else {
        report.check(
            "unit conditional single real source",
            false,
            names.names().source_units(),
        );
        return;
    };
    let unit = source_names.source_unit();
    let expression = |id| UnitExpressionId::new(unit, id);
    let statement = UnitStatementId::new(unit, a.statement);
    let (Some(d), Some(p)) = (
        typed.map_iteration(statement),
        owned.map_iteration(statement),
    ) else {
        report.check(
            "unit conditional real descriptor/plan",
            false,
            (typed.map_iterations(), owned.map_iterations()),
        );
        return;
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
    let mode = if f.map {
        ParameterMode::Value
    } else {
        ParameterMode::Borrow
    };
    let capability = if f.map {
        OwnershipBindingKind::Owned
    } else {
        OwnershipBindingKind::Shared
    };
    let source = reference(f, names, a.source);
    let source_symbol = source.and_then(|id| {
        source_names
            .resolution()
            .symbols()
            .iter()
            .find(|s| s.id() == id.symbol())
    });
    report.check("unit conditional For/source/K/V identity", source_names.resolution().source_id() == f.parsed.source_id()
        && typed.map_iterations().len() == 1 && owned.map_iterations().len() == 1 && d.statement() == statement
        && d.source() == expression(a.source) && p.descriptor() == d && typed.expression_type(expression(a.source)) == Some(d.source_type())
        && typed.sequential_iteration(statement).is_none() && owned.iteration(statement).is_none()
        && matches!(typed.types().get(d.source_type()), Some(UnitTypeKind::Intrinsic { constructor: actual, arguments }) if *actual == constructor && d.kind() == kind && arguments.as_slice() == [d.key_type(), d.value_type()])
        && [d.key_type(), d.value_type()].iter().all(|t| typed.types().get(*t) == Some(&UnitTypeKind::Builtin(BuiltinType::String))), (unit, d, p));
    report.check("unit conditional direct parameter source capability", source_symbol.is_some_and(|s| s.kind() == SymbolKind::ValueParameter && !s.is_synthetic() && s.name() == "source")
        && source.is_some_and(|id| id.source_unit() == unit && owned.binding_kind(id) == Some(capability))
        && matches!(p.source(), Some(UnitLoanTarget::Place(path)) if path.is_root() && Some(path.root()) == source)
        && matches!(p.source_kind(), IterationSource::BorrowedSource(_))
        && p.source_access() == if f.map { UnitIterationSourceAccess::Owned } else { UnitIterationSourceAccess::Shared }, (source_symbol, p.source_kind(), p.source_access()));
    for function in &a.functions {
        let declarations = names
            .names()
            .index()
            .declarations()
            .iter()
            .filter(|d| {
                d.source_unit() == unit
                    && d.root() == function.item
                    && d.name_span() == function.name_span
                    && d.kind() == SymbolKind::Function
            })
            .collect::<Vec<_>>();
        let callables = typed
            .signatures()
            .declarations()
            .iter()
            .filter_map(|s| s.callable())
            .filter(|c| c.name_span() == function.name_span)
            .collect::<Vec<_>>();
        report.check(
            "unit conditional actual function DeclarationId and signature",
            matches!(declarations.as_slice(), [_]) && matches!(callables.as_slice(), [_]),
            (function, &declarations, &callables),
        );
        if let [callable] = callables.as_slice() {
            if function.name == "scan" {
                report.check("unit conditional actual scan parameter slot/mode", callable.receiver().is_none()
                    && matches!(callable.parameters(), [parameter] if parameter.symbol() == source && parameter.mode() == mode && parameter.ty() == d.source_type()), callable);
            } else if ["read", "operand"].contains(&function.name) {
                report.check(
                    "unit conditional synchronous dual String Borrow parameters",
                    callable.receiver().is_none()
                        && callable.parameters().len() == 2
                        && callable.parameters().iter().all(|p| {
                            p.mode() == ParameterMode::Borrow
                                && typed.types().get(p.ty())
                                    == Some(&UnitTypeKind::Builtin(BuiltinType::String))
                                && p.symbol().is_some_and(|id| {
                                    id.source_unit() == unit
                                        && owned.binding_kind(id)
                                            == Some(OwnershipBindingKind::Shared)
                                })
                        }),
                    callable,
                );
            }
        }
    }
    report.check(
        "unit conditional distinct component bindings/loans",
        d.key_binding() != d.value_binding() && p.loans()[0] != p.loans()[1],
        p.loans(),
    );
    for (i, component) in [MapIterationComponent::Key, MapIterationComponent::Value]
        .into_iter()
        .enumerate()
    {
        let binding = d.bindings()[i];
        let loan = p.loan(component);
        report.check(
            "unit conditional real String Shared component lineage",
            binding.source_unit() == unit
                && has_span(names, binding, a.bindings[i], Some(SymbolKind::ForBinding))
                && typed.symbol_type(binding) == Some([d.key_type(), d.value_type()][i])
                && owned.binding_kind(binding) == Some(OwnershipBindingKind::Shared)
                && p.bindings()[i].symbol() == binding
                && p.bindings()[i].kind() == OwnershipBindingKind::Shared
                && loan.statement() == statement
                && loan.source() == expression(a.source)
                && loan.binding() == binding
                && loan.component() == component
                && loan.kind() == LoanKind::Shared
                && Some(loan.origin()) == p.source(),
            loan,
        );
    }
    for branch in &a.branches {
        report.check(
            "unit conditional If uses actual key and typed String equality",
            reference(f, names, branch.key) == Some(d.key_binding())
                && typed.expression_type(expression(branch.key)) == Some(d.key_type())
                && typed.expression_type(expression(branch.literal)) == Some(d.key_type())
                && typed
                    .expression_type(expression(branch.condition))
                    .and_then(|t| typed.types().get(t))
                    == Some(&UnitTypeKind::Builtin(BuiltinType::Boolean))
                && typed
                    .expression_type(expression(branch.expression))
                    .and_then(|t| typed.types().get(t))
                    == Some(&UnitTypeKind::Builtin(BuiltinType::Unit))
                && typed
                    .expression_type(expression(branch.transfer))
                    .and_then(|t| typed.types().get(t))
                    == Some(&UnitTypeKind::Builtin(BuiltinType::Nothing)),
            (branch, branch.body),
        );
    }
    let locals = owned
        .formed_owned_locals()
        .iter()
        .copied()
        .filter(|id| id.source_unit() == unit && has_span(names, *id, a.local, None))
        .collect::<Vec<_>>();
    let local = match locals.as_slice() {
        [id] => Some(*id),
        _ => None,
    };
    let construction = typed.construction(expression(a.guard));
    report.check("unit conditional actual Guard construction and binding origin", local.is_some()
        && construction.is_some_and(|c| c.expression() == expression(a.guard) && c.arguments().is_empty()
            && typed.expression_type(expression(a.guard)) == Some(c.result_type())
            && local.is_some_and(|id| typed.symbol_type(id) == Some(c.result_type()))
            && typed.is_resource_type(c.result_type()) == Some(true)
            && matches!(c.target(), UnitConstructionTarget::Nominal(id) if names.names().index().declarations().iter().any(|d| d.id() == id && d.source_unit() == unit && d.name_span() == a.guard_name))), (a.local, f.expression(a.guard), construction));
    println!(
        "SPEC0331 CONDITIONAL UNIT GUARD binding={local:?} binding_origin={:?} construction={:#?} drops={:#?}",
        a.local,
        f.expression(a.guard),
        owned
            .drops()
            .iter()
            .filter(|drop| matches!(drop.target(), UnitDropTarget::Named(id) if Some(id) == local))
            .collect::<Vec<_>>()
    );
    for (name, id, arguments) in &a.component_calls {
        let call = typed.call(expression(*id));
        let target = a
            .functions
            .iter()
            .find(|v| v.name == *name)
            .and_then(|v| {
                names.names().index().declarations().iter().find(|d| {
                    d.source_unit() == unit && d.root() == v.item && d.name_span() == v.name_span
                })
            })
            .map(|d| d.id());
        report.check(
            "unit conditional actual component consumer and Borrow arguments",
            call.is_some_and(|c| {
                matches!(c.target(), UnitCallTarget::Declaration(id) if Some(id) == target)
                    && c.receiver().is_none()
                    && c.arguments().len() == 2
                    && c.arguments().iter().enumerate().all(|(i, arg)| {
                        arg.argument_index() == i
                            && arg.parameter_index() == i
                            && arg.mode() == ParameterMode::Borrow
                            && arg.parameter_type() == [d.key_type(), d.value_type()][i]
                            && reference(f, names, arguments[i]) == Some(d.bindings()[i])
                    })
            }),
            (name, expression(*id), call),
        );
        report.check(
            "unit conditional actual consumer result type",
            typed
                .expression_type(expression(*id))
                .and_then(|t| typed.types().get(t))
                == Some(&UnitTypeKind::Builtin(if *name == "operand" {
                    BuiltinType::Int
                } else {
                    BuiltinType::Unit
                })),
            call,
        );
    }
    let operand = a.functions.iter().find(|v| v.name == "operand");
    let read = a
        .functions
        .iter()
        .find(|v| v.name == "read")
        .and_then(|v| {
            names.names().index().declarations().iter().find(|d| {
                d.source_unit() == unit && d.root() == v.item && d.name_span() == v.name_span
            })
        })
        .map(|d| d.id());
    let signature = typed
        .signatures()
        .declarations()
        .iter()
        .filter_map(|d| d.callable())
        .find(|c| operand.is_some_and(|v| c.name_span() == v.name_span));
    let call = typed.call(expression(a.operand_read.0));
    report.check(
        "unit conditional operand forwards its own two Borrow parameters",
        signature.is_some_and(|signature| {
            signature.parameters().len() == 2
                && call.is_some_and(|call| {
                    matches!(call.target(), UnitCallTarget::Declaration(id) if Some(id) == read)
                        && call.receiver().is_none()
                        && call.arguments().len() == 2
                        && call.arguments().iter().enumerate().all(|(i, argument)| {
                            argument.argument_index() == i
                                && argument.parameter_index() == i
                                && argument.mode() == ParameterMode::Borrow
                                && argument.parameter_type() == signature.parameters()[i].ty()
                                && signature.parameters()[i].symbol().is_some()
                                && reference(f, names, a.operand_read.1[i])
                                    == signature.parameters()[i].symbol()
                        })
                })
        }),
        (a.operand_read, signature, call),
    );
    let scan = a
        .functions
        .iter()
        .find(|v| v.name == "scan")
        .and_then(|v| {
            names.names().index().declarations().iter().find(|d| {
                d.source_unit() == unit && d.root() == v.item && d.name_span() == v.name_span
            })
        })
        .map(|d| d.id());
    for entry in &a.entry_calls {
        let call_id = expression(entry.call);
        let argument_id = expression(entry.argument);
        let call = typed.call(call_id);
        let argument_source = reference(f, names, entry.argument);
        let result = reference(f, names, entry.result_reference);
        let int = Some(&UnitTypeKind::Builtin(BuiltinType::Int));
        report.check("unit conditional entry call real scan/slot/mode/source and result reference", call.is_some_and(|c| matches!(c.target(), UnitCallTarget::Declaration(id) if Some(id) == scan)
            && c.receiver().is_none() && matches!(c.arguments(), [arg] if arg.argument_index() == 0 && arg.parameter_index() == 0 && arg.mode() == mode && arg.parameter_type() == d.source_type()))
            && argument_source.is_some_and(|id| has_span(names, id, entry.source_binding, None))
            && result.is_some_and(|id| has_span(names, id, entry.result_binding, None))
            && [entry.call, entry.result_reference, entry.result_literal].iter().all(|id| typed.expression_type(expression(*id)).and_then(|t| typed.types().get(t)) == int)
            && typed.expression_type(expression(entry.comparison)).and_then(|t| typed.types().get(t)) == Some(&UnitTypeKind::Builtin(BuiltinType::Boolean))
            && typed.expression_type(expression(entry.source_initializer)) == Some(d.source_type()), (entry, call, argument_source, result));
        let contracts = owned
            .call_argument_contracts_for(call_id)
            .collect::<Vec<_>>();
        report.check("unit conditional entry actual ownership contract", matches!(contracts.as_slice(), [contract] if contract.argument() == argument_id && contract.parameter_index() == 0
            && contract.kind() == if f.map { UnitCallArgumentOwnershipKind::Value } else { UnitCallArgumentOwnershipKind::SharedLoan }), contracts);
        let deliveries = owned
            .value_deliveries()
            .iter()
            .filter(|delivery| delivery.call() == call_id)
            .collect::<Vec<_>>();
        report.check("unit conditional entry actual Move or Borrow delivery", if f.map {
            matches!(deliveries.as_slice(), [delivery] if delivery.argument() == argument_id && delivery.kind() == UnitValueDeliveryKind::Move && delivery.place().is_some_and(|path| path.is_root() && Some(path.root()) == argument_source))
        } else { deliveries.is_empty() }, deliveries);
    }
    for exit in p.exits() {
        println!(
            "SPEC0331 CONDITIONAL UNIT EXIT actual={exit:#?} complete={:#?}",
            owned.iteration_cleanup_at(exit.point())
        );
    }
    // No unit owner/selector/phi table is invented: the public unit plan has no such accessor.
}
