//! Test-only traversal snapshots. No owner token invention or Carry permission.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super) enum Point {
    Enter(UnitStatementId),
    Leave(UnitStatementId),
    Primitive(UnitExpressionId),
}

#[derive(Debug)]
pub(in super::super) struct Visit {
    pub(in super::super) point: Point,
    pub(in super::super) control: Option<UnitStatementId>,
    pub(in super::super) roots: Vec<(UnitSymbolId, Option<UnitOwnerId>)>,
    pub(in super::super) observation_active: bool,
}

impl DropPlanner<'_, '_> {
    pub(in super::super) fn carry_probe(&mut self, point: Point, state: &ValueState) {
        if self.carry_probe.is_none() {
            return;
        }
        let visit = Visit {
            point,
            control: self.owner_controls.current(),
            roots: state
                .values
                .iter()
                .map(|value| (value.symbol, value.owner.clone()))
                .collect(),
            observation_active: self.owner_observation_active(),
        };
        self.carry_probe.as_mut().unwrap().push(visit);
    }
}
