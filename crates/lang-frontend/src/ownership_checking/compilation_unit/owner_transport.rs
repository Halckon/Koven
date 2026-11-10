//! Narrow owner identity observations, independent of cleanup eligibility.
mod loops;
pub(in crate::ownership_checking::compilation_unit) use loops::{
    CallConsumerTarget, ReplaceBreakSuffix, ReturnConsumerTarget,
};
pub(super) use loops::{
    InvariantLoopBoundaries, InvariantLoopValueV1, LoopBodyCompletion, LoopBranchCompletion,
    LoopBranchExit, publish_observations,
};
pub use loops::{
    UnitOwnerContinueArmBoundaryV1, UnitOwnerInvariantBreakObservationV1,
    UnitOwnerInvariantLoopObservationV1, UnitOwnerInvariantMixedJoinObservationV1,
    UnitOwnerInvariantNormalJoinObservationV1, UnitOwnerInvariantSplitContinueObservationV1,
    UnitOwnerInvariantSplitExitObservationV1, UnitOwnerJumpArmBoundaryV1, UnitOwnerLoopBindingV1,
    UnitOwnerLoopEdgeV1, UnitOwnerLoopObservationV1, UnitOwnerMixedArmBoundaryV1,
    UnitOwnerNormalArmBoundaryV1, UnitOwnerReplaceBreakCallActionV1,
    UnitOwnerReplaceBreakCallConsumerV1, UnitOwnerReplaceBreakConsumerActionV1,
    UnitOwnerReplaceBreakConsumerV1, UnitOwnerReplaceBreakObservationV1,
    UnitOwnerReplaceBreakReturnActionV1, UnitOwnerReplaceBreakReturnConsumerV1,
    UnitOwnerReplaceBreakStateV1,
};
pub(super) mod values;
pub use values::{
    UnitOwnerChoiceIdV1, UnitOwnerChoiceInputV1, UnitOwnerChoiceV1, UnitOwnerProducerCatalogV1,
    UnitOwnerValueEventV1, UnitOwnerValueObservationV1, UnitOwnerValueV1,
};

use super::UnitDropPoint;
use crate::{
    name_resolution::UnitSymbolId,
    type_checking::{UnitExpressionId, UnitItemId},
};
use std::sync::Arc;

/// Callable whose evaluation owns this observation transaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitOwnerCallable {
    /// Ordinary function body.
    Function(UnitItemId),
    /// Lambda body (not supported by this initial observation slice).
    Lambda(UnitExpressionId),
    /// Deinitializer body (not supported by this initial observation slice).
    Deinit(UnitItemId),
}

/// Opaque definition token; compare only within one ownership analysis product.
/// It is not a runtime allocation identity or a source-span identity.
#[derive(Clone)]
pub struct UnitOwnerId {
    pub(super) identity: Arc<()>,
    pub(super) callable: UnitOwnerCallable,
    pub(super) index: usize,
}

impl std::fmt::Debug for UnitOwnerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UnitOwnerId")
            .field("callable", &self.callable)
            .field("index", &self.index)
            .finish()
    }
}
impl PartialEq for UnitOwnerId {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.identity, &other.identity)
            && self.callable == other.callable
            && self.index == other.index
    }
}
impl Eq for UnitOwnerId {}

/// Actual producer of a complete plain owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitOwnerOrigin {
    /// Value parameter admitted at function entry.
    Parameter(UnitSymbolId),
    /// Completed non-interpolated String construction.
    String(UnitExpressionId),
    /// Completed leaf move-closure environment with owned String captures.
    Closure(UnitExpressionId),
}

/// One owner definition, never synthesized by Name, binding or replace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitOwnerDefinition {
    pub(super) id: UnitOwnerId,
    pub(super) origin: UnitOwnerOrigin,
}
impl UnitOwnerDefinition {
    /// Token local to this analysis product.
    pub fn id(&self) -> UnitOwnerId {
        self.id.clone()
    }
    /// Source-qualified construction or entry parameter.
    pub fn origin(&self) -> UnitOwnerOrigin {
        self.origin
    }
}

/// Reason this observation cannot describe a whole callable; not a language error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitOwnerUnsupportedReason {
    /// Unsupported control flow, destructuring, capture, assignment or another node.
    Syntax,
    /// A call without the supported checked root-replace contract.
    OpaqueCall,
    /// The required complete plain owner was not available at the actual transition.
    MissingOwner,
    /// Internal observation conservation check failed; legacy ownership remains independent.
    InconsistentTransport,
    /// The callable has an implicit receiver or unsupported parameter shape.
    Callable,
}

/// Observation capability is separate from ownership/cleanup validation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitOwnerObservationStatus {
    /// Every visited transition in this callable belongs to the narrow slice.
    Complete,
    /// All partial definitions and events have been discarded.
    Unsupported {
        /// First unsupported source-qualified execution point.
        point: UnitDropPoint,
        /// Unsupported capability, not an ownership diagnostic.
        reason: UnitOwnerUnsupportedReason,
    },
}

/// Events emitted at real evaluation/registration/commit transitions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UnitOwnerTransportEvent {
    /// A Unit-valued or both-terminal If; continuing paths preserve surviving identities.
    /// The condition precedes this event. Each ordered arm includes its real cleanup;
    /// arm 0 is true and arm 1 is false. These are alternatives, never a linear trace.
    If {
        /// Source-qualified control expression.
        control: UnitExpressionId,
        /// True path, including branch-exit cleanup.
        then_events: Vec<UnitOwnerTransportEvent>,
        /// Explicit false path, including branch-exit cleanup.
        else_events: Vec<UnitOwnerTransportEvent>,
    },
    /// A real bare Unit return after all path-local cleanup. No successor events.
    UnitReturn {
        /// Source-qualified return expression; not an owner producer.
        expression: UnitExpressionId,
    },
    /// Formation consumes actual current binding tokens into a new environment.
    Capture {
        /// Source-qualified lambda evaluated at this transition.
        expression: UnitExpressionId,
        /// Newly constructed environment owner, distinct from its captured values.
        environment: UnitOwnerId,
        /// Checked capture order and the exact values removed by the planner.
        inputs: Vec<(UnitSymbolId, UnitOwnerId)>,
    },
    /// Shared formation reads current named tokens without transferring their owners.
    SharedCapture {
        /// Actual source-qualified lambda formation.
        expression: UnitExpressionId,
        /// Newly formed environment token.
        environment: UnitOwnerId,
        /// Ordered borrowed slots and their actual current source owners.
        inputs: Vec<(UnitSymbolId, UnitOwnerId)>,
    },
    /// Actual shared capture loan retirement, immediately before environment cleanup.
    SharedCaptureEnd {
        /// Actual cleanup point shared with the environment Drop.
        point: UnitDropPoint,
        /// Exact formed environment.
        environment: UnitOwnerId,
        /// Source-qualified borrowed slot.
        source: UnitSymbolId,
        /// Current source token, still owned by its named binding.
        owner: UnitOwnerId,
    },
    /// Actual captured-value cleanup before the containing environment is dropped.
    CaptureDrop {
        /// Actual cleanup execution point.
        point: UnitDropPoint,
        /// Exact environment instance within this straight-line observation.
        environment: UnitOwnerId,
        /// Source-qualified capture slot key, not a source-span identity.
        source: UnitSymbolId,
        /// Exact owner placed into this slot at formation.
        owner: UnitOwnerId,
    },
    /// A consuming Name removed this exact current binding value.
    Move {
        /// Source-qualified evaluated expression.
        expression: UnitExpressionId,
        /// Source-qualified named binding.
        symbol: UnitSymbolId,
        /// Exact transported definition token.
        owner: UnitOwnerId,
    },
    /// A real named cleanup removed the current value (not a source of identity).
    Drop {
        /// Actual cleanup execution point.
        point: UnitDropPoint,
        /// Source-qualified named binding.
        symbol: UnitSymbolId,
        /// Exact transported definition token.
        owner: UnitOwnerId,
    },
    /// An expression produced or forwarded an owner into its caller's local result.
    Result {
        /// Source-qualified evaluated expression.
        expression: UnitExpressionId,
        /// Exact transported definition token.
        owner: UnitOwnerId,
    },
    /// A new binding received the initializer's existing owner.
    Binding {
        /// Source-qualified named binding.
        symbol: UnitSymbolId,
        /// Expression whose result was accepted by this binding.
        initializer: UnitExpressionId,
        /// Exact transported definition token.
        owner: UnitOwnerId,
    },
    /// An evaluated Value operand was registered in this call's pending slot.
    Pending {
        /// Checked call identity, isolated from nested calls.
        call: UnitExpressionId,
        /// Exact evaluated Value operand identity.
        argument: UnitExpressionId,
        /// Exact transported definition token.
        owner: UnitOwnerId,
    },
    /// Atomic checked swap of two complete String roots; produces no owner result.
    Swap {
        /// Checked call identity.
        call: UnitExpressionId,
        /// Left complete root.
        left: UnitSymbolId,
        /// Right complete root.
        right: UnitSymbolId,
        /// Owner held by the left root before commit.
        left_owner: UnitOwnerId,
        /// Owner held by the right root before commit.
        right_owner: UnitOwnerId,
    },
    /// Atomic checked replacement: root receives incoming; result receives previous.
    Replace {
        /// Checked call identity, isolated from nested calls.
        call: UnitExpressionId,
        /// Complete named root updated by the commit.
        root: UnitSymbolId,
        /// Exact evaluated Value operand identity.
        argument: UnitExpressionId,
        /// Root owner before commit; also the result owner.
        previous: UnitOwnerId,
        /// Pending RHS owner; also the root owner after commit.
        incoming: UnitOwnerId,
    },
}

/// Whole-callable atomic legacy observation; unsupported definitions/events are empty.
/// Separately sealed capabilities have their own bounded contracts.
/// Equality compares complete structural facts across analyses, not token interchangeability.
#[derive(Clone, Debug)]
pub struct UnitOwnerObservation {
    pub(super) identity: Arc<()>,
    pub(super) callable: UnitOwnerCallable,
    pub(super) status: UnitOwnerObservationStatus,
    pub(super) definitions: Vec<UnitOwnerDefinition>,
    pub(super) events: Vec<UnitOwnerTransportEvent>,
    pub(super) choice_value: Option<Box<UnitOwnerValueObservationV1>>,
    pub(super) loop_value: Option<Box<UnitOwnerLoopObservationV1>>,
    pub(super) replace_break_value: Option<Box<UnitOwnerReplaceBreakObservationV1>>,
    pub(super) invariant_loop_value: Option<Box<InvariantLoopValueV1>>,
}
impl UnitOwnerObservation {
    /// Source-qualified callable.
    pub fn callable(&self) -> UnitOwnerCallable {
        self.callable
    }
    /// Whether the entire callable was observed.
    pub fn status(&self) -> UnitOwnerObservationStatus {
        self.status
    }
    /// Actual owner definitions; empty when unsupported.
    pub fn definitions(&self) -> &[UnitOwnerDefinition] {
        &self.definitions
    }
    /// Actual state-transition events; empty when unsupported.
    pub fn events(&self) -> &[UnitOwnerTransportEvent] {
        &self.events
    }
    /// Resolve a token only in its owning callable observation.
    pub fn definition(&self, id: UnitOwnerId) -> Option<&UnitOwnerDefinition> {
        self.definitions
            .get(id.index)
            .filter(|definition| definition.id == id)
    }
}

// Only the enclosing observation can relate its own product-local tokens across analyses.
// Standalone tokens, definitions, events and lookup retain strict product identity.
// This legacy equality intentionally excludes the separately validated V1 capability.
impl Eq for UnitOwnerObservation {}

impl PartialEq for UnitOwnerObservation {
    fn eq(&self, other: &Self) -> bool {
        let owner_eq = |left: &UnitOwnerId, right: &UnitOwnerId| {
            let left_local = Arc::ptr_eq(&left.identity, &self.identity);
            let right_local = Arc::ptr_eq(&right.identity, &other.identity);
            left.callable == right.callable
                && left.index == right.index
                && left_local == right_local
                && (left_local || Arc::ptr_eq(&left.identity, &right.identity))
        };
        self.callable == other.callable
            && self.status == other.status
            && self.definitions.len() == other.definitions.len()
            && self
                .definitions
                .iter()
                .zip(&other.definitions)
                .all(|(left, right)| left.origin == right.origin && owner_eq(&left.id, &right.id))
            && self.events.len() == other.events.len()
            && self
                .events
                .iter()
                .zip(&other.events)
                .all(|(left, right)| left.eq_facts(right, &owner_eq))
    }
}

impl UnitOwnerTransportEvent {
    // Exhaustive on the left so adding an event requires defining all of its fact equality.
    fn eq_facts(
        &self,
        other: &Self,
        owner_eq: &impl Fn(&UnitOwnerId, &UnitOwnerId) -> bool,
    ) -> bool {
        match self {
            Self::If {
                control,
                then_events,
                else_events,
            } => match other {
                Self::If {
                    control: rhs_control,
                    then_events: rhs_then,
                    else_events: rhs_else,
                } => {
                    control == rhs_control
                        && then_events.len() == rhs_then.len()
                        && else_events.len() == rhs_else.len()
                        && then_events
                            .iter()
                            .zip(rhs_then)
                            .all(|(left, right)| left.eq_facts(right, owner_eq))
                        && else_events
                            .iter()
                            .zip(rhs_else)
                            .all(|(left, right)| left.eq_facts(right, owner_eq))
                }
                _ => false,
            },
            Self::UnitReturn { expression } => matches!(other,
                Self::UnitReturn { expression: rhs } if expression == rhs),
            Self::Capture {
                expression,
                environment,
                inputs,
            } => matches!(other,
                Self::Capture { expression: rhs_expression, environment: rhs_environment, inputs: rhs_inputs }
                if expression == rhs_expression && owner_eq(environment, rhs_environment)
                    && inputs.len() == rhs_inputs.len()
                    && inputs.iter().zip(rhs_inputs).all(|((source, owner), (rhs_source, rhs_owner))|
                        source == rhs_source && owner_eq(owner, rhs_owner))),
            Self::CaptureDrop {
                point,
                environment,
                source,
                owner,
            } => matches!(other,
                Self::CaptureDrop { point: rhs_point, environment: rhs_environment, source: rhs_source, owner: rhs_owner }
                if point == rhs_point && source == rhs_source
                    && owner_eq(environment, rhs_environment) && owner_eq(owner, rhs_owner)),
            Self::SharedCapture {
                expression,
                environment,
                inputs,
            } => matches!(other,
                Self::SharedCapture { expression: rhs_expression, environment: rhs_environment, inputs: rhs_inputs }
                if expression == rhs_expression && owner_eq(environment, rhs_environment)
                    && inputs.len() == rhs_inputs.len()
                    && inputs.iter().zip(rhs_inputs).all(|((source, owner), (rhs_source, rhs_owner))|
                        source == rhs_source && owner_eq(owner, rhs_owner))),
            Self::SharedCaptureEnd {
                point,
                environment,
                source,
                owner,
            } => matches!(other,
                Self::SharedCaptureEnd { point: rhs_point, environment: rhs_environment, source: rhs_source, owner: rhs_owner }
                if point == rhs_point && source == rhs_source
                    && owner_eq(environment, rhs_environment) && owner_eq(owner, rhs_owner)),
            Self::Move {
                expression,
                symbol,
                owner,
            } => matches!(other,
                Self::Move { expression: rhs_expression, symbol: rhs_symbol, owner: rhs_owner }
                if expression == rhs_expression && symbol == rhs_symbol && owner_eq(owner, rhs_owner)),
            Self::Drop {
                point,
                symbol,
                owner,
            } => matches!(other,
                Self::Drop { point: rhs_point, symbol: rhs_symbol, owner: rhs_owner }
                if point == rhs_point && symbol == rhs_symbol && owner_eq(owner, rhs_owner)),
            Self::Result { expression, owner } => matches!(other,
                Self::Result { expression: rhs_expression, owner: rhs_owner }
                if expression == rhs_expression && owner_eq(owner, rhs_owner)),
            Self::Binding {
                symbol,
                initializer,
                owner,
            } => matches!(other,
                Self::Binding { symbol: rhs_symbol, initializer: rhs_initializer, owner: rhs_owner }
                if symbol == rhs_symbol && initializer == rhs_initializer && owner_eq(owner, rhs_owner)),
            Self::Pending {
                call,
                argument,
                owner,
            } => matches!(other,
                Self::Pending { call: rhs_call, argument: rhs_argument, owner: rhs_owner }
                if call == rhs_call && argument == rhs_argument && owner_eq(owner, rhs_owner)),
            Self::Swap {
                call,
                left,
                right,
                left_owner,
                right_owner,
            } => matches!(other,
                Self::Swap { call: c, left: l, right: r, left_owner: lo, right_owner: ro }
                if call == c && left == l && right == r
                    && owner_eq(left_owner, lo) && owner_eq(right_owner, ro)),
            Self::Replace {
                call,
                root,
                argument,
                previous,
                incoming,
            } => matches!(other,
                Self::Replace { call: rhs_call, root: rhs_root, argument: rhs_argument, previous: rhs_previous, incoming: rhs_incoming }
                if call == rhs_call && root == rhs_root && argument == rhs_argument
                    && owner_eq(previous, rhs_previous) && owner_eq(incoming, rhs_incoming)),
        }
    }
}
