//! Private symbolic environment candidate from one actual planner traversal.
//! Closed source envelope; still not CFG, final-owned or lowering authority.
use super::origin_probe::{Event, WhileRole};
use super::*;
use std::sync::Arc;
mod coverage;
#[cfg(feature = "d10-catalog-experiment")]
mod dual;
#[cfg(feature = "d10-catalog-experiment")]
mod dual_coverage;
#[cfg(feature = "d10-catalog-experiment")]
mod nested;
#[cfg(feature = "d10-catalog-experiment")]
mod nested_coverage;

/// A site denotes construction in the current iteration, not a runtime object ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Generation {
    EntrySite(UnitExpressionId),
    Header {
        control: UnitStatementId,
        root: UnitSymbolId,
    },
    IterationSite(UnitExpressionId),
    Detached {
        call: UnitExpressionId,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Retained {
    pub(super) root: UnitSymbolId,
    pub(super) initial: UnitExpressionId,
    pub(super) call: UnitExpressionId,
    pub(super) loan: crate::ownership_checking::UnitLoanFact,
    pub(super) cleanup: crate::ownership_checking::UnitDropFact,
}
pub(super) struct Candidate {
    pub(super) retained: Option<Retained>,
    typed: Arc<()>,
    pub(super) coverage: coverage::Coverage,
    pub(super) control: UnitStatementId,
    pub(super) root: UnitSymbolId,
    pub(super) entry: Generation,
    pub(super) header: Generation,
    pub(super) backedge: Generation,
    pub(super) exit: Generation,
    pub(super) replacement: (UnitExpressionId, Generation, Generation, Generation),
    pub(super) cleanup: [(Generation, crate::ownership_checking::UnitDropFact); 2],
}
impl Candidate {
    #[cfg(test)]
    pub(super) fn matches(&self, typed: &crate::type_checking::CompilationUnitTypes) -> bool {
        Arc::ptr_eq(&self.typed, typed.analysis_owner())
    }
}

impl DropPlanner<'_, '_> {
    /// Never accepts a caller-supplied event vector; reads this planner's own complete trace.
    /// The result remains pending: no consumer can use it as a primitive capability.
    pub(super) fn loop_generation_candidate(&self) -> Option<Candidate> {
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
        let (loop_events, retained) = if rest.len() == 12 {
            (rest, None)
        } else {
            let [
                Event::Literal(kept),
                Event::Bound(kept_root, bound_kept),
                tail @ ..,
            ] = rest
            else {
                return None;
            };
            let loop_events = tail.get(..12)?;
            let [
                Event::UncoveredCall(suffix),
                Event::BorrowRegistered(shared),
                Event::PendingBorrowRemoved {
                    loan: removed_shared,
                    call: end_call,
                },
                Event::Dropped(kept_drop),
            ] = tail.get(12..)?
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
                Some(Retained {
                    root: *kept_root,
                    initial: *kept,
                    call: *suffix,
                    loan: shared.clone(),
                    cleanup: *kept_drop,
                }),
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
        if let Some(kept) = &retained {
            expected_drops.push(kept.cleanup);
        }
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
        let coverage = self.loop_generation_coverage(
            *control,
            *root,
            *old_root,
            (*initial, *incoming, *call),
            retained.as_ref(),
        )?;
        let header = Generation::Header {
            control: *control,
            root: *root,
        };
        let incoming = Generation::IterationSite(*incoming);
        let detached = Generation::Detached { call: *call };
        Some(Candidate {
            retained,
            typed: self.checker.typed.analysis_owner().clone(),
            coverage,
            control: *control,
            root: *root,
            entry: Generation::EntrySite(*initial),
            header,
            backedge: incoming,
            exit: header,
            replacement: (*call, header, incoming, detached),
            cleanup: [(detached, *old_drop), (header, *exit_drop)],
        })
    }
}

#[cfg(feature = "d10-catalog-experiment")]
impl DropPlanner<'_, '_> {
    pub(in super::super) fn enable_loop_generation_probe(&mut self) {
        let ast = self.checker.parsed.ast();
        if matches!(
            (
                ast.items().iter().count(),
                ast.statements().iter().count(),
                ast.expressions().iter().count()
            ),
            (4, 7, 12) | (5, 9, 16) | (6, 13, 23) | (7, 14, 27) | (7, 15, 30)
        ) {
            self.origin_probe = Some(Vec::with_capacity(34));
        }
    }
    pub(in super::super) fn loop_pending(
        &self,
    ) -> Option<crate::ownership_checking::compilation_unit::d10_catalog_experiment::LoopPending>
    {
        use crate::ownership_checking::compilation_unit::d10_catalog_experiment::LoopPending;
        let c = self.loop_generation_candidate()?;
        let (Generation::EntrySite(initial), Generation::IterationSite(incoming)) =
            (c.entry, c.backedge)
        else {
            return None;
        };
        if c.exit != c.header
            || c.replacement.1 != c.header
            || c.replacement.2 != c.backedge
            || c.cleanup[0].0 != c.replacement.3
            || c.cleanup[1].0 != c.header
        {
            return None;
        }
        let loan = self.origin_probe.as_ref()?.iter().find_map(|e| match e {
            Event::BorrowRegistered(loan) => Some(loan.clone()),
            _ => None,
        })?;
        Some(LoopPending {
            retained: c.retained.map(|r| crate::ownership_checking::compilation_unit::d10_catalog_experiment::RetainedLoopRoot { root: r.root, initial: r.initial, call: r.call, loan: r.loan, cleanup: r.cleanup }),
            typed: c.typed,
            callable: c.coverage.callable,
            control: c.control,
            root: c.root,
            counter: c.coverage.counter,
            parameter: c.coverage.parameter,
            condition: c.coverage.condition,
            step: c.coverage.step,
            initial,
            incoming,
            call: c.replacement.0,
            cleanup: [c.cleanup[0].1, c.cleanup[1].1],
            loan,
        })
    }
}

#[cfg(feature = "d10-catalog-experiment")]
mod after_child;
#[cfg(feature = "d10-catalog-experiment")]
mod after_child_coverage;
