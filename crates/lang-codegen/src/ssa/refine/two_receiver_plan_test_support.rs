//! Test-only snapshot of the real planned body immediately before SSA emission.
use super::body::{ROp, RStmt, RefinedBody, Slot};
use crate::ssa::model::{FunctionId, LoanKind};
use lang_frontend::source::Span;
use std::cell::RefCell;

thread_local! {
    static PLANS: RefCell<Option<Vec<Planned>>> = const { RefCell::new(None) };
}

pub(in crate::ssa) struct Planned {
    pub(in crate::ssa) function: FunctionId,
    parameters: Vec<Slot>,
    blocks: Vec<Vec<RStmt>>,
}

pub(super) fn record(function: FunctionId, body: &RefinedBody) {
    PLANS.with(|plans| {
        if let Some(plans) = plans.borrow_mut().as_mut() {
            plans.push(Planned {
                function,
                parameters: body.parameters.clone(),
                blocks: body
                    .blocks
                    .iter()
                    .map(|block| block.statements.clone())
                    .collect(),
            });
        }
    });
}

pub(in crate::ssa) fn observe<T>(lower: impl FnOnce() -> T) -> (T, Vec<Planned>) {
    PLANS.with(|plans| *plans.borrow_mut() = Some(Vec::new()));
    let output = lower();
    (
        output,
        PLANS.with(|plans| plans.borrow_mut().take().unwrap()),
    )
}

impl Planned {
    /// Cross-check the exact source receiver/call span and dispatched callee already
    /// identified in SSA, but validate every slot identity independently before SSA.
    pub(in crate::ssa) fn check_two_calls(
        &self,
        expected: &[(Span, Span, FunctionId); 2],
    ) -> Option<Span> {
        let [owner] = self.parameters.as_slice() else {
            return None;
        };
        let [statements] = self.blocks.as_slice() else {
            return None;
        };
        let drop = check(statements, *owner, expected)?;
        let calls = statements
            .iter()
            .enumerate()
            .filter_map(|(index, statement)| {
                if let ROp::DirectCall {
                    receiver: Some(loan),
                    ..
                } = statement.op
                {
                    Some((index, loan))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        let [(_, first), (second_index, _)] = calls.as_slice() else {
            return None;
        };
        // Corrupt only a copied ROp operand, never either public product.
        let mut wrong = statements.clone();
        let ROp::DirectCall { receiver, .. } = &mut wrong[*second_index].op else {
            return None;
        };
        *receiver = Some(*first);
        assert!(
            check(&wrong, *owner, expected).is_none(),
            "prior loan cannot be reused"
        );
        Some(drop)
    }
}

fn check(
    statements: &[RStmt],
    owner: Slot,
    expected: &[(Span, Span, FunctionId); 2],
) -> Option<Span> {
    let lifetime = statements
        .iter()
        .enumerate()
        .filter(|(_, statement)| {
            matches!(
                statement.op,
                ROp::RootPlace { .. }
                    | ROp::BorrowBegin { .. }
                    | ROp::DirectCall { .. }
                    | ROp::BorrowEnd { .. }
                    | ROp::Drop { .. }
            )
        })
        .collect::<Vec<_>>();
    let mut actions = lifetime.into_iter().peekable();
    let mut slots = Vec::new();
    let mut loans = Vec::new();
    for (receiver_span, call_span, callee) in expected {
        if let Some((_, op)) = actions.peek()
            && let ROp::RootPlace { owner: actual } = op.op
        {
            let [place] = op.results.as_slice() else {
                return None;
            };
            if actual != owner || op.span != *receiver_span || slots.contains(place) {
                return None;
            }
            slots.push(*place);
            actions.next();
        }
        let (_, begin) = actions.next()?;
        let ROp::BorrowBegin {
            place,
            kind: LoanKind::Shared,
        } = begin.op
        else {
            return None;
        };
        let [loan] = begin.results.as_slice() else {
            return None;
        };
        let (_, call) = actions.next()?;
        let ROp::DirectCall {
            callee: actual,
            receiver,
            arguments,
        } = &call.op
        else {
            return None;
        };
        let (_, end) = actions.next()?;
        let ROp::BorrowEnd { loan: retired } = end.op else {
            return None;
        };
        if !slots.contains(&place)
            || loans.contains(loan)
            || *loan == owner
            || begin.span != *receiver_span
            || *actual != *callee
            || *receiver != Some(*loan)
            || !arguments.is_empty()
            || call.results.len() != 1
            || call.span != *call_span
            || retired != *loan
            || !end.results.is_empty()
            || end.span != *call_span
        {
            return None;
        }
        loans.push(*loan);
    }
    let (_, drop) = actions.next()?;
    if !matches!(drop.op, ROp::Drop { owner: actual } if actual == owner)
        || !drop.results.is_empty()
        || actions.next().is_some()
    {
        return None;
    }
    for slot in loans.into_iter().chain(slots) {
        if statements
            .iter()
            .filter(|statement| statement.results.contains(&slot))
            .count()
            != 1
        {
            return None;
        }
    }
    if statements
        .iter()
        .any(|statement| statement.results.contains(&owner))
    {
        return None;
    }
    Some(drop.span)
}
