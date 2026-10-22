//! Shared private ownership analysis; public capability gates remain distinct.
use super::{
    CompilationUnitOwnership, OwnershipBindingKind, OwnershipCheckingError,
    UnitOwnershipBindingDescriptor, capture, contracts, dataflow,
};
use crate::{
    name_resolution::{CompilationUnitNames, SourceUnitInput, UnitSymbolId},
    source::{SourceMap, Span},
    type_checking::{CompilationUnitTypes, ParameterMode, TypeEnvironment, UnitCallableSignature},
};
use std::collections::BTreeMap;

/// Strict 与 recovery 共用算法；任何调用均先复核真实身份，不借宿主授予资格。
pub(super) fn analyze(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'_>],
    names: &CompilationUnitNames,
    environment: &TypeEnvironment,
    typed: &CompilationUnitTypes,
    constant_control: bool,
) -> Result<CompilationUnitOwnership, OwnershipCheckingError> {
    if !typed.is_compatible_with_raw(sources, inputs, names, environment) {
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
    for plan in typed.diagnostic_sequential_iterations() {
        for symbol in plan.binding().symbols() {
            insert_binding(
                names,
                &mut bindings,
                UnitOwnershipBindingDescriptor::new(
                    symbol,
                    binding_kind(plan.delivery()),
                    unit_symbol_span(names, symbol)?,
                ),
            )?;
        }
    }
    // 全表逐项复核真实 For；不从未选择 AST 创建 required roots。
    for plan in typed.diagnostic_map_iterations() {
        let source = names
            .index()
            .source_units()
            .iter()
            .find(|source| source.id() == plan.statement().source_unit());
        let parsed = source
            .and_then(|source| {
                inputs
                    .iter()
                    .find(|input| input.source_id() == source.source_id())
            })
            .map(|input| input.parsed());
        if parsed.is_none_or(|parsed| {
            !crate::type_checking::map_iteration_validation::validate_unit_map_iteration_facts(
                parsed,
                names,
                typed,
                plan.statement(),
                Some(plan),
                typed.diagnostic_sequential_iteration(plan.statement()),
            )
        }) {
            return Err(OwnershipCheckingError::InvalidMapIteration {
                source_unit: Some(plan.statement().source_unit().index()),
                statement: plan.statement().statement().index(),
            });
        }
        for symbol in plan.bindings() {
            insert_binding(
                names,
                &mut bindings,
                UnitOwnershipBindingDescriptor::new(
                    symbol,
                    OwnershipBindingKind::Shared,
                    unit_symbol_span(names, symbol)?,
                ),
            )?;
        }
    }
    let call_argument_contracts = contracts::collect_call_argument_contracts(inputs, names, typed)?;
    let call_receiver_contracts = contracts::collect_call_receiver_contracts(inputs, names, typed)?;
    let capture = capture::analyze(inputs, names, typed)?;
    let mut dataflow = dataflow::analyze(
        sources,
        inputs,
        names,
        typed,
        constant_control,
        &bindings,
        dataflow::CallInputs::new(&call_argument_contracts, &call_receiver_contracts),
        dataflow::ClosureInputs::new(
            &capture.captures,
            &capture.closures,
            &capture.transferabilities,
        ),
    )?;
    if !dataflow.diagnostics.is_empty() || !typed.has_valid_type_facts() {
        dataflow.formed_owned_locals.clear();
        dataflow.receiver_facts.clear();
        dataflow.conditional_receiver_deliveries.clear();
        dataflow.loans.clear();
        dataflow.value_deliveries.clear();
        dataflow.rc_effects.clear();
        dataflow.construction_plans.clear();
        dataflow.non_null_assertions.clear();
        dataflow.nullable_whens.clear();
        dataflow.ownership_primitives.clear();
        dataflow.field_replacements.clear();
        dataflow.drops.clear();
        dataflow.conditional_receiver_drops.clear();
    }

    let owned = CompilationUnitOwnership::new(
        typed,
        bindings.into_values().collect(),
        call_argument_contracts,
        call_receiver_contracts,
        capture,
        dataflow,
    );
    Ok(owned)
}

fn collect_callable_bindings(
    callable: &UnitCallableSignature,
    names: &CompilationUnitNames,
    bindings: &mut BTreeMap<UnitSymbolId, UnitOwnershipBindingDescriptor>,
) -> Result<(), OwnershipCheckingError> {
    if let Some(symbol) = callable.extension_receiver_symbol() {
        insert_binding(
            names,
            bindings,
            UnitOwnershipBindingDescriptor::new(
                symbol,
                OwnershipBindingKind::Shared,
                unit_symbol_span(names, symbol)?,
            ),
        )?;
    }
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
    names: &CompilationUnitNames,
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
    names: &CompilationUnitNames,
    symbol: UnitSymbolId,
) -> Result<Span, OwnershipCheckingError> {
    names
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
