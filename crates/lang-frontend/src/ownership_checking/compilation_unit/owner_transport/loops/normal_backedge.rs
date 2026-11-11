//! Finite source equation for a changing header; H is never a producer token.
use super::super::*;
use crate::type_checking::UnitStatementId;

/// Which existing obligation is supplied to an activation of this header.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UnitOwnerHeaderSourceV1 {
    /// The real prefix definition, supplied on zero-iteration entry.
    Entry(UnitOwnerId),
    /// The new instance of the real body definition, supplied on each normal backedge.
    Backedge(UnitOwnerId),
}

/// A sealed finite changing-owner loop with one normal backedge and one Return exit.
/// The raw first traversal is retained as evidence, not mistaken for all activations.
#[derive(Clone, Debug)]
pub(in crate::ownership_checking::compilation_unit) struct HeaderCore {
    pub(in crate::ownership_checking::compilation_unit) completion: HeaderCompletion,
    pub(in crate::ownership_checking::compilation_unit) identity: Arc<()>,
    pub(in crate::ownership_checking::compilation_unit) callable: UnitOwnerCallable,
    pub(in crate::ownership_checking::compilation_unit) statement: UnitStatementId,
    pub(in crate::ownership_checking::compilation_unit) condition: UnitExpressionId,
    pub(in crate::ownership_checking::compilation_unit) body: UnitStatementId,
    pub(in crate::ownership_checking::compilation_unit) initial: UnitExpressionId,
    pub(in crate::ownership_checking::compilation_unit) argument: UnitExpressionId,
    pub(in crate::ownership_checking::compilation_unit) call: UnitExpressionId,
    pub(in crate::ownership_checking::compilation_unit) previous: UnitSymbolId,
    pub(in crate::ownership_checking::compilation_unit) previous_statement: UnitStatementId,
    pub(in crate::ownership_checking::compilation_unit) inputs: UnitOwnerProducerCatalogV1,
    pub(in crate::ownership_checking::compilation_unit) producers: UnitOwnerProducerCatalogV1,
    pub(in crate::ownership_checking::compilation_unit) prefix: Vec<UnitOwnerTransportEvent>,
    pub(in crate::ownership_checking::compilation_unit) events: Vec<UnitOwnerTransportEvent>,
    pub(in crate::ownership_checking::compilation_unit) entry: UnitOwnerReplaceBreakStateV1,
    pub(in crate::ownership_checking::compilation_unit) backedge: UnitOwnerReplaceBreakStateV1,
    pub(in crate::ownership_checking::compilation_unit) false_sample: UnitOwnerReplaceBreakStateV1,
    pub(in crate::ownership_checking::compilation_unit) target: ReturnConsumerTarget,
    pub(in crate::ownership_checking::compilation_unit) assignment: UnitExpressionId,
    pub(in crate::ownership_checking::compilation_unit) increment: Option<UnitExpressionId>,
    pub(in crate::ownership_checking::compilation_unit) assignment_completed: bool,
    pub(in crate::ownership_checking::compilation_unit) body_completed: bool,
    pub(in crate::ownership_checking::compilation_unit) taken: bool,
    pub(in crate::ownership_checking::compilation_unit) delivered: bool,
    pub(in crate::ownership_checking::compilation_unit) cleaned: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ownership_checking::compilation_unit) enum HeaderCompletion {
    Fallthrough,
    Continue(UnitExpressionId),
    Mixed(UnitExpressionId),
}
/// Sealed ordinary fallthrough backedge. Never a Continue product.
#[derive(Clone, Debug)]
pub struct UnitOwnerNormalBackedgeV1 {
    pub(in crate::ownership_checking::compilation_unit) core: HeaderCore,
}
/// Sealed explicit Continue after the real jump cleanup, targeting this header.
#[derive(Clone, Debug)]
pub struct UnitOwnerContinueBackedgeV1 {
    pub(in crate::ownership_checking::compilation_unit) core: HeaderCore,
    pub(in crate::ownership_checking::compilation_unit) jump: UnitExpressionId,
    pub(in crate::ownership_checking::compilation_unit) target: UnitStatementId,
    pub(in crate::ownership_checking::compilation_unit) identity: Arc<()>,
    pub(in crate::ownership_checking::compilation_unit) snapshot: UnitOwnerReplaceBreakStateV1,
    pub(in crate::ownership_checking::compilation_unit) stopped: bool,
}
/// A separately sealed Continue/header and Break/exit split of the current owner.
#[derive(Clone, Debug)]
pub struct UnitOwnerMixedBackedgeV1 {
    pub(in crate::ownership_checking::compilation_unit) core: HeaderCore,
    pub(in crate::ownership_checking::compilation_unit) split: UnitExpressionId,
    pub(in crate::ownership_checking::compilation_unit) split_condition: UnitExpressionId,
    pub(in crate::ownership_checking::compilation_unit) bodies: [UnitStatementId; 2],
    pub(in crate::ownership_checking::compilation_unit) jumps: [UnitExpressionId; 2],
    pub(in crate::ownership_checking::compilation_unit) roles: [LoopBodyCompletion; 2],
    pub(in crate::ownership_checking::compilation_unit) identity: Arc<()>,
    pub(in crate::ownership_checking::compilation_unit) split_state: UnitOwnerReplaceBreakStateV1,
    pub(in crate::ownership_checking::compilation_unit) entries: [UnitOwnerReplaceBreakStateV1; 2],
    pub(in crate::ownership_checking::compilation_unit) transferred:
        [UnitOwnerReplaceBreakStateV1; 2],
    pub(in crate::ownership_checking::compilation_unit) completed:
        [UnitOwnerReplaceBreakStateV1; 2],
    pub(in crate::ownership_checking::compilation_unit) stopped: bool,
}
impl UnitOwnerMixedBackedgeV1 {
    /// The actual If whose true arm continues and false arm breaks.
    pub fn split(&self) -> UnitExpressionId {
        self.split
    }
    /// Actual pure branch comparison, distinct from the loop condition.
    pub fn split_condition(&self) -> UnitExpressionId {
        self.split_condition
    }
    /// Actual Continue source; only this arm supplies the header.
    pub fn continue_jump(&self) -> UnitExpressionId {
        self.jumps[0]
    }
    /// Actual Break source; only this arm supplies the loop exit.
    pub fn break_jump(&self) -> UnitExpressionId {
        self.jumps[1]
    }
    /// Source-qualified Continue arm body.
    pub fn continue_body(&self) -> UnitStatementId {
        self.bodies[0]
    }
    /// Source-qualified Break arm body.
    pub fn break_body(&self) -> UnitStatementId {
        self.bodies[1]
    }
    /// Check actual split/arm completion evidence without merging edge roles.
    pub fn is_consistent(&self) -> bool {
        let ids = [
            self.split,
            self.split_condition,
            self.jumps[0],
            self.jumps[1],
        ];
        self.core.is_consistent()
            && self.core.completion == HeaderCompletion::Mixed(self.split)
            && self.core.increment.is_some()
            && self.roles
                == [
                    LoopBodyCompletion::Continue(self.jumps[0]),
                    LoopBodyCompletion::Break(self.jumps[1]),
                ]
            && self.stopped
            && Arc::ptr_eq(&self.identity, &self.core.producers.identity)
            && self.split_state == self.core.backedge
            && self
                .entries
                .iter()
                .chain(self.transferred.iter())
                .chain(self.completed.iter())
                .all(|s| s == &self.split_state)
            && ids.iter().enumerate().all(|(i, id)| {
                id.source_unit() == self.core.statement.source_unit()
                    && !ids[..i].contains(id)
                    && ![
                        self.core.condition,
                        self.core.initial,
                        self.core.argument,
                        self.core.call,
                        self.core.assignment,
                        self.core.target.payload,
                        self.core.target.expression,
                    ]
                    .contains(id)
                    && self.core.increment != Some(*id)
            })
            && self.bodies[0] != self.bodies[1]
            && self.bodies.iter().all(|b| {
                b.source_unit() == self.core.statement.source_unit()
                    && ![
                        self.core.statement,
                        self.core.body,
                        self.core.previous_statement,
                        self.core.target.statement,
                    ]
                    .contains(b)
            })
    }
}
// Both wrappers expose the H equation without exposing or converting their edge role.
macro_rules! header_accessors {
    ($wrapper:ty) => {
        impl $wrapper {
            /// Source-qualified callable owning both catalogs and the control equation.
            pub fn callable(&self) -> UnitOwnerCallable {
                self.core.callable
            }
            /// Actual While and header identity, not a value producer.
            pub fn statement(&self) -> UnitStatementId {
                self.core.statement
            }
            /// Actual condition, whose false edge transports the current header value.
            pub fn condition(&self) -> UnitExpressionId {
                self.core.condition
            }
            /// Actual loop body.
            pub fn body(&self) -> UnitStatementId {
                self.core.body
            }
            /// Actual root carried by the header.
            pub fn root(&self) -> UnitSymbolId {
                self.core.entry.symbol()
            }
            /// Actual creation site of the entry owner.
            pub fn initial(&self) -> UnitExpressionId {
                self.core.initial
            }
            /// Actual per-activation body creation site.
            pub fn argument(&self) -> UnitExpressionId {
                self.core.argument
            }
            /// Actual Replace site, consuming current H after Q is completely formed.
            pub fn call(&self) -> UnitExpressionId {
                self.core.call
            }
            /// Exact initial catalog; no synthetic H definition is appended.
            pub fn input_producers(&self) -> &UnitOwnerProducerCatalogV1 {
                &self.core.inputs
            }
            /// Actual body definitions; one static Q site creates a fresh instance per activation.
            pub fn body_producers(&self) -> &UnitOwnerProducerCatalogV1 {
                &self.core.producers
            }
            /// Complete reachable header incoming equation: entry P or the previous activation's Q.
            pub fn incoming(&self) -> [UnitOwnerHeaderSourceV1; 2] {
                [
                    UnitOwnerHeaderSourceV1::Entry(self.core.entry.owner()),
                    UnitOwnerHeaderSourceV1::Backedge(self.core.backedge.owner()),
                ]
            }
            /// Exact checked Int increment whose normal and Abort edges remain distinct.
            pub fn increment(&self) -> Option<UnitExpressionId> {
                self.core.increment
            }
            /// The terminal Name consumes H, never a fixed P or Q token.
            pub fn return_payload(&self) -> UnitExpressionId {
                self.core.target.payload
            }
            /// The real Return receives H before its cleanup completes.
            pub fn return_expression(&self) -> UnitExpressionId {
                self.core.target.expression
            }
            /// Real prefix transitions, retained independently from the header source equation.
            pub fn prefix_events(&self) -> &[UnitOwnerTransportEvent] {
                &self.core.prefix
            }
            /// One actual planner traversal, used to establish the parametric body rule.
            pub fn body_events(&self) -> &[UnitOwnerTransportEvent] {
                &self.core.events
            }
        }
    };
}
header_accessors!(UnitOwnerNormalBackedgeV1);
header_accessors!(UnitOwnerContinueBackedgeV1);
header_accessors!(UnitOwnerMixedBackedgeV1);
impl UnitOwnerNormalBackedgeV1 {
    /// True only for a complete ordinary backedge with no explicit jump role.
    pub fn is_consistent(&self) -> bool {
        self.core.completion == HeaderCompletion::Fallthrough && self.core.is_consistent()
    }
}
impl UnitOwnerContinueBackedgeV1 {
    /// The actual source-qualified Continue, not an inferred Goto.
    pub fn jump(&self) -> UnitExpressionId {
        self.jump
    }
    /// Checks the distinct jump role, completed cleanup, target and shared H equation.
    pub fn is_consistent(&self) -> bool {
        self.core.completion == HeaderCompletion::Continue(self.jump)
            && self.core.is_consistent()
            && self.stopped
            && self.target == self.core.statement
            && self.jump.source_unit() == self.core.statement.source_unit()
            && ![
                self.core.initial,
                self.core.argument,
                self.core.call,
                self.core.assignment,
                self.core.condition,
                self.core.target.payload,
                self.core.target.expression,
            ]
            .contains(&self.jump)
            && self.core.increment != Some(self.jump)
            && Arc::ptr_eq(&self.identity, &self.core.producers.identity)
            && self.snapshot == self.core.backedge
    }
}
impl HeaderCore {
    fn root(&self) -> UnitSymbolId {
        self.entry.symbol()
    }
    /// Validate the finite linear rule, not a bounded unrolling of the loop.
    /// Replace's old input is the sole occurrence consumed by the detached binding/Drop;
    /// no operation tests its identity. Substitution of any initialized H preserves the
    /// rule: form fresh Q, replace H with Q, drop H once, carry only Q to the next header.
    pub(in crate::ownership_checking::compilation_unit) fn is_consistent(&self) -> bool {
        use UnitOwnerTransportEvent as E;
        let p = self.entry.owner();
        let q = self.backedge.owner();
        let source = self.statement.source_unit();
        self.increment.is_none_or(|id| {
            id.source_unit() == source
                && ![
                    self.initial,
                    self.argument,
                    self.call,
                    self.assignment,
                    self.condition,
                    self.target.payload,
                    self.target.expression,
                ]
                .contains(&id)
        }) && self.assignment_completed
            && self.body_completed
            && self.taken
            && self.delivered
            && self.cleaned
            && self.inputs.is_consistent()
            && self.producers.is_consistent()
            && self.inputs.callable() == self.callable
            && self.producers.callable() == self.callable
            && Arc::ptr_eq(&self.identity, &self.inputs.identity)
            && !Arc::ptr_eq(&self.identity, &self.producers.identity)
            && self.inputs.definitions().len() == 1
            && self.producers.definitions().len() == 1
            && self
                .inputs
                .definition(p.clone())
                .is_some_and(|d| d.origin() == UnitOwnerOrigin::String(self.initial))
            && self
                .producers
                .definition(q.clone())
                .is_some_and(|d| d.origin() == UnitOwnerOrigin::String(self.argument))
            && p != q
            && self.root() != self.previous
            && self.entry == self.false_sample
            && self.entry.symbol == self.backedge.symbol
            && self.entry.declaration == self.backedge.declaration
            && self.entry.scope_depth == 1
            && self.backedge.scope_depth == 1
            && self.target.binding.is_none()
            && self.target.replacement.is_none()
            && [self.body, self.previous_statement, self.target.statement]
                .iter()
                .all(|id| id.source_unit() == source && *id != self.statement)
            && [
                self.initial,
                self.argument,
                self.call,
                self.assignment,
                self.condition,
                self.target.payload,
                self.target.expression,
            ]
            .iter()
            .enumerate()
            .all(|(i, id)| {
                id.source_unit() == source
                    && ![
                        self.initial,
                        self.argument,
                        self.call,
                        self.assignment,
                        self.condition,
                        self.target.payload,
                        self.target.expression,
                    ][..i]
                        .contains(id)
            })
            && self.root().source_unit() == source
            && self.previous.source_unit() == source
            && self.prefix
                == [
                    E::Result {
                        expression: self.initial,
                        owner: p.clone(),
                    },
                    E::Binding {
                        symbol: self.root(),
                        initializer: self.initial,
                        owner: p.clone(),
                    },
                ]
            && self.events
                == [
                    E::Result {
                        expression: self.argument,
                        owner: q.clone(),
                    },
                    E::Pending {
                        call: self.call,
                        argument: self.argument,
                        owner: q.clone(),
                    },
                    E::Replace {
                        call: self.call,
                        root: self.root(),
                        argument: self.argument,
                        previous: p.clone(),
                        incoming: q,
                    },
                    E::Result {
                        expression: self.call,
                        owner: p.clone(),
                    },
                    E::Binding {
                        symbol: self.previous,
                        initializer: self.call,
                        owner: p.clone(),
                    },
                    E::Drop {
                        point: UnitDropPoint::AfterStatement(self.previous_statement),
                        symbol: self.previous,
                        owner: p,
                    },
                ]
    }
}
