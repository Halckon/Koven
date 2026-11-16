//! Same-assembly identity exits. Input candidates precede emission and are not receipt-derived.
use super::*;
use crate::ssa::{
    lowering_support::callable_instances::SourceToken,
    model::{BlockId, LoanId, TerminatorKind},
    refine::{RefineBody, RefineInputs, identity_return::IdentityReturnReceipt},
};
use lang_frontend::{
    parser::Expression,
    type_checking::{UnitExpressionId, UnitItemId},
};
use std::rc::Rc;
#[derive(Clone, Debug, PartialEq, Eq)]
struct Expected {
    token: SourceToken,
    key: UnitFunctionInstanceKey,
    callable: UnitItemId,
    expression: UnitExpressionId,
}
#[derive(Default)]
struct State {
    expected: Vec<Expected>,
    receipts: Vec<IdentityReturnReceipt>,
}
type Shared = Rc<RefCell<State>>;
type Returns = BTreeSet<(FunctionId, BlockId, LoanId)>;
thread_local! { static RUNS: RefCell<Vec<Shared>> = const { RefCell::new(Vec::new()) }; }
pub(super) struct Transaction {
    state: Shared,
    planned: BTreeMap<SourceToken, (UnitFunctionInstanceKey, UnitItemId)>,
}
impl Transaction {
    pub(super) fn begin(plan: &UnitInstancePlan) -> Self {
        let state = Rc::new(RefCell::new(State::default()));
        RUNS.with(|runs| runs.borrow_mut().push(state.clone()));
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
    ) -> (Returns, Vec<String>) {
        let state = self.state.borrow();
        let mut returns = Returns::new();
        let mut gaps = Vec::new();
        let mut seen = BTreeSet::new();
        for expected in &state.expected {
            if !seen.insert(expected.token) {
                gaps.push("duplicate identity input candidate".into());
                continue;
            }
            if self.planned.get(&expected.token) != Some(&(expected.key.clone(), expected.callable))
            {
                gaps.push("identity input outside current plan".into());
                continue;
            }
            let matches: Vec<_> = state
                .receipts
                .iter()
                .filter(|r| r.source_token() == expected.token)
                .collect();
            let [receipt] = matches.as_slice() else {
                gaps.push("missing/duplicate identity receipt".into());
                continue;
            };
            if receipt.key() != &expected.key
                || receipt.callable() != expected.callable
                || receipt.expression() != expected.expression
                || mapping.get(&expected.key) != Some(&receipt.function())
                || !receipt.matches_final(program)
            {
                gaps.push("identity final source/definition mismatch".into());
                continue;
            }
            let functions: Vec<_> = program
                .modules
                .iter()
                .flat_map(|m| &m.functions)
                .filter(|f| f.id == receipt.function())
                .collect();
            let [function] = functions.as_slice() else {
                gaps.push("identity definition count mismatch".into());
                continue;
            };
            let [block] = function.blocks.as_slice() else {
                gaps.push("identity block count mismatch".into());
                continue;
            };
            if block.terminator.as_ref().map(|t| &t.kind)
                != Some(&TerminatorKind::BorrowReturn {
                    loan: receipt.loan(),
                })
            {
                gaps.push("identity exit mismatch".into());
                continue;
            }
            returns.insert((function.id, block.id, receipt.loan()));
        }
        for receipt in &state.receipts {
            if !seen.contains(&receipt.source_token()) {
                gaps.push("identity receipt without input candidate".into());
            }
        }
        (returns, gaps)
    }
}
impl Drop for Transaction {
    fn drop(&mut self) {
        RUNS.with(|runs| runs.borrow_mut().retain(|s| !Rc::ptr_eq(s, &self.state)));
    }
}
pub(in crate::ssa) fn expect(inputs: &RefineInputs<'_>) {
    let RefineBody::Expression(raw) = inputs.body else {
        return;
    };
    if inputs.borrow_return.is_none()
        || !inputs
            .parsed
            .ast()
            .expressions()
            .get(raw)
            .is_ok_and(|n| matches!(n.payload(), Expression::Name))
    {
        return;
    }
    let Some(item) = inputs.function_item else {
        return;
    };
    let Some(key) = inputs.source_plan.source(inputs.source_token) else {
        return;
    };
    let expected = Expected {
        token: inputs.source_token,
        key: key.clone(),
        callable: UnitItemId::new(inputs.source_unit, item),
        expression: UnitExpressionId::new(inputs.source_unit, raw),
    };
    RUNS.with(|runs| {
        if let Some(state) = runs.borrow().last() {
            state.borrow_mut().expected.push(expected);
        }
    });
}
pub(crate) fn record(receipt: IdentityReturnReceipt) {
    RUNS.with(|runs| {
        if let Some(state) = runs.borrow().last() {
            state.borrow_mut().receipts.push(receipt);
        }
    });
}
