//! The selected owner leaves through the real Return payload, independently of B/Call.
use super::*;

/// Actual transitions of one selected owned String return payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UnitOwnerReplaceBreakReturnActionV1 {
    /// The exact Name takes the selected incoming value once.
    Take(UnitExpressionId, UnitSymbolId),
    /// The actual post-insert immutable binding carries the same selected owner.
    Bind(UnitExpressionId, UnitSymbolId),
    /// The evaluated payload is delivered before the existing Return cleanup.
    Return {
        /// The actual explicit Return expression.
        expression: UnitExpressionId,
        /// The Name payload already taken from the selected root.
        payload: UnitExpressionId,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::ownership_checking::compilation_unit) struct ReturnConsumerTarget {
    pub(in crate::ownership_checking::compilation_unit) expression: UnitExpressionId,
    pub(in crate::ownership_checking::compilation_unit) payload: UnitExpressionId,
    pub(in crate::ownership_checking::compilation_unit) statement: UnitStatementId,
    pub(in crate::ownership_checking::compilation_unit) binding:
        Option<(UnitExpressionId, UnitStatementId, UnitSymbolId)>,
}

/// Complete P/Q selection delivered by the function's unique owned String Return.
/// This is a transfer, not a caller Drop or a new producer definition.
#[derive(Clone, Debug)]
pub struct UnitOwnerReplaceBreakReturnConsumerV1 {
    pub(in crate::ownership_checking::compilation_unit) incoming: [UnitOwnerReplaceBreakStateV1; 2],
    pub(in crate::ownership_checking::compilation_unit) target: ReturnConsumerTarget,
    pub(in crate::ownership_checking::compilation_unit) actions:
        Vec<UnitOwnerReplaceBreakReturnActionV1>,
}
impl UnitOwnerReplaceBreakReturnConsumerV1 {
    /// Condition-false P followed by Break Q, as mutually exclusive alternatives.
    pub fn incoming(&self) -> &[UnitOwnerReplaceBreakStateV1; 2] {
        &self.incoming
    }
    /// Actual Takes and optional binding before payload delivery, with no caller Drop.
    pub fn actions(&self) -> &[UnitOwnerReplaceBreakReturnActionV1] {
        &self.actions
    }
    /// Source-qualified Return that actually consumed this payload.
    pub fn return_expression(&self) -> UnitExpressionId {
        self.target.expression
    }
    /// Exact Name operand evaluated once by the Return traversal.
    pub fn payload(&self) -> UnitExpressionId {
        self.target.payload
    }
    pub(super) fn matches_boundary(&self, boundary: &UnitOwnerReplaceBreakObservationV1) -> bool {
        use UnitOwnerReplaceBreakReturnActionV1 as A;
        let ReplaceBreakSuffix::Return(target) = &boundary.suffix else {
            return false;
        };
        let source = boundary.statement.source_unit();
        let mut expected = if let Some((initializer, statement, symbol)) = target.binding {
            if initializer.source_unit() != source
                || statement.source_unit() != source
                || symbol.source_unit() != source
                || symbol == boundary.entry.symbol
                || initializer == self.payload()
                || statement == target.statement
                || [
                    boundary.condition,
                    boundary.jump,
                    boundary.call,
                    boundary.initial,
                    boundary.argument,
                    self.return_expression(),
                ]
                .contains(&initializer)
                || [
                    boundary.statement,
                    boundary.body,
                    boundary.previous_statement,
                ]
                .contains(&statement)
            {
                return false;
            }
            vec![
                A::Take(initializer, boundary.entry.symbol),
                A::Bind(initializer, symbol),
                A::Take(self.payload(), symbol),
            ]
        } else {
            vec![A::Take(self.payload(), boundary.entry.symbol)]
        };
        expected.push(A::Return {
            expression: self.return_expression(),
            payload: self.payload(),
        });
        self.target == *target
            && self.incoming
                == [
                    boundary.condition_false.clone(),
                    boundary.post_break.clone(),
                ]
            && self.return_expression().source_unit() == source
            && self.payload().source_unit() == source
            && target.statement.source_unit() == source
            && self.return_expression() != self.payload()
            && ![
                boundary.condition,
                boundary.jump,
                boundary.call,
                boundary.initial,
                boundary.argument,
            ]
            .contains(&self.return_expression())
            && ![
                boundary.condition,
                boundary.jump,
                boundary.call,
                boundary.initial,
                boundary.argument,
            ]
            .contains(&self.payload())
            && ![
                boundary.statement,
                boundary.body,
                boundary.previous_statement,
            ]
            .contains(&target.statement)
            && self.actions == expected
    }
}
