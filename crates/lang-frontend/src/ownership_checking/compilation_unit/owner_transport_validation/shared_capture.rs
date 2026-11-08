//! Shared slots borrow existing owners; they never create or move their sources.
use super::{UnitDropPoint, UnitOwnerId, UnitOwnerTransportEvent};
use crate::name_resolution::UnitSymbolId;

pub(super) fn consistent(events: &[UnitOwnerTransportEvent]) -> bool {
    let mut loans: Vec<(UnitOwnerId, UnitSymbolId, UnitOwnerId)> = Vec::new();
    let mut retiring: Option<(UnitOwnerId, UnitDropPoint)> = None;
    for event in events {
        // Loan ends are a single contiguous retirement immediately before the
        // matching environment drop, never an independently movable permission.
        if let Some((environment, point)) = &retiring {
            match event {
                UnitOwnerTransportEvent::SharedCaptureEnd {
                    environment: actual,
                    point: at,
                    ..
                } if actual == environment && at == point => {}
                UnitOwnerTransportEvent::Drop {
                    owner, point: at, ..
                } if owner == environment && at == point => {}
                _ => return false,
            }
        }
        let borrowed = |owner: &UnitOwnerId| loans.iter().any(|(_, _, source)| source == owner);
        match event {
            UnitOwnerTransportEvent::SharedCapture {
                environment,
                inputs,
                ..
            } => {
                if loans.iter().any(|(prior, _, _)| prior == environment) {
                    return false;
                }
                loans.extend(
                    inputs
                        .iter()
                        .map(|(symbol, owner)| (environment.clone(), *symbol, owner.clone())),
                );
            }
            UnitOwnerTransportEvent::SharedCaptureEnd {
                environment,
                source,
                owner,
                point,
            } => {
                let Some(index) = loans
                    .iter()
                    .rposition(|(formed, _, _)| formed == environment)
                else {
                    return false;
                };
                if loans[index].1 != *source || loans[index].2 != *owner {
                    return false;
                }
                loans.remove(index);
                retiring = Some((environment.clone(), *point));
            }
            UnitOwnerTransportEvent::Drop { owner, .. } => {
                if borrowed(owner) || loans.iter().any(|(environment, _, _)| environment == owner) {
                    return false;
                }
                retiring = None;
            }
            UnitOwnerTransportEvent::Move { owner, .. } if borrowed(owner) => return false,
            UnitOwnerTransportEvent::Capture { inputs, .. }
                if inputs.iter().any(|(_, owner)| borrowed(owner)) =>
            {
                return false;
            }
            UnitOwnerTransportEvent::Swap {
                left_owner,
                right_owner,
                ..
            } if borrowed(left_owner) || borrowed(right_owner) => return false,
            UnitOwnerTransportEvent::Replace {
                previous, incoming, ..
            } if borrowed(previous) || borrowed(incoming) => return false,
            UnitOwnerTransportEvent::If {
                then_events,
                else_events,
                ..
            } if !loans.is_empty() || !consistent(then_events) || !consistent(else_events) => {
                // Shared environments across alternatives need a later conditional protocol.
                return false;
            }
            _ => {}
        }
    }
    loans.is_empty() && retiring.is_none()
}

#[cfg(test)]
mod tests;
