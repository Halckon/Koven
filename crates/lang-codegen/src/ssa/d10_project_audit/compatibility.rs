//! Actual Compatibility loan/Drop consumption. Never a Refine Plan or receipt.
use super::*;
use crate::ssa::{
    model::{BlockId, EntityData, EntityId, Function, Instruction, InstructionId, LoanId, ValueId},
    protocol1_operation::{self, Expected},
};
use lang_frontend::{
    ownership_checking::{
        UnitDropFact, UnitDropPoint, UnitIterationCleanupAction, UnitLoanAction, UnitLoanActionId,
        UnitLoanFact, UnitLoweringFacts,
    },
    type_checking::{UnitExpressionId, UnitItemId},
};
use std::rc::Rc;

pub(crate) struct Pending {
    pub(crate) caller: FunctionId,
    pub(crate) callable: UnitItemId,
    pub(crate) call: UnitExpressionId,
    pub(crate) callee: FunctionId,
    pub(crate) arguments: Vec<(UnitExpressionId, usize, Vec<u8>)>,
    pub(crate) returns_value: bool,
    pub(crate) block: BlockId,
    pub(crate) start: usize,
}
struct Completed {
    pending: Pending,
    ids: Vec<InstructionId>,
    instructions: Vec<Instruction>,
    results: Vec<(EntityId, EntityData)>,
}
#[derive(Default)]
struct Records {
    calls: Vec<Completed>,
    loans: Vec<(FunctionId, UnitLoanFact, InstructionId, LoanId)>,
    drops: Vec<(FunctionId, UnitDropFact, InstructionId, ValueId)>,
    invalid: bool,
}
type State = Rc<RefCell<Records>>;
thread_local! { static RUNS: RefCell<Vec<State>> = const { RefCell::new(Vec::new()) }; }
pub(super) struct Transaction(State);
impl Transaction {
    pub(super) fn begin() -> Self {
        let state = Rc::new(RefCell::new(Records::default()));
        RUNS.with(|runs| runs.borrow_mut().push(state.clone()));
        Self(state)
    }
    pub(super) fn finish(
        &self,
        program: &Program,
        expected: &BTreeMap<UnitExpressionId, (UnitItemId, Vec<UnitLoanActionId>)>,
        callers: &BTreeMap<UnitItemId, FunctionId>,
        owned: &dyn UnitLoweringFacts,
    ) -> (Vec<UnitExpressionId>, Vec<String>) {
        let records = self.0.borrow();
        let mut gaps = Vec::new();
        let mut groups = BTreeMap::<FunctionId, Vec<&Completed>>::new();
        for call in &records.calls {
            groups.entry(call.pending.caller).or_default().push(call);
        }
        let mut linked = Vec::new();
        for (caller, calls) in groups {
            let functions = program
                .modules
                .iter()
                .flat_map(|m| &m.functions)
                .filter(|f| f.id == caller)
                .collect::<Vec<_>>();
            let [function] = functions.as_slice() else {
                gaps.push("ambiguous Compatibility function".into());
                continue;
            };
            let ids = calls
                .iter()
                .flat_map(|c| c.ids.iter().copied())
                .collect::<Vec<_>>();
            let instructions = calls
                .iter()
                .flat_map(|c| c.instructions.iter().cloned())
                .collect::<Vec<_>>();
            let results = calls
                .iter()
                .flat_map(|c| c.results.iter().cloned())
                .collect::<Vec<_>>();
            if calls.len() != 2
                || records.invalid
                || !super::literal_consumption::complete_function(
                    function,
                    &ids,
                    &instructions,
                    &results,
                )
                || calls
                    .iter()
                    .any(|c| !valid_call(c, &records, expected, callers, owned))
            {
                gaps.push(format!(
                    "Compatibility fact/complete caller gap: {caller:?}"
                ));
                continue;
            }
            linked.extend(calls.iter().map(|c| c.pending.call));
        }
        (linked, gaps)
    }
}
impl Drop for Transaction {
    fn drop(&mut self) {
        RUNS.with(|runs| {
            let mut runs = runs.borrow_mut();
            if let Some(i) = runs.iter().position(|r| Rc::ptr_eq(r, &self.0)) {
                runs.remove(i);
            }
        });
    }
}
fn record(f: impl FnOnce(&mut Records)) {
    RUNS.with(|runs| {
        if let Some(state) = runs.borrow().last() {
            f(&mut state.borrow_mut());
        }
    });
}
pub(crate) fn loan(
    caller: FunctionId,
    fact: &UnitLoanFact,
    instruction: InstructionId,
    id: LoanId,
) {
    record(|r| r.loans.push((caller, fact.clone(), instruction, id)));
}
pub(crate) fn drop_fact(
    caller: FunctionId,
    fact: UnitDropFact,
    instruction: InstructionId,
    owner: ValueId,
) {
    record(|r| r.drops.push((caller, fact, instruction, owner)));
}
pub(crate) fn finish_call(pending: Pending, function: &Function) {
    record(|r| {
        let Some(ids) = function
            .block(pending.block)
            .and_then(|b| b.instructions.get(pending.start..))
            .map(<[_]>::to_vec)
        else {
            r.invalid = true;
            return;
        };
        let Some(instructions) = ids
            .iter()
            .map(|id| function.instruction(*id).cloned())
            .collect::<Option<Vec<_>>>()
        else {
            r.invalid = true;
            return;
        };
        let Some(results) = instructions
            .iter()
            .flat_map(|i| &i.results)
            .map(|id| function.entity(*id).cloned().map(|data| (*id, data)))
            .collect::<Option<Vec<_>>>()
        else {
            r.invalid = true;
            return;
        };
        r.calls.push(Completed {
            pending,
            ids,
            instructions,
            results,
        });
    });
}
fn valid_call(
    c: &Completed,
    records: &Records,
    expected: &BTreeMap<UnitExpressionId, (UnitItemId, Vec<UnitLoanActionId>)>,
    callers: &BTreeMap<UnitItemId, FunctionId>,
    owned: &dyn UnitLoweringFacts,
) -> bool {
    let p = &c.pending;
    let Some((item, ids)) = expected.get(&p.call) else {
        return false;
    };
    if *item != p.callable || callers.get(item) != Some(&p.caller) {
        return false;
    }
    let regions = owned
        .loan_action_observations()
        .iter()
        .flat_map(|o| {
            o.regions()
                .iter()
                .filter(move |r| r.call() == p.call)
                .map(move |r| (o, r))
        })
        .collect::<Vec<_>>();
    let [(observation, region)] = regions.as_slice() else {
        return false;
    };
    if !observation.owns_region(region) || region.action_ids().collect::<Vec<_>>() != *ids {
        return false;
    }
    let Some(actions) = ids
        .iter()
        .map(|id| observation.action(id))
        .collect::<Option<Vec<_>>>()
    else {
        return false;
    };
    let n = p.arguments.len();
    if n == 0 || actions.len() != 3 * n || c.ids.len() != 5 * n + 1 {
        return false;
    }
    let loans = records
        .loans
        .iter()
        .filter(|(f, fact, _, _)| *f == p.caller && fact.call() == p.call)
        .collect::<Vec<_>>();
    let drops = records
        .drops
        .iter()
        .filter(|(f, fact, _, _)| {
            *f == p.caller && fact.point() == UnitDropPoint::CallReturn(p.call)
        })
        .collect::<Vec<_>>();
    if loans.len() != n || drops.len() != n {
        return false;
    }
    let mut arguments = vec![None; n];
    for (i, (argument, parameter, literal_bytes)) in p.arguments.iter().enumerate() {
        if *parameter >= n || arguments[*parameter].is_some() {
            return false;
        }
        let (
            UnitLoanAction::Begin(begin),
            UnitLoanAction::Cleanup {
                point: end_point,
                action: UnitIterationCleanupAction::EndCallLoan(end),
            },
            UnitLoanAction::Cleanup {
                point: drop_point,
                action: UnitIterationCleanupAction::Drop(drop),
            },
        ) = (actions[i], actions[2 * n - 1 - i], actions[3 * n - 1 - i])
        else {
            return false;
        };
        let (_, consumed, begin_id, loan) = loans[i];
        let (_, consumed_drop, drop_id, owner) = drops[n - 1 - i];
        if begin.argument() != *argument
            || consumed != begin
            || end != begin
            || consumed_drop != drop
            || *end_point != UnitDropPoint::CallReturn(p.call)
            || *drop_point != *end_point
            || *begin_id != c.ids[3 * i + 2]
            || *drop_id != c.ids[5 * n - i]
        {
            return false;
        }
        let [owner_id] = c.instructions[3 * i].results.as_slice() else {
            return false;
        };
        let [place] = c.instructions[3 * i + 1].results.as_slice() else {
            return false;
        };
        let owner_entity = EntityId::Value(*owner);
        let loan_entity = EntityId::Loan(*loan);
        if *owner_id != owner_entity
            || protocol1_operation::validate(
                Expected::Literal {
                    bytes: literal_bytes,
                    owner: owner_entity,
                },
                &c.instructions[3 * i],
            )
            .is_none()
            || protocol1_operation::validate(
                Expected::Root {
                    owner: owner_entity,
                    place: *place,
                },
                &c.instructions[3 * i + 1],
            )
            .is_none()
            || protocol1_operation::validate(
                Expected::Begin {
                    place: *place,
                    loan: loan_entity,
                    caller: p.caller,
                },
                &c.instructions[3 * i + 2],
            )
            .is_none()
            || protocol1_operation::validate(
                Expected::End { loan: loan_entity },
                &c.instructions[4 * n - i],
            )
            .is_none()
            || protocol1_operation::validate(
                Expected::Drop {
                    owner: owner_entity,
                },
                &c.instructions[5 * n - i],
            )
            .is_none()
        {
            return false;
        }
        arguments[*parameter] = Some(loan_entity);
    }
    let Some(arguments) = arguments.into_iter().collect::<Option<Vec<_>>>() else {
        return false;
    };
    protocol1_operation::validate(
        Expected::Call {
            callee: p.callee,
            arguments: &arguments,
            returns_value: p.returns_value,
        },
        &c.instructions[3 * n],
    )
    .is_some()
}
