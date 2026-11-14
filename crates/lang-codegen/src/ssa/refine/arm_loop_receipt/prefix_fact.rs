//! Distinct owning domains share only mechanical scalar-prefix validation.
use super::*;
use lang_frontend::ownership_checking::{UnitLoopBreakPrefixFact, UnitLoopPrefixCounterBreakFact};
#[derive(Clone, Debug)]
pub(super) enum PrefixFact {
    Tail(UnitLoopBreakPrefixFact),
    Counter(UnitLoopPrefixCounterBreakFact),
}
impl PrefixFact {
    pub(super) fn prefix(&self) -> UnitExpressionId {
        match self {
            Self::Tail(f) => f.prefix(),
            Self::Counter(f) => f.prefix(),
        }
    }
    pub(super) fn condition(&self) -> UnitExpressionId {
        match self {
            Self::Tail(f) => f.condition(),
            Self::Counter(f) => f.condition(),
        }
    }
    pub(super) fn calls(&self) -> [UnitExpressionId; 2] {
        match self {
            Self::Tail(f) => f.calls(),
            Self::Counter(f) => f.calls(),
        }
    }
    pub(super) fn pair(&self) -> &UnitLoopBranchNoCarryFact {
        match self {
            Self::Tail(f) => f.pair().pair(),
            Self::Counter(f) => f.counter_break().counter().pair(),
        }
    }
    pub(super) fn owns(&self, inputs: &RefineInputs<'_>) -> bool {
        match self {
            Self::Tail(f) => inputs.owned.owns_no_carry_break_prefix(f),
            Self::Counter(f) => inputs.owned.owns_no_carry_prefix_counter_break(f),
        }
    }
    pub(super) fn matches_plan(&self, p: &Plan) -> bool {
        match self {
            Self::Tail(f) => {
                p.tail_break
                    .as_ref()
                    .is_some_and(|b| b.fact.same_identity(f.pair()))
                    && p.counter_break.is_none()
            }
            Self::Counter(f) => {
                p.counter_break
                    .as_ref()
                    .is_some_and(|b| b.fact.same_identity(f.counter_break()))
                    && p.tail_break.is_none()
            }
        }
    }
}
