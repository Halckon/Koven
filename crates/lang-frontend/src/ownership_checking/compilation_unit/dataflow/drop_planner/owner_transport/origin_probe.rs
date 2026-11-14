//! Actual traversal events, independent of legacy UnitOwnerId state.
//! This is a source-evaluation trace, not a runtime loop or CFG certificate.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in super::super) enum Event {
    Literal(UnitExpressionId),
    BorrowRegistered(crate::ownership_checking::UnitLoanFact),
    ReceiverRegistered(crate::ownership_checking::UnitReceiverOwnershipFact),
    ReceiverRemoved(crate::ownership_checking::UnitReceiverOwnershipFact),
    UnwindBorrowRemoved {
        loan: crate::ownership_checking::UnitLoanFact,
        point: crate::ownership_checking::UnitDropPoint,
    },
    PendingBorrowRemoved {
        loan: crate::ownership_checking::UnitLoanFact,
        call: UnitExpressionId,
    },
    DirectBorrowEnded {
        loan: crate::ownership_checking::UnitLoanFact,
        point: crate::ownership_checking::UnitDropPoint,
    },
    WhileTraversal {
        control: UnitStatementId,
        role: WhileRole,
    },
    UncoveredCall(UnitExpressionId),
    Branch {
        control: UnitExpressionId,
        role: BranchRole,
    },
    Moved {
        expression: UnitExpressionId,
        source: UnitSymbolId,
    },
    Dropped(crate::ownership_checking::UnitDropFact),
    Bound(UnitSymbolId, UnitExpressionId),
    Applied {
        call: UnitExpressionId,
        root: UnitSymbolId,
        incoming: UnitExpressionId,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super) enum BranchRole {
    Enter(u8),
    Exit(u8, bool),
    JoinInputs(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super) enum WhileRole {
    Enter,
    ConditionEvaluated,
    BodyEnter,
    BodyExit(Option<bool>),
    Exit(Option<bool>),
}

impl DropPlanner<'_, '_> {
    pub(in super::super) fn origin_while(&mut self, control: UnitStatementId, role: WhileRole) {
        if let Some(probe) = &mut self.origin_probe {
            probe.push(Event::WhileTraversal { control, role });
        }
    }

    pub(in super::super) fn origin_call_gap(&mut self, expression: ExpressionId) {
        let call = self.checker.unit_expression(expression);
        if self.checker.ownership_primitives.iter().any(|plan| {
            plan.descriptor().expression() == call
                && plan.descriptor().kind() == OwnershipPrimitiveKind::Replace
                && plan.places().len() == 1
                && plan.places()[0].is_root()
        }) {
            return;
        }
        if let Some(probe) = &mut self.origin_probe {
            probe.push(Event::UncoveredCall(call));
        }
    }

    pub(in super::super) fn origin_branch(&mut self, control: ExpressionId, role: BranchRole) {
        if let Some(probe) = &mut self.origin_probe {
            probe.push(Event::Branch {
                control: self.checker.unit_expression(control),
                role,
            });
        }
    }

    pub(in super::super) fn origin_moved(
        &mut self,
        expression: ExpressionId,
        source: UnitSymbolId,
    ) {
        if let Some(probe) = &mut self.origin_probe {
            probe.push(Event::Moved {
                expression: self.checker.unit_expression(expression),
                source,
            });
        }
    }
    pub(in super::super) fn origin_dropped(&mut self, fact: PlannerDropFact) {
        if let Some(probe) = &mut self.origin_probe {
            probe.push(Event::Dropped(fact.into_unit(self.checker.source_unit)));
        }
    }

    pub(in super::super) fn origin_literal(&mut self, expression: ExpressionId) {
        if let Some(probe) = &mut self.origin_probe {
            probe.push(Event::Literal(self.checker.unit_expression(expression)));
        }
    }
    pub(in super::super) fn origin_bound(
        &mut self,
        symbol: UnitSymbolId,
        initializer: ExpressionId,
    ) {
        if let Some(probe) = &mut self.origin_probe {
            probe.push(Event::Bound(
                symbol,
                self.checker.unit_expression(initializer),
            ));
        }
    }
    pub(in super::super) fn origin_applied(
        &mut self,
        call: ExpressionId,
        updated_root: UnitSymbolId,
    ) {
        let Some(probe) = &mut self.origin_probe else {
            return;
        };
        let call = self.checker.unit_expression(call);
        let Some(plan) = self
            .checker
            .ownership_primitives
            .iter()
            .find(|plan| plan.descriptor().expression() == call)
        else {
            return;
        };
        if plan.descriptor().kind() != OwnershipPrimitiveKind::Replace {
            return;
        }
        let [place] = plan.places() else { return };
        let [_, incoming] = plan.descriptor().operands();
        if !place.is_root() || place.root() != updated_root {
            return;
        }
        probe.push(Event::Applied {
            call,
            root: place.root(),
            incoming,
        });
    }
}

/// A partition of observed events, not proof of complete source semantics or CFG.
pub(in super::super) struct IfPartition<'a> {
    pub(in super::super) arms: [&'a [Event]; 2],
    pub(in super::super) continues: [bool; 2],
}

pub(in super::super) fn partition_if(events: &[Event]) -> Option<IfPartition<'_>> {
    let Event::Branch {
        control,
        role: BranchRole::Enter(0),
    } = events.first()?
    else {
        return None;
    };
    let control = *control;
    let mut position = 1;
    let mut arms = [&events[0..0]; 2];
    let mut continues = [false; 2];
    for arm in 0..2 {
        if arm == 1 {
            if events.get(position)
                != Some(&Event::Branch {
                    control,
                    role: BranchRole::Enter(1),
                })
            {
                return None;
            }
            position += 1;
        }
        let start = position;
        loop {
            match events.get(position)? {
                Event::Branch {
                    control: actual,
                    role: BranchRole::Exit(index, normal),
                } if *actual == control && usize::from(*index) == arm => {
                    arms[arm] = &events[start..position];
                    continues[arm] = *normal;
                    position += 1;
                    break;
                }
                Event::Branch { .. }
                | Event::UncoveredCall(_)
                | Event::WhileTraversal { .. }
                | Event::BorrowRegistered(_)
                | Event::DirectBorrowEnded { .. }
                | Event::PendingBorrowRemoved { .. }
                | Event::UnwindBorrowRemoved { .. }
                | Event::ReceiverRegistered(_)
                | Event::ReceiverRemoved(_) => {
                    return None;
                }
                _ => position += 1,
            }
        }
    }
    if events.get(position)
        != Some(&Event::Branch {
            control,
            role: BranchRole::JoinInputs(continues.iter().filter(|normal| **normal).count()),
        })
        || position + 1 != events.len()
    {
        return None;
    }
    Some(IfPartition { arms, continues })
}

/// Structural attribution of one observed branch. Opaque calls remain opaque;
/// this is not full semantic coverage, dynamic paths, or SSA predecessors.
pub(in super::super) struct ObservedIf<'a> {
    pub(in super::super) prefix: &'a [Event],
    pub(in super::super) arms: [&'a [Event]; 2],
    pub(in super::super) suffix: &'a [Event],
    pub(in super::super) continuing_arms: Vec<usize>,
}

pub(in super::super) fn attribute_observed_if(events: &[Event]) -> Option<ObservedIf<'_>> {
    if events
        .iter()
        .any(|event| matches!(event, Event::WhileTraversal { .. }))
    {
        return None;
    }
    let start = events
        .iter()
        .position(|event| matches!(event, Event::Branch { .. }))?;
    let Event::Branch {
        control,
        role: BranchRole::Enter(0),
    } = events.get(start)?
    else {
        return None;
    };
    let mut position = start + 1;
    let mut arms = [&events[0..0]; 2];
    let mut continuing_arms = Vec::new();
    for (arm, contents) in arms.iter_mut().enumerate() {
        if arm == 1 {
            if events.get(position)
                != Some(&Event::Branch {
                    control: *control,
                    role: BranchRole::Enter(1),
                })
            {
                return None;
            }
            position += 1;
        }
        let begin = position;
        loop {
            match events.get(position)? {
                Event::Branch {
                    control: actual,
                    role: BranchRole::Exit(index, normal),
                } if actual == control && usize::from(*index) == arm => {
                    *contents = &events[begin..position];
                    if *normal {
                        continuing_arms.push(arm);
                    }
                    position += 1;
                    break;
                }
                Event::Branch { .. } => return None,
                _ => position += 1,
            }
        }
    }
    if events.get(position)
        != Some(&Event::Branch {
            control: *control,
            role: BranchRole::JoinInputs(continuing_arms.len()),
        })
    {
        return None;
    }
    let suffix = &events[position + 1..];
    if suffix
        .iter()
        .any(|event| matches!(event, Event::Branch { .. }))
    {
        return None;
    }
    Some(ObservedIf {
        prefix: &events[..start],
        arms,
        suffix,
        continuing_arms,
    })
}

/// One static, normally completing body traversal, not a dynamic backedge.
pub(in super::super) fn observed_normal_loop(events: &[Event]) -> Option<&[Event]> {
    let start = events
        .iter()
        .position(|event| matches!(event, Event::WhileTraversal { .. }))?;
    if events[..start]
        .iter()
        .any(|event| matches!(event, Event::Branch { .. }))
    {
        return None;
    }
    let Event::WhileTraversal {
        control,
        role: WhileRole::Enter,
    } = events.get(start)?
    else {
        return None;
    };
    let marker = |role| Event::WhileTraversal {
        control: *control,
        role,
    };
    if events.get(start + 1) != Some(&marker(WhileRole::ConditionEvaluated))
        || events.get(start + 2) != Some(&marker(WhileRole::BodyEnter))
    {
        return None;
    }
    let begin = start + 3;
    let mut end = begin;
    while let Some(event) = events.get(end) {
        match event {
            Event::WhileTraversal { .. } => break,
            Event::Branch { .. } => return None,
            _ => end += 1,
        }
    }
    if events.get(end) != Some(&marker(WhileRole::BodyExit(Some(true)))) {
        return None;
    }
    // Cleanup may follow body completion; it is not part of the body slice.
    let mut exit = end + 1;
    while let Some(event) = events.get(exit) {
        match event {
            Event::WhileTraversal { .. } => break,
            Event::Dropped(_) => exit += 1,
            _ => return None,
        }
    }
    if events.get(exit) != Some(&marker(WhileRole::Exit(Some(true))))
        || events[exit + 1..]
            .iter()
            .any(|event| matches!(event, Event::WhileTraversal { .. } | Event::Branch { .. }))
    {
        return None;
    }
    Some(&events[begin..end])
}
