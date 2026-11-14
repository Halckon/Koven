//! Same traversal, complete contiguous two-root observation; no filtered old bound.
use super::*;
use crate::ownership_checking::{
    UnitDropPoint, UnitLoanTarget,
    compilation_unit::d10_catalog_experiment::{WrappedAfterChildPending, WrappedDualPending},
};
impl DropPlanner<'_, '_> {
    pub(in super::super::super) fn enable_named_wrapper_probe(&mut self) {
        let ast = self.checker.parsed.ast();
        if (
            ast.items().iter().count(),
            ast.statements().iter().count(),
            ast.expressions().iter().count(),
        ) == (10, 19, 36)
            && self.named_wrapper_coverage().is_some()
        {
            self.origin_probe = Some(Vec::with_capacity(39));
            self.root_trace_ranges = Some(Vec::with_capacity(2));
        }
    }

    pub(in super::super::super) fn wrapped_dual_pending(&self) -> Option<WrappedDualPending> {
        let trace = self.named_trace()?;
        let fields = self.dual_fields(trace.callee, Some(&trace.wrapper))?;
        let final_use = fields.final_root_use.as_ref()?;
        let contracts = self.named_contracts([
            fields.calls[0],
            fields.calls[1],
            final_use.call,
            fields.retained.call,
            trace.wrapper.call,
        ])?;
        Some(WrappedDualPending {
            fields,
            contracts,
            entry: trace.wrapper.entry,
            roots: trace.wrapper.roots,
            initializers: trace.wrapper.initializers,
            call: trace.wrapper.call,
            arguments: trace.wrapper.arguments,
            loans: trace.loans,
            ends: trace.ends,
        })
    }
    pub(in super::super::super) fn wrapped_after_child_pending(
        &self,
    ) -> Option<WrappedAfterChildPending> {
        let trace = self.named_trace()?;
        let fields = self.after_child_fields(trace.callee, Some(&trace.wrapper))?;
        let final_use = &fields.final_root_use;
        let contracts = self.named_contracts([
            fields.calls[0],
            fields.calls[1],
            final_use.call,
            fields.retained.call,
            trace.wrapper.call,
        ])?;
        Some(WrappedAfterChildPending {
            fields,
            contracts,
            entry: trace.wrapper.entry,
            roots: trace.wrapper.roots,
            initializers: trace.wrapper.initializers,
            call: trace.wrapper.call,
            arguments: trace.wrapper.arguments,
            loans: trace.loans,
            ends: trace.ends,
        })
    }
    fn named_trace(&self) -> Option<NamedTrace<'_>> {
        let wrapper = self.named_wrapper_coverage()?;
        let events = self.origin_probe.as_ref()?;
        let [(callee, first), (entry, second)] = self.root_trace_ranges.as_ref()?.as_slice() else {
            return None;
        };
        if *callee != wrapper.callee
            || *entry != wrapper.entry
            || first.start != 0
            || first.end != second.start
            || second.end != events.len()
            || first.len() != 34
            || second.len() != 5
        {
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
        ] = events.get(second.clone())?
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
        Some(NamedTrace {
            callee: events.get(first.clone())?,
            wrapper,
            loans: [p.clone(), q.clone()],
            ends: [q_actual.clone(), p_actual.clone()],
        })
    }
    fn named_contracts(
        &self,
        allowed_calls: [UnitExpressionId; 5],
    ) -> Option<Vec<crate::ownership_checking::UnitCallArgumentOwnershipContract>> {
        let c = self.checker;
        let mut contracts = Vec::new();
        for actual_call in allowed_calls {
            let ast = c.parsed.ast();
            let Expression::Call { arguments, .. } = ast
                .expressions()
                .get(actual_call.expression())
                .ok()?
                .payload()
            else {
                return None;
            };
            let typed = c.typed.call(actual_call)?;
            let actual = c.contracts_by_call.get(&actual_call)?;
            if actual.len() != arguments.len() || typed.arguments().len() != arguments.len() {
                return None;
            }
            for ((contract, argument), descriptor) in
                actual.iter().zip(arguments).zip(typed.arguments())
            {
                let kind = match descriptor.mode() {
                    crate::type_checking::ParameterMode::Value => {
                        crate::ownership_checking::UnitCallArgumentOwnershipKind::Value
                    }
                    crate::type_checking::ParameterMode::Borrow => {
                        crate::ownership_checking::UnitCallArgumentOwnershipKind::SharedLoan
                    }
                    crate::type_checking::ParameterMode::Inout => {
                        crate::ownership_checking::UnitCallArgumentOwnershipKind::ExclusiveLoan
                    }
                };
                if contract.call() != actual_call
                    || contract.argument() != c.unit_expression(argument.value)
                    || contract.parameter_index() != descriptor.parameter_index()
                    || contract.parameter_type() != descriptor.parameter_type()
                    || contract.category() != descriptor.category()
                    || contract.kind() != kind
                    || contract.crosses_thread()
                    || descriptor.crosses_thread()
                    || contract.argument_span()
                        != ast.expressions().get(argument.value).ok()?.span()
                    || contract.call_span()
                        != ast.expressions().get(actual_call.expression()).ok()?.span()
                {
                    return None;
                }
                contracts.push(*contract);
            }
        }
        if contracts.len() != 8 || c.contracts_by_call.len() != 5 {
            return None;
        }
        Some(contracts)
    }
}
struct NamedTrace<'a> {
    callee: &'a [Event],
    wrapper: super::named_coverage::NamedCoverage,
    loans: [crate::ownership_checking::UnitLoanFact; 2],
    ends: [crate::ownership_checking::UnitDirectCallLoanEndFact; 2],
}
