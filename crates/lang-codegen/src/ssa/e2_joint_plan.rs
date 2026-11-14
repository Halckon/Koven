//! Closed pre-emission selection for the restricted experiment, not ordinary Unit authority.
use lang_frontend::{
    ownership_checking::two_source_experiment::{
        CalleeLeaf, EntryDescriptor, ExperimentalUnitHandoff, TwoSourceCounterFact,
        TwoSourceExperiment,
    },
    type_checking::{UnitCallTarget, UnitCallableInstanceKey, ValidatedCompilationUnitTypes},
};

enum Route<'a> {
    RestrictedSourceLeaf(&'a CalleeLeaf),
    E2Caller(&'a TwoSourceCounterFact),
}

/// No SSA IDs exist until this closed selection is consumed.
pub(super) struct JointPlan<'a> {
    experiment: &'a TwoSourceExperiment,
    typed: &'a ValidatedCompilationUnitTypes,
    routes: Vec<(UnitCallableInstanceKey, Route<'a>)>,
    entries: Vec<(&'a EntryDescriptor, UnitCallableInstanceKey)>,
}
impl<'a> JointPlan<'a> {
    pub(super) fn build(
        handoff: &ExperimentalUnitHandoff<'a, '_>,
        experiment: &'a TwoSourceExperiment,
        facts: &[&'a TwoSourceCounterFact],
        entries: &[&'a EntryDescriptor],
    ) -> Option<Self> {
        let typed = handoff.types();
        if !experiment.matches_types(typed) || entries.len() != facts.len() {
            return None;
        }
        let selection = handoff.for_facts(facts)?;
        let parsed =
            super::lowering_support::parsed_by_source_unit(selection.inputs(), selection.names())
                .ok()?;
        super::unit_lower::preflight::validate(
            &parsed,
            selection.names(),
            typed.types(),
            &selection,
        )
        .ok()?;
        let mut routes: Vec<(UnitCallableInstanceKey, Route<'a>)> = Vec::new();
        let mut planned_entries: Vec<(&EntryDescriptor, UnitCallableInstanceKey)> = Vec::new();
        for entry in entries {
            if !experiment.owns_entry(entry)
                || planned_entries
                    .iter()
                    .any(|(old, _)| old.entry() == entry.entry())
            {
                return None;
            }
            let key = typed.types().call(entry.contracts()[0].call())?.instance();
            if key != entry.instance() || routes.iter().any(|(old, _)| old == key) {
                return None;
            }
            let mut owners = facts.iter().copied().filter(|fact| {
                experiment.callable(fact).map(UnitCallTarget::Declaration) == Some(key.target())
            });
            let fact = owners.next()?;
            if owners.next().is_some() || !experiment.owns(fact) {
                return None;
            }
            experiment.function_shell(fact, typed)?;
            routes.push((key.clone(), Route::E2Caller(fact)));
            planned_entries.push((entry, key.clone()));
        }
        // Actual entered calls and opaque leaf witnesses determine every remaining route.
        for fact in facts {
            for (call, recorded) in fact.calls().iter().flatten() {
                let key = typed.types().call(*call)?.instance();
                if key != recorded {
                    return None;
                }
                let mut witnesses = experiment
                    .callee_leaves_for(fact, typed)?
                    .iter()
                    .filter(|leaf| leaf.instance() == key);
                let leaf = witnesses.next()?;
                if witnesses.next().is_some() || !experiment.owns_leaf(leaf) {
                    return None;
                }
                if let Some((_, route)) = routes.iter().find(|(old, _)| old == key) {
                    let Route::RestrictedSourceLeaf(old) = route else {
                        return None;
                    };
                    if old.body() != leaf.body() || old.span() != leaf.span() {
                        return None;
                    }
                } else {
                    routes.push((key.clone(), Route::RestrictedSourceLeaf(leaf)));
                }
            }
        }
        Some(Self {
            experiment,
            typed,
            routes,
            entries: planned_entries,
        })
    }

    pub(super) fn experiment(&self) -> &'a TwoSourceExperiment {
        self.experiment
    }
    pub(super) fn typed(&self) -> &'a ValidatedCompilationUnitTypes {
        self.typed
    }
    pub(super) fn facts(&self) -> Vec<&'a TwoSourceCounterFact> {
        self.routes
            .iter()
            .filter_map(|(_, route)| match route {
                Route::E2Caller(fact) => Some(*fact),
                Route::RestrictedSourceLeaf(_) => None,
            })
            .collect()
    }
    /// Dispatch only the sealed routes; final publication independently re-derives source coverage.
    pub(super) fn publish(self) -> Option<super::e2_arm_fragments::PublishedEntrySet<'a>> {
        let mut leaves = Vec::new();
        for (_, route) in &self.routes {
            match route {
                Route::RestrictedSourceLeaf(leaf) => leaves.push(*leaf),
                Route::E2Caller(_) => {}
            }
        }
        let descriptors: Vec<_> = self.entries.iter().map(|(entry, _)| *entry).collect();
        let leaves = super::e2_callee_leaves::emit_selected(self.experiment, self.typed, leaves)?;
        super::e2_arm_fragments::emit_from_plan(&self, leaves)?
            .append_callers(self.typed)?
            .publish_entries(&descriptors, self.typed)
    }
}
