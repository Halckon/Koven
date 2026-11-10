//! A separate capability for the selected owner transferred to one ordinary own call.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
/// Real transitions of the bounded selected owner into a resolved own call.
pub enum UnitOwnerReplaceBreakCallActionV1 {
    /// The exact Name takes the selected incoming value once.
    Take(UnitExpressionId, UnitSymbolId),
    /// The actual pending Value operand transfers at the checked call boundary.
    Call {
        /// Actual completed call.
        call: UnitExpressionId,
        /// Actual moved Name argument.
        argument: UnitExpressionId,
        /// Resolved ordinary function symbol.
        target: UnitSymbolId,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::ownership_checking::compilation_unit) struct CallConsumerTarget {
    pub(in crate::ownership_checking::compilation_unit) call: UnitExpressionId,
    pub(in crate::ownership_checking::compilation_unit) argument: UnitExpressionId,
    pub(in crate::ownership_checking::compilation_unit) statement: UnitStatementId,
    pub(in crate::ownership_checking::compilation_unit) target: UnitSymbolId,
}

/// Complete P/Q selection moved through a real pending operand into its own callee.
/// No caller Bind/Drop is implied; the callee retains its existing cleanup contract.
#[derive(Clone, Debug)]
pub struct UnitOwnerReplaceBreakCallConsumerV1 {
    pub(in crate::ownership_checking::compilation_unit) incoming: [UnitOwnerReplaceBreakStateV1; 2],
    pub(in crate::ownership_checking::compilation_unit) target: CallConsumerTarget,
    pub(in crate::ownership_checking::compilation_unit) actions:
        Vec<UnitOwnerReplaceBreakCallActionV1>,
}
impl UnitOwnerReplaceBreakCallConsumerV1 {
    /// Condition-false P and Break Q, retaining their original catalog identities.
    pub fn incoming(&self) -> &[UnitOwnerReplaceBreakStateV1; 2] {
        &self.incoming
    }
    /// Actual Take and completed Call handoff, in planner order.
    pub fn actions(&self) -> &[UnitOwnerReplaceBreakCallActionV1] {
        &self.actions
    }
    /// The original call expression receiving the selected owner.
    pub fn call(&self) -> UnitExpressionId {
        self.target.call
    }
    /// The original Name expression that takes the selected root.
    pub fn argument(&self) -> UnitExpressionId {
        self.target.argument
    }
    /// Resolved top-level declaration symbol from the checked compilation unit.
    pub fn target(&self) -> UnitSymbolId {
        self.target.target
    }
    pub(super) fn matches_boundary(&self, boundary: &UnitOwnerReplaceBreakObservationV1) -> bool {
        use UnitOwnerReplaceBreakCallActionV1 as A;
        let ReplaceBreakSuffix::Call(target) = &boundary.suffix else {
            return false;
        };
        let source = boundary.statement.source_unit();
        self.target == *target
            && self.incoming
                == [
                    boundary.condition_false.clone(),
                    boundary.post_break.clone(),
                ]
            && self.call().source_unit() == source
            && self.argument().source_unit() == source
            && target.statement.source_unit() == source
            && self.call() != self.argument()
            && ![
                boundary.condition,
                boundary.jump,
                boundary.call,
                boundary.initial,
                boundary.argument,
            ]
            .contains(&self.call())
            && ![
                boundary.condition,
                boundary.jump,
                boundary.call,
                boundary.initial,
                boundary.argument,
            ]
            .contains(&self.argument())
            && ![
                boundary.statement,
                boundary.body,
                boundary.previous_statement,
            ]
            .contains(&target.statement)
            && self.actions
                == [
                    A::Take(self.argument(), boundary.entry.symbol),
                    A::Call {
                        call: self.call(),
                        argument: self.argument(),
                        target: self.target(),
                    },
                ]
    }
}
