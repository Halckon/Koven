//! Exact original or final-root-use traversal. Never accepts, filters or reconstructs external events.
use super::*;
use crate::ownership_checking::compilation_unit::d10_catalog_experiment::{
    DualNestedPending, RetainedLoopRoot,
};
impl DropPlanner<'_, '_> {
    pub(in super::super::super) fn dual_nested_pending(&self) -> Option<DualNestedPending> {
        self.dual_fields(self.origin_probe.as_ref()?, None)
    }

    pub(super) fn dual_fields(
        &self,
        events: &[Event],
        wrapper: Option<&super::named_coverage::NamedCoverage>,
    ) -> Option<DualNestedPending> {
        use crate::ownership_checking::{LoanKind, UnitDropPoint, UnitDropTarget, UnitLoanTarget};
        let [
            Event::Literal(initial),
            Event::Bound(root, bound_initial),
            Event::Literal(kept),
            Event::Bound(kept_root, bound_kept),
            Event::WhileTraversal {
                control: parent,
                role: WhileRole::Enter,
            },
            Event::WhileTraversal {
                control: pcond,
                role: WhileRole::ConditionEvaluated,
            },
            Event::WhileTraversal {
                control: pbody,
                role: WhileRole::BodyEnter,
            },
            Event::BorrowRegistered(parent_loan),
            Event::Literal(parent_incoming),
            Event::Applied {
                call: parent_call,
                root: parent_root,
                incoming: parent_applied,
            },
            Event::PendingBorrowRemoved {
                loan: parent_removed,
                call: parent_removed_call,
            },
            Event::Bound(parent_old, parent_old_call),
            Event::Dropped(parent_drop),
            Event::WhileTraversal {
                control: child,
                role: WhileRole::Enter,
            },
            Event::WhileTraversal {
                control: ccond,
                role: WhileRole::ConditionEvaluated,
            },
            Event::WhileTraversal {
                control: cbody,
                role: WhileRole::BodyEnter,
            },
            Event::BorrowRegistered(child_loan),
            Event::Literal(child_incoming),
            Event::Applied {
                call: child_call,
                root: child_root,
                incoming: child_applied,
            },
            Event::PendingBorrowRemoved {
                loan: child_removed,
                call: child_removed_call,
            },
            Event::Bound(child_old, child_old_call),
            Event::Dropped(child_drop),
            Event::WhileTraversal {
                control: cback,
                role: WhileRole::BodyExit(Some(true)),
            },
            Event::WhileTraversal {
                control: cexit,
                role: WhileRole::Exit(Some(true)),
            },
            Event::WhileTraversal {
                control: pback,
                role: WhileRole::BodyExit(Some(true)),
            },
            tail @ ..,
        ] = events
        else {
            return None;
        };
        // Match the original slice directly; no filtered or reconstructed trajectory.
        let (
            root_drop,
            pexit,
            suffix,
            shared,
            removed_shared,
            suffix_end,
            kept_drop,
            final_root_use,
        ) = match tail {
            [
                Event::Dropped(root_drop),
                Event::WhileTraversal {
                    control: pexit,
                    role: WhileRole::Exit(Some(true)),
                },
                Event::UncoveredCall(suffix),
                Event::BorrowRegistered(shared),
                Event::PendingBorrowRemoved {
                    loan: removed_shared,
                    call: suffix_end,
                },
                Event::Dropped(kept_drop),
            ] => (
                root_drop,
                pexit,
                suffix,
                shared,
                removed_shared,
                suffix_end,
                kept_drop,
                None,
            ),
            [
                Event::WhileTraversal {
                    control: pexit,
                    role: WhileRole::Exit(Some(true)),
                },
                Event::UncoveredCall(root_call),
                Event::BorrowRegistered(root_loan),
                Event::PendingBorrowRemoved {
                    loan: root_removed,
                    call: root_end,
                },
                Event::Dropped(root_drop),
                Event::UncoveredCall(suffix),
                Event::BorrowRegistered(shared),
                Event::PendingBorrowRemoved {
                    loan: removed_shared,
                    call: suffix_end,
                },
                Event::Dropped(kept_drop),
            ] => {
                if root_loan != root_removed || root_call != root_end || root_call == suffix {
                    return None;
                }
                (
                    root_drop,
                    pexit,
                    suffix,
                    shared,
                    removed_shared,
                    suffix_end,
                    kept_drop,
                    Some(Retained {
                        root: *root,
                        initial: *initial,
                        call: *root_call,
                        loan: root_loan.clone(),
                        cleanup: *root_drop,
                    }),
                )
            }
            _ => return None,
        };
        let c = self.checker;
        let source = c.source_unit;
        if parent == child
            || [pcond, pbody, pback, pexit].iter().any(|id| *id != parent)
            || [ccond, cbody, cback, cexit].iter().any(|id| *id != child)
            || [parent, child].iter().any(|id| id.source_unit() != source)
            || initial != bound_initial
            || kept != bound_kept
            || parent_root != root
            || child_root != root
            || parent_incoming != parent_applied
            || child_incoming != child_applied
            || parent_call != parent_old_call
            || child_call != child_old_call
            || parent_call != parent_removed_call
            || child_call != child_removed_call
            || parent_loan != parent_removed
            || child_loan != child_removed
            || shared != removed_shared
            || suffix != suffix_end
            || shared.call() != *suffix
            || shared.kind() != LoanKind::Shared
            || !matches!(shared.target(),UnitLoanTarget::Place(p) if p.is_root() && p.root() == *kept_root)
            || root_drop.target() != UnitDropTarget::Named(*root)
            || root_drop.point()
                != final_root_use
                    .as_ref()
                    .map_or(UnitDropPoint::LoopExit(*parent), |r| {
                        UnitDropPoint::CallReturn(r.call)
                    })
            || kept_drop.target() != UnitDropTarget::Named(*kept_root)
            || kept_drop.point() != UnitDropPoint::CallReturn(*suffix)
        {
            return None;
        }
        let symbols = [*root, *kept_root, *parent_old, *child_old];
        if symbols
            .iter()
            .enumerate()
            .any(|(i, id)| id.source_unit() != source || symbols[..i].contains(id))
        {
            return None;
        }
        let expressions = [
            *initial,
            *kept,
            *parent_incoming,
            *child_incoming,
            *parent_call,
            *child_call,
            *suffix,
        ];
        if expressions
            .iter()
            .enumerate()
            .any(|(i, id)| id.source_unit() != source || expressions[..i].contains(id))
        {
            return None;
        }
        let ast = c.parsed.ast();
        // Each detached cleanup must refer to its own real declaration, not a sibling's span.
        for (old, call, loan, drop) in [
            (parent_old, parent_call, parent_loan, parent_drop),
            (child_old, child_call, child_loan, child_drop),
        ] {
            let UnitDropPoint::AfterStatement(statement) = drop.point() else {
                return None;
            };
            if statement.source_unit() != source
                || drop.target() != UnitDropTarget::Named(*old)
                || loan.call() != *call
                || loan.kind() != LoanKind::Exclusive
                || !matches!(loan.target(),UnitLoanTarget::Place(p) if p.is_root() && p.root()==*root)
            {
                return None;
            }
            let crate::parser::Statement::LocalVariable { declaration } =
                ast.statements().get(statement.statement()).ok()?.payload()
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
            if c.unit_expression(*initializer) != *call
                || c.symbols_by_span.get(&span_key(*span)) != Some(old)
                || drop.value_origin() != *span
            {
                return None;
            }
            let crate::parser::Expression::Call { arguments, .. } =
                ast.expressions().get(call.expression()).ok()?.payload()
            else {
                return None;
            };
            let [first, _] = arguments.as_slice() else {
                return None;
            };
            let Some(crate::parser::ParameterModeMarker::Inout(marker)) = first.mode_marker else {
                return None;
            };
            // Exclusive call loans begin at the actual `&` marker, not the Name node.
            // This follows the existing call contract; never synthesize an adjusted span.
            if loan.argument() != c.unit_expression(first.value)
                || loan.begin_span() != marker
                || loan.end_span() != ast.expressions().get(call.expression()).ok()?.span()
            {
                return None;
            }
        }
        // The current root's final cleanup is also tied to its actual declaration.
        let mut root_spans = c
            .symbols_by_span
            .iter()
            .filter(|(_, symbol)| **symbol == *root);
        let (root_span, _) = root_spans.next()?;
        if root_spans.next().is_some() || span_key(root_drop.value_origin()) != *root_span {
            return None;
        }
        let expected_drops = [*parent_drop, *child_drop, *root_drop, *kept_drop];
        let observed = self
            .facts
            .iter()
            .map(|f| f.into_unit(source))
            .collect::<Vec<_>>();
        if observed != expected_drops {
            return None;
        }
        let retained = Retained {
            root: *kept_root,
            initial: *kept,
            call: *suffix,
            loan: shared.clone(),
            cleanup: *kept_drop,
        };
        let coverage = self.dual_generation_coverage(
            (*parent, *child),
            *root,
            [*parent_old, *child_old],
            [
                *initial,
                *parent_incoming,
                *parent_call,
                *child_incoming,
                *child_call,
            ],
            (&retained, final_root_use.as_ref()),
            wrapper,
        )?;
        Some(DualNestedPending {
            typed: c.typed.analysis_owner().clone(),
            callable: coverage.callable,
            controls: [*parent, *child],
            root: *root,
            counters: [coverage.parent_counter, coverage.child_counter],
            counter_parameters: coverage.counter_parameters,
            counter_initializers: coverage.counter_initializers,
            conditions: [coverage.parent_condition, coverage.child_condition],
            steps: [coverage.parent_step, coverage.child_step],
            sites: [*initial, *parent_incoming, *child_incoming],
            calls: [*parent_call, *child_call],
            cleanup: [*parent_drop, *child_drop, *root_drop],
            loans: [parent_loan.clone(), child_loan.clone()],
            final_root_use: final_root_use.map(|r| RetainedLoopRoot {
                root: r.root,
                initial: r.initial,
                call: r.call,
                loan: r.loan,
                cleanup: r.cleanup,
            }),
            retained: RetainedLoopRoot {
                root: retained.root,
                initial: retained.initial,
                call: retained.call,
                loan: retained.loan,
                cleanup: retained.cleanup,
            },
        })
    }
}
