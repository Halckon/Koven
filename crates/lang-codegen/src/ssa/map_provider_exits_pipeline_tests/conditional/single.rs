//! Same-analysis single-file facts. Exit/condition/owner tables are observations, not a new oracle.
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
        BuiltinType, CallableTarget, ConstructionTarget, IntrinsicTypeConstructor,
        MapIterationKind, ParameterMode, TypeKind, TypedFile,
    },
};

fn reference(names: &NameResolution, span: Span) -> Option<SymbolId> {
    let refs = names
        .references()
        .iter()
        .filter(|r| r.span() == span)
        .collect::<Vec<_>>();
    match refs.as_slice() {
        [r] => match r.target() {
            ReferenceTarget::Symbol(id) => Some(*id),
            _ => None,
        },
        _ => None,
    }
}

fn symbol(names: &NameResolution, span: Span) -> Option<SymbolId> {
    let symbols = names
        .symbols()
        .iter()
        .filter(|s| s.span() == span && !s.is_synthetic())
        .collect::<Vec<_>>();
    match symbols.as_slice() {
        [s] => Some(s.id()),
        _ => None,
    }
}

fn ref_at(f: &Input, names: &NameResolution, expression: ExpressionId) -> Option<SymbolId> {
    f.expression(expression)
        .ok()
        .and_then(|(_, span)| reference(names, span))
}

pub(super) fn observe(
    f: &Input,
    anchors: Option<&Anchors>,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
    report: &mut Report,
) {
    // Print before any optional query so a broken query never hides genuine products.
    println!(
        "SPEC0331 CONDITIONAL SINGLE REAL source={:?} maps={:#?} drops={:#?} cleanup={:#?} conditions={:#?} loans={:#?} loan_ends={:#?}",
        typed.source_id(),
        owned.map_iterations(),
        owned.drops(),
        owned.cleanup_steps(),
        owned.cleanup_conditions(),
        owned.loans(),
        owned.loan_ends()
    );
    let Some(a) = anchors else { return };
    report.check(
        "conditional actual For source location",
        a.span.source_id() == f.parsed.source_id()
            && f.sources.source_name(a.span.source_id()) == Ok("provider.ko"),
        (a.statement, a.body, a.span),
    );
    let (Some(d), Some(p)) = (
        typed.map_iteration(a.statement),
        owned.map_iteration(a.statement),
    ) else {
        report.check(
            "single conditional real descriptor/plan",
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
    let source = ref_at(f, names, a.source);
    let source_symbol = source.and_then(|id| names.symbols().iter().find(|s| s.id() == id));
    report.check("single conditional For/source/K/V identity", typed.map_iterations().len() == 1 && owned.map_iterations().len() == 1
        && d.statement() == a.statement && d.source() == a.source && p.descriptor() == d
        && typed.expression_type(a.source) == Some(d.source_type())
        && typed.sequential_iteration(a.statement).is_none() && owned.iteration(a.statement).is_none()
        && matches!(typed.types().get(d.source_type()), Some(TypeKind::Intrinsic { constructor: actual, arguments }) if *actual == constructor && d.kind() == kind && arguments.as_slice() == [d.key_type(), d.value_type()])
        && [d.key_type(), d.value_type()].iter().all(|t| typed.types().get(*t) == Some(&TypeKind::Builtin(BuiltinType::String))), (d, p));
    report.check("single conditional direct parameter source capability", source_symbol.is_some_and(|s| s.kind() == SymbolKind::ValueParameter && !s.is_synthetic() && s.name() == "source")
        && matches!(p.source(), Some(LoanTarget::Place(path)) if path.is_root() && Some(path.root()) == source)
        && matches!(p.source_kind(), IterationSource::BorrowedSource(_))
        && p.source_access() == if f.map { MapIterationSourceAccess::Owned } else { MapIterationSourceAccess::Shared }
        && source.is_some_and(|id| owned.binding_kind(id) == Some(capability)), (source_symbol, p.source_kind(), p.source_access()));
    for function in &a.functions {
        let id = symbol(names, function.name_span);
        let callables = typed
            .callables()
            .iter()
            .filter(|c| Some(c.symbol()) == id)
            .collect::<Vec<_>>();
        report.check(
            "single conditional actual callable AST/symbol identity",
            id.is_some() && matches!(callables.as_slice(), [_]),
            (function, id, &callables),
        );
        if let [callable] = callables.as_slice() {
            if function.name == "scan" {
                report.check("single conditional actual scan parameter slot/mode", callable.receiver().is_none() && callable.parameter_symbols() == [source]
                    && matches!(callable.parameters(), [parameter] if parameter.mode == mode && parameter.ty == d.source_type()), callable);
            } else if ["read", "operand"].contains(&function.name) {
                report.check(
                    "single conditional synchronous dual String Borrow parameters",
                    callable.receiver().is_none()
                        && callable.parameters().len() == 2
                        && callable.parameter_symbols().len() == 2
                        && callable.parameters().iter().all(|p| {
                            p.mode == ParameterMode::Borrow
                                && typed.types().get(p.ty)
                                    == Some(&TypeKind::Builtin(BuiltinType::String))
                        })
                        && callable.parameter_symbols().iter().all(|id| {
                            id.is_some_and(|id| {
                                owned.binding_kind(id) == Some(OwnershipBindingKind::Shared)
                            })
                        }),
                    callable,
                );
            }
        }
    }
    report.check(
        "single conditional distinct component bindings/loans",
        d.key_binding() != d.value_binding() && p.loans()[0] != p.loans()[1],
        p.loans(),
    );
    for (i, component) in [MapIterationComponent::Key, MapIterationComponent::Value]
        .into_iter()
        .enumerate()
    {
        let binding = d.bindings()[i];
        let actual = names.symbols().iter().find(|s| s.id() == binding);
        let loan = p.loan(component);
        report.check(
            "single conditional real String Shared component lineage",
            actual.is_some_and(|s| {
                s.span() == a.bindings[i] && s.kind() == SymbolKind::ForBinding && !s.is_synthetic()
            }) && typed.symbol_type(binding) == Some([d.key_type(), d.value_type()][i])
                && owned.binding_kind(binding) == Some(OwnershipBindingKind::Shared)
                && p.bindings()[i].symbol() == binding
                && p.bindings()[i].kind() == OwnershipBindingKind::Shared
                && loan.statement() == a.statement
                && loan.source() == a.source
                && loan.binding() == binding
                && loan.component() == component
                && loan.kind() == LoanKind::Shared
                && Some(loan.origin()) == p.source(),
            (actual, loan),
        );
    }
    for branch in &a.branches {
        report.check(
            "single conditional If uses actual key and typed String equality",
            ref_at(f, names, branch.key) == Some(d.key_binding())
                && typed.expression_type(branch.key) == Some(d.key_type())
                && typed.expression_type(branch.literal) == Some(d.key_type())
                && typed
                    .expression_type(branch.condition)
                    .and_then(|t| typed.types().get(t))
                    == Some(&TypeKind::Builtin(BuiltinType::Boolean))
                && typed
                    .expression_type(branch.expression)
                    .and_then(|t| typed.types().get(t))
                    == Some(&TypeKind::Builtin(BuiltinType::Unit))
                && typed
                    .expression_type(branch.transfer)
                    .and_then(|t| typed.types().get(t))
                    == Some(&TypeKind::Builtin(BuiltinType::Nothing)),
            (branch, branch.body),
        );
    }
    let local = symbol(names, a.local);
    let construction = typed.construction(a.guard);
    report.check("single conditional actual Guard construction and binding origin", local.is_some_and(|id| owned.formed_owned_locals().contains(&id))
        && construction.is_some_and(|c| c.expression() == a.guard && c.arguments().is_empty()
            && typed.expression_type(a.guard) == Some(c.result_type())
            && local.is_some_and(|id| typed.symbol_type(id) == Some(c.result_type()))
            && typed.is_resource_type(c.result_type()) == Some(true)
            && matches!(c.target(), ConstructionTarget::Nominal(nominal) if symbol(names, a.guard_name) == Some(nominal.symbol()))), (a.local, f.expression(a.guard), construction));
    println!("SPEC0331 CONDITIONAL SINGLE GUARD binding={local:?} binding_origin={:?} construction={:#?} drops={:#?}", a.local, f.expression(a.guard), owned.drops().iter().filter(|drop| matches!(drop.target(), lang_frontend::ownership_checking::DropTarget::Named(id) if Some(id) == local)).collect::<Vec<_>>());
    for (name, expression, arguments) in &a.component_calls {
        let call = typed.call(*expression);
        let target = a
            .functions
            .iter()
            .find(|v| v.name == *name)
            .and_then(|v| symbol(names, v.name_span));
        report.check(
            "single conditional actual component consumer and Borrow arguments",
            call.is_some_and(|c| {
                matches!(c.target(), CallableTarget::Source(id) if Some(id) == target)
                    && c.receiver().is_none()
                    && c.arguments().len() == 2
                    && c.arguments().iter().enumerate().all(|(i, arg)| {
                        arg.argument_index() == i
                            && arg.parameter_index() == i
                            && arg.mode() == ParameterMode::Borrow
                            && arg.parameter_type() == [d.key_type(), d.value_type()][i]
                            && ref_at(f, names, arguments[i]) == Some(d.bindings()[i])
                    })
            }),
            (name, expression, call),
        );
        report.check(
            "single conditional actual consumer result type",
            typed
                .expression_type(*expression)
                .and_then(|t| typed.types().get(t))
                == Some(&TypeKind::Builtin(if *name == "operand" {
                    BuiltinType::Int
                } else {
                    BuiltinType::Unit
                })),
            call,
        );
    }
    let operand = a
        .functions
        .iter()
        .find(|v| v.name == "operand")
        .and_then(|v| symbol(names, v.name_span));
    let read = a
        .functions
        .iter()
        .find(|v| v.name == "read")
        .and_then(|v| symbol(names, v.name_span));
    let signature = typed
        .callables()
        .iter()
        .find(|c| Some(c.symbol()) == operand);
    let call = typed.call(a.operand_read.0);
    report.check(
        "single conditional operand forwards its own two Borrow parameters",
        signature.is_some_and(|signature| {
            signature.parameter_symbols().len() == 2
                && signature.parameters().len() == 2
                && call.is_some_and(|call| {
                    matches!(call.target(), CallableTarget::Source(id) if Some(id) == read)
                        && call.receiver().is_none()
                        && call.arguments().len() == 2
                        && call.arguments().iter().enumerate().all(|(i, argument)| {
                            argument.argument_index() == i
                                && argument.parameter_index() == i
                                && argument.mode() == ParameterMode::Borrow
                                && argument.parameter_type() == signature.parameters()[i].ty
                                && signature.parameter_symbols()[i].is_some()
                                && ref_at(f, names, a.operand_read.1[i])
                                    == signature.parameter_symbols()[i]
                        })
                })
        }),
        (a.operand_read, signature, call),
    );
    let scan = a
        .functions
        .iter()
        .find(|v| v.name == "scan")
        .and_then(|v| symbol(names, v.name_span));
    for entry in &a.entry_calls {
        let call = typed.call(entry.call);
        let argument_source = symbol(names, entry.source_binding);
        let result = symbol(names, entry.result_binding);
        let int = Some(&TypeKind::Builtin(BuiltinType::Int));
        report.check("single conditional entry call real scan/slot/mode/source and result reference", call.is_some_and(|c| matches!(c.target(), CallableTarget::Source(id) if Some(id) == scan)
            && c.receiver().is_none() && matches!(c.arguments(), [arg] if arg.argument_index() == 0 && arg.parameter_index() == 0 && arg.mode() == mode && arg.parameter_type() == d.source_type()))
            && argument_source.is_some() && ref_at(f, names, entry.argument) == argument_source
            && result.is_some() && ref_at(f, names, entry.result_reference) == result
            && [entry.call, entry.result_reference, entry.result_literal].iter().all(|id| typed.expression_type(*id).and_then(|t| typed.types().get(t)) == int)
            && typed.expression_type(entry.comparison).and_then(|t| typed.types().get(t)) == Some(&TypeKind::Builtin(BuiltinType::Boolean))
            && typed.expression_type(entry.source_initializer) == Some(d.source_type()), (entry, call, argument_source, result));
        let loans = owned
            .loans()
            .iter()
            .filter(|loan| loan.call() == entry.call)
            .collect::<Vec<_>>();
        report.check("single conditional entry actual Value or Shared delivery", if f.map { loans.is_empty() } else {
            matches!(loans.as_slice(), [loan] if loan.argument() == entry.argument && loan.kind() == LoanKind::Shared && matches!(loan.target(), LoanTarget::Place(path) if path.is_root() && Some(path.root()) == argument_source))
        }, loans);
    }
    // No expected exit count, owner topology, synthetic Return incoming, or cleanup-shape assertion.
    for exit in p.exits() {
        let full = owned
            .cleanup_steps()
            .iter()
            .filter(|(point, _)| *point == exit.point())
            .collect::<Vec<_>>();
        println!(
            "SPEC0331 CONDITIONAL SINGLE EXIT actual={exit:#?} condition={:#?} complete={full:#?}",
            owned.cleanup_conditions().get(exit.condition())
        );
    }
    println!(
        "SPEC0331 CONDITIONAL SINGLE PHIS actual={:#?} incomings={:#?}",
        p.closure_phis(),
        p.closure_phi_incomings()
    );
}
