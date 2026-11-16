//! Same-assembly original branch certificate consumption. Never a new lowering route.
use super::*;
use crate::ssa::{
    model::{BlockId, Definition, Function, InstructionId, LoanId},
    refine::branch_receipt::{Plans, Receipt},
};
use lang_frontend::{
    ownership_checking::{
        UnitBorrowBranchForwardTopology, UnitLoanActionCoverage, UnitLoweringFacts,
    },
    type_checking::{UnitExpressionId, UnitItemId},
};
use std::rc::Rc;
/// Original declared branch regions closed against their final complete caller CFG.
#[derive(Debug, Default)]
pub struct BranchConsumption {
    /// Independently selected original splits of planned plain callables.
    pub expected: Vec<UnitExpressionId>,
    /// Original both-return or folded-tail splits whose complete final caller and paths match.
    pub linked: Vec<UnitExpressionId>,
    /// Other topologies, missing/ambiguous provenance or final CFG mismatches.
    pub gaps: Vec<String>,
    // Private evidence only from successful original-path/final-CFG validation below.
    pub(super) edges: BTreeSet<(FunctionId, InstructionId, FunctionId)>,
    pub(super) returns: BTreeSet<(FunctionId, BlockId, LoanId)>,
}
struct Snapshot {
    plans: Plans,
    receipt: Receipt,
    function: Function,
}
type State = Rc<RefCell<Vec<Snapshot>>>;
thread_local! { static RUNS: RefCell<Vec<State>> = const { RefCell::new(Vec::new()) }; }
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
    ) -> BranchConsumption {
        let mut result = BranchConsumption::default();
        let mut callers = BTreeMap::new();
        for (key, function) in mapping {
            if let UnitCallableTarget::Declaration(declaration) = key.target()
                && *key == UnitFunctionInstanceKey::for_entry(declaration)
                && let Some(item) = items.get(&declaration)
                && callers.insert(*item, *function).is_some()
            {
                result.gaps.push("ambiguous branch caller mapping".into());
            }
        }
        let mut expected = BTreeMap::new();
        for catalog in owned.loan_action_observations() {
            for region in catalog.branch_forward_regions() {
                if !callers.contains_key(&region.callable()) {
                    continue;
                }
                if expected.insert(region.split(), (catalog, region)).is_some() {
                    result.gaps.push("duplicate original branch region".into());
                }
            }
        }
        result.expected = expected.keys().copied().collect();
        let snapshots = self.state.borrow();
        for (split, (catalog, region)) in &expected {
            // Both original topologies share the final three-block shape. Their distinction
            // remains in the original Plans/Receipt validator: TailContinuation must retain
            // its exact false-arm -> join fold, while BothArmsReturn must have none.
            if !matches!(
                region.topology(),
                UnitBorrowBranchForwardTopology::BothArmsReturn
                    | UnitBorrowBranchForwardTopology::TailContinuation
            ) {
                result
                    .gaps
                    .push(format!("branch topology outside bounded domain: {split:?}"));
                continue;
            }
            if region.coverage() != UnitLoanActionCoverage::Complete
                || !catalog.owns_branch_forward_region(region)
                || owned.d10_branch_link(*split) != Ok(region.callable())
            {
                result
                    .gaps
                    .push(format!("branch source/callable provenance gap: {split:?}"));
                continue;
            }
            let matches: Vec<_> = snapshots
                .iter()
                .filter(|s| s.receipt.split == *split)
                .collect();
            let [snapshot] = matches.as_slice() else {
                result
                    .gaps
                    .push(format!("missing/duplicate branch receipt: {split:?}"));
                continue;
            };
            let receipt = &snapshot.receipt;
            let mut paths: Vec<_> = region.paths().iter().collect();
            paths.sort_by_key(|p| !p.branch().then_arm());
            if receipt.callable != region.callable()
                || receipt.condition != region.condition()
                || receipt.topology != region.topology()
                || callers.get(&receipt.callable) != Some(&receipt.caller)
                || receipt.paths.len() != paths.len()
                || receipt.paths.iter().zip(paths).any(|(actual, original)| {
                    actual.branch != original.branch()
                        || actual.ids != original.action_ids().collect::<Vec<_>>()
                        || mapping.values().filter(|id| **id == actual.callee).count() != 1
                        || program
                            .modules
                            .iter()
                            .flat_map(|m| &m.functions)
                            .filter(|f| f.id == actual.callee)
                            .count()
                            != 1
                })
                || !matches_final(snapshot, program)
            {
                result.gaps.push(format!(
                    "branch final CFG/path identity mismatch: {split:?}"
                ));
                continue;
            }
            for path in &receipt.paths {
                result
                    .edges
                    .insert((receipt.caller, path.instruction, path.callee));
                result
                    .returns
                    .insert((receipt.caller, path.block, path.result));
            }
            result.linked.push(*split);
        }
        for snapshot in snapshots.iter() {
            if !expected.contains_key(&snapshot.receipt.split) {
                result
                    .gaps
                    .push("branch receipt outside original planned domain".into());
            }
        }
        if result.expected.is_empty() {
            result.gaps.push("no nonempty branch domain".into());
        }
        result
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
pub(crate) fn record(plans: &Plans, function: &Function, receipt: &Receipt) {
    RUNS.with(|runs| {
        if let Some(state) = runs.borrow().last() {
            state.borrow_mut().push(Snapshot {
                plans: plans.clone(),
                receipt: receipt.clone(),
                function: function.clone(),
            });
        }
    });
}
fn matches_final(snapshot: &Snapshot, program: &Program) -> bool {
    let functions: Vec<_> = program
        .modules
        .iter()
        .flat_map(|m| &m.functions)
        .filter(|f| f.id == snapshot.receipt.caller)
        .collect();
    let [function] = functions.as_slice() else {
        return false;
    };
    // Original producer validator is reused, not reinterpreted by an observation layer.
    if **function != snapshot.function
        || snapshot
            .receipt
            .validate(&snapshot.plans, function, snapshot.function.origin.span())
            .is_err()
    {
        return false;
    }
    let receipt = &snapshot.receipt;
    if function.entry_block() != Some(receipt.entry)
        || function.blocks.len() != 3
        || function.instructions.len() != 3
        || function.borrow_return != Some(0)
        || function.receiver.is_some()
        || function.carrier_return.is_some()
        || function.never_returns
        || function.return_types.len() != 1
    {
        return false;
    }
    let mut entities = BTreeSet::new();
    let mut instructions = BTreeSet::new();
    let mut blocks = BTreeSet::new();
    for block in &function.blocks {
        if !blocks.insert(block.id) || function.block(block.id) != Some(block) {
            return false;
        }
        let expected = if block.id == receipt.entry {
            receipt.read
        } else {
            let paths: Vec<_> = receipt
                .paths
                .iter()
                .filter(|p| p.block == block.id)
                .collect();
            let [path] = paths.as_slice() else {
                return false;
            };
            path.instruction
        };
        if block.instructions != [expected] {
            return false;
        }
        for (index, id) in block.parameters.iter().enumerate() {
            if !entities.insert(*id)
                || function.entity(*id).is_none_or(|e| {
                    e.definition
                        != (Definition::BlockParameter {
                            block: block.id,
                            index,
                        })
                })
            {
                return false;
            }
        }
        for id in &block.instructions {
            let Some(data) = function.instruction(*id) else {
                return false;
            };
            if !instructions.insert(*id) || data.id != *id || data.block != block.id {
                return false;
            }
            for (index, entity) in data.results.iter().enumerate() {
                if !entities.insert(*entity)
                    || function.entity(*entity).is_none_or(|e| {
                        e.definition
                            != (Definition::InstructionResult {
                                instruction: *id,
                                index,
                            })
                    })
                {
                    return false;
                }
            }
        }
    }
    instructions.len() == function.instructions.len()
        && entities.len() == function.values.len() + function.places.len() + function.loans.len()
        && entities.iter().all(|id| id.function() == function.id)
}

#[cfg(test)]
mod tests;
