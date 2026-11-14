//! unit 实例规划、函数入口与完整 SSA module 组装。
use lang_frontend::ownership_checking::UnitLoweringFacts;

#[cfg(test)]
mod pr120_range_source_contract_tests;
#[cfg(test)]
mod pr120_verifier_diagnostics;
use super::*;
use crate::ssa::refine::CallableRoute;
#[cfg(all(test, feature = "loan-observation-test-support"))]
pub(in crate::ssa) use pr120_verifier_diagnostics::observe_final_verifier;

/// 共享 lowering 只读取本轮事实；调用方必须先验证完整分析身份链。
pub(in crate::ssa) fn lower_unit_from_facts(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'_>],
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &dyn UnitLoweringFacts,
    constant_owned: Option<&ConstEnabledOwnedUnit>,
    entry: DeclarationId,
) -> Result<(Program, FunctionId), LoweringError> {
    lower_unit_impl(
        (sources, inputs),
        names,
        typed,
        owned,
        constant_owned,
        entry,
        #[cfg(test)]
        None,
    )
}

mod root_set;
use root_set::lower_unit_impl;

// One assembly transaction for all roots; production still calls the singleton adapter.
fn lower_unit_roots_impl(
    source_inputs: (&SourceMap, &[SourceUnitInput<'_>]),
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &dyn UnitLoweringFacts,
    constant_owned: Option<&ConstEnabledOwnedUnit>,
    roots: (&[DeclarationId], bool),
    #[cfg(test)] capture: Option<&mut pr120_verifier_diagnostics::Capture>,
) -> Result<(Program, BTreeMap<DeclarationId, FunctionId>), LoweringError> {
    let (entries, top_level_only) = roots;
    #[cfg(test)]
    let branch_publication = crate::ssa::refine::branch_receipt::UnitPublication::begin();
    #[cfg(test)]
    let nested_publication = crate::ssa::refine::nested_receipt::UnitPublication::begin();
    #[cfg(test)]
    let bounded_publication = crate::ssa::refine::bounded_receipt::UnitPublication::begin();
    #[cfg(test)]
    let loop_publication = crate::ssa::refine::loop_receipt::UnitPublication::begin();
    #[cfg(test)]
    let fresh_loop_publication = crate::ssa::refine::fresh_loop_receipt::UnitPublication::begin();
    #[cfg(test)]
    let arm_loop_publication = crate::ssa::refine::arm_loop_receipt::UnitPublication::begin();
    #[cfg(test)]
    let single_publication = crate::ssa::refine::single_receipt::UnitPublication::begin();
    #[cfg(test)]
    let multi_publication = crate::ssa::refine::multi_receipt::UnitPublication::begin();
    #[cfg(test)]
    let aux_publication = crate::ssa::refine::aux_binding_receipt::UnitPublication::begin();
    #[cfg(test)]
    let distinct_aux_publication =
        crate::ssa::refine::distinct_aux_receipt::UnitPublication::begin();
    #[cfg(test)]
    let later_aux_publication = crate::ssa::refine::later_aux_receipt::UnitPublication::begin();
    #[cfg(test)]
    let auxiliary_sequence_publication =
        crate::ssa::refine::auxiliary_sequence_receipt::UnitPublication::begin();
    #[cfg(test)]
    let interleaved_aux_publication =
        crate::ssa::refine::interleaved_aux_receipt::UnitPublication::begin();
    #[cfg(test)]
    let multi_binding_publication =
        crate::ssa::refine::multi_binding_receipt::UnitPublication::begin();
    let (sources, inputs) = source_inputs;
    let parsed_by_source = super::super::lowering_support::parsed_by_source_unit(inputs, names)?;
    let while_scopes: Vec<crate::ssa::refine::WhileScopes> = (0..parsed_by_source.len())
        .map(|_| Default::default())
        .collect();
    super::preflight::validate(&parsed_by_source, names, typed, owned)?;
    let instance_plan = if top_level_only {
        super::super::unit_plan::plan_unit_root_set_from_facts(
            &parsed_by_source,
            names,
            typed,
            owned,
            entries,
            MAX_UNIT_GENERIC_INSTANCES,
            true,
        )?
    } else {
        let [entry] = entries else {
            return Err(LoweringError {
                kind: LoweringErrorKind::InvalidModel,
                span: None,
            });
        };
        plan_unit_instances_from_facts(
            &parsed_by_source,
            names,
            typed,
            owned,
            *entry,
            MAX_UNIT_GENERIC_INSTANCES,
        )?
    };
    let mut program = Program::default();
    let module_id = program.add_module("main");
    let module = program
        .module_mut(module_id)
        .expect("new unit module must exist");
    let super::planned_declarations::DeclaredUnit {
        mut types,
        function_ids,
        callable_returns,
        plans,
        source_plan,
        callable_abi,
        callable_plans,
    } = super::planned_declarations::declare(
        module,
        &parsed_by_source,
        names,
        typed,
        owned,
        instance_plan,
    )?;

    let entry_ids = entries
        .iter()
        .map(|entry| {
            let id = function_ids
                .get(&UnitFunctionInstanceKey::for_entry(*entry))
                .copied()
                .ok_or(LoweringError {
                    kind: LoweringErrorKind::MissingFact,
                    span: None,
                })?;
            Ok((*entry, id))
        })
        .collect::<Result<BTreeMap<_, _>, LoweringError>>()?;
    let known_functions = callable_abi.known_functions(&function_ids, &source_plan);

    let body_context = super::planned_body::PlannedBodyContext {
        sources,
        parsed_by_source: &parsed_by_source,
        while_scopes: &while_scopes,
        names,
        typed,
        owned,
        constant_owned,
        function_ids: &function_ids,
        callable_returns: &callable_returns,
        known_functions: &known_functions,
        source_plan: &source_plan,
        callable_abi: &callable_abi,
        callable_plans: &callable_plans,
    };
    for plan in plans {
        body_context.emit(module, &mut types, plan)?;
    }

    for plan in callable_plans.values() {
        let parsed = parsed_by_source[plan.source_unit.index()];
        let references = symbol_references(names, plan.source_unit, Namespace::Value);
        let refine_callables = closure::refine_callables(&callable_plans, plan.thunk);
        let entry = module.functions[plan.thunk.index()]
            .entry_block()
            .expect("planned unit closure thunk has entry");
        let entities = module.functions[plan.thunk.index()]
            .block(entry)
            .expect("entry block exists")
            .parameters
            .clone();
        let inputs = crate::ssa::refine::RefineInputs {
            sources,
            parsed,
            while_scopes: &while_scopes[plan.source_unit.index()],
            source_unit: plan.source_unit,
            names,
            typed,
            owned,
            constant_owned,
            function_ids: &function_ids,
            callable_returns: &callable_returns,
            known_functions: &known_functions,
            source_plan: &source_plan,
            source_token: plan.source_token,
            references: &references,
            function_item: None,
            body: crate::ssa::refine::RefineBody::Lambda(closure::refine_thunk(plan)),
            return_type: plan.return_type,
            receiver: None,
            parameters: plan.parameter_symbols.clone(),
            modes: plan.parameter_modes.clone(),
            span: plan.span,
            substitutions: &plan.substitutions,
            static_self: plan.static_self,
            callables: &refine_callables,
            views: callable_abi.views(),
            borrow_return: None,
            carrier_return: None,
        };
        if !matches!(crate::ssa::refine::route(&inputs), CallableRoute::Refine) {
            return Err(lowering_error(
                LoweringErrorKind::UnsupportedNode,
                plan.span,
            ));
        }
        for ty in crate::ssa::refine::type_demands(&inputs) {
            types.intern(module, typed, ty, plan.span)?;
        }
        let function = module
            .functions
            .get_mut(plan.thunk.index())
            .expect("planned unit closure thunk exists");
        let refine_types = crate::ssa::refine::RefineTypes {
            type_ids: types.type_ids(),
            heap_payloads: types.heap_payloads(),
            field_indices: types.field_indices(),
            enum_payloads: types.enum_payloads(),
            map_results: &module.map_results,
            nullable_maps: &module.nullable_maps,
            ssa_types: &module.types,
        };
        crate::ssa::refine::lower(&inputs, function, entry, &entities, &refine_types)?;
    }

    #[cfg(test)]
    crate::ssa::refine::fresh_loop_receipt::before_final_verifier(&mut program);
    #[cfg(test)]
    crate::ssa::refine::arm_loop_receipt::before_final_verifier(&mut program);
    let verification = verify_program(&program);
    #[cfg(test)]
    pr120_verifier_diagnostics::record_final_probe(&program, &verification);
    #[cfg(test)]
    if let Some(capture) = capture {
        capture.observe(
            &program,
            &verification,
            (sources, names, typed, owned),
            &function_ids,
            &types,
            &source_plan,
        );
    }
    verification.map_err(|_| LoweringError {
        kind: LoweringErrorKind::InvalidModel,
        span: None,
    })?;
    #[cfg(test)]
    branch_publication.commit();
    #[cfg(test)]
    nested_publication.commit();
    #[cfg(test)]
    bounded_publication.commit();
    #[cfg(test)]
    loop_publication.commit();
    #[cfg(test)]
    fresh_loop_publication.commit();
    #[cfg(test)]
    arm_loop_publication.commit();
    #[cfg(test)]
    single_publication.commit();
    #[cfg(test)]
    multi_publication.commit();
    #[cfg(test)]
    aux_publication.commit();
    #[cfg(test)]
    distinct_aux_publication.commit();
    #[cfg(test)]
    later_aux_publication.commit();
    #[cfg(test)]
    auxiliary_sequence_publication.commit();
    #[cfg(test)]
    interleaved_aux_publication.commit();
    #[cfg(test)]
    multi_binding_publication.commit();
    Ok((program, entry_ids))
}
