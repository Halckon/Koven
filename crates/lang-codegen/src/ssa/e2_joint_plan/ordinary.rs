//! Same-analysis ordinary planner reconciliation, not a merged callable arena.
use super::*;
use crate::ssa::unit_plan::{
    MAX_UNIT_GENERIC_INSTANCES, UnitFunctionInstanceKey, UnitInstancePlan,
    plan_unit_instances_from_facts,
};
use lang_frontend::name_resolution::DeclarationId;
use std::collections::BTreeSet;

/// Each entry retains its complete runtime demands and callable arena. Tokens from
/// separate plans must never be joined or passed to a different body's context.
pub(super) struct OrdinaryPlans(Vec<(DeclarationId, UnitInstancePlan)>);
impl OrdinaryPlans {
    pub(super) fn build(
        selected: &ExperimentalUnitHandoff<'_, '_>,
        experiment: &TwoSourceExperiment,
        entries: &[(&EntryDescriptor, UnitCallableInstanceKey)],
        facts: &[&TwoSourceCounterFact],
    ) -> Option<Self> {
        let parsed = crate::ssa::lowering_support::parsed_by_source_unit(
            selected.inputs(),
            selected.names(),
        )
        .ok()?;
        let mut plans = Vec::new();
        for (entry, key) in entries {
            let mut owners = facts.iter().copied().filter(|fact| {
                experiment.callable(fact).map(UnitCallTarget::Declaration) == Some(key.target())
            });
            let fact = owners.next()?;
            if owners.next().is_some() {
                return None;
            }
            let plan = plan_unit_instances_from_facts(
                &parsed,
                selected.names(),
                selected.types().types(),
                selected,
                entry.entry(),
                MAX_UNIT_GENERIC_INSTANCES,
            )
            .ok()?;
            let expected = required(experiment, fact, entry, key, selected.types())?;
            check(&plan, &expected)?;
            plans.push((entry.entry(), plan));
        }
        Some(Self(plans))
    }
    pub(super) fn verify(&self, joint: &JointPlan<'_>) -> Option<()> {
        if self.0.len() != joint.entries.len() {
            return None;
        }
        for (entry, key) in &joint.entries {
            let mut plans = self.0.iter().filter(|(id, _)| *id == entry.entry());
            let (_, plan) = plans.next()?;
            if plans.next().is_some() {
                return None;
            }
            let mut owners = joint
                .routes
                .iter()
                .filter_map(|(candidate, route)| match route {
                    Route::E2Caller(fact) if candidate == key => Some(*fact),
                    _ => None,
                });
            let fact = owners.next()?;
            if owners.next().is_some() {
                return None;
            }
            check(
                plan,
                &required(joint.experiment, fact, entry, key, joint.typed)?,
            )?;
        }
        Some(())
    }
}
fn source_key(key: &UnitCallableInstanceKey) -> Option<UnitFunctionInstanceKey> {
    // Current experiment has no receiver, specialization, deinit or closure ABI.
    let UnitCallTarget::Declaration(declaration) = key.target() else {
        return None;
    };
    if !key.type_arguments().is_empty() {
        return None;
    }
    Some(UnitFunctionInstanceKey::for_entry(declaration))
}
fn required(
    experiment: &TwoSourceExperiment,
    fact: &TwoSourceCounterFact,
    entry: &EntryDescriptor,
    caller: &UnitCallableInstanceKey,
    typed: &ValidatedCompilationUnitTypes,
) -> Option<BTreeSet<UnitFunctionInstanceKey>> {
    let mut keys = BTreeSet::from([
        UnitFunctionInstanceKey::for_entry(entry.entry()),
        source_key(caller)?,
    ]);
    for key in crate::ssa::e2_replay::required_keys(experiment, fact, typed)? {
        keys.insert(source_key(&key)?);
    }
    Some(keys)
}
fn check(plan: &UnitInstancePlan, expected: &BTreeSet<UnitFunctionInstanceKey>) -> Option<()> {
    let actual: BTreeSet<_> = plan.iter().map(|instance| instance.key().clone()).collect();
    // Equality includes static_self, deinit and callable arguments, not just declaration.
    (actual.len() == plan.len() && &actual == expected).then_some(())
}
