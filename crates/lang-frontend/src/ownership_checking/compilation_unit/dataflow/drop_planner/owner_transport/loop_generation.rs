//! Private symbolic environment candidate from one actual planner traversal.
//! Not source-completeness, CFG, final-owned or lowering authority.
use super::origin_probe::{Event, WhileRole};
use super::*;
use std::sync::Arc;

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

pub(super) struct Candidate {
    typed: Arc<()>,
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
        ] = events.as_slice()
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
        if observed_drops != [*old_drop, *exit_drop] {
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
        let header = Generation::Header {
            control: *control,
            root: *root,
        };
        let incoming = Generation::IterationSite(*incoming);
        let detached = Generation::Detached { call: *call };
        Some(Candidate {
            typed: self.checker.typed.analysis_owner().clone(),
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
