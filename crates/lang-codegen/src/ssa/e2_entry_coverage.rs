//! Exact bounded source-instance coverage; synthetic helpers never count as source definitions.
use super::{e2_entry::EmittedEntry, model::*};
use lang_frontend::{
    name_resolution::DeclarationId,
    ownership_checking::two_source_experiment::{TwoSourceCounterFact, TwoSourceExperiment},
    type_checking::{UnitCallableInstanceKey, ValidatedCompilationUnitTypes},
};
/// Root is a checked zero-parameter source declaration, not an invented call instance.
pub(super) struct Coverage {
    root: (DeclarationId, FunctionId),
    called: Vec<(UnitCallableInstanceKey, FunctionId)>,
    helpers: [FunctionId; 2],
}
impl Coverage {
    pub(super) fn root(&self) -> (DeclarationId, FunctionId) {
        self.root
    }
    pub(super) fn called(&self) -> &[(UnitCallableInstanceKey, FunctionId)] {
        &self.called
    }
    pub(super) fn helpers(&self) -> [FunctionId; 2] {
        self.helpers
    }
}
pub(super) fn audit(
    emitted: &EmittedEntry<'_>,
    experiment: &TwoSourceExperiment,
    fact: &TwoSourceCounterFact,
    typed: &ValidatedCompilationUnitTypes,
) -> Option<Coverage> {
    // These gates establish COMPLETE bounded bodies. Without them, enumerating known
    // calls could silently omit unsupported source expressions.
    super::e2_entry_replay::replay(emitted, experiment, fact, typed)?;
    let descriptor = emitted.descriptor();
    let entry_call = typed.types().call(descriptor.contracts()[0].call())?;
    if entry_call.instance() != descriptor.instance() {
        return None;
    }
    let mut required = vec![entry_call.instance().clone()];
    for arm in fact.calls() {
        for (call, recorded_key) in arm {
            let actual = typed.types().call(*call)?;
            if actual.instance() != recorded_key {
                return None;
            }
            if !required.contains(actual.instance()) {
                required.push(actual.instance().clone());
            }
        }
    }
    // The selected source definitions are locators, not the source of required keys.
    let mut selected = vec![(descriptor.instance().clone(), emitted.function.caller)];
    selected.extend(emitted.function.fragments.leaves.functions.iter().cloned());
    if selected.len() != required.len() {
        return None;
    }
    for key in &required {
        if selected
            .iter()
            .filter(|(candidate, _)| candidate == key)
            .count()
            != 1
        {
            return None;
        }
    }
    let helpers = emitted.function.fragments.arms;
    let mut identities = vec![emitted.entry];
    for (_, function) in &selected {
        if identities.contains(function) {
            return None;
        }
        identities.push(*function);
    }
    for helper in helpers {
        if identities.contains(&helper) {
            return None;
        }
        identities.push(helper);
    }
    let [module] = emitted.function.fragments.leaves.program.modules.as_slice() else {
        return None;
    };
    if module.functions.len() != identities.len()
        || identities.iter().any(|id| module.function(*id).is_none())
    {
        return None;
    }
    Some(Coverage {
        root: (descriptor.entry(), emitted.entry),
        called: selected,
        helpers,
    })
}
