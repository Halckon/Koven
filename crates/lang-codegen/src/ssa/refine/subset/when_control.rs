//! Admission for subjectless when and its continuing branches.
use std::collections::BTreeSet;

use lang_frontend::{
    ast::ExpressionId,
    name_resolution::UnitSymbolId,
    parser::{WhenCondition, WhenEntry},
    source::Span,
    type_checking::{ExpressionCategory, ParameterMode, UnitTypeId},
};

use super::Checker;

impl Checker<'_, '_> {
    /// The subject owner lives through the selected arm. The planner publishes
    /// its cleanup after the when expression or on the actual control transfer.
    pub(super) fn temporary_enum_when_subject(&mut self, subject: ExpressionId) -> bool {
        self.inputs.typed.expression_category(self.id(subject))
            == Some(ExpressionCategory::Temporary)
            && self.expression(subject)
    }

    fn intersect_when_views(
        intersection: &mut Option<BTreeSet<UnitSymbolId>>,
        views: &BTreeSet<UnitSymbolId>,
    ) {
        if let Some(intersection) = intersection {
            intersection.retain(|symbol| views.contains(symbol));
        } else {
            *intersection = Some(views.clone());
        }
    }

    pub(in crate::ssa::refine::subset) fn subjectless_when(
        &mut self,
        ty: UnitTypeId,
        entries: &[WhenEntry],
        span: Span,
    ) -> bool {
        self.migrated = true;
        if !self.unit(ty) && !entries.iter().any(|entry| entry.else_span.is_some()) {
            return false;
        }
        let outer_views = self.view_symbols.clone();
        let mut continuing = None;
        let mut can_reach_entry = true;
        for entry in entries {
            if !can_reach_entry {
                break;
            }
            let is_else = entry.else_span.is_some();
            if is_else != entry.conditions.is_empty() {
                return false;
            }
            let mut matched = is_else.then(|| self.view_symbols.clone());
            for condition in &entry.conditions {
                if !can_reach_entry {
                    break;
                }
                let WhenCondition::Expression(condition) = condition else {
                    return false;
                };
                let (yes, no) = self.condition_outcomes(*condition);
                can_reach_entry = no;
                if !self.condition(*condition, false) {
                    return false;
                }
                if yes {
                    Self::intersect_when_views(&mut matched, &self.view_symbols);
                }
            }
            if let Some(matched) = matched {
                let unmatched_views = std::mem::replace(&mut self.view_symbols, matched);
                if !self.statement(entry.body) {
                    return false;
                }
                if self.statement_may_continue(entry.body) {
                    Self::intersect_when_views(&mut continuing, &self.view_symbols);
                }
                self.view_symbols = unmatched_views;
            }
            can_reach_entry &= !is_else;
        }
        if can_reach_entry {
            Self::intersect_when_views(&mut continuing, &self.view_symbols);
        }
        self.view_symbols = continuing
            .unwrap_or(outer_views.clone())
            .into_iter()
            .filter(|symbol| {
                outer_views.contains(symbol)
                    || (matches!(
                        self.parameter_mode(*symbol),
                        Some(ParameterMode::Borrow | ParameterMode::Inout)
                    ) && self.inputs.borrowed_narrowing_used_after(*symbol, span))
            })
            .collect();
        true
    }
}
