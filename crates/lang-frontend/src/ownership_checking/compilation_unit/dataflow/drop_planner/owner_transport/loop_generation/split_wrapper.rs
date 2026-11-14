//! Independent complete single-source wrapper trace; never synthesizes a named two-root trace.
use super::*;
use crate::ownership_checking::compilation_unit::d10_catalog_experiment::{
    SplitSourcePending, SplitWrapperPending,
};
use crate::ownership_checking::{UnitDropPoint, UnitLoanTarget};
impl DropPlanner<'_, '_> {
    pub(in super::super::super) fn enable_split_wrapper_probe(&mut self) {
        if self.split_wrapper_coverage().is_some() {
            self.origin_probe = Some(Vec::with_capacity(5));
            self.root_trace_ranges = Some(Vec::with_capacity(1));
        }
    }
    pub(in super::super::super) fn split_source_pending(&self) -> Option<SplitSourcePending> {
        if let Some(wrapper) = self.split_wrapper_pending() {
            return Some(SplitSourcePending::Wrapper(wrapper));
        }
        // These collectors already prove complete single-root source coverage and consume only
        // the original observed trace. No old singleton bound is constructed.
        match (self.dual_nested_pending(), self.after_child_pending()) {
            (Some(p), None) => {
                let last = p.final_root_use.as_ref()?;
                if p.counter_parameters[0] == p.counter_parameters[1] {
                    return None;
                }
                let contracts =
                    self.source_contracts([p.calls[0], p.calls[1], last.call, p.retained.call], 6)?;
                Some(SplitSourcePending::Before(p, contracts))
            }
            (None, Some(p)) => {
                if p.counter_parameters[0] == p.counter_parameters[1] {
                    return None;
                }
                let contracts = self.source_contracts(
                    [
                        p.calls[0],
                        p.calls[1],
                        p.final_root_use.call,
                        p.retained.call,
                    ],
                    6,
                )?;
                Some(SplitSourcePending::After(p, contracts))
            }
            _ => None,
        }
    }
    fn split_wrapper_pending(&self) -> Option<SplitWrapperPending> {
        let wrapper = self.split_wrapper_coverage()?;
        let events = self.origin_probe.as_ref()?;
        let [(root, range)] = self.root_trace_ranges.as_ref()?.as_slice() else {
            return None;
        };
        if *root != wrapper.entry || range.start != 0 || range.end != events.len() {
            return None;
        }
        let [
            Event::UncoveredCall(call),
            Event::BorrowRegistered(p),
            Event::BorrowRegistered(q),
            Event::DirectBorrowEnded {
                loan: q_end,
                point: q_point,
            },
            Event::DirectBorrowEnded {
                loan: p_end,
                point: p_point,
            },
        ] = events.as_slice()
        else {
            return None;
        };
        if *call != wrapper.call
            || p != p_end
            || q != q_end
            || *p_point != UnitDropPoint::CallReturn(*call)
            || q_point != p_point
        {
            return None;
        }
        let c = self.checker;
        if !c.entered_calls.contains(call)
            || c.call_dispatch.get(call) != Some(&true)
            || wrapper
                .arguments
                .iter()
                .any(|id| !c.completed_arguments.contains(id))
        {
            return None;
        }
        for ((loan, argument), root) in [p, q].into_iter().zip(wrapper.arguments).zip(wrapper.roots)
        {
            let span = c
                .parsed
                .ast()
                .expressions()
                .get(argument.expression())
                .ok()?
                .span();
            if loan.call() != *call
                || loan.argument() != argument
                || loan.kind() != crate::ownership_checking::LoanKind::Shared
                || !matches!(loan.target(),UnitLoanTarget::Place(place) if place.is_root() && place.root()==root)
                || loan.begin_span() != span
                || loan.end_span()
                    != c.parsed
                        .ast()
                        .expressions()
                        .get(call.expression())
                        .ok()?
                        .span()
            {
                return None;
            }
        }
        let ends = self.direct_call_loan_ends();
        let [q_actual, p_actual] = ends.as_slice() else {
            return None;
        };
        if q_actual.loan() != q
            || p_actual.loan() != p
            || q_actual.point() != *q_point
            || p_actual.point() != *p_point
            || q_actual.preceding_call_loan_ends() != 0
            || p_actual.preceding_call_loan_ends() != 1
            || q_actual.preceding_drops() != 0
            || p_actual.preceding_drops() != 0
        {
            return None;
        }
        Some(SplitWrapperPending {
            typed: c.typed.analysis_owner().clone(),
            analysis_owner: c.analysis_owner.clone(),
            entry: wrapper.entry,
            callee: wrapper.callee,
            target: wrapper.target,
            roots: wrapper.roots,
            initializers: wrapper.initializers,
            call: wrapper.call,
            arguments: wrapper.arguments,
            loans: [p.clone(), q.clone()],
            ends: [q_actual.clone(), p_actual.clone()],
            contracts: self.source_contracts([wrapper.call], 2)?,
        })
    }
}
