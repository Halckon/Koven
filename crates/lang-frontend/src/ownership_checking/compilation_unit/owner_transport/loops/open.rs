//! One open-loop product: live Break or delivered Return, never a closed-loop relaxation.
use super::super::*;
use super::replace_break::UnitOwnerReplaceBreakObservationV1;
use crate::{source::Span, type_checking::UnitStatementId};

/// The common real P -> Q prefix, independent of its terminal edge.
#[derive(Clone, Debug)]
pub struct UnitOwnerOpenLoopPrefixV1 {
    pub(in crate::ownership_checking::compilation_unit) identity: Arc<()>,
    pub(in crate::ownership_checking::compilation_unit) callable: UnitOwnerCallable,
    pub(in crate::ownership_checking::compilation_unit) statement: UnitStatementId,
    pub(in crate::ownership_checking::compilation_unit) condition: UnitExpressionId,
    pub(in crate::ownership_checking::compilation_unit) body: UnitStatementId,
    pub(in crate::ownership_checking::compilation_unit) jump: UnitExpressionId,
    pub(in crate::ownership_checking::compilation_unit) call: UnitExpressionId,
    pub(in crate::ownership_checking::compilation_unit) call_origin: Span,
    pub(in crate::ownership_checking::compilation_unit) initial: UnitExpressionId,
    pub(in crate::ownership_checking::compilation_unit) argument: UnitExpressionId,
    pub(in crate::ownership_checking::compilation_unit) previous: UnitSymbolId,
    pub(in crate::ownership_checking::compilation_unit) previous_statement: UnitStatementId,
    pub(in crate::ownership_checking::compilation_unit) inputs: UnitOwnerProducerCatalogV1,
    pub(in crate::ownership_checking::compilation_unit) producers: UnitOwnerProducerCatalogV1,
    pub(in crate::ownership_checking::compilation_unit) prefix: Vec<UnitOwnerTransportEvent>,
    pub(in crate::ownership_checking::compilation_unit) events: Vec<UnitOwnerTransportEvent>,
    pub(in crate::ownership_checking::compilation_unit) entry: UnitOwnerReplaceBreakStateV1,
    pub(in crate::ownership_checking::compilation_unit) installed: UnitOwnerReplaceBreakStateV1,
    pub(in crate::ownership_checking::compilation_unit) condition_false:
        UnitOwnerReplaceBreakStateV1,
}
impl UnitOwnerOpenLoopPrefixV1 {
    /// Actual callable, shared by both catalogs.
    pub fn callable(&self) -> UnitOwnerCallable {
        self.callable
    }
    /// Actual While statement.
    pub fn statement(&self) -> UnitStatementId {
        self.statement
    }
    /// Actual condition, with its original true and false roles.
    pub fn condition(&self) -> UnitExpressionId {
        self.condition
    }
    /// Actual loop body.
    pub fn body(&self) -> UnitStatementId {
        self.body
    }
    /// The source-qualified replaced root.
    pub fn root(&self) -> UnitSymbolId {
        self.entry.symbol
    }
    /// Actual initializer: a String construction or the consuming Parameter Name for P.
    pub fn initial(&self) -> UnitExpressionId {
        self.initial
    }
    /// Body String construction for Q.
    pub fn argument(&self) -> UnitExpressionId {
        self.argument
    }
    /// Checked actual root Replace.
    pub fn call(&self) -> UnitExpressionId {
        self.call
    }
    /// Detached old-value binding, independently dropped in the body.
    pub fn previous(&self) -> UnitSymbolId {
        self.previous
    }
    /// Actual statement whose ASAP cleanup drops the detached P.
    pub fn previous_statement(&self) -> UnitStatementId {
        self.previous_statement
    }
    /// Original P catalog, never a renumbered body producer.
    pub fn input_producers(&self) -> &UnitOwnerProducerCatalogV1 {
        &self.inputs
    }
    /// Actual Q catalog, distinct from the prefix catalog.
    pub fn body_producers(&self) -> &UnitOwnerProducerCatalogV1 {
        &self.producers
    }
    /// The state really read at condition completion.
    pub fn entry(&self) -> &UnitOwnerReplaceBreakStateV1 {
        &self.entry
    }
    /// Independent original state after condition-false LoopExit cleanup.
    pub fn condition_false_exit(&self) -> &UnitOwnerReplaceBreakStateV1 {
        &self.condition_false
    }
    /// Actual initial Result and Binding, preceded by Move for a Parameter input.
    pub fn prefix_events(&self) -> &[UnitOwnerTransportEvent] {
        &self.prefix
    }
    /// Real body transitions, including a Return payload's Move and Result when present.
    pub fn body_events(&self) -> &[UnitOwnerTransportEvent] {
        &self.events
    }

    // Shared conservation check for both terminal variants. No closed transport is fabricated.
    pub(in crate::ownership_checking::compilation_unit) fn prefix_is_consistent(&self) -> bool {
        let UnitOwnerCallable::Function(function) = self.callable else {
            return false;
        };
        let source = function.source_unit();
        let p = self.entry.owner();
        let q = self.installed.owner();
        let root = self.entry.symbol;
        if !self.inputs.is_consistent()
            || !self.producers.is_consistent()
            || self.inputs.callable() != self.callable
            || self.producers.callable() != self.callable
            || self.inputs.definitions().len() != 1
            || self.producers.definitions().len() != 1
            || !Arc::ptr_eq(&self.identity, &self.inputs.identity)
            || Arc::ptr_eq(&self.identity, &self.producers.identity)
            || self
                .producers
                .definition(q.clone())
                .is_none_or(|d| d.origin() != UnitOwnerOrigin::String(self.argument))
            || p == q
            || root == self.previous
            || root.source_unit() != source
            || self.previous.source_unit() != source
            || [self.statement, self.body, self.previous_statement]
                .iter()
                .any(|id| id.source_unit() != source)
            || self.statement == self.body
            || self.previous_statement == self.statement
            || self.previous_statement == self.body
            || [
                self.condition,
                self.jump,
                self.call,
                self.initial,
                self.argument,
            ]
            .iter()
            .enumerate()
            .any(|(i, id)| {
                id.source_unit() != source
                    || [
                        self.condition,
                        self.jump,
                        self.call,
                        self.initial,
                        self.argument,
                    ][..i]
                        .contains(id)
            })
            || self.entry != self.condition_false
            || self.installed.symbol != root
            || self.entry.declaration != self.installed.declaration
            || self.entry.scope_depth != self.installed.scope_depth
            || self.entry.scope_depth != 1
            || self.entry.origin != self.entry.declaration
            || self.installed.origin != self.call_origin
            || self.call_origin.source_id() != self.entry.declaration.source_id()
        {
            return false;
        }
        use UnitOwnerTransportEvent as E;
        let mut prefix = Vec::new();
        match self
            .inputs
            .definition(p.clone())
            .map(|definition| definition.origin())
        {
            Some(UnitOwnerOrigin::String(expression)) if expression == self.initial => {}
            Some(UnitOwnerOrigin::Parameter(symbol))
                if symbol != root && symbol != self.previous =>
            {
                prefix.push(E::Move {
                    expression: self.initial,
                    symbol,
                    owner: p.clone(),
                });
            }
            _ => return false,
        }
        prefix.extend([
            E::Result {
                expression: self.initial,
                owner: p.clone(),
            },
            E::Binding {
                symbol: root,
                initializer: self.initial,
                owner: p.clone(),
            },
        ]);
        self.prefix == prefix
            && self.events.get(..6)
                == Some(
                    &[
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
                            root,
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
                    ][..],
                )
    }
}

/// Actual transitions of one terminal payload, never two alternative incoming values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UnitOwnerReturnActionV1 {
    /// A real consuming Name removed this exact root and owner.
    Take {
        /// The source-qualified Name expression actually evaluated.
        expression: UnitExpressionId,
        /// The binding removed by this Take.
        root: UnitSymbolId,
        /// The exact current owner removed from that binding.
        owner: UnitOwnerId,
    },
    /// The evaluated payload was delivered before the ordinary Return cleanup.
    Return {
        /// The actual terminal Return expression.
        expression: UnitExpressionId,
        /// The evaluated Name operand delivered by this Return.
        payload: UnitExpressionId,
        /// The transferred owner, no longer live in the returning function.
        owner: UnitOwnerId,
    },
}

/// One actual payload delivery whose ordinary Return cleanup completed.
#[derive(Clone, Debug)]
pub struct UnitOwnerReturnEndpointV1 {
    pub(in crate::ownership_checking::compilation_unit) target: ReturnConsumerTarget,
    pub(in crate::ownership_checking::compilation_unit) taken: UnitOwnerReplaceBreakStateV1,
    pub(in crate::ownership_checking::compilation_unit) actions: Vec<UnitOwnerReturnActionV1>,
    pub(in crate::ownership_checking::compilation_unit) cleaned: bool,
}
impl UnitOwnerReturnEndpointV1 {
    /// Actual Return expression.
    pub fn return_expression(&self) -> UnitExpressionId {
        self.target.expression
    }
    /// Its evaluated Name payload.
    pub fn payload(&self) -> UnitExpressionId {
        self.target.payload
    }
    /// Delivered owner, no longer live in the function.
    pub fn owner(&self) -> UnitOwnerId {
        self.taken.owner()
    }
    /// Actual root consumed by the Name.
    pub fn root(&self) -> UnitSymbolId {
        self.taken.symbol()
    }
    /// Take before delivery; cleanup completion is separately sealed.
    pub fn actions(&self) -> &[UnitOwnerReturnActionV1] {
        &self.actions
    }
    /// Check local source identity, exact actions, and the post-cleanup seal.
    pub fn is_consistent(&self) -> bool {
        let source = self.root().source_unit();
        self.cleaned
            && self.target.binding.is_none()
            && self.target.replacement.is_none()
            && self.target.statement.source_unit() == source
            && self.return_expression().source_unit() == source
            && self.payload().source_unit() == source
            && self.return_expression() != self.payload()
            && self.actions
                == [
                    UnitOwnerReturnActionV1::Take {
                        expression: self.payload(),
                        root: self.root(),
                        owner: self.owner(),
                    },
                    UnitOwnerReturnActionV1::Return {
                        expression: self.return_expression(),
                        payload: self.payload(),
                        owner: self.owner(),
                    },
                ]
    }
}

/// Terminal body Q delivery and separate condition-false P delivery, without a join.
#[derive(Clone, Debug)]
pub struct UnitOwnerOpenLoopReturnV1 {
    pub(in crate::ownership_checking::compilation_unit) prefix: UnitOwnerOpenLoopPrefixV1,
    pub(in crate::ownership_checking::compilation_unit) body_return: UnitOwnerReturnEndpointV1,
    pub(in crate::ownership_checking::compilation_unit) condition_false_return:
        UnitOwnerReturnEndpointV1,
    pub(in crate::ownership_checking::compilation_unit) body_stopped: bool,
}
impl std::ops::Deref for UnitOwnerOpenLoopReturnV1 {
    type Target = UnitOwnerOpenLoopPrefixV1;
    fn deref(&self) -> &Self::Target {
        &self.prefix
    }
}
impl UnitOwnerOpenLoopReturnV1 {
    /// Q delivered by the actual body Return, after an actual Ok(false) body completion.
    pub fn body_return(&self) -> &UnitOwnerReturnEndpointV1 {
        &self.body_return
    }
    /// P delivered by the independent tail Return reached only on condition false.
    pub fn condition_false_return(&self) -> &UnitOwnerReturnEndpointV1 {
        &self.condition_false_return
    }
    /// Shared prefix conservation and both real terminal completion seals.
    pub fn is_consistent(&self) -> bool {
        let body = &self.body_return;
        let tail = &self.condition_false_return;
        let expressions = [
            body.return_expression(),
            body.payload(),
            tail.return_expression(),
            tail.payload(),
        ];
        self.prefix.prefix_is_consistent()
            && self.body_stopped
            && body.is_consistent()
            && tail.is_consistent()
            && body.taken == self.installed
            && tail.taken == self.condition_false
            && self.jump == body.return_expression()
            && expressions.iter().enumerate().all(|(i, expression)| {
                !expressions[..i].contains(expression)
                    && ![self.condition, self.call, self.initial, self.argument]
                        .contains(expression)
            })
            && body.target.statement != tail.target.statement
            && [body.target.statement, tail.target.statement]
                .iter()
                .all(|statement| {
                    ![self.statement, self.body, self.previous_statement].contains(statement)
                })
            && self.events.get(6..)
                == Some(
                    &[
                        UnitOwnerTransportEvent::Move {
                            expression: body.payload(),
                            symbol: self.root(),
                            owner: body.owner(),
                        },
                        UnitOwnerTransportEvent::Result {
                            expression: body.payload(),
                            owner: body.owner(),
                        },
                    ][..],
                )
    }
}

/// Unified open-body endpoint contract; only Break leaves a live owner for a loop exit.
#[derive(Clone, Debug)]
pub enum UnitOwnerOpenLoopObservationV1 {
    /// A real Break leaves Q live, preserving all existing suffix consumer contracts.
    BreakLive(Box<UnitOwnerReplaceBreakObservationV1>),
    /// A real body Return delivers Q; the condition-false path separately delivers P.
    ReturnDelivered(Box<UnitOwnerOpenLoopReturnV1>),
    /// Normal backedge carries a fresh Q instance into the header; false exit returns H.
    NormalBackedge(Box<super::UnitOwnerNormalBackedgeV1>),
}
impl UnitOwnerOpenLoopObservationV1 {
    fn identity(&self) -> &Arc<()> {
        match self {
            Self::BreakLive(v) => &v.identity,
            Self::ReturnDelivered(v) => &v.identity,
            Self::NormalBackedge(v) => &v.identity,
        }
    }
    fn callable(&self) -> UnitOwnerCallable {
        match self {
            Self::BreakLive(v) => v.callable,
            Self::ReturnDelivered(v) => v.callable,
            Self::NormalBackedge(v) => v.callable,
        }
    }
    fn statement(&self) -> UnitStatementId {
        match self {
            Self::BreakLive(v) => v.statement,
            Self::ReturnDelivered(v) => v.statement,
            Self::NormalBackedge(v) => v.statement,
        }
    }
    /// Verify the selected terminal variant without treating Return as a loop exit.
    pub fn is_consistent(&self) -> bool {
        match self {
            Self::BreakLive(value) => value.is_consistent(),
            Self::ReturnDelivered(value) => value.is_consistent(),
            Self::NormalBackedge(value) => value.is_consistent(),
        }
    }
}
impl UnitOwnerObservation {
    /// The single sealed open-loop product. Legacy getters only project their original variant.
    pub fn open_loop_observation_v1(&self) -> Option<&UnitOwnerOpenLoopObservationV1> {
        let value = self.open_loop_value.as_ref()?;
        (self.loop_value.is_none() && self.invariant_loop_value.is_none() && self.choice_value.is_none()
            && self.definitions.is_empty() && self.events.is_empty()
            && matches!(self.status, UnitOwnerObservationStatus::Unsupported { point: UnitDropPoint::AfterStatement(statement), reason: UnitOwnerUnsupportedReason::Syntax } if statement == value.statement())
            && self.callable == value.callable() && Arc::ptr_eq(&self.identity, value.identity())
            && value.is_consistent()).then_some(value)
    }
}
