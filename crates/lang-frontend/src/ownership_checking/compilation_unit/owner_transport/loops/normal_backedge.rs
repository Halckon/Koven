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
pub(in crate::ownership_checking::compilation_unit) struct HeaderEquation {
    pub(in crate::ownership_checking::compilation_unit) completion: HeaderCompletion,
    pub(in crate::ownership_checking::compilation_unit) identity: Arc<()>,
    pub(in crate::ownership_checking::compilation_unit) callable: UnitOwnerCallable,
    pub(in crate::ownership_checking::compilation_unit) statement: UnitStatementId,
    pub(in crate::ownership_checking::compilation_unit) condition: UnitExpressionId,
    pub(in crate::ownership_checking::compilation_unit) body: UnitStatementId,
    pub(in crate::ownership_checking::compilation_unit) initial: UnitExpressionId,
    pub(in crate::ownership_checking::compilation_unit) initial_origin: UnitOwnerOrigin,
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
    pub(in crate::ownership_checking::compilation_unit) assignment: UnitExpressionId,
    pub(in crate::ownership_checking::compilation_unit) increment: Option<UnitExpressionId>,
    pub(in crate::ownership_checking::compilation_unit) assignment_completed: bool,
    pub(in crate::ownership_checking::compilation_unit) body_completed: bool,
}
#[derive(Clone, Debug)]
pub(in crate::ownership_checking::compilation_unit) struct HeaderCore {
    pub(in crate::ownership_checking::compilation_unit) equation: HeaderEquation,
    pub(in crate::ownership_checking::compilation_unit) target: ReturnConsumerTarget,
    pub(in crate::ownership_checking::compilation_unit) taken: bool,
    pub(in crate::ownership_checking::compilation_unit) delivered: bool,
    pub(in crate::ownership_checking::compilation_unit) cleaned: bool,
}
// Private implementation sharing only; public wrappers expose no Deref/conversion.
impl std::ops::Deref for HeaderCore {
    type Target = HeaderEquation;
    fn deref(&self) -> &HeaderEquation {
        &self.equation
    }
}
impl std::ops::DerefMut for HeaderCore {
    fn deref_mut(&mut self) -> &mut HeaderEquation {
        &mut self.equation
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ownership_checking::compilation_unit) enum HeaderCompletion {
    Fallthrough,
    Continue(UnitExpressionId),
    Mixed(UnitExpressionId),
    ConditionalBreak(UnitExpressionId),
    ConditionalReturn(UnitExpressionId),
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
// Current-callable declaration set, semantic limit/stop symbols, raw RHS IDs,
// and the actual loop condition. None belongs exclusively to the literal-2 slice.
#[derive(Clone, Debug)]
pub(in crate::ownership_checking::compilation_unit) struct MixedParameterThreshold {
    pub(in crate::ownership_checking::compilation_unit) callable: UnitOwnerCallable,
    pub(in crate::ownership_checking::compilation_unit) parameters: [UnitSymbolId; 2],
    pub(in crate::ownership_checking::compilation_unit) symbols: [UnitSymbolId; 2],
    pub(in crate::ownership_checking::compilation_unit) operands: [UnitExpressionId; 2],
    pub(in crate::ownership_checking::compilation_unit) condition: UnitExpressionId,
}
/// A true Break and a genuine implicit-false normal backedge, not two terminal arms.
#[derive(Clone, Debug)]
pub struct UnitOwnerConditionalBreakBackedgeV1 {
    pub(in crate::ownership_checking::compilation_unit) core: HeaderCore,
    pub(in crate::ownership_checking::compilation_unit) threshold: MixedParameterThreshold,
    pub(in crate::ownership_checking::compilation_unit) split: UnitExpressionId,
    pub(in crate::ownership_checking::compilation_unit) split_condition: UnitExpressionId,
    pub(in crate::ownership_checking::compilation_unit) break_body: UnitStatementId,
    pub(in crate::ownership_checking::compilation_unit) break_jump: UnitExpressionId,
    pub(in crate::ownership_checking::compilation_unit) identity: Arc<()>,
    // Real split, then entry, jump cleanup, then completion, implicit entry/exit,
    // If continuation and final body completion, in traversal order.
    pub(in crate::ownership_checking::compilation_unit) snapshots:
        Vec<UnitOwnerReplaceBreakStateV1>,
    pub(in crate::ownership_checking::compilation_unit) split_sealed: bool,
    pub(in crate::ownership_checking::compilation_unit) jump_role: LoopBodyCompletion,
}
impl UnitOwnerConditionalBreakBackedgeV1 {
    /// Actual conditional expression; no synthetic else node exists.
    pub fn split(&self) -> UnitExpressionId {
        self.split
    }
    /// Actual strict threshold comparison.
    pub fn split_condition(&self) -> UnitExpressionId {
        self.split_condition
    }
    /// The sole explicit arm.
    pub fn break_body(&self) -> UnitStatementId {
        self.break_body
    }
    /// Actual cleaned Break transfer to the loop exit.
    pub fn break_jump(&self) -> UnitExpressionId {
        self.break_jump
    }
    /// Validate the independent false-continuation and true-Break evidence.
    pub fn is_consistent(&self) -> bool {
        let source = self.core.statement.source_unit();
        let ids = [self.split, self.split_condition, self.break_jump];
        let t = &self.threshold;
        self.core.is_consistent()
            && self.core.completion == HeaderCompletion::ConditionalBreak(self.split)
            && self.jump_role == LoopBodyCompletion::Break(self.break_jump)
            && self.core.increment.is_some()
            && self.split_sealed
            && Arc::ptr_eq(&self.identity, &self.core.producers.identity)
            && self.snapshots.len() == 8
            && self.snapshots.iter().all(|s| s == &self.core.backedge)
            && self.break_body.source_unit() == source
            && ![
                self.core.body,
                self.core.statement,
                self.core.previous_statement,
            ]
            .contains(&self.break_body)
            && ids.iter().enumerate().all(|(i, id)| {
                id.source_unit() == source
                    && !ids[..i].contains(id)
                    && ![
                        self.core.condition,
                        self.core.initial,
                        self.core.argument,
                        self.core.call,
                        self.core.assignment,
                        self.core.target.expression,
                        self.core.target.payload,
                    ]
                    .contains(id)
                    && Some(*id) != self.core.increment
            })
            && t.callable == self.core.callable
            && t.condition == self.core.condition
            && t.parameters[0] != t.parameters[1]
            && t.symbols[0] != t.symbols[1]
            && t.symbols
                .iter()
                .all(|id| t.parameters.contains(id) && id.source_unit() == source)
            && t.operands[0] != t.operands[1]
            && t.operands.iter().all(|id| {
                id.source_unit() == source
                    && !ids.contains(id)
                    && *id != self.core.condition
                    && *id != self.core.assignment
                    && Some(*id) != self.core.increment
            })
    }
}
/// A delivered true-arm Return and a genuine implicit-false normal backedge.
#[derive(Clone, Debug)]
pub struct UnitOwnerConditionalReturnBackedgeV1 {
    pub(in crate::ownership_checking::compilation_unit) core: HeaderCore,
    pub(in crate::ownership_checking::compilation_unit) threshold: MixedParameterThreshold,
    pub(in crate::ownership_checking::compilation_unit) split: UnitExpressionId,
    pub(in crate::ownership_checking::compilation_unit) split_condition: UnitExpressionId,
    pub(in crate::ownership_checking::compilation_unit) return_body: UnitStatementId,
    pub(in crate::ownership_checking::compilation_unit) body_return: UnitOwnerReturnEndpointV1,
    pub(in crate::ownership_checking::compilation_unit) true_events: Vec<UnitOwnerTransportEvent>,
    pub(in crate::ownership_checking::compilation_unit) arm_stopped: bool,
    pub(in crate::ownership_checking::compilation_unit) identity: Arc<()>,
    // Live states only: split, then entry, implicit entry/exit, If and body completion.
    // The terminated true arm is represented by its separately cleaned Return endpoint.
    pub(in crate::ownership_checking::compilation_unit) snapshots:
        Vec<UnitOwnerReplaceBreakStateV1>,
    pub(in crate::ownership_checking::compilation_unit) split_sealed: bool,
}
impl UnitOwnerConditionalReturnBackedgeV1 {
    /// Actual conditional expression; no synthetic else node exists.
    pub fn split(&self) -> UnitExpressionId {
        self.split
    }
    /// Actual strict threshold comparison.
    pub fn split_condition(&self) -> UnitExpressionId {
        self.split_condition
    }
    /// The sole explicit arm.
    pub fn return_body(&self) -> UnitStatementId {
        self.return_body
    }
    /// Actual body Q delivery; distinct from the suffix Return of H.
    pub fn body_return(&self) -> &UnitOwnerReturnEndpointV1 {
        &self.body_return
    }
    /// The true-arm Return expression.
    pub fn body_return_expression(&self) -> UnitExpressionId {
        self.body_return.return_expression()
    }
    /// The true-arm Name payload.
    pub fn body_return_payload(&self) -> UnitExpressionId {
        self.body_return.payload()
    }
    /// Validate the independent false-continuation and true-Break evidence.
    pub fn is_consistent(&self) -> bool {
        let source = self.core.statement.source_unit();
        let ids = [
            self.split,
            self.split_condition,
            self.body_return_expression(),
            self.body_return_payload(),
        ];
        let t = &self.threshold;
        self.core.is_consistent()
            && self.core.completion == HeaderCompletion::ConditionalReturn(self.split)
            && self.body_return.is_consistent()
            && self.arm_stopped
            && self.body_return.taken == self.core.backedge
            && self.body_return.target.expression != self.core.target.expression
            && self.body_return.target.payload != self.core.target.payload
            && self.true_events
                == [
                    UnitOwnerTransportEvent::Move {
                        expression: self.body_return_payload(),
                        symbol: self.core.root(),
                        owner: self.body_return.owner(),
                    },
                    UnitOwnerTransportEvent::Result {
                        expression: self.body_return_payload(),
                        owner: self.body_return.owner(),
                    },
                ]
            && self.core.increment.is_some()
            && self.split_sealed
            && Arc::ptr_eq(&self.identity, &self.core.producers.identity)
            && self.snapshots.len() == 6
            && self.snapshots.iter().all(|s| s == &self.core.backedge)
            && self.return_body.source_unit() == source
            && ![
                self.core.body,
                self.core.statement,
                self.core.previous_statement,
            ]
            .contains(&self.return_body)
            && ids.iter().enumerate().all(|(i, id)| {
                id.source_unit() == source
                    && !ids[..i].contains(id)
                    && ![
                        self.core.condition,
                        self.core.initial,
                        self.core.argument,
                        self.core.call,
                        self.core.assignment,
                        self.core.target.expression,
                        self.core.target.payload,
                    ]
                    .contains(id)
                    && Some(*id) != self.core.increment
            })
            && t.callable == self.core.callable
            && t.condition == self.core.condition
            && t.parameters[0] != t.parameters[1]
            && t.symbols[0] != t.symbols[1]
            && t.symbols
                .iter()
                .all(|id| t.parameters.contains(id) && id.source_unit() == source)
            && t.operands[0] != t.operands[1]
            && t.operands.iter().all(|id| {
                id.source_unit() == source
                    && !ids.contains(id)
                    && *id != self.core.condition
                    && *id != self.core.assignment
                    && Some(*id) != self.core.increment
            })
    }
}
/// A separately sealed Continue/header and Break/exit split of the current owner.
#[derive(Clone, Debug)]
pub struct UnitOwnerMixedBackedgeV1 {
    pub(in crate::ownership_checking::compilation_unit) parameter_count: usize,
    pub(in crate::ownership_checking::compilation_unit) threshold: Option<MixedParameterThreshold>,
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
            && matches!(
                (self.parameter_count, self.threshold.as_ref()),
                (1, None) | (2, Some(_))
            )
            && self.threshold.as_ref().is_none_or(|t| {
                let source = self.core.statement.source_unit();
                t.callable == self.core.callable
                    && t.condition == self.core.condition
                    && t.parameters[0] != t.parameters[1]
                    && t.symbols[0] != t.symbols[1]
                    && t.symbols
                        .iter()
                        .all(|id| t.parameters.contains(id) && id.source_unit() == source)
                    && t.operands[0] != t.operands[1]
                    && t.operands.iter().all(|id| {
                        id.source_unit() == source
                            && !ids.contains(id)
                            && *id != self.core.condition
                            && *id != self.core.assignment
                            && Some(*id) != self.core.increment
                    })
            })
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
    /// Actual entry producer origin; a consumed parameter is never recreated as a String.
    pub fn initial_origin(&self) -> UnitOwnerOrigin {
        self.core.initial_origin
    }
    /// The precise consumed input parameter for the parameter-origin slice.
    pub fn input_parameter(&self) -> Option<UnitSymbolId> {
        match self.core.initial_origin {
            UnitOwnerOrigin::Parameter(symbol) => Some(symbol),
            _ => None,
        }
    }

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
impl HeaderEquation {
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
        let origin_valid = match self.initial_origin {
            UnitOwnerOrigin::String(expression) => expression == self.initial,
            UnitOwnerOrigin::Parameter(symbol) => {
                symbol.source_unit() == source
                    && symbol != self.root()
                    && symbol != self.previous
                    && self.completion == HeaderCompletion::Fallthrough
                    && self.increment.is_some()
            }
            _ => false,
        };
        let mut prefix = Vec::new();
        if let UnitOwnerOrigin::Parameter(symbol) = self.initial_origin {
            prefix.push(E::Move {
                expression: self.initial,
                symbol,
                owner: p.clone(),
            });
        }
        prefix.extend([
            E::Result {
                expression: self.initial,
                owner: p.clone(),
            },
            E::Binding {
                symbol: self.root(),
                initializer: self.initial,
                owner: p.clone(),
            },
        ]);

        origin_valid
            && self.increment.is_none_or(|id| {
                id.source_unit() == source
                    && ![
                        self.initial,
                        self.argument,
                        self.call,
                        self.assignment,
                        self.condition,
                    ]
                    .contains(&id)
            })
            && self.assignment_completed
            && self.body_completed
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
                .is_some_and(|d| d.origin() == self.initial_origin)
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
            && [self.body, self.previous_statement]
                .iter()
                .all(|id| id.source_unit() == source && *id != self.statement)
            && [
                self.initial,
                self.argument,
                self.call,
                self.assignment,
                self.condition,
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
                    ][..i]
                        .contains(id)
            })
            && self.root().source_unit() == source
            && self.previous.source_unit() == source
            && self.prefix == prefix
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

impl HeaderCore {
    pub(in crate::ownership_checking::compilation_unit) fn is_consistent(&self) -> bool {
        let ids = [self.target.payload, self.target.expression];
        self.equation.is_consistent()
            && self.taken
            && self.delivered
            && self.cleaned
            && self.target.binding.is_none()
            && self.target.replacement.is_none()
            && self.target.statement.source_unit() == self.statement.source_unit()
            && self.target.statement != self.statement
            && ids[0] != ids[1]
            && ids.iter().all(|id| {
                id.source_unit() == self.statement.source_unit()
                    && ![
                        self.initial,
                        self.argument,
                        self.call,
                        self.assignment,
                        self.condition,
                    ]
                    .contains(id)
                    && Some(*id) != self.increment
            })
    }
}

header_accessors!(UnitOwnerConditionalBreakBackedgeV1);

header_accessors!(UnitOwnerConditionalReturnBackedgeV1);

/// Normal changing header consumed by one real binding and its final Drop.
#[derive(Clone, Debug)]
pub struct UnitOwnerNormalBindDropV1 {
    pub(in crate::ownership_checking::compilation_unit) core: HeaderEquation,
    pub(in crate::ownership_checking::compilation_unit) initializer: UnitExpressionId,
    pub(in crate::ownership_checking::compilation_unit) statement: UnitStatementId,
    pub(in crate::ownership_checking::compilation_unit) retired: UnitSymbolId,
    pub(in crate::ownership_checking::compilation_unit) taken: UnitOwnerReplaceBreakStateV1,
    pub(in crate::ownership_checking::compilation_unit) bound: UnitOwnerReplaceBreakStateV1,
    pub(in crate::ownership_checking::compilation_unit) point: UnitDropPoint,
    pub(in crate::ownership_checking::compilation_unit) empty: bool,
    pub(in crate::ownership_checking::compilation_unit) result_completed: bool,
}
impl UnitOwnerNormalBindDropV1 {
    /// Exact consumed Name of the exiting header.
    pub fn initializer(&self) -> UnitExpressionId {
        self.initializer
    }
    /// Actual terminal val declaration.
    pub fn binding_statement(&self) -> UnitStatementId {
        self.statement
    }
    /// Actual bound symbol dropped after the declaration.
    pub fn retired(&self) -> UnitSymbolId {
        self.retired
    }
    /// Actual Drop location; not a synthesized Return.
    pub fn drop_point(&self) -> UnitDropPoint {
        self.point
    }
    /// Validate the shared H equation and actual Take/Bind/Drop terminal evidence.
    pub fn is_consistent(&self) -> bool {
        self.core.is_consistent()
            && self.core.completion == HeaderCompletion::Fallthrough
            && self.core.increment.is_some()
            && self.core.initial_origin == UnitOwnerOrigin::String(self.core.initial)
            && self.taken == self.core.false_sample
            && self.bound.owner == self.taken.owner
            && self.bound.symbol == self.retired
            && self.bound.scope_depth == 1
            && self.bound.origin == self.bound.declaration
            && self.retired.source_unit() == self.core.statement.source_unit()
            && ![self.core.root(), self.core.previous].contains(&self.retired)
            && self.statement.source_unit() == self.core.statement.source_unit()
            && ![
                self.core.statement,
                self.core.body,
                self.core.previous_statement,
            ]
            .contains(&self.statement)
            && self.initializer.source_unit() == self.core.statement.source_unit()
            && ![
                self.core.initial,
                self.core.argument,
                self.core.call,
                self.core.condition,
                self.core.assignment,
            ]
            .contains(&self.initializer)
            && Some(self.initializer) != self.core.increment
            && self.point == UnitDropPoint::AfterStatement(self.statement)
            && self.empty
            && self.result_completed
    }
}

impl UnitOwnerNormalBindDropV1 {
    /// Shared finite header evidence.
    pub fn callable(&self) -> UnitOwnerCallable {
        self.core.callable
    }
    /// Shared finite header evidence.
    pub fn statement(&self) -> UnitStatementId {
        self.core.statement
    }
    /// Shared finite header evidence.
    pub fn condition(&self) -> UnitExpressionId {
        self.core.condition
    }
    /// Shared finite header evidence.
    pub fn body(&self) -> UnitStatementId {
        self.core.body
    }
    /// Shared finite header evidence.
    pub fn root(&self) -> UnitSymbolId {
        self.core.entry.symbol()
    }
    /// Shared finite header evidence.
    pub fn initial(&self) -> UnitExpressionId {
        self.core.initial
    }
    /// Shared finite header evidence.
    pub fn argument(&self) -> UnitExpressionId {
        self.core.argument
    }
    /// Shared finite header evidence.
    pub fn call(&self) -> UnitExpressionId {
        self.core.call
    }
    /// Shared finite header evidence.
    pub fn input_producers(&self) -> &UnitOwnerProducerCatalogV1 {
        &self.core.inputs
    }
    /// Shared finite header evidence.
    pub fn body_producers(&self) -> &UnitOwnerProducerCatalogV1 {
        &self.core.producers
    }
    /// Shared finite header evidence.
    pub fn incoming(&self) -> [UnitOwnerHeaderSourceV1; 2] {
        [
            UnitOwnerHeaderSourceV1::Entry(self.core.entry.owner()),
            UnitOwnerHeaderSourceV1::Backedge(self.core.backedge.owner()),
        ]
    }
    /// Shared finite header evidence.
    pub fn increment(&self) -> Option<UnitExpressionId> {
        self.core.increment
    }
    /// Shared finite header evidence.
    pub fn prefix_events(&self) -> &[UnitOwnerTransportEvent] {
        &self.core.prefix
    }
    /// Shared finite header evidence.
    pub fn body_events(&self) -> &[UnitOwnerTransportEvent] {
        &self.core.events
    }
}
