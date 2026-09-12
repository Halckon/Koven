//! Shared private ownership analysis; public capability gates remain distinct.
use super::{
    CompilationUnitOwnership, OwnershipBindingKind, OwnershipCheckingError,
    UnitOwnershipBindingDescriptor, capture, contracts, dataflow,
};
use crate::{
    name_resolution::{SourceUnitInput, UnitSymbolId, ValidatedCompilationUnitNames},
    source::{SourceMap, Span},
    type_checking::{CompilationUnitTypes, ParameterMode, TypeEnvironment, UnitCallableSignature},
};
use std::collections::BTreeMap;

/// Callers provide their validated capability's view; identity is rechecked before any analysis.
pub(super) fn analyze(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'_>],
    names: &ValidatedCompilationUnitNames,
    environment: &TypeEnvironment,
    typed: &CompilationUnitTypes,
) -> Result<CompilationUnitOwnership, OwnershipCheckingError> {
    if !typed.is_compatible_with(sources, inputs, names, environment) {
        return Err(OwnershipCheckingError::MismatchedCompilationUnitTypes);
    }

    let mut bindings = BTreeMap::new();
    for declaration in typed.signatures().declarations() {
        if let Some(callable) = declaration.callable() {
            collect_callable_bindings(callable, names, &mut bindings)?;
        }
        if let Some(nominal) = declaration.nominal() {
            for callable in nominal.members().iter().chain(nominal.companion_members()) {
                collect_callable_bindings(callable, names, &mut bindings)?;
            }
        }
    }
    for (symbol, mode) in typed.body_parameter_modes() {
        let span = unit_symbol_span(names, *symbol)?;
        insert_binding(
            names,
            &mut bindings,
            UnitOwnershipBindingDescriptor::new(*symbol, binding_kind(*mode), span),
        )?;
    }
    let call_argument_contracts = contracts::collect_call_argument_contracts(inputs, names, typed)?;
    let call_receiver_contracts = contracts::collect_call_receiver_contracts(inputs, names, typed)?;
    let capture = capture::analyze(inputs, names, typed)?;
    let mut dataflow = dataflow::analyze(
        sources,
        inputs,
        names,
        typed,
        &bindings,
        dataflow::CallInputs::new(&call_argument_contracts, &call_receiver_contracts),
        dataflow::ClosureInputs::new(
            &capture.captures,
            &capture.closures,
            &capture.transferabilities,
        ),
    )?;
    if !dataflow.diagnostics.is_empty() {
        dataflow.receiver_facts.clear();
        dataflow.conditional_receiver_deliveries.clear();
        dataflow.loans.clear();
        dataflow.value_deliveries.clear();
        dataflow.rc_effects.clear();
        dataflow.construction_plans.clear();
        dataflow.non_null_assertions.clear();
        dataflow.drops.clear();
        dataflow.conditional_receiver_drops.clear();
    }

    Ok(CompilationUnitOwnership::new(
        typed,
        bindings.into_values().collect(),
        call_argument_contracts,
        call_receiver_contracts,
        capture,
        dataflow,
    ))
}

fn collect_callable_bindings(
    callable: &UnitCallableSignature,
    names: &ValidatedCompilationUnitNames,
    bindings: &mut BTreeMap<UnitSymbolId, UnitOwnershipBindingDescriptor>,
) -> Result<(), OwnershipCheckingError> {
    for parameter in callable.parameters() {
        let Some(symbol) = parameter.symbol() else {
            continue;
        };
        insert_binding(
            names,
            bindings,
            UnitOwnershipBindingDescriptor::new(
                symbol,
                binding_kind(parameter.mode()),
                parameter.span(),
            ),
        )?;
    }
    Ok(())
}

fn insert_binding(
    names: &ValidatedCompilationUnitNames,
    bindings: &mut BTreeMap<UnitSymbolId, UnitOwnershipBindingDescriptor>,
    descriptor: UnitOwnershipBindingDescriptor,
) -> Result<(), OwnershipCheckingError> {
    let symbol = descriptor.symbol();
    unit_symbol_span(names, symbol)?;
    if bindings.insert(symbol, descriptor).is_some() {
        return Err(OwnershipCheckingError::DuplicateUnitBinding {
            source_unit: symbol.source_unit().index(),
            symbol: symbol.symbol().index(),
        });
    }
    Ok(())
}

fn unit_symbol_span(
    names: &ValidatedCompilationUnitNames,
    symbol: UnitSymbolId,
) -> Result<Span, OwnershipCheckingError> {
    names
        .names()
        .source_units()
        .get(symbol.source_unit().index())
        .and_then(|source| source.resolution().symbols().get(symbol.symbol().index()))
        .map(|symbol| symbol.span())
        .ok_or(OwnershipCheckingError::InvalidUnitSymbol {
            source_unit: symbol.source_unit().index(),
            symbol: symbol.symbol().index(),
        })
}

const fn binding_kind(mode: ParameterMode) -> OwnershipBindingKind {
    match mode {
        ParameterMode::Value => OwnershipBindingKind::Owned,
        ParameterMode::Borrow => OwnershipBindingKind::Shared,
        ParameterMode::Inout => OwnershipBindingKind::Exclusive,
    }
}
