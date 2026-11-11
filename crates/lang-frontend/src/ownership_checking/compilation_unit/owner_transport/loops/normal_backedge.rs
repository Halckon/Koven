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
pub struct UnitOwnerNormalBackedgeV1 {
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
impl UnitOwnerNormalBackedgeV1 {
    /// Source-qualified callable owning both catalogs and the control equation.
    pub fn callable(&self) -> UnitOwnerCallable {
        self.callable
    }
    /// Actual While and header identity, not a value producer.
    pub fn statement(&self) -> UnitStatementId {
        self.statement
    }
    /// Actual condition, whose false edge transports the current header value.
    pub fn condition(&self) -> UnitExpressionId {
        self.condition
    }
    /// Actual normal body.
    pub fn body(&self) -> UnitStatementId {
        self.body
    }
    /// Actual root carried by the header.
    pub fn root(&self) -> UnitSymbolId {
        self.entry.symbol()
    }
    /// Actual creation site of the entry owner.
    pub fn initial(&self) -> UnitExpressionId {
        self.initial
    }
    /// Actual per-activation body creation site.
    pub fn argument(&self) -> UnitExpressionId {
        self.argument
    }
    /// Actual Replace site, consuming current H after Q is completely formed.
    pub fn call(&self) -> UnitExpressionId {
        self.call
    }
    /// Exact initial catalog; no synthetic H definition is appended.
    pub fn input_producers(&self) -> &UnitOwnerProducerCatalogV1 {
        &self.inputs
    }
    /// Actual body definitions; one static Q site creates a fresh instance per activation.
    pub fn body_producers(&self) -> &UnitOwnerProducerCatalogV1 {
        &self.producers
    }
    /// Complete reachable header incoming equation: entry P or the previous activation's Q.
    pub fn incoming(&self) -> [UnitOwnerHeaderSourceV1; 2] {
        [
            UnitOwnerHeaderSourceV1::Entry(self.entry.owner()),
            UnitOwnerHeaderSourceV1::Backedge(self.backedge.owner()),
        ]
    }
    /// Exact checked Int increment whose normal and Abort edges remain distinct.
    pub fn increment(&self) -> Option<UnitExpressionId> {
        self.increment
    }
    /// The terminal Name consumes H, never a fixed P or Q token.
    pub fn return_payload(&self) -> UnitExpressionId {
        self.target.payload
    }
    /// The real Return receives H before its cleanup completes.
    pub fn return_expression(&self) -> UnitExpressionId {
        self.target.expression
    }
    /// Real prefix transitions, retained independently from the header source equation.
    pub fn prefix_events(&self) -> &[UnitOwnerTransportEvent] {
        &self.prefix
    }
    /// One actual planner traversal, used to establish the parametric body rule.
    pub fn body_events(&self) -> &[UnitOwnerTransportEvent] {
        &self.events
    }
    /// Validate the finite linear rule, not a bounded unrolling of the loop.
    /// Replace's old input is the sole occurrence consumed by the detached binding/Drop;
    /// no operation tests its identity. Substitution of any initialized H preserves the
    /// rule: form fresh Q, replace H with Q, drop H once, carry only Q to the next header.
    pub fn is_consistent(&self) -> bool {
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
