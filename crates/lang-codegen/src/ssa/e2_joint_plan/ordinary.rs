//! Same-analysis common-root planner: one callable arena, never joined per-entry plans.
use super::*;
use crate::ssa::unit_plan::{
    MAX_UNIT_GENERIC_INSTANCES, UnitFunctionInstanceKey, UnitInstancePlan,
    plan_unit_root_set_from_facts,
};
use lang_frontend::name_resolution::DeclarationId;
use std::collections::BTreeSet;

/// The original roots and all instances/runtime demands share one source-token arena.
pub(super) struct OrdinaryPlans {
    roots: BTreeSet<DeclarationId>,
    plan: UnitInstancePlan,
}
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
        let roots: Vec<_> = entries.iter().map(|(entry, _)| entry.entry()).collect();
        let mut expected = BTreeSet::new();
        for (entry, key) in entries {
            let mut owners = facts.iter().copied().filter(|fact| {
                experiment.callable(fact).map(UnitCallTarget::Declaration) == Some(key.target())
            });
            let fact = owners.next()?;
            if owners.next().is_some() {
                return None;
            }
            expected.extend(required(experiment, fact, entry, key, selected.types())?);
        }
        // Reuse the D10 root-set worklist: a single callable planner and one global instance budget.
        let plan = plan_unit_root_set_from_facts(
            &parsed,
            selected.names(),
            selected.types().types(),
            selected,
            &roots,
            MAX_UNIT_GENERIC_INSTANCES,
            true,
        )
        .ok()?;
        check(&plan, &expected)?;
        Some(Self {
            roots: roots.into_iter().collect(),
            plan,
        })
    }
    pub(super) fn into_plan(self) -> UnitInstancePlan {
        self.plan
    }
    pub(super) fn verify(&self, joint: &JointPlan<'_>) -> Option<()> {
        let roots: BTreeSet<_> = joint
            .entries
            .iter()
            .map(|(entry, _)| entry.entry())
            .collect();
        if roots.len() != joint.entries.len() || roots != self.roots {
            return None;
        }
        let mut expected = BTreeSet::new();
        for (entry, key) in &joint.entries {
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
            expected.extend(required(joint.experiment, fact, entry, key, joint.typed)?);
        }
        check(&self.plan, &expected)
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
    let actual: BTreeSet<_> = plan
        .instances()
        .iter()
        .map(|instance| instance.key().clone())
        .collect();
    // Equality includes static_self, deinit and callable arguments, not just declaration.
    (actual.len() == plan.instances().len() && &actual == expected).then_some(())
}
