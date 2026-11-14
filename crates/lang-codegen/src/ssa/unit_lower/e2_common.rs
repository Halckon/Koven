//! Sealed common-arena experimental package, never ordinary Unit authority.
use super::*;
use crate::ssa::unit_plan::UnitFunctionInstanceKey;
use crate::ssa::{
    e2_arm_fragments, e2_callee_leaves, e2_entry, e2_entry_replay, e2_function, e2_replay,
};
use lang_frontend::ownership_checking::two_source_experiment::{
    EntryDescriptor, ExperimentalUnitHandoff, TwoSourceCounterFact, TwoSourceExperiment,
};
use lang_frontend::type_checking::{UnitCallTarget, UnitCallableInstanceKey};

fn key(k: &UnitCallableInstanceKey) -> Option<UnitFunctionInstanceKey> {
    let UnitCallTarget::Declaration(d) = k.target() else {
        return None;
    };
    k.type_arguments()
        .is_empty()
        .then(|| UnitFunctionInstanceKey::for_entry(d))
}

/// Constructor is confined to the fully checked emitter below. No mutable IR or ownership escapes.
pub(crate) struct PublishedCommon<'a> {
    program: Program,
    experiment: &'a TwoSourceExperiment,
    facts: Vec<&'a TwoSourceCounterFact>,
    entries: Vec<(&'a EntryDescriptor, FunctionId)>,
}
impl PublishedCommon<'_> {
    pub(crate) fn matches_handoff(&self, handoff: &ExperimentalUnitHandoff<'_, '_>) -> bool {
        self.experiment.matches_types(handoff.types()) && handoff.for_facts(&self.facts).is_some()
    }
    pub(crate) fn entry(
        &self,
        descriptor: &EntryDescriptor,
        typed: &lang_frontend::type_checking::ValidatedCompilationUnitTypes,
    ) -> Option<FunctionId> {
        if !self.experiment.matches_types(typed) || !self.experiment.owns_entry(descriptor) {
            return None;
        }
        let mut found = self
            .entries
            .iter()
            .filter(|(original, _)| original.entry() == descriptor.entry());
        let (_, id) = found.next()?;
        if found.next().is_some() {
            return None;
        }
        Some(*id)
    }
    pub(crate) fn program(&self) -> &Program {
        &self.program
    }
}

pub(in crate::ssa) fn publish<'a>(
    handoff: &ExperimentalUnitHandoff<'_, '_>,
    joint: crate::ssa::e2_joint_plan::JointPlan<'a>,
) -> Option<PublishedCommon<'a>> {
    let (experiment, plan, leaves, callers, entries) = joint.into_common(handoff)?;
    let facts: Vec<_> = callers.iter().map(|(fact, _)| *fact).collect();
    let selected = handoff.for_facts(&facts)?;
    let handoff = &selected;
    #[cfg(test)]
    let _branch_publication = crate::ssa::refine::branch_receipt::UnitPublication::begin();
    #[cfg(test)]
    let _nested_publication = crate::ssa::refine::nested_receipt::UnitPublication::begin();
    #[cfg(test)]
    let _bounded_publication = crate::ssa::refine::bounded_receipt::UnitPublication::begin();
    #[cfg(test)]
    let _loop_publication = crate::ssa::refine::loop_receipt::UnitPublication::begin();
    #[cfg(test)]
    let _fresh_loop_publication = crate::ssa::refine::fresh_loop_receipt::UnitPublication::begin();
    #[cfg(test)]
    let _arm_loop_publication = crate::ssa::refine::arm_loop_receipt::UnitPublication::begin();
    #[cfg(test)]
    let _single_publication = crate::ssa::refine::single_receipt::UnitPublication::begin();
    #[cfg(test)]
    let _multi_publication = crate::ssa::refine::multi_receipt::UnitPublication::begin();
    #[cfg(test)]
    let _aux_publication = crate::ssa::refine::aux_binding_receipt::UnitPublication::begin();
    #[cfg(test)]
    let _distinct_aux_publication =
        crate::ssa::refine::distinct_aux_receipt::UnitPublication::begin();
    #[cfg(test)]
    let _later_aux_publication = crate::ssa::refine::later_aux_receipt::UnitPublication::begin();
    #[cfg(test)]
    let _auxiliary_sequence_publication =
        crate::ssa::refine::auxiliary_sequence_receipt::UnitPublication::begin();
    #[cfg(test)]
    let _interleaved_aux_publication =
        crate::ssa::refine::interleaved_aux_receipt::UnitPublication::begin();
    #[cfg(test)]
    let _multi_binding_publication =
        crate::ssa::refine::multi_binding_receipt::UnitPublication::begin();
    let parsed =
        crate::ssa::lowering_support::parsed_by_source_unit(handoff.inputs(), handoff.names())
            .ok()?;
    let typed = handoff.types().types();
    super::preflight::validate(&parsed, handoff.names(), typed, handoff).ok()?;
    let expected: BTreeMap<_, _> = plan.iter().map(|p| (p.key().clone(), p.span())).collect();
    if expected.len() != plan.len() {
        return None;
    }
    let mut program = Program::default();
    let module_id = program.add_module("main");
    let module = program.module_mut(module_id)?;
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
        &parsed,
        handoff.names(),
        typed,
        handoff,
        plan,
    )
    .ok()?;
    // This experiment has no hidden thunk route; every original instance has one explicit body owner.
    if !callable_plans.is_empty()
        || function_ids.len() != expected.len()
        || module.functions.len() != expected.len()
    {
        return None;
    }
    let known_functions = callable_abi.known_functions(&function_ids, &source_plan);
    let scopes: Vec<crate::ssa::refine::WhileScopes> =
        (0..parsed.len()).map(|_| Default::default()).collect();
    let context = super::planned_body::PlannedBodyContext {
        sources: handoff.sources(),
        parsed_by_source: &parsed,
        while_scopes: &scopes,
        names: handoff.names(),
        typed,
        owned: handoff,
        constant_owned: None,
        function_ids: &function_ids,
        callable_returns: &callable_returns,
        known_functions: &known_functions,
        source_plan: &source_plan,
        callable_abi: &callable_abi,
        callable_plans: &callable_plans,
    };
    let mut functions = Vec::new();
    for body in plans {
        let original = body.instance.key();
        if function_ids.get(original) != Some(&body.id)
            || expected.get(original) != Some(&body.instance.span())
        {
            return None;
        }
        let leaf_matches: Vec<_> = leaves
            .iter()
            .filter(|leaf| key(leaf.instance()).as_ref() == Some(original))
            .collect();
        let caller_count = callers
            .iter()
            .filter(|(_, k)| key(k).as_ref() == Some(original))
            .count();
        let entry_count = entries
            .iter()
            .filter(|(e, _)| &UnitFunctionInstanceKey::for_entry(e.entry()) == original)
            .count();
        if leaf_matches.len() + caller_count + entry_count != 1 {
            return None;
        }
        if let [leaf] = leaf_matches.as_slice() {
            functions.push((leaf.instance().clone(), body.id));
            context.emit(module, &mut types, body).ok()?;
        }
    }
    if functions.len() != leaves.len() {
        return None;
    }
    // Independently replay original ordinary bodies against source witnesses and retained plan spans.
    for leaf in &leaves {
        let k = key(leaf.instance())?;
        let id = *function_ids.get(&k)?;
        e2_replay::replay_declared_leaf(module, id, leaf, &functions, *expected.get(&k)?)?;
    }
    let mut emitted = e2_callee_leaves::EmittedLeaves { program, functions };
    let mut arm_records = Vec::new();
    for (fact, k) in &callers {
        let arms = e2_arm_fragments::append_unverified(experiment, fact, handoff.types(), emitted)?;
        emitted = arms.leaves;
        let module = emitted.program.module_mut(module_id)?;
        let shell = experiment.function_shell(fact, handoff.types())?;
        let caller_key = key(k)?;
        let id = *function_ids.get(&caller_key)?;
        e2_function::fill_caller(
            module,
            id,
            shell,
            arms.arms,
            arms.columns,
            experiment.arm_spans(fact)?,
        )?;
        e2_replay::replay_arms(
            module,
            &arms.arms,
            arms.columns,
            &emitted.functions,
            experiment,
            fact,
            handoff.types(),
        )?;
        e2_replay::replay_declared_caller(
            module,
            id,
            arms.arms,
            arms.columns,
            shell,
            experiment.arm_spans(fact)?,
            *expected.get(&caller_key)?,
        )?;
        arm_records.push(arms.arms);
    }
    for (entry, caller) in &entries {
        let entry_key = UnitFunctionInstanceKey::for_entry(entry.entry());
        let id = *function_ids.get(&entry_key)?;
        let caller = *function_ids.get(&key(caller)?)?;
        let module = emitted.program.module_mut(module_id)?;
        e2_entry::fill_entry(module, id, caller, entry)?;
        e2_entry_replay::replay_declared_body(
            module,
            id,
            caller,
            entry,
            *expected.get(&entry_key)?,
        )?;
    }
    let module = emitted.program.module(module_id)?;
    let source_ids: std::collections::BTreeSet<_> = function_ids.values().copied().collect();
    let helper_ids: std::collections::BTreeSet<_> = arm_records.iter().flatten().copied().collect();
    if source_ids.len() != expected.len()
        || helper_ids.len() != 2 * callers.len()
        || !source_ids.is_disjoint(&helper_ids)
        || module.functions.len() != source_ids.len() + helper_ids.len()
    {
        return None;
    }
    crate::ssa::verify::verify_program(&emitted.program).ok()?;
    let entries = entries
        .iter()
        .map(|(entry, _)| {
            Some((
                *entry,
                *function_ids.get(&UnitFunctionInstanceKey::for_entry(entry.entry()))?,
            ))
        })
        .collect::<Option<Vec<_>>>()?;
    Some(PublishedCommon {
        program: emitted.program,
        experiment,
        facts: callers.iter().map(|(fact, _)| *fact).collect(),
        entries,
    })
}
