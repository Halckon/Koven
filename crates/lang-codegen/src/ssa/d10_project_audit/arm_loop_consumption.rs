//! Atomic same-run ordinary arm-loop certificates; no generic loop admission.
use super::*;
use crate::ssa::{
    lowering_support::callable_instances::SourceToken,
    model::{Function, InstructionId, SsaTypeId},
    refine::{RefineInputs, RefineTypes, arm_loop_receipt::Receipt},
};
use lang_frontend::{
    ownership_checking::{UnitLoopBranchNoCarryFact, UnitLoweringFacts},
    type_checking::{ParameterMode, UnitExpressionId, UnitItemId},
};
use std::rc::Rc;
mod source;
type Call = (UnitFunctionInstanceKey, FunctionId);
type Edge = (FunctionId, InstructionId, FunctionId);
struct Expected {
    token: SourceToken,
    key: UnitFunctionInstanceKey,
    fact: UnitLoopBranchNoCarryFact,
    source: Option<source::Source>,
}
struct Snapshot {
    receipt: Receipt,
    function: Function,
}
#[derive(Default)]
struct State {
    expected: Vec<Expected>,
    snapshots: Vec<Snapshot>,
}
type Shared = Rc<RefCell<State>>;
thread_local! {static RUNS:RefCell<Vec<Shared>>=const{RefCell::new(Vec::new())};}
pub(super) struct Transaction {
    state: Shared,
    planned: BTreeMap<SourceToken, (UnitFunctionInstanceKey, UnitItemId)>,
}
impl Transaction {
    pub(super) fn begin(plan: &UnitInstancePlan) -> Self {
        let state = Rc::new(RefCell::new(State::default()));
        RUNS.with(|r| r.borrow_mut().push(state.clone()));
        Self {
            state,
            planned: plan
                .instances()
                .iter()
                .map(|p| {
                    (
                        p.source_token(),
                        (p.key().clone(), UnitItemId::new(p.source_unit(), p.item())),
                    )
                })
                .collect(),
        }
    }
    pub(super) fn finish(
        &self,
        program: &Program,
        mapping: &BTreeMap<UnitFunctionInstanceKey, FunctionId>,
        owned: &dyn UnitLoweringFacts,
    ) -> (BTreeSet<Edge>, Vec<String>) {
        let state = self.state.borrow();
        let mut edges = BTreeSet::new();
        let mut gaps = Vec::new();
        let mut used = BTreeSet::new();
        let mut seen = BTreeSet::new();
        for e in &state.expected {
            let result = (|| {
                let UnitCallableTarget::Declaration(declaration) = e.key.target() else {
                    return None;
                };
                if !seen.insert(e.token)
                    || e.key != UnitFunctionInstanceKey::for_entry(declaration)
                    || self.planned.get(&e.token)
                        != Some(&(e.key.clone(), e.fact.arms()[0].callable()))
                    || state.expected.iter().filter(|x| x.token == e.token).count() != 1
                {
                    return None;
                }
                let originals: Vec<_> = owned
                    .no_carry_branches()
                    .iter()
                    .filter(|f| f.arms()[0].callable() == e.fact.arms()[0].callable())
                    .collect();
                let [original] = originals.as_slice() else {
                    return None;
                };
                if owned.d10_arm_loop_link(e.fact.branch()).ok()
                    != Some(e.fact.arms()[0].callable())
                    || !owned.owns_no_carry_branch(&e.fact)
                    || !original.same_identity(&e.fact)
                    || **original != e.fact
                {
                    return None;
                }
                let source = e.source.as_ref()?;
                let caller = *mapping.get(&e.key)?;
                let snapshots: Vec<_> = state
                    .snapshots
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| s.receipt.caller == caller)
                    .collect();
                let [(index, s)] = snapshots.as_slice() else {
                    return None;
                };
                if used.contains(index) || mapping.values().filter(|id| **id == caller).count() != 1
                {
                    return None;
                }
                for (key, id) in source.calls.iter().flatten() {
                    if mapping.get(key) != Some(id)
                        || mapping.values().filter(|x| *x == id).count() != 1
                        || program
                            .modules
                            .iter()
                            .flat_map(|m| &m.functions)
                            .filter(|f| f.id == *id)
                            .count()
                            != 1
                    {
                        return None;
                    }
                }
                let functions: Vec<_> = program
                    .modules
                    .iter()
                    .flat_map(|m| &m.functions)
                    .filter(|f| f.id == caller)
                    .collect();
                let [function] = functions.as_slice() else {
                    return None;
                };
                if **function != s.function {
                    return None;
                }
                let calls = source
                    .calls
                    .clone()
                    .map(|c| c.into_iter().map(|(_, id)| id).collect::<Vec<_>>());
                let result = s.receipt.certify_ordinary(
                    program,
                    &e.fact,
                    &source.abi,
                    &source.roles,
                    &source.conditions,
                    &calls,
                )?;
                if result[0] == result[1] || result.iter().any(|x| edges.contains(x)) {
                    return None;
                }
                used.insert(*index);
                Some(result)
            })();
            match result {
                Some(pair) => edges.extend(pair),
                None => gaps.push("ordinary arm-loop full-caller evidence gap".into()),
            }
        }
        if used.len() != state.snapshots.len() {
            gaps.push("ordinary arm-loop unconsumed receipt".into());
        }
        (edges, gaps)
    }
}
impl Drop for Transaction {
    fn drop(&mut self) {
        RUNS.with(|r| r.borrow_mut().retain(|s| !Rc::ptr_eq(s, &self.state)));
    }
}
pub(in crate::ssa) fn expect(inputs: &RefineInputs<'_>, types: &RefineTypes<'_>) {
    let Some(item) = inputs.function_item else {
        return;
    };
    let Some(key) = inputs.source_plan.source(inputs.source_token) else {
        return;
    };
    let callable = UnitItemId::new(inputs.source_unit, item);
    for fact in inputs
        .owned
        .no_carry_branches()
        .iter()
        .filter(|f| f.arms()[0].callable() == callable)
    {
        let expected = Expected {
            token: inputs.source_token,
            key: key.clone(),
            fact: fact.clone(),
            source: source::Source::collect(inputs, types, fact),
        };
        RUNS.with(|r| {
            if let Some(state) = r.borrow().last() {
                state.borrow_mut().expected.push(expected);
            }
        });
    }
}
pub(crate) fn record(function: &Function, receipt: &Receipt) {
    RUNS.with(|r| {
        if let Some(state) = r.borrow().last() {
            state.borrow_mut().snapshots.push(Snapshot {
                receipt: receipt.clone(),
                function: function.clone(),
            });
        }
    });
}
