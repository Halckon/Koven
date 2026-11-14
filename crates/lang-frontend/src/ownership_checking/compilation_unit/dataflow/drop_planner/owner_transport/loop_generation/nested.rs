//! Original planner trace matching for the independent nested experiment.
//! No trace filtering, synthetic token, or singleton generation permission.
use super::*;
use crate::ownership_checking::compilation_unit::d10_catalog_experiment::{
    LoopPending, NestedPending, RetainedLoopRoot,
};
impl DropPlanner<'_, '_> {
    pub(in super::super::super) fn nested_pending(&self) -> Option<NestedPending> {
        use crate::ownership_checking::{UnitDropPoint, UnitDropTarget};
        let events = self.origin_probe.as_ref()?;
        let [
            Event::Literal(initial),
            Event::Bound(root, bound_initial),
            rest @ ..,
        ] = events.as_slice()
        else {
            return None;
        };
        let (loop_events, retained) = {
            let [
                Event::Literal(kept),
                Event::Bound(kept_root, bound_kept),
                tail @ ..,
            ] = rest
            else {
                return None;
            };
            let loop_events = tail.get(..17)?;
            let [
                Event::UncoveredCall(suffix),
                Event::BorrowRegistered(shared),
                Event::PendingBorrowRemoved {
                    loan: removed_shared,
                    call: end_call,
                },
                Event::Dropped(kept_drop),
            ] = tail.get(17..)?
            else {
                return None;
            };
            if kept != bound_kept
                || [*initial, *bound_initial].contains(kept)
                || root == kept_root
                || shared != removed_shared
                || end_call != suffix
                || shared.call() != *suffix
                || shared.kind() != crate::ownership_checking::LoanKind::Shared
                || kept_drop.point() != UnitDropPoint::CallReturn(*suffix)
                || kept_drop.target() != UnitDropTarget::Named(*kept_root)
            {
                return None;
            }
            let crate::ownership_checking::UnitLoanTarget::Place(place) = shared.target() else {
                return None;
            };
            if !place.is_root() || place.root() != *kept_root {
                return None;
            }
            (
                loop_events,
                Retained {
                    root: *kept_root,
                    initial: *kept,
                    call: *suffix,
                    loan: shared.clone(),
                    cleanup: *kept_drop,
                },
            )
        };
        let [
            Event::WhileTraversal {
                control,
                role: WhileRole::Enter,
            },
            Event::WhileTraversal {
                control: condition_control,
                role: WhileRole::ConditionEvaluated,
            },
            Event::WhileTraversal {
                control: body_control,
                role: WhileRole::BodyEnter,
            },
            Event::WhileTraversal {
                control: child,
                role: WhileRole::Enter,
            },
            Event::WhileTraversal {
                control: child_condition,
                role: WhileRole::ConditionEvaluated,
            },
            Event::WhileTraversal {
                control: child_body,
                role: WhileRole::BodyEnter,
            },
            Event::BorrowRegistered(loan),
            Event::Literal(incoming),
            Event::Applied {
                call,
                root: replaced,
                incoming: applied_incoming,
            },
            Event::PendingBorrowRemoved {
                loan: removed,
                call: removal_call,
            },
            Event::Bound(old_root, old_call),
            Event::Dropped(old_drop),
            Event::WhileTraversal {
                control: child_back,
                role: WhileRole::BodyExit(Some(true)),
            },
            Event::WhileTraversal {
                control: child_exit,
                role: WhileRole::Exit(Some(true)),
            },
            Event::WhileTraversal {
                control: back_control,
                role: WhileRole::BodyExit(Some(true)),
            },
            Event::Dropped(exit_drop),
            Event::WhileTraversal {
                control: exit_control,
                role: WhileRole::Exit(Some(true)),
            },
        ] = loop_events
        else {
            return None;
        };
        let source = self.checker.source_unit;
        if [
            *condition_control,
            *body_control,
            *back_control,
            *exit_control,
        ]
        .iter()
        .any(|id| id != control)
            || child == control
            || [child_condition, child_body, child_back, child_exit]
                .iter()
                .any(|id| *id != child)
            || child.source_unit() != source
            || control.source_unit() != source
            || [*initial, *incoming, *call]
                .iter()
                .any(|id| id.source_unit() != source)
            || [*root, *old_root]
                .iter()
                .any(|id| id.source_unit() != source)
            || initial != bound_initial
            || incoming != applied_incoming
            || call != old_call
            || root != replaced
            || root == old_root
            || initial == incoming
            || loan != removed
            || removal_call != call
            || loan.call() != *call
            || old_drop.target() != UnitDropTarget::Named(*old_root)
            || exit_drop.target() != UnitDropTarget::Named(*root)
            || exit_drop.point() != UnitDropPoint::LoopExit(*control)
        {
            return None;
        }
        let UnitDropPoint::AfterStatement(old_statement) = old_drop.point() else {
            return None;
        };
        if old_statement.source_unit() != source {
            return None;
        }
        let ast = self.checker.parsed.ast();
        let crate::parser::Statement::LocalVariable { declaration } = ast
            .statements()
            .get(old_statement.statement())
            .ok()?
            .payload()
        else {
            return None;
        };
        let crate::parser::Item::Variable {
            name: crate::parser::NameMarker::Present(span),
            initializer,
            ..
        } = ast.items().get(*declaration).ok()?.payload()
        else {
            return None;
        };
        if self.checker.unit_expression(*initializer) != *call
            || self.checker.symbols_by_span.get(&span_key(*span)) != Some(old_root)
            || old_drop.value_origin() != *span
            || loan.kind() != crate::ownership_checking::LoanKind::Exclusive
        {
            return None;
        }
        let observed_drops = self
            .facts
            .iter()
            .map(|fact| fact.into_unit(source))
            .collect::<Vec<_>>();
        let mut expected_drops = vec![*old_drop, *exit_drop];
        expected_drops.push(retained.cleanup);
        if observed_drops != expected_drops {
            return None;
        }
        // Confirm the actual applied operation, never infer Replace from syntax alone.
        let plan = self
            .checker
            .ownership_primitives
            .iter()
            .find(|p| p.descriptor().expression() == *call)?;
        if plan.descriptor().kind() != OwnershipPrimitiveKind::Replace
            || plan.places().len() != 1
            || !plan.places()[0].is_root()
            || plan.places()[0].root() != *root
            || plan.descriptor().operands()[1] != *incoming
        {
            return None;
        }
        let crate::ownership_checking::UnitLoanTarget::Place(place) = loan.target() else {
            return None;
        };
        if !place.is_root() || place.root() != *root {
            return None;
        }
        let coverage = self.nested_generation_coverage(
            *control,
            *child,
            *root,
            *old_root,
            (*initial, *incoming, *call),
            &retained,
        )?;
        Some(NestedPending {
            child: *child,
            child_counter: coverage.child_counter,
            child_condition: coverage.child_condition,
            child_step: coverage.child_step,
            facts: LoopPending {
                retained: Some(RetainedLoopRoot {
                    root: retained.root,
                    initial: retained.initial,
                    call: retained.call,
                    loan: retained.loan,
                    cleanup: retained.cleanup,
                }),
                typed: self.checker.typed.analysis_owner().clone(),
                callable: coverage.callable,
                control: *control,
                root: *root,
                counter: coverage.parent_counter,
                parameter: coverage.parameter,
                condition: coverage.parent_condition,
                step: coverage.parent_step,
                initial: *initial,
                incoming: *incoming,
                call: *call,
                cleanup: [*old_drop, *exit_drop],
                loan: loan.clone(),
            },
        })
    }
}
