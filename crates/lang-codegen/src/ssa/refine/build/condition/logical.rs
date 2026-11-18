//! Proof transport between short-circuit operands and their result edges.
use std::collections::BTreeMap;

use lang_frontend::{
    ast::ExpressionId,
    ownership_checking::{UnitDropPoint, UnitShortCircuitRhs},
    source::Span,
    type_checking::UnitExpressionId,
};

use super::super::{Builder, View, when::PredecessorBranch};
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::{EntityType, LoanKind, SsaTypeId, SsaTypeKind},
    refine::body::{BlockRef, ROp, RStmt, RTerm, Slot},
};

#[derive(Clone, Copy)]
pub(super) enum BorrowedProofEdges {
    Neither,
    True,
    False,
    Both,
}

impl BorrowedProofEdges {
    fn edge(value: bool) -> Self {
        if value { Self::True } else { Self::False }
    }

    pub(super) fn allows(self, edge: bool) -> bool {
        matches!(
            (self, edge),
            (Self::Both, _) | (Self::True, true) | (Self::False, false)
        )
    }

    pub(super) fn negated(self) -> Self {
        match self {
            Self::True => Self::False,
            Self::False => Self::True,
            other => other,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct ConditionViews {
    pub(super) ordinary: bool,
    pub(super) borrowed: BorrowedProofEdges,
}

impl ConditionViews {
    pub(super) fn normal(views: bool) -> Self {
        Self {
            ordinary: views,
            borrowed: if views {
                BorrowedProofEdges::True
            } else {
                BorrowedProofEdges::Neither
            },
        }
    }
}

impl Builder<'_, '_> {
    /// Short-circuit: Basic numbers the evaluated exit 0 and the skip 1. Const
    /// carries its own true/false edge numbering. A joined outcome keeps only
    /// borrowed nullable proofs established on every reachable path.
    pub(super) fn logical(
        &mut self,
        expression: ExpressionId,
        and: bool,
        (left, right): (ExpressionId, ExpressionId),
        when_true: BlockRef,
        when_false: BlockRef,
        views: ConditionViews,
    ) -> Result<(), LoweringError> {
        let operand_views = ConditionViews {
            ordinary: views.ordinary && and,
            borrowed: BorrowedProofEdges::edge(and),
        };
        let carry_views = operand_views.ordinary || views.borrowed.allows(and);
        let evaluated_target = if and { when_true } else { when_false };
        let carrying_target = carry_views.then_some(evaluated_target);
        let join_requested = views.borrowed.allows(!and);
        let left_views = ConditionViews {
            borrowed: if join_requested {
                BorrowedProofEdges::Both
            } else {
                operand_views.borrowed
            },
            ..operand_views
        };
        let span = self.span(expression);
        let control = self.unit(expression);
        // A constant left operand fixes the plan: `Never` yields the left value and skips the
        // right operand; `Always` evaluates the left operand and yields the right value.
        let rhs_branch = if let Some(plan) = self
            .inputs
            .constant_owned
            .and_then(|owned| owned.short_circuit_at(control))
        {
            match plan.rhs() {
                UnitShortCircuitRhs::Never => {
                    self.unreachable_drop_scopes.push(self.span(right));
                    if self.required(left)?.is_some() {
                        let target = if and { when_false } else { when_true };
                        self.goto(target, span)?;
                    }
                    return Ok(());
                }
                UnitShortCircuitRhs::Always => {
                    if self.required(left)?.is_none() {
                        return Ok(());
                    }
                    let views = ConditionViews {
                        ordinary: operand_views.ordinary,
                        ..views
                    };
                    return self
                        .condition_internal(right, when_true, when_false, views, None, true);
                }
                UnitShortCircuitRhs::Conditional => plan.rhs_branch(),
            }
        } else {
            0
        };
        if self.inputs.typed.expression_falls_through(control) == Some(false) {
            // If the left side completes, the frontend proved the RHS is required
            // and has no normal exit. Do not synthesize a skipped predecessor.
            self.unreachable_drop_points.extend([
                UnitDropPoint::BranchExit { control, branch: 0 },
                UnitDropPoint::BranchExit { control, branch: 1 },
            ]);
            if self.required(left)?.is_none() {
                self.unreachable_drop_scopes.push(self.span(right));
            } else if self.required(right)?.is_some() {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            return Ok(());
        }
        let middle = self.body.block();
        let skip = self.body.block();
        let ordinary = self.inputs.constant_owned.is_none();
        if ordinary && self.inputs.typed.ordinary_rhs_bypassed(control) {
            if self.required(left)?.is_some() {
                self.goto(skip, span)?;
            }
        } else if ordinary && self.inputs.typed.ordinary_rhs_required(control) {
            if self.required(left)?.is_some() {
                self.goto(middle, span)?;
            }
        } else if and {
            self.condition_internal(left, middle, skip, left_views, None, true)?;
        } else {
            self.condition_internal(left, skip, middle, left_views, None, true)?;
        }
        // The plain branch that selects between the right operand and the skip edge is the
        // short circuit's own control transfer.
        for block in &mut self.body.blocks {
            if let Some((
                RTerm::Branch {
                    when_true,
                    when_false,
                    ..
                },
                origin,
            )) = &mut block.terminator
                && ((and && *when_true == middle && *when_false == skip)
                    || (!and && *when_true == skip && *when_false == middle))
            {
                *origin = span;
            }
        }
        let skip_target = if and { when_false } else { when_true };
        let skip_point = UnitDropPoint::BranchExit {
            control,
            branch: 1 - rhs_branch,
        };
        let rhs_point = UnitDropPoint::BranchExit {
            control,
            branch: rhs_branch,
        };
        let skipped = self.collect_drops(skip_point);
        let outer_temporaries = self.temporaries.clone();
        let outer_loans = self.pending_call_loans.clone();
        let outer_ranges = self.short_ranges.clone();
        let outer_views = self.active_views.clone();
        let mut skip_state = None;
        let mut skip_bridge = None;
        if self.body.blocks[skip.0].has_predecessor {
            let mark = self.enter(skip);
            self.sync_dead();
            let pre_skip_dead = self.dead.clone();
            self.emit_drops(&skipped)?;
            self.finish_drops(&skipped);
            if join_requested
                && self.active_views[mark.min(self.active_views.len())..]
                    .iter()
                    .any(|view| self.shared_nullable_proof(view))
            {
                skip_bridge = Some(self.body.block());
            }
            let target = skip_bridge.unwrap_or(skip_target);
            if skip_bridge.is_some() {
                self.route_borrowed_proofs(mark, target, span)?;
            } else {
                self.route_views(mark, target, None, span)?;
            }
            self.goto(target, span)?;
            skip_state = Some((
                pre_skip_dead,
                self.temporaries.clone(),
                self.active_views.clone(),
            ));
        } else {
            self.unreachable_drop_points.push(skip_point);
        }

        if !self.body.blocks[middle.0].has_predecessor {
            self.unreachable_drop_scopes.push(self.span(right));
            self.unreachable_drop_points.push(rhs_point);
            if let Some(bridge) = skip_bridge {
                self.join_short_circuit_proofs(bridge, None, skip_target, span)?;
            }
            return Ok(());
        }
        if let Some((pre_skip_dead, _, _)) = &skip_state {
            self.dead.clone_from(pre_skip_dead);
            self.dead_block = Some(middle);
            self.entry_dead
                .entry(middle)
                .or_default()
                .extend(self.dead.iter().copied());
        }
        self.temporaries = outer_temporaries;
        self.active_views = outer_views;
        let mark = self.enter(middle);
        let inherited: Vec<_> = self.active_views[..mark]
            .iter()
            .map(|view| view.slot)
            .collect();
        let evaluated_true = self.body.block();
        let evaluated_false = self.body.block();
        let carry_bypass = join_requested && (skip_bridge.is_some() || skip_state.is_none());
        let right_views = ConditionViews {
            ordinary: operand_views.ordinary,
            borrowed: match (carry_views, carry_bypass) {
                (true, true) => BorrowedProofEdges::Both,
                (true, false) => BorrowedProofEdges::edge(and),
                (false, true) => BorrowedProofEdges::edge(!and),
                (false, false) => BorrowedProofEdges::Neither,
            },
        };
        self.condition_internal(
            right,
            evaluated_true,
            evaluated_false,
            right_views,
            None,
            true,
        )?;
        let mark = self
            .active_views
            .iter()
            .take_while(|view| inherited.contains(&view.slot))
            .count();
        let left_views: Vec<_> = self.active_views.drain(mark..).collect();
        let evaluated = self.collect_drops(rhs_point);
        let mut evaluated_edges = Vec::with_capacity(2);
        let mut evaluated_bridge = None;
        for (path, target) in [(evaluated_true, when_true), (evaluated_false, when_false)] {
            if !self.body.blocks[path.0].has_predecessor {
                continue;
            }
            evaluated_edges.push(path);
            let mark = self.enter(path);
            self.active_views.extend(left_views.iter().copied());
            self.emit_drops(&evaluated)?;
            if carry_bypass && target == skip_target {
                if skip_bridge.is_some() {
                    evaluated_bridge = Some(self.body.block());
                }
                let target = evaluated_bridge.unwrap_or(target);
                self.route_borrowed_proofs(mark, target, span)?;
                self.goto(target, span)?;
            } else {
                self.route_views(mark, target, carrying_target, span)?;
                self.goto(target, span)?;
            }
        }
        if !evaluated_edges.is_empty() {
            self.finish_drops(&evaluated);
            if let Some((_, skipped, skipped_views)) = &skip_state {
                self.join_conditional_rc_temporaries(skip, skipped, &evaluated_edges, span)?;
                let skips_only =
                    !evaluated_edges.contains(&if and { evaluated_false } else { evaluated_true });
                for view in skipped_views.iter().rev() {
                    if self.inout_loan(view.symbol).is_none()
                        || self.active_views.iter().any(|rhs| rhs.slot == view.slot)
                    {
                        continue;
                    }
                    if skips_only {
                        self.pending_views.push(View {
                            target: skip_bridge.unwrap_or(skip_target),
                            ..*view
                        });
                    } else {
                        // The RHS invalidated this proof. Its skipped predecessor still owns it.
                        self.body.blocks[skip.0].statements.push(RStmt {
                            loop_tag: None,
                            fresh_loop_tag: None,
                            loan_tag: None,
                            op: ROp::BorrowEnd { loan: view.slot },
                            results: Vec::new(),
                            span,
                        });
                    }
                }
            }
        } else {
            self.unreachable_drop_points.push(rhs_point);
            if let Some((_, temporaries, views)) = skip_state {
                // An exit on the evaluated edge does not consume the skipped edge's state.
                self.temporaries = temporaries;
                self.pending_call_loans = outer_loans;
                self.short_ranges = outer_ranges;
                self.active_views = views;
            }
        }
        if let Some(bridge) = skip_bridge {
            self.join_short_circuit_proofs(bridge, evaluated_bridge, skip_target, span)?;
        }
        Ok(())
    }

    /// Expose the same cleanup slot on every normal result edge of the short circuit.
    fn join_conditional_rc_temporaries(
        &mut self,
        skip: BlockRef,
        skipped: &BTreeMap<UnitExpressionId, Slot>,
        evaluated_edges: &[BlockRef],
        span: Span,
    ) -> Result<(), LoweringError> {
        let additions = self
            .temporaries
            .iter()
            .filter_map(|(&id, &slot)| {
                (self.conditional_temporary_ids.contains(&id) && !skipped.contains_key(&id))
                    .then_some((id, slot))
            })
            .collect::<Vec<_>>();
        for (id, slot) in additions {
            let EntityType::Value(ty) = self.body.slots[slot.0] else {
                return Err(error(LoweringErrorKind::InvalidModel, span));
            };
            let (nullable, present) = match self.types.ssa_types.get(ty.index()) {
                Some(SsaTypeKind::NullableHandle { .. }) => (ty, ROp::Alias { source: slot }),
                Some(SsaTypeKind::SharedOwner { .. }) => {
                    let index = self
                        .types
                        .ssa_types
                        .iter()
                        .position(|kind| kind == &SsaTypeKind::NullableHandle { inner: ty })
                        .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                    let nullable = SsaTypeId {
                        module: ty.module(),
                        index,
                    };
                    (
                        nullable,
                        ROp::NullableWrap {
                            nullable,
                            owner: slot,
                        },
                    )
                }
                _ => return Err(error(LoweringErrorKind::InvalidModel, span)),
            };
            let joined = self.body.slot(EntityType::Value(nullable));
            // The edge terminators and their receipt links are already fixed. These
            // assignments form the new SSA column immediately before each terminator.
            for (block, op) in std::iter::once((skip, ROp::NullableNull { nullable })).chain(
                evaluated_edges
                    .iter()
                    .map(|&block| (block, present.clone())),
            ) {
                self.body.blocks[block.0].statements.push(RStmt {
                    loop_tag: None,
                    fresh_loop_tag: None,
                    loan_tag: None,
                    op,
                    results: vec![joined],
                    span,
                });
            }
            self.temporaries.insert(id, joined);
        }
        Ok(())
    }

    pub(super) fn shared_nullable_proof(&self, view: &View) -> bool {
        view.when_scope.is_none()
            && self.borrowed.contains_key(&view.symbol)
            && matches!(
                self.body.slots[view.slot.0],
                EntityType::Loan { kind: LoanKind::Shared, target }
                    if matches!(
                        self.types.ssa_types.get(target.index()),
                        Some(SsaTypeKind::StringOwner | SsaTypeKind::SharedOwner { .. })
                    )
            )
    }

    fn route_borrowed_proofs(
        &mut self,
        mark: usize,
        target: BlockRef,
        span: Span,
    ) -> Result<(), LoweringError> {
        let mark = mark.min(self.active_views.len());
        let views: Vec<_> = self.active_views.drain(mark..).collect();
        for view in views.into_iter().rev() {
            if self.shared_nullable_proof(&view) {
                self.pending_views.push(View { target, ..view });
            } else {
                self.push(ROp::BorrowEnd { loan: view.slot }, Vec::new(), span)?;
            }
        }
        Ok(())
    }

    fn join_short_circuit_proofs(
        &mut self,
        skipped: BlockRef,
        evaluated: Option<BlockRef>,
        target: BlockRef,
        span: Span,
    ) -> Result<(), LoweringError> {
        let outer = std::mem::take(&mut self.active_views);
        let mut predecessors = Vec::new();
        for block in std::iter::once(skipped).chain(evaluated) {
            self.enter(block);
            self.sync_dead();
            predecessors.push(PredecessorBranch {
                block,
                views: std::mem::take(&mut self.active_views),
                dead: self.dead.clone(),
                this_dead: self.this_dead,
            });
        }
        self.coalesce_borrowed_proofs(&mut predecessors, span)?;
        self.join_views(predecessors, target, span)?;
        let joined = std::mem::replace(&mut self.active_views, outer);
        self.pending_views
            .extend(joined.into_iter().map(|view| View { target, ..view }));
        self.block = None;
        Ok(())
    }

    /// Give proofs of the same borrowed source a common slot before the regular
    /// join retires predecessor-local proofs and passes live slots as block arguments.
    pub(in crate::ssa::refine::build) fn coalesce_borrowed_proofs(
        &mut self,
        predecessors: &mut [PredecessorBranch],
        span: Span,
    ) -> Result<(), LoweringError> {
        let [first, _, ..] = predecessors else {
            return Ok(());
        };
        let mut bindings = BTreeMap::new();
        for view in &first.views {
            if self.shared_nullable_proof(view) {
                bindings.insert(view.symbol, self.body.slots[view.slot.0]);
            }
        }
        for (symbol, ty) in bindings {
            // An inherited proof keeps its slot across nested controls and loop exits.
            if predecessors[0].views.iter().any(|view| {
                view.symbol == symbol
                    && self.shared_nullable_proof(view)
                    && predecessors[1..]
                        .iter()
                        .all(|pred| pred.views.iter().any(|other| other.slot == view.slot))
            }) {
                continue;
            }
            let found = predecessors
                .iter()
                .map(|pred| {
                    pred.views.iter().rposition(|view| {
                        view.symbol == symbol
                            && self.body.slots[view.slot.0] == ty
                            && self.shared_nullable_proof(view)
                    })
                })
                .collect::<Option<Vec<_>>>();
            let Some(found) = found else { continue };
            let joined = self.body.slot(ty);
            for (pred, position) in predecessors.iter_mut().zip(found) {
                self.block = Some(pred.block);
                self.dead.clone_from(&pred.dead);
                self.this_dead = pred.this_dead;
                self.dead_block = self.block;
                let view = &mut pred.views[position];
                self.push(ROp::Alias { source: view.slot }, vec![joined], span)?;
                view.slot = joined;
            }
        }
        Ok(())
    }

    /// Continue views above `mark` on the selected outgoing edge, or end them.
    fn route_views(
        &mut self,
        mark: usize,
        target: BlockRef,
        carrying_target: Option<BlockRef>,
        span: lang_frontend::source::Span,
    ) -> Result<(), LoweringError> {
        let mark = mark.min(self.active_views.len());
        let views: Vec<_> = self.active_views.drain(mark..).collect();
        if Some(target) == carrying_target {
            self.pending_views
                .extend(views.into_iter().map(|view| View { target, ..view }));
            return Ok(());
        }
        for view in views {
            self.push(ROp::BorrowEnd { loan: view.slot }, Vec::new(), span)?;
        }
        Ok(())
    }
}
