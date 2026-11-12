//! Replay only the emitted transitions to check internal conservation and slot isolation.
mod shared_capture;

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
        | UnitDropPoint::BranchEntry { control, .. }
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
        if let Some(nested) = &self.nested_normal {
            return nested.matches(self);
        }
        if let Some(chain) = &self.private_control_chain {
            return chain.matches_observation(&self.identity, self.callable)
                && self.definitions.is_empty()
                && self.events.is_empty()
                && self.control_bridge.is_none()
                && self.counter_batch.is_none()
                && self.counter_tail.is_none()
                && self.closed_loop_batch.is_none()
                && self.multi_root_boundary.is_none()
                && self.loop_value.is_none()
                && self.open_loop_value.is_none()
                && self.invariant_loop_value.is_none()
                && self.choice_value.is_none()
                && matches!(self.status, UnitOwnerObservationStatus::Unsupported {
                    point: UnitDropPoint::AfterStatement(control) | UnitDropPoint::LoopExit(control),
                    reason: UnitOwnerUnsupportedReason::Syntax,
                } if control == chain.first_control());
        }
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
        if !shared_capture::consistent(&self.events) {
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
        )
        .is_some()
            && !locations
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
    ) -> Option<bool> {
        for (event_index, event) in events.iter().enumerate() {
            let valid_owner = |id: &UnitOwnerId| self.definition(id.clone()).is_some();
            match event {
                UnitOwnerTransportEvent::If {
                    control,
                    then_events,
                    else_events,
                } => {
                    if control.source_unit() != source || controls.contains(control) {
                        return None;
                    }
                    controls.push(*control);
                    let mut left = locations.to_vec();
                    let mut right = locations.to_vec();
                    let left_continues = self.replay_owner_events(
                        then_events,
                        source,
                        &mut left,
                        created,
                        controls,
                        Some((*control, 0)),
                    )?;
                    let right_continues = self.replay_owner_events(
                        else_events,
                        source,
                        &mut right,
                        created,
                        controls,
                        Some((*control, 1)),
                    )?;
                    match (left_continues, right_continues) {
                        (false, false) => {
                            if event_index + 1 != events.len() {
                                return None;
                            }
                            // Neither edge reaches a join. Preserve only static
                            // retirement, including definitions formed on one arm.
                            for ((joined, left), right) in locations.iter_mut().zip(left).zip(right)
                            {
                                *joined = match (left, right) {
                                    (
                                        Location::Dropped,
                                        Location::Dropped | Location::Uncreated,
                                    )
                                    | (Location::Uncreated, Location::Dropped) => Location::Dropped,
                                    (Location::Uncreated, Location::Uncreated) => {
                                        Location::Uncreated
                                    }
                                    _ => return None,
                                };
                            }
                            return Some(false);
                        }
                        (false, true) | (true, false) => {
                            let (continuing, terminated) = if left_continues {
                                (&left, &right)
                            } else {
                                (&right, &left)
                            };
                            for ((joined, live), dead) in
                                locations.iter_mut().zip(continuing).zip(terminated)
                            {
                                // A definition created and retired only on the terminal
                                // path is unavailable on the survivor, not a live phi.
                                *joined =
                                    if *live == Location::Uncreated && *dead == Location::Dropped {
                                        Location::Dropped
                                    } else {
                                        *live
                                    };
                            }
                            continue;
                        }
                        (true, true) => {}
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
                            return None;
                        }
                    }
                }
                UnitOwnerTransportEvent::UnitReturn { expression } => {
                    if edge.is_none()
                        || expression.source_unit() != source
                        || event_index + 1 != events.len()
                        || locations.iter().any(|location| {
                            !matches!(location, Location::Uncreated | Location::Dropped)
                        })
                    {
                        return None;
                    }
                    return Some(false);
                }
                UnitOwnerTransportEvent::Capture {
                    expression,
                    environment,
                    inputs,
                } => {
                    if edge.is_some()
                        || !valid_owner(environment)
                        || expression.source_unit() != source
                        || self.definitions[environment.index].origin
                            != UnitOwnerOrigin::Closure(*expression)
                        || locations[environment.index] != Location::Uncreated
                        || inputs.is_empty()
                    {
                        return None;
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
                            return None;
                        }
                        locations[owner.index] = Location::Captured(environment.index, *symbol);
                    }
                    locations[environment.index] = Location::Result(*expression);
                }
                UnitOwnerTransportEvent::SharedCapture {
                    expression,
                    environment,
                    inputs,
                } => {
                    if !valid_owner(environment)
                        || expression.source_unit() != source
                        || self.definitions[environment.index].origin
                            != UnitOwnerOrigin::Closure(*expression)
                        || locations[environment.index] != Location::Uncreated
                        || created[environment.index]
                        || inputs.is_empty()
                    {
                        return None;
                    }
                    for (index, (symbol, owner)) in inputs.iter().enumerate() {
                        if !valid_owner(owner)
                            || symbol.source_unit() != source
                            || !matches!(
                                self.definitions[owner.index].origin,
                                UnitOwnerOrigin::String(_) | UnitOwnerOrigin::Parameter(_)
                            )
                            || locations[owner.index] != Location::Named(*symbol)
                            || inputs[..index].iter().any(|(prior, _)| prior == symbol)
                        {
                            return None;
                        }
                    }
                    created[environment.index] = true;
                    locations[environment.index] = Location::Result(*expression);
                }
                UnitOwnerTransportEvent::SharedCaptureEnd {
                    point,
                    environment,
                    source: symbol,
                    owner,
                } => {
                    if !valid_owner(owner)
                        || !valid_owner(environment)
                        || point_source(*point) != source
                        || matches!(point, UnitDropPoint::BranchEntry { control, branch }
                            | UnitDropPoint::BranchExit { control, branch }
                            if edge != Some((*control, *branch)))
                        || symbol.source_unit() != source
                        || !matches!(locations[environment.index], Location::Named(_))
                        || locations[owner.index] != Location::Named(*symbol)
                    {
                        return None;
                    }
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
                        || matches!(point, UnitDropPoint::BranchEntry { control, branch }
                            | UnitDropPoint::BranchExit { control, branch }
                            if edge != Some((*control, *branch)))
                        || !matches!(locations[environment.index], Location::Named(_))
                        || locations[owner.index] != Location::Captured(environment.index, *symbol)
                    {
                        return None;
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
                        return None;
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
                        || matches!(point, UnitDropPoint::BranchEntry { control, branch }
                            | UnitDropPoint::BranchExit { control, branch }
                            if edge != Some((*control, *branch)))
                        || locations.iter().any(|location| {
                            matches!(location,
                            Location::Captured(environment, _) if *environment == owner.index)
                        })
                    {
                        return None;
                    }
                    locations[owner.index] = Location::Dropped;
                }
                UnitOwnerTransportEvent::Result { expression, owner } => {
                    if !valid_owner(owner) || expression.source_unit() != source {
                        return None;
                    }
                    match locations[owner.index] {
                        Location::Uncreated
                            if self.definitions[owner.index].origin
                                == UnitOwnerOrigin::String(*expression) =>
                        {
                            if created[owner.index] {
                                return None;
                            }
                            created[owner.index] = true;
                        }
                        Location::Result(_) => {}
                        _ => return None,
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
                        return None;
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
                        return None;
                    }
                    locations[owner.index] = Location::Pending(*call, *argument);
                }
                UnitOwnerTransportEvent::Swap {
                    call,
                    left,
                    right,
                    left_owner,
                    right_owner,
                } => {
                    if !valid_owner(left_owner)
                        || !valid_owner(right_owner)
                        || left == right
                        || left_owner == right_owner
                        || call.source_unit() != source
                        || left.source_unit() != source
                        || right.source_unit() != source
                        || locations[left_owner.index] != Location::Named(*left)
                        || locations[right_owner.index] != Location::Named(*right)
                    {
                        return None;
                    }
                    locations[left_owner.index] = Location::Named(*right);
                    locations[right_owner.index] = Location::Named(*left);
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
                        return None;
                    }
                    locations[previous.index] = Location::Result(*call);
                    locations[incoming.index] = Location::Named(*root);
                }
            }
        }
        Some(true)
    }
}

#[cfg(test)]
mod terminal_tests;
