//! Actual entry call edges only; lifecycle gaps remain separate from graph reachability.
use super::*;
use crate::ssa::{model::InstructionId, refine::entry_borrow_receipt::Receipt};
use lang_frontend::{ownership_checking::UnitLoweringFacts, type_checking::UnitItemId};
use std::rc::Rc;
type State = Rc<RefCell<Vec<Receipt>>>;
type Edges = BTreeSet<(FunctionId, InstructionId, FunctionId)>;
thread_local! { static RUNS: RefCell<Vec<State>> = const {RefCell::new(Vec::new())}; }
pub(super) struct Transaction {
    state: State,
}
impl Transaction {
    pub(super) fn begin() -> Self {
        let state = Rc::new(RefCell::new(Vec::new()));
        RUNS.with(|runs| runs.borrow_mut().push(state.clone()));
        Self { state }
    }
    pub(super) fn finish(
        &self,
        items: &BTreeMap<DeclarationId, UnitItemId>,
        program: &Program,
        mapping: &BTreeMap<UnitFunctionInstanceKey, FunctionId>,
        owned: &dyn UnitLoweringFacts,
    ) -> (Edges, Vec<String>) {
        let mut expected = BTreeMap::new();
        let mut gaps = Vec::new();
        for handoff in owned.borrow_call_handoffs() {
            let call = handoff.source_loan().call();
            let Some(callable) = owned.d10_entry_borrow_link(call) else {
                continue;
            };
            if !mapping.keys().any(|key| matches!(key.target(), UnitCallableTarget::Declaration(d) if items.get(&d) == Some(&callable))) { continue; }
            if expected.insert(call, callable).is_some() {
                gaps.push("ambiguous original entry handoff".into());
            }
        }
        let snapshots = self.state.borrow();
        let mut edges = Edges::new();
        for (call, callable) in &expected {
            let matches: Vec<_> = snapshots.iter().filter(|r| r.call() == *call).collect();
            let [receipt] = matches.as_slice() else {
                gaps.push(format!("missing/duplicate entry call receipt: {call:?}"));
                continue;
            };
            let functions: Vec<_> = program
                .modules
                .iter()
                .flat_map(|m| &m.functions)
                .filter(|f| f.id == receipt.caller())
                .collect();
            let [function] = functions.as_slice() else {
                gaps.push("entry caller definition gap".into());
                continue;
            };
            let (caller_key, callee_key) = receipt.keys();
            if receipt.callable() != *callable
                || mapping.get(caller_key) != Some(&receipt.caller())
                || mapping.get(callee_key) != Some(&receipt.callee())
                || program
                    .modules
                    .iter()
                    .flat_map(|m| &m.functions)
                    .filter(|f| f.id == receipt.callee())
                    .count()
                    != 1
                || !receipt.matches_final(function)
            {
                gaps.push(format!("entry final call identity gap: {call:?}"));
                continue;
            }
            edges.insert((receipt.caller(), receipt.instruction.id, receipt.callee()));
            gaps.push(format!("entry binding lifetime unproven: {call:?}"));
        }
        for receipt in snapshots.iter() {
            if !expected.contains_key(&receipt.call()) {
                gaps.push("entry receipt outside original expected handoffs".into());
            }
        }
        (edges, gaps)
    }
}
impl Drop for Transaction {
    fn drop(&mut self) {
        RUNS.with(|runs| {
            runs.borrow_mut()
                .retain(|run| !Rc::ptr_eq(run, &self.state))
        });
    }
}
pub(crate) fn record(receipt: Receipt) {
    RUNS.with(|runs| {
        if let Some(state) = runs.borrow().last() {
            state.borrow_mut().push(receipt);
        }
    });
}
