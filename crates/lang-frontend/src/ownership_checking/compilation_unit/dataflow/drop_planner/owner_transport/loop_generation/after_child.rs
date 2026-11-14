//! Complete actual child-then-parent traversal; no compatibility adapter to the old matcher.
use super::*;
use crate::ownership_checking::compilation_unit::d10_catalog_experiment::{
    AfterChildPending, RetainedLoopRoot,
};
use crate::ownership_checking::{LoanKind, UnitDropPoint, UnitDropTarget, UnitLoanTarget};
impl DropPlanner<'_, '_> {
    pub(in super::super::super) fn after_child_pending(&self) -> Option<AfterChildPending> {
        self.after_child_fields(self.origin_probe.as_ref()?, None)
    }
    pub(super) fn after_child_fields(
        &self,
        events: &[Event],
        wrapper: Option<&super::named_coverage::NamedCoverage>,
    ) -> Option<AfterChildPending> {
        let roles = self.after_child_coverage(wrapper)?;
        let c = self.checker;
        let source = c.source_unit;
        let ast = c.parsed.ast();
        let [a, b, outer, parent_old, inner, child_old] = roles.bindings;
        let expr = |id| c.unit_expression(id);
        let controls = roles.controls.map(|id| UnitStatementId::new(source, id));
        let calls = [expr(child_old.initializer), expr(parent_old.initializer)];
        let sites = [
            expr(a.initializer),
            expr(roles.incoming[1]),
            expr(roles.incoming[0]),
        ];
        let final_call = expr(roles.final_use?);
        let suffix_call = expr(roles.suffix);
        // Every event is consumed at its original position. Bind only actual loan/drop facts.
        let [
            Event::Literal(initial),
            Event::Bound(root, bound_initial),
            Event::Literal(kept),
            Event::Bound(kept_root, bound_kept),
            Event::WhileTraversal {
                control: p0,
                role: WhileRole::Enter,
            },
            Event::WhileTraversal {
                control: p1,
                role: WhileRole::ConditionEvaluated,
            },
            Event::WhileTraversal {
                control: p2,
                role: WhileRole::BodyEnter,
            },
            Event::WhileTraversal {
                control: c0,
                role: WhileRole::Enter,
            },
            Event::WhileTraversal {
                control: c1,
                role: WhileRole::ConditionEvaluated,
            },
            Event::WhileTraversal {
                control: c2,
                role: WhileRole::BodyEnter,
            },
            Event::BorrowRegistered(child_loan),
            Event::Literal(child_value),
            Event::Applied {
                call: child_call,
                root: child_root,
                incoming: child_applied,
            },
            Event::PendingBorrowRemoved {
                loan: child_removed,
                call: child_removed_call,
            },
            Event::Bound(child_old_root, child_old_call),
            Event::Dropped(child_drop),
            Event::WhileTraversal {
                control: c3,
                role: WhileRole::BodyExit(Some(true)),
            },
            Event::WhileTraversal {
                control: c4,
                role: WhileRole::Exit(Some(true)),
            },
            Event::BorrowRegistered(parent_loan),
            Event::Literal(parent_value),
            Event::Applied {
                call: parent_call,
                root: parent_root,
                incoming: parent_applied,
            },
            Event::PendingBorrowRemoved {
                loan: parent_removed,
                call: parent_removed_call,
            },
            Event::Bound(parent_old_root, parent_old_call),
            Event::Dropped(parent_drop),
            Event::WhileTraversal {
                control: p3,
                role: WhileRole::BodyExit(Some(true)),
            },
            Event::WhileTraversal {
                control: p4,
                role: WhileRole::Exit(Some(true)),
            },
            Event::UncoveredCall(final_seen),
            Event::BorrowRegistered(final_loan),
            Event::PendingBorrowRemoved {
                loan: final_removed,
                call: final_removed_call,
            },
            Event::Dropped(final_drop),
            Event::UncoveredCall(suffix_seen),
            Event::BorrowRegistered(suffix_loan),
            Event::PendingBorrowRemoved {
                loan: suffix_removed,
                call: suffix_removed_call,
            },
            Event::Dropped(suffix_drop),
        ] = events
        else {
            return None;
        };
        if [p0, p1, p2, p3, p4].iter().any(|id| **id != controls[0])
            || [c0, c1, c2, c3, c4].iter().any(|id| **id != controls[1])
            || [root, child_root, parent_root]
                .iter()
                .any(|id| **id != a.symbol)
            || *kept_root != b.symbol
            || *initial != sites[0]
            || bound_initial != initial
            || *kept != expr(b.initializer)
            || bound_kept != kept
            || *child_value != sites[1]
            || child_applied != child_value
            || *parent_value != sites[2]
            || parent_applied != parent_value
            || [child_call, child_removed_call, child_old_call]
                .iter()
                .any(|id| **id != calls[0])
            || [parent_call, parent_removed_call, parent_old_call]
                .iter()
                .any(|id| **id != calls[1])
            || *child_old_root != child_old.symbol
            || *parent_old_root != parent_old.symbol
            || child_loan != child_removed
            || parent_loan != parent_removed
            || final_loan != final_removed
            || suffix_loan != suffix_removed
            || *final_seen != final_call
            || *final_removed_call != final_call
            || *suffix_seen != suffix_call
            || *suffix_removed_call != suffix_call
        {
            return None;
        }
        for (loan, call, root, kind) in [
            (child_loan, calls[0], a.symbol, LoanKind::Exclusive),
            (parent_loan, calls[1], a.symbol, LoanKind::Exclusive),
            (final_loan, final_call, a.symbol, LoanKind::Shared),
            (suffix_loan, suffix_call, b.symbol, LoanKind::Shared),
        ] {
            let crate::parser::Expression::Call { arguments, .. } =
                ast.expressions().get(call.expression()).ok()?.payload()
            else {
                return None;
            };
            let first = arguments.first()?;
            let begin = match (kind, first.mode_marker) {
                (LoanKind::Exclusive, Some(crate::parser::ParameterModeMarker::Inout(span))) => {
                    span
                }
                (LoanKind::Shared, None) => ast.expressions().get(first.value).ok()?.span(),
                _ => return None,
            };
            if loan.call() != call
                || loan.kind() != kind
                || loan.argument() != expr(first.value)
                || loan.begin_span() != begin
                || loan.end_span() != ast.expressions().get(call.expression()).ok()?.span()
                || !matches!(loan.target(),UnitLoanTarget::Place(p) if p.is_root() && p.root()==root)
            {
                return None;
            }
        }
        for (drop, binding) in [(child_drop, child_old), (parent_drop, parent_old)] {
            let UnitDropPoint::AfterStatement(stmt) = drop.point() else {
                return None;
            };
            if stmt.source_unit() != source
                || !matches!(ast.statements().get(stmt.statement()).ok()?.payload(),crate::parser::Statement::LocalVariable{declaration} if *declaration==binding.item)
                || drop.target() != UnitDropTarget::Named(binding.symbol)
                || drop.value_origin() != binding.span
            {
                return None;
            }
        }
        for (drop, binding, call) in [(final_drop, a, final_call), (suffix_drop, b, suffix_call)] {
            if drop.point() != UnitDropPoint::CallReturn(call)
                || drop.target() != UnitDropTarget::Named(binding.symbol)
                || drop.value_origin() != binding.span
            {
                return None;
            }
        }
        let drops = [*child_drop, *parent_drop, *final_drop, *suffix_drop];
        if self
            .facts
            .iter()
            .map(|fact| fact.into_unit(source))
            .collect::<Vec<_>>()
            != drops
        {
            return None;
        }
        if c.ownership_primitives.len() != 2 {
            return None;
        }
        for ((call, incoming), plan) in calls
            .into_iter()
            .zip(sites[1..].iter().copied())
            .zip(c.ownership_primitives.iter())
        {
            if plan.descriptor().expression() != call
                || plan.descriptor().kind() != OwnershipPrimitiveKind::Replace
                || plan.places().len() != 1
                || !plan.places()[0].is_root()
                || plan.places()[0].root() != a.symbol
                || plan.descriptor().operands().get(1) != Some(&incoming)
            {
                return None;
            }
        }
        Some(AfterChildPending {
            typed: c.typed.analysis_owner().clone(),
            analysis_owner: c.analysis_owner.clone(),
            callable: crate::type_checking::UnitItemId::new(source, roles.function),
            controls,
            root: a.symbol,
            counters: [outer.symbol, inner.symbol],
            counter_parameters: roles.parameters,
            counter_initializers: [expr(outer.initializer), expr(inner.initializer)],
            conditions: roles.conditions.map(expr),
            steps: roles.steps.map(expr),
            sites,
            calls,
            cleanup: [*child_drop, *parent_drop, *final_drop],
            loans: [child_loan.clone(), parent_loan.clone()],
            final_root_use: RetainedLoopRoot {
                root: a.symbol,
                initial: sites[0],
                call: final_call,
                loan: final_loan.clone(),
                cleanup: *final_drop,
            },
            retained: RetainedLoopRoot {
                root: b.symbol,
                initial: expr(b.initializer),
                call: suffix_call,
                loan: suffix_loan.clone(),
                cleanup: *suffix_drop,
            },
        })
    }
}
