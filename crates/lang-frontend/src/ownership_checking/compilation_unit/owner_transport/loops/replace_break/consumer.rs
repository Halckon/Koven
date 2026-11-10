//! The single admitted suffix consumes one edge-selected value, not two values.
use super::*;

/// Actual transitions of the single bounded suffix value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UnitOwnerReplaceBreakConsumerActionV1 {
    /// The exact Name expression takes the edge-selected root once.
    Take(UnitExpressionId, UnitSymbolId),
    /// Its actual declaration binds that same selection once.
    Bind(UnitExpressionId, UnitSymbolId),
    /// The actual ASAP cleanup retires the selected value once.
    Drop(UnitDropPoint, UnitSymbolId),
}

/// A complete two-incoming selection consumed by the only admitted suffix.
/// The original producer identities remain in the enclosing boundary's catalogs.
#[derive(Clone, Debug)]
pub struct UnitOwnerReplaceBreakConsumerV1 {
    pub(in crate::ownership_checking::compilation_unit) incoming: [UnitOwnerReplaceBreakStateV1; 2],
    pub(in crate::ownership_checking::compilation_unit) initializer: UnitExpressionId,
    pub(in crate::ownership_checking::compilation_unit) statement: UnitStatementId,
    pub(in crate::ownership_checking::compilation_unit) retired: UnitSymbolId,
    pub(in crate::ownership_checking::compilation_unit) actions:
        Vec<UnitOwnerReplaceBreakConsumerActionV1>,
}
impl UnitOwnerReplaceBreakConsumerV1 {
    /// Condition-false P followed by Break Q; these are alternatives, not two drops.
    pub fn incoming(&self) -> &[UnitOwnerReplaceBreakStateV1; 2] {
        &self.incoming
    }
    /// Actual take, bind and drop of one selected value, in planner order.
    pub fn actions(&self) -> &[UnitOwnerReplaceBreakConsumerActionV1] {
        &self.actions
    }
    pub(super) fn matches_boundary(&self, boundary: &UnitOwnerReplaceBreakObservationV1) -> bool {
        use UnitOwnerReplaceBreakConsumerActionV1 as A;
        let ReplaceBreakSuffix::Bind {
            initializer,
            statement,
            retired,
        } = boundary.suffix
        else {
            return false;
        };
        let source = boundary.statement.source_unit();
        self.incoming
            == [
                boundary.condition_false.clone(),
                boundary.post_break.clone(),
            ]
            && self.initializer == initializer
            && self.statement == statement
            && self.retired == retired
            && self.initializer.source_unit() == source
            && self.statement.source_unit() == source
            && self.retired.source_unit() == source
            && self.retired != boundary.entry.symbol
            && self.retired != boundary.previous
            && ![
                boundary.statement,
                boundary.body,
                boundary.previous_statement,
            ]
            .contains(&self.statement)
            && self.actions
                == [
                    A::Take(self.initializer, boundary.entry.symbol),
                    A::Bind(self.initializer, self.retired),
                    A::Drop(UnitDropPoint::AfterStatement(self.statement), self.retired),
                ]
    }
}
