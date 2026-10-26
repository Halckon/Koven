//! Subjectless `when`: Boolean alternatives tested in source order; the first match
//! enters its body.
use lang_frontend::{
    ast::ExpressionId,
    ownership_checking::UnitDropPoint,
    parser::{WhenCondition, WhenEntry},
    source::Span,
};

use super::{Builder, Lowered};
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::EntityType,
    refine::body::ROp,
};

impl Builder<'_, '_> {
    /// Each alternative consumes its `WhenAlternativeMatch` drops on the matched edge;
    /// each body consumes its `BranchExit`, and a statement `when` without `else` consumes
    /// `BranchExit(entries.len())` on the fall-through edge.
    pub(super) fn when_expression(
        &mut self,
        expression: ExpressionId,
        entries: &[WhenEntry],
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let control = self.unit(expression);
        let ty = self.expression_type(expression)?;
        let result = if self.is_unit(ty) {
            None
        } else {
            Some(self.body.slot(EntityType::Value(self.ssa_type(ty, span)?)))
        };
        let join = self.body.block();
        let mut reached = false;
        let mut falls_through = true;
        for (index, entry) in entries.iter().enumerate() {
            let body = self.body.block();
            if entry.else_span.is_some() {
                falls_through = false;
                self.goto(body, entry.span)?;
            }
            for (alternative, condition) in entry.conditions.iter().enumerate() {
                let WhenCondition::Expression(condition) = condition else {
                    return Err(error(LoweringErrorKind::UnsupportedNode, entry.span));
                };
                let matched = self.body.block();
                let next = self.body.block();
                self.condition(*condition, matched, next)?;
                let mark = self.enter(matched);
                self.leave_unclaimed(mark, entry.span)?;
                self.drops(UnitDropPoint::WhenAlternativeMatch {
                    control,
                    entry: index,
                    alternative,
                })?;
                self.goto(body, entry.span)?;
                let mark = self.enter(next);
                self.leave_unclaimed(mark, entry.span)?;
            }
            let unmatched = self.block;
            let mark = self.enter(body);
            let value = self.branch_value(entry.body)?;
            self.leave(mark, span)?;
            if let Some(value) = value {
                if let Some(result) = result {
                    let (slot, source) =
                        value.ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                    let slot = self.adapt(slot, source, ty)?;
                    self.deliver(source);
                    self.push(ROp::Alias { source: slot }, vec![result], span)?;
                }
                self.drops(UnitDropPoint::BranchExit {
                    control,
                    branch: index,
                })?;
                self.goto(join, span)?;
                reached = true;
            }
            self.block = unmatched;
            if entry.else_span.is_some() {
                break;
            }
        }
        if falls_through {
            if result.is_some() {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            self.drops(UnitDropPoint::BranchExit {
                control,
                branch: entries.len(),
            })?;
            self.goto(join, span)?;
            reached = true;
        }
        if !self.pending_views.is_empty() {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        if !reached {
            self.block = None;
            return Ok(Lowered::Diverged);
        }
        self.block = Some(join);
        Ok(result.map_or(Lowered::Unit, Lowered::Value))
    }
}
