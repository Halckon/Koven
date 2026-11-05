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
    Captured(usize, UnitSymbolId),
    Dropped,
}

pub(super) fn point_source(point: UnitDropPoint) -> SourceUnitId {
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
    /// Check source/product identity and path-local binding/pending/replace conservation.
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
        // Do not silently grant capture × branch composition through replay. An If
        // nested anywhere requires a top-level If, and every capture has a Closure
        // definition; reject this union even when its individual paths conserve owners.
        if self
            .definitions
            .iter()
            .any(|definition| matches!(definition.origin, UnitOwnerOrigin::Closure(_)))
            && self
                .events
                .iter()
                .any(|event| matches!(event, UnitOwnerTransportEvent::If { .. }))
        {
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
                UnitOwnerOrigin::String(expression) | UnitOwnerOrigin::Closure(expression)
                    if expression.source_unit() == source =>
                {
                    Location::Uncreated
                }
                _ => return false,
            };
            if matches!(location, Location::Named(_)) && locations.contains(&location) {
                return false;
            }
            locations.push(location);
        }
        let mut created = locations
            .iter()
            .map(|location| *location != Location::Uncreated)
            .collect::<Vec<_>>();
        self.replay_owner_events(
            &self.events,
            source,
            &mut locations,
            &mut created,
            &mut Vec::new(),
            None,
        ) && !locations
            .iter()
            .any(|location| matches!(location, Location::Uncreated | Location::Pending(..)))
    }

    fn replay_owner_events(
        &self,
        events: &[UnitOwnerTransportEvent],
        source: SourceUnitId,
        locations: &mut [Location],
        created: &mut [bool],
        controls: &mut Vec<UnitExpressionId>,
        edge: Option<(UnitExpressionId, usize)>,
    ) -> bool {
        for event in events {
            let valid_owner = |id: &UnitOwnerId| self.definition(id.clone()).is_some();
            match event {
                UnitOwnerTransportEvent::If {
                    control,
                    then_events,
                    else_events,
                } => {
                    if control.source_unit() != source || controls.contains(control) {
                        return false;
                    }
                    controls.push(*control);
                    let mut left = locations.to_vec();
                    let mut right = locations.to_vec();
                    if !self.replay_owner_events(
                        then_events,
                        source,
                        &mut left,
                        created,
                        controls,
                        Some((*control, 0)),
                    ) || !self.replay_owner_events(
                        else_events,
                        source,
                        &mut right,
                        created,
                        controls,
                        Some((*control, 1)),
                    ) {
                        return false;
                    }
                    for ((joined, left), right) in locations.iter_mut().zip(left).zip(right) {
                        if left == right {
                            *joined = left;
                        } else if *joined == Location::Uncreated
                            && matches!(
                                (left, right),
                                (Location::Uncreated, Location::Dropped)
                                    | (Location::Dropped, Location::Uncreated)
                            )
                        {
                            // This static definition exists on one edge only and was really
                            // dropped there. It is unavailable after either continuation.
                            *joined = Location::Dropped;
                        } else {
                            return false;
                        }
                    }
                }
                UnitOwnerTransportEvent::Capture {
                    expression,
                    environment,
                    inputs,
                } => {
                    if !valid_owner(environment)
                        || expression.source_unit() != source
                        || self.definitions[environment.index].origin
                            != UnitOwnerOrigin::Closure(*expression)
                        || locations[environment.index] != Location::Uncreated
                        || inputs.is_empty()
                    {
                        return false;
                    }
                    for (symbol, owner) in inputs {
                        if !valid_owner(owner)
                            || symbol.source_unit() != source
                            || !matches!(
                                self.definitions[owner.index].origin,
                                UnitOwnerOrigin::String(_) | UnitOwnerOrigin::Parameter(_)
                            )
                            || locations[owner.index] != Location::Named(*symbol)
                            || locations.contains(&Location::Captured(environment.index, *symbol))
                        {
                            return false;
                        }
                        locations[owner.index] = Location::Captured(environment.index, *symbol);
                    }
                    locations[environment.index] = Location::Result(*expression);
                }
                UnitOwnerTransportEvent::CaptureDrop {
                    point,
                    environment,
                    source: symbol,
                    owner,
                } => {
                    if !valid_owner(owner)
                        || !valid_owner(environment)
                        || point_source(*point) != source
                        || symbol.source_unit() != source
                        || !matches!(locations[environment.index], Location::Named(_))
                        || locations[owner.index] != Location::Captured(environment.index, *symbol)
                    {
                        return false;
                    }
                    locations[owner.index] = Location::Dropped;
                }

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
                        || matches!(point, UnitDropPoint::BranchExit { control, branch }
                            if edge != Some((*control, *branch)))
                        || locations.iter().any(|location| {
                            matches!(location,
                            Location::Captured(environment, _) if *environment == owner.index)
                        })
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
                                == UnitOwnerOrigin::String(*expression) =>
                        {
                            if created[owner.index] {
                                return false;
                            }
                            created[owner.index] = true;
                        }
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
        true
    }
}
