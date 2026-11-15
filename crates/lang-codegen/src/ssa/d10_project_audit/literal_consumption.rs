//! Existing refinement receipts, scoped to one original Unit assembly and final verification.
use super::*;
use crate::ssa::{
    model::{EntityData, EntityId, Function, Instruction, Operation, TerminatorKind},
    refine::temporary_receipt::TemporaryReceipt,
};
use lang_frontend::{
    ownership_checking::UnitLoweringFacts,
    type_checking::{UnitExpressionId, UnitItemId},
};
use std::rc::Rc;
/// Bounded literal-call consumption, separate from general callable/CFG completeness.
#[derive(Debug, Default)]
pub struct LiteralConsumption {
    /// Independently selected original linked regions belonging to planned plain callables.
    pub expected: Vec<UnitExpressionId>,
    /// Exact original calls whose existing receipt survives final Program validation.
    pub linked: Vec<UnitExpressionId>,
    /// Missing receipts, unsupported source protocols/routes or inconsistent final identities.
    pub gaps: Vec<String>,
}
#[derive(Clone)]
struct Snapshot {
    receipt: TemporaryReceipt,
    instructions: Option<Vec<Instruction>>,
    entities: Option<Vec<EntityData>>,
    results: Option<Vec<(EntityId, EntityData)>>,
}
type State = Rc<RefCell<Vec<Snapshot>>>;
thread_local! { static RUNS: RefCell<Vec<State>> = const { RefCell::new(Vec::new()) }; }
/// Private per-assembly identity. Unwinding/early failure discards unpublished receipts.
pub(super) struct Transaction {
    state: State,
}
impl Transaction {
    pub(super) fn begin() -> Self {
        let state = Rc::new(RefCell::new(Vec::new()));
        RUNS.with(|runs| runs.borrow_mut().push(state.clone()));
        Self { state }
    }
}
impl Drop for Transaction {
    fn drop(&mut self) {
        RUNS.with(|runs| {
            let mut runs = runs.borrow_mut();
            if let Some(index) = runs.iter().position(|run| Rc::ptr_eq(run, &self.state)) {
                runs.remove(index);
            }
        });
    }
}
pub(crate) fn record_temporary(receipts: &[TemporaryReceipt], function: &Function) {
    RUNS.with(|runs| {
        let runs = runs.borrow();
        let Some(state) = runs.last() else {
            return;
        };
        for receipt in receipts {
            state.borrow_mut().push(Snapshot {
                instructions: receipt
                    .instructions
                    .iter()
                    .map(|id| function.instruction(*id).cloned())
                    .collect(),
                entities: receipt
                    .entities
                    .iter()
                    .map(|id| function.entity(*id).cloned())
                    .collect(),
                results: receipt
                    .instructions
                    .iter()
                    .try_fold(Vec::new(), |mut all, id| {
                        for result in &function.instruction(*id)?.results {
                            all.push((*result, function.entity(*result)?.clone()));
                        }
                        Some(all)
                    }),
                receipt: receipt.clone(),
            });
        }
    });
}
pub(super) fn finish(
    transaction: &Transaction,
    items: &BTreeMap<DeclarationId, UnitItemId>,
    program: &Program,
    mapping: &BTreeMap<UnitFunctionInstanceKey, FunctionId>,
    owned: &dyn UnitLoweringFacts,
) -> LiteralConsumption {
    let mut result = LiteralConsumption::default();
    let mut callers = BTreeMap::new();
    for (key, function) in mapping {
        if let UnitCallableTarget::Declaration(declaration) = key.target()
            && *key == UnitFunctionInstanceKey::for_entry(declaration)
            && let Some(item) = items.get(&declaration)
            && callers.insert(*item, *function).is_some()
        {
            result.gaps.push("ambiguous planned callable".into());
        }
    }
    // Construct expectations from original facts and plan, never from observed receipts.
    let mut expected = BTreeMap::new();
    for observation in owned.loan_action_observations() {
        for region in observation.regions() {
            let Some(item) = region.callable().filter(|item| callers.contains_key(item)) else {
                continue;
            };
            let call = region.call();
            if owned.d10_literal_call_link(call) != Ok(item)
                || !observation.owns_region(region)
                || observation.source() != call.source_unit()
            {
                result
                    .gaps
                    .push(format!("unsupported original linked region: {call:?}"));
                continue;
            }
            if expected
                .insert(call, (item, region.action_ids().collect::<Vec<_>>()))
                .is_some()
            {
                result
                    .gaps
                    .push(format!("ambiguous original region: {call:?}"));
            }
        }
    }
    result.expected = expected.keys().copied().collect();
    if expected.is_empty() {
        result
            .gaps
            .push("no supported nonempty literal-call domain".into());
    }
    let snapshots = transaction.state.borrow();
    let mut seen = BTreeSet::new();
    for snapshot in snapshots.iter() {
        let receipt = &snapshot.receipt;
        let call = receipt.call;
        let Some((item, ids)) = expected.get(&call) else {
            result.gaps.push(format!(
                "receipt outside expected original domain: {call:?}"
            ));
            continue;
        };
        if !seen.insert(call) {
            result.gaps.push(format!("duplicate receipt: {call:?}"));
            continue;
        }
        if receipt.callable != *item
            || receipt.ids != *ids
            || callers.get(item) != Some(&receipt.caller)
            || !matches_final(snapshot, program)
        {
            result
                .gaps
                .push(format!("receipt/final Program identity mismatch: {call:?}"));
            continue;
        }
        result.linked.push(call);
    }
    for call in expected.keys().filter(|call| !seen.contains(call)) {
        result.gaps.push(format!(
            "missing real refinement receipt (route gap): {call:?}"
        ));
    }
    result.linked.sort();
    // An incomplete set is never a successful bounded closure, even when some calls matched.
    if result.linked != result.expected && result.gaps.is_empty() {
        result
            .gaps
            .push("incomplete literal-call consumption set".into());
    }
    result
}
fn matches_final(snapshot: &Snapshot, program: &Program) -> bool {
    let receipt = &snapshot.receipt;
    let functions: Vec<_> = program
        .modules
        .iter()
        .flat_map(|m| &m.functions)
        .filter(|f| f.id == receipt.caller)
        .collect();
    let [function] = functions.as_slice() else {
        return false;
    };
    let (Some(instructions), Some(entities)) = (&snapshot.instructions, &snapshot.entities) else {
        return false;
    };
    if instructions.is_empty()
        || entities.is_empty()
        || instructions.len() != receipt.instructions.len()
        || entities.len() != receipt.entities.len()
        || receipt.instructions.iter().collect::<BTreeSet<_>>().len() != instructions.len()
        || receipt.entities.iter().collect::<BTreeSet<_>>().len() != entities.len()
    {
        return false;
    }
    let [block] = function.blocks.as_slice() else {
        return false;
    };
    if function.receiver.is_some()
        || function.borrow_return.is_some()
        || function.carrier_return.is_some()
        || function.never_returns
        || !function.return_types.is_empty()
        || !block.parameters.is_empty()
        || block.instructions != receipt.instructions
        || function.instructions != *instructions
        || !matches!(&block.terminator, Some(term)
            if matches!(&term.kind, TerminatorKind::Return { values } if values.is_empty()))
    {
        return false;
    }
    for (id, instruction) in receipt.instructions.iter().zip(instructions) {
        if function.instruction(*id) != Some(instruction) || instruction.block != block.id {
            return false;
        }
    }
    // Include the actual DirectCall result, not only the receipt's owner/place/loan triples.
    let Some(results) = &snapshot.results else {
        return false;
    };
    let result_ids = results.iter().map(|(id, _)| *id).collect::<BTreeSet<_>>();
    if result_ids.len() != results.len()
        || results.len() != function.values.len() + function.places.len() + function.loans.len()
        || receipt.entities.iter().any(|id| !result_ids.contains(id))
    {
        return false;
    }
    for instruction in instructions {
        for id in &instruction.results {
            if !receipt.entities.contains(id)
                && (!matches!(&instruction.operation, Operation::DirectCall { .. })
                    || !matches!(id, EntityId::Value(_)))
            {
                return false;
            }
        }
    }
    for (id, entity) in results {
        if function.entity(*id) != Some(entity) {
            return false;
        }
        let crate::ssa::model::Definition::InstructionResult { instruction, index } =
            entity.definition
        else {
            return false;
        };
        if !receipt.instructions.contains(&instruction)
            || function
                .instruction(instruction)
                .and_then(|i| i.results.get(index))
                != Some(id)
        {
            return false;
        }
    }
    for (id, entity) in receipt.entities.iter().zip(entities) {
        if function.entity(*id) != Some(entity) {
            return false;
        }
        let crate::ssa::model::Definition::InstructionResult { instruction, index } =
            entity.definition
        else {
            return false;
        };
        if !receipt.instructions.contains(&instruction)
            || function
                .instruction(instruction)
                .and_then(|i| i.results.get(index))
                != Some(id)
        {
            return false;
        }
    }
    true
}
