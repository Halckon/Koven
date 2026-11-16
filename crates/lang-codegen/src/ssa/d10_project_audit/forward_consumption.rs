//! Same-assembly consumption of the original live-forward producer certificate.
use super::*;
use crate::ssa::{
    lowering_support::callable_instances::SourceToken,
    model::{
        BlockId, Definition, EntityId, Function, InstructionId, LoanId, Operation, SsaTypeKind,
    },
    refine::{
        RefineInputs,
        forward_receipt::{Plans, Receipt},
    },
};
use lang_frontend::{
    ownership_checking::{UnitLoanActionCoverage, UnitLoanActionId, UnitLoweringFacts},
    type_checking::{UnitExpressionId, UnitItemId},
};
use std::rc::Rc;
struct Expected {
    token: SourceToken,
    key: UnitFunctionInstanceKey,
    callable: UnitItemId,
    call: UnitExpressionId,
    ids: Vec<UnitLoanActionId>,
    callee: Option<(UnitFunctionInstanceKey, FunctionId)>,
}
struct Snapshot {
    plans: Plans,
    receipt: Receipt,
    function: Function,
}
#[derive(Default)]
struct State {
    expected: Vec<Expected>,
    snapshots: Vec<Snapshot>,
}
type Shared = Rc<RefCell<State>>;
pub(super) type Edges = BTreeSet<(FunctionId, InstructionId, FunctionId)>;
pub(super) type Returns = BTreeSet<(FunctionId, BlockId, LoanId)>;
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
    ) -> (Edges, Returns, Vec<String>) {
        let state = self.state.borrow();
        let mut edges = Edges::new();
        let mut returns = Returns::new();
        let mut gaps = Vec::new();
        let mut seen = BTreeSet::new();
        for expected in &state.expected {
            if !seen.insert(expected.call)
                || self.planned.get(&expected.token)
                    != Some(&(expected.key.clone(), expected.callable))
            {
                gaps.push("duplicate/outside-plan forward candidate".into());
                continue;
            }
            let originals: Vec<_> = owned
                .loan_action_observations()
                .iter()
                .flat_map(|c| {
                    c.forward_regions()
                        .iter()
                        .filter(|r| r.body() == expected.call)
                        .map(move |r| (c, r))
                })
                .collect();
            let [(catalog, region)] = originals.as_slice() else {
                gaps.push("missing/duplicate original forward region".into());
                continue;
            };
            if !catalog.owns_forward_region(region)
                || region.coverage() != UnitLoanActionCoverage::Complete
                || region.action_ids().collect::<Vec<_>>() != expected.ids
                || owned.d10_forward_link(expected.call) != Ok(expected.callable)
            {
                gaps.push("forward original callable/action provenance gap".into());
                continue;
            }
            let matches: Vec<_> = state
                .snapshots
                .iter()
                .filter(|s| s.receipt.call == expected.call)
                .collect();
            let [snapshot] = matches.as_slice() else {
                gaps.push("missing/duplicate forward receipt".into());
                continue;
            };
            let r = &snapshot.receipt;
            if r.ids != expected.ids
                || r.callable != expected.callable
                || mapping.get(&expected.key) != Some(&r.caller)
            {
                gaps.push("forward source/mapping mismatch".into());
                continue;
            }
            let UnitCallableTarget::Declaration(declaration) = expected.key.target() else {
                gaps.push("forward callable outside plain domain".into());
                continue;
            };
            if expected.key != UnitFunctionInstanceKey::for_entry(declaration) {
                gaps.push("forward callable outside plain domain".into());
                continue;
            }
            let Some(callee) = matches_final(snapshot, program) else {
                gaps.push("forward final caller closure mismatch".into());
                continue;
            };
            if expected
                .callee
                .as_ref()
                .is_none_or(|(key, id)| *id != callee || mapping.get(key) != Some(id))
                || mapping.values().filter(|f| **f == callee).count() != 1
                || program
                    .modules
                    .iter()
                    .flat_map(|m| &m.functions)
                    .filter(|f| f.id == callee)
                    .count()
                    != 1
            {
                gaps.push("forward callee definition mismatch".into());
                continue;
            }
            if !edges.insert((r.caller, r.instruction, callee))
                || !returns.insert((r.caller, r.block, r.result))
            {
                gaps.push("duplicate forward certified edge/return".into());
            }
        }
        for snapshot in &state.snapshots {
            if !seen.contains(&snapshot.receipt.call) {
                gaps.push("forward receipt without original input".into());
            }
        }
        (edges, returns, gaps)
    }
}
impl Drop for Transaction {
    fn drop(&mut self) {
        RUNS.with(|r| r.borrow_mut().retain(|s| !Rc::ptr_eq(s, &self.state)));
    }
}
pub(in crate::ssa) fn expect(inputs: &RefineInputs<'_>) {
    let Some(item) = inputs.function_item else {
        return;
    };
    let callable = UnitItemId::new(inputs.source_unit, item);
    let Some(key) = inputs.source_plan.source(inputs.source_token) else {
        return;
    };
    for catalog in inputs.owned.loan_action_observations() {
        for region in catalog
            .forward_regions()
            .iter()
            .filter(|r| r.callable() == callable)
        {
            let expected = Expected {
                token: inputs.source_token,
                key: key.clone(),
                callable,
                call: region.body(),
                ids: region.action_ids().collect(),
                callee: inputs
                    .source_plan
                    .call_site(inputs.source_token, region.body())
                    .and_then(|site| {
                        if !site.delegation().is_empty() {
                            return None;
                        }
                        let key = site.dispatch_key()?;
                        Some((key.clone(), *inputs.function_ids.get(key)?))
                    }),
            };
            RUNS.with(|r| {
                if let Some(state) = r.borrow().last() {
                    state.borrow_mut().expected.push(expected);
                }
            });
        }
    }
}
pub(crate) fn record(plans: &Plans, function: &Function, receipt: &Receipt) {
    RUNS.with(|r| {
        if let Some(state) = r.borrow().last() {
            state.borrow_mut().snapshots.push(Snapshot {
                plans: plans.clone(),
                receipt: receipt.clone(),
                function: function.clone(),
            });
        }
    });
}
fn matches_final(snapshot: &Snapshot, program: &Program) -> Option<FunctionId> {
    let r = &snapshot.receipt;
    let functions: Vec<_> = program
        .modules
        .iter()
        .flat_map(|m| &m.functions)
        .filter(|f| f.id == r.caller)
        .collect();
    let [f] = functions.as_slice() else {
        return None;
    };
    if **f != snapshot.function
        || r.validate(&snapshot.plans, f, f.origin.span()).is_err()
        || f.entry_block() != Some(r.block)
        || f.borrow_return != Some(0)
        || f.carrier_return.is_some()
        || f.receiver.is_some()
        || f.never_returns
        || f.return_types.len() != 1
        || !f.values.is_empty()
        || !f.places.is_empty()
        || f.loans.len() != 2
    {
        return None;
    }
    let block = f.block(r.block)?;
    if block.instructions != [r.instruction] || block.terminator.as_ref()?.abort.is_some() {
        return None;
    }
    let instruction = f.instruction(r.instruction)?;
    let Operation::BorrowCall { callee, .. } = instruction.operation else {
        return None;
    };
    let source = f.entity(EntityId::Loan(r.source))?;
    let result = f.entity(EntityId::Loan(r.result))?;
    if source.definition
        != (Definition::BlockParameter {
            block: r.block,
            index: 0,
        })
        || result.definition
            != (Definition::InstructionResult {
                instruction: r.instruction,
                index: 0,
            })
        || source.ty != result.ty
    {
        return None;
    }
    let crate::ssa::model::EntityType::Loan {
        target,
        kind: crate::ssa::model::LoanKind::Shared,
    } = source.ty
    else {
        return None;
    };
    let modules: Vec<_> = program
        .modules
        .iter()
        .filter(|m| m.id == target.module)
        .collect();
    let [module] = modules.as_slice() else {
        return None;
    };
    if target.module != f.id.module
        || module.types.get(target.index) != Some(&SsaTypeKind::StringOwner)
        || f.return_types != [target]
        || r.source == r.result
        || r.source.function() != f.id
        || r.result.function() != f.id
    {
        return None;
    }
    Some(callee)
}
