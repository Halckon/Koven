//! Replay only the emitted transitions to check internal conservation and slot isolation.
use super::UnitDropPoint;
use super::owner_transport::*;
use crate::name_resolution::SourceUnitId;
use crate::{name_resolution::UnitSymbolId, type_checking::UnitExpressionId};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Location {
    Uncreated,
    Named(UnitSymbolId),
    Result(UnitExpressionId),
    Pending(UnitExpressionId, UnitExpressionId),
    Dropped,
}

fn point_source(point: UnitDropPoint) -> SourceUnitId {
    match point {
        UnitDropPoint::AfterExpression(id)
        | UnitDropPoint::AfterBinaryOperands(id)
        | UnitDropPoint::CallReturn(id)
        | UnitDropPoint::ControlTransfer(id)
        | UnitDropPoint::LambdaEntry(id)
        | UnitDropPoint::BeforeReplacement(id)
        | UnitDropPoint::AfterReplacement(id) => id.source_unit(),
        UnitDropPoint::WhenComparisonCompleted { control, .. }
        | UnitDropPoint::WhenAlternativeNoMatch { control, .. }
        | UnitDropPoint::BranchExit { control, .. }
        | UnitDropPoint::WhenAlternativeMatch { control, .. } => control.source_unit(),
        UnitDropPoint::AfterStatement(id) | UnitDropPoint::LoopExit(id) => id.source_unit(),
        UnitDropPoint::FunctionEntry(id) => id.source_unit(),
    }
}

impl UnitOwnerObservation {
    /// Check source/product identity and linear binding/pending/replace conservation.
    /// This does not grant unsupported syntax or replace ordinary ownership validation.
    pub fn is_consistent(&self) -> bool {
        let source = match self.callable {
            UnitOwnerCallable::Function(item) | UnitOwnerCallable::Deinit(item) => {
                item.source_unit()
            }
            UnitOwnerCallable::Lambda(expression) => expression.source_unit(),
        };
        if let UnitOwnerObservationStatus::Unsupported { point, .. } = self.status {
            return point_source(point) == source
                && self.definitions.is_empty()
                && self.events.is_empty();
        }
        if !matches!(self.callable, UnitOwnerCallable::Function(_)) {
            return false;
        }
        let mut locations = Vec::new();
        for (index, definition) in self.definitions.iter().enumerate() {
            if definition.id.callable != self.callable
                || definition.id.index != index
                || !std::sync::Arc::ptr_eq(&definition.id.identity, &self.identity)
                || self.definitions[..index]
                    .iter()
                    .any(|prior| prior.origin == definition.origin)
            {
                return false;
            }
            let location = match definition.origin {
                UnitOwnerOrigin::Parameter(symbol) if symbol.source_unit() == source => {
                    Location::Named(symbol)
                }
                UnitOwnerOrigin::String(expression) if expression.source_unit() == source => {
                    Location::Uncreated
                }
                _ => return false,
            };
            if matches!(location, Location::Named(_)) && locations.contains(&location) {
                return false;
            }
            locations.push(location);
        }
        for event in &self.events {
            let valid_owner = |id: &UnitOwnerId| self.definition(id.clone()).is_some();
            match event {
                UnitOwnerTransportEvent::Move {
                    expression,
                    symbol,
                    owner,
                } => {
                    if !valid_owner(owner)
                        || expression.source_unit() != source
                        || symbol.source_unit() != source
                        || locations[owner.index] != Location::Named(*symbol)
                    {
                        return false;
                    }
                    locations[owner.index] = Location::Result(*expression);
                }
                UnitOwnerTransportEvent::Drop {
                    point,
                    symbol,
                    owner,
                } => {
                    if !valid_owner(owner)
                        || point_source(*point) != source
                        || symbol.source_unit() != source
                        || locations[owner.index] != Location::Named(*symbol)
                    {
                        return false;
                    }
                    locations[owner.index] = Location::Dropped;
                }
                UnitOwnerTransportEvent::Result { expression, owner } => {
                    if !valid_owner(owner) || expression.source_unit() != source {
                        return false;
                    }
                    match locations[owner.index] {
                        Location::Uncreated
                            if self.definitions[owner.index].origin
                                == UnitOwnerOrigin::String(*expression) => {}
                        Location::Result(_) => {}
                        _ => return false,
                    }
                    locations[owner.index] = Location::Result(*expression);
                }
                UnitOwnerTransportEvent::Binding {
                    symbol,
                    initializer,
                    owner,
                } => {
                    if !valid_owner(owner)
                        || symbol.source_unit() != source
                        || initializer.source_unit() != source
                        || locations[owner.index] != Location::Result(*initializer)
                        || locations.contains(&Location::Named(*symbol))
                    {
                        return false;
                    }
                    locations[owner.index] = Location::Named(*symbol);
                }
                UnitOwnerTransportEvent::Pending {
                    call,
                    argument,
                    owner,
                } => {
                    if !valid_owner(owner)
                        || call.source_unit() != source
                        || argument.source_unit() != source
                        || locations[owner.index] != Location::Result(*argument)
                        || locations.contains(&Location::Pending(*call, *argument))
                    {
                        return false;
                    }
                    locations[owner.index] = Location::Pending(*call, *argument);
                }
                UnitOwnerTransportEvent::Replace {
                    call,
                    root,
                    argument,
                    previous,
                    incoming,
                } => {
                    if !valid_owner(previous)
                        || !valid_owner(incoming)
                        || previous == incoming
                        || call.source_unit() != source
                        || root.source_unit() != source
                        || argument.source_unit() != source
                        || locations[previous.index] != Location::Named(*root)
                        || locations[incoming.index] != Location::Pending(*call, *argument)
                    {
                        return false;
                    }
                    locations[previous.index] = Location::Result(*call);
                    locations[incoming.index] = Location::Named(*root);
                }
            }
        }
        !locations
            .iter()
            .any(|location| matches!(location, Location::Uncreated | Location::Pending(..)))
    }
}
