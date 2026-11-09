//! Shared slots borrow existing owners; they never create or move their sources.
use super::{UnitDropPoint, UnitOwnerId, UnitOwnerTransportEvent};
use crate::name_resolution::UnitSymbolId;

type LoanSlots = Vec<(UnitOwnerId, UnitSymbolId, UnitOwnerId)>;

pub(super) fn consistent(events: &[UnitOwnerTransportEvent]) -> bool {
    replay(events, &[]).is_some_and(|(loans, _)| loans.is_empty())
}

// Both edges start from identical slots. Only an exactly equal completed state
// can join; formation still requires the separate planner/location gates.
fn replay(
    events: &[UnitOwnerTransportEvent],
    inherited: &[(UnitOwnerId, UnitSymbolId, UnitOwnerId)],
) -> Option<(LoanSlots, bool)> {
    let mut loans = inherited.to_vec();
    let mut retiring: Option<(UnitOwnerId, UnitDropPoint)> = None;
    for (event_index, event) in events.iter().enumerate() {
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
                _ => return None,
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
                    return None;
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
                let index = loans
                    .iter()
                    .rposition(|(formed, _, _)| formed == environment)?;
                if loans[index].1 != *source || loans[index].2 != *owner {
                    return None;
                }
                loans.remove(index);
                retiring = Some((environment.clone(), *point));
            }
            UnitOwnerTransportEvent::Drop { owner, .. } => {
                if borrowed(owner) || loans.iter().any(|(environment, _, _)| environment == owner) {
                    return None;
                }
                retiring = None;
            }
            UnitOwnerTransportEvent::Move { owner, .. } if borrowed(owner) => return None,
            UnitOwnerTransportEvent::Capture { inputs, .. }
                if inputs.iter().any(|(_, owner)| borrowed(owner)) =>
            {
                return None;
            }
            UnitOwnerTransportEvent::Swap {
                left_owner,
                right_owner,
                ..
            } if borrowed(left_owner) || borrowed(right_owner) => return None,
            UnitOwnerTransportEvent::Replace {
                previous, incoming, ..
            } if borrowed(previous) || borrowed(incoming) => return None,
            UnitOwnerTransportEvent::If {
                then_events,
                else_events,
                ..
            } => {
                let (left, left_continues) = replay(then_events, &loans)?;
                let (right, right_continues) = replay(else_events, &loans)?;
                loans = match (left_continues, right_continues) {
                    (false, false) => {
                        // A nested terminal fork ends its enclosing path too.
                        // No event may run after either already-retired alternative.
                        if event_index + 1 != events.len() || !left.is_empty() || !right.is_empty()
                        {
                            return None;
                        }
                        return Some((left, false));
                    }
                    (false, true) => right,
                    (true, false) => left,
                    (true, true) if left == right => left,
                    _ => return None,
                };
            }
            UnitOwnerTransportEvent::UnitReturn { .. } => {
                if event_index + 1 != events.len() || !loans.is_empty() || retiring.is_some() {
                    return None;
                }
                return Some((loans, false));
            }
            _ => {}
        }
    }
    retiring.is_none().then_some((loans, true))
}

#[cfg(test)]
mod tests;
