//! One immediately following declaration, driven by real completion events.
use super::*;
use crate::ownership_checking::{
    UnitDropFact, UnitDropPoint, UnitDropTarget, UnitOwnerNestedResumeWriteV1,
};
pub(in super::super) struct Resume {
    parent: UnitStatementId,
    live: Vec<UnitSymbolId>,
    started: bool,
    phase: u8,
    expected: Option<(
        UnitStatementId,
        UnitExpressionId,
        UnitSymbolId,
        UnitExpressionId,
        UnitSymbolId,
    )>,
    drop: Option<UnitDropFact>,
    completed: Option<UnitOwnerNestedResumeWriteV1>,
}
impl Resume {
    pub(in super::super) fn new(parent: UnitStatementId, live: Vec<UnitSymbolId>) -> Self {
        Self {
            parent,
            live,
            started: false,
            phase: 0,
            expected: None,
            drop: None,
            completed: None,
        }
    }
    fn revoke_repeat(&mut self, matches: bool) {
        if matches && self.completed.is_some() {
            self.completed = None;
            self.phase = 255;
        }
    }
    fn clean(&self, state: &ValueState) -> bool {
        boundary_values(state).is_some_and(|v| {
            v.len() == self.live.len()
                && self
                    .live
                    .iter()
                    .all(|r| v.iter().filter(|x| x.symbol == *r).count() == 1)
        })
    }
    pub(in super::super) fn begin(
        &mut self,
        checker: &Checker<'_>,
        id: UnitStatementId,
        parent: Option<UnitStatementId>,
        state: &ValueState,
        branching: bool,
    ) {
        self.revoke_repeat(self.expected.is_some_and(|e| e.0 == id));
        if self.completed.is_some() {
            return;
        }
        if self.started {
            self.phase = 255;
            return;
        }
        self.started = true;
        self.phase = 255;
        if parent != Some(self.parent) || branching || !self.clean(state) {
            return;
        }
        let Some((binding, call)) =
            replace_break::variable(checker, id.statement(), VariableKind::Val)
        else {
            return;
        };
        let call = checker.unit_expression(call);
        let Some(p) = checker
            .ownership_primitives
            .iter()
            .find(|p| p.descriptor().expression() == call)
        else {
            return;
        };
        if p.descriptor().kind() != OwnershipPrimitiveKind::Replace
            || p.places().len() != 1
            || !p.places()[0].is_root()
            || p.descriptor().operands().len() != 2
        {
            return;
        }
        let root = p.places()[0].root();
        let producer = p.descriptor().operands()[1];
        if !self.live.contains(&root)
            || self.live.contains(&binding)
            || !replace_break::literal(checker, producer.expression())
        {
            return;
        }
        self.expected = Some((id, call, root, producer, binding));
        self.phase = 0;
    }
    pub(in super::super) fn literal(&mut self, id: UnitExpressionId) {
        self.revoke_repeat(self.expected.is_some_and(|e| e.3 == id));
        #[cfg(test)]
        if self.expected.is_some() && fault(self.parent, 1) {
            return;
        }
        if self.completed.is_none() {
            self.phase = if self.phase == 0 && self.expected.is_some_and(|e| e.3 == id) {
                1
            } else {
                255
            };
        }
    }
    pub(in super::super) fn replace(
        &mut self,
        call: UnitExpressionId,
        root: UnitSymbolId,
        producer: UnitExpressionId,
    ) {
        self.revoke_repeat(self.expected.is_some_and(|e| e.1 == call));
        #[cfg(test)]
        if self.expected.is_some() && fault(self.parent, 2) {
            return;
        }
        if self.completed.is_none() {
            self.phase = if self.phase == 1
                && self
                    .expected
                    .is_some_and(|e| (e.1, e.2, e.3) == (call, root, producer))
            {
                2
            } else {
                255
            };
        }
    }
    pub(in super::super) fn bind(&mut self, call: UnitExpressionId, binding: UnitSymbolId) {
        self.revoke_repeat(self.expected.is_some_and(|e| e.1 == call || e.4 == binding));
        #[cfg(test)]
        if self.expected.is_some() && fault(self.parent, 3) {
            return;
        }
        if self.completed.is_none() {
            self.phase = if self.phase == 2
                && self.expected.is_some_and(|e| (e.1, e.4) == (call, binding))
            {
                3
            } else {
                255
            };
        }
    }
    pub(in super::super) fn drop_result(&mut self, fact: UnitDropFact) {
        self.revoke_repeat(self.drop == Some(fact));
        #[cfg(test)]
        if self.expected.is_some() && fault(self.parent, 4) {
            return;
        }
        if self.completed.is_none() {
            if self.phase == 3
                && self.expected.is_some_and(|e| {
                    fact.target() == UnitDropTarget::Named(e.4)
                        && fact.point() == UnitDropPoint::AfterStatement(e.0)
                })
            {
                self.drop = Some(fact);
                self.phase = 4;
            } else {
                self.phase = 255;
            }
        }
    }
    pub(in super::super) fn finish(&mut self, id: UnitStatementId, state: &ValueState) {
        self.revoke_repeat(self.expected.is_some_and(|e| e.0 == id));
        #[cfg(test)]
        if self.expected.is_some() && fault(self.parent, 5) {
            return;
        }

        if self.completed.is_some() {
            return;
        }
        if self.phase != 4 || !self.clean(state) {
            self.phase = 255;
            return;
        }
        let Some((statement, call, root, producer, binding)) = self.expected.filter(|e| e.0 == id)
        else {
            self.phase = 255;
            return;
        };
        if let Some(drop) = self.drop {
            self.completed = Some(UnitOwnerNestedResumeWriteV1 {
                statement,
                call,
                root,
                producer,
                binding,
                drop,
            });
            #[cfg(test)]
            if fault(self.parent, 6) {
                self.replace(call, root, producer);
            }
        }
    }
    pub(in super::super) fn result(&self) -> Option<UnitOwnerNestedResumeWriteV1> {
        self.completed.clone()
    }
}
impl DropPlanner<'_, '_> {
    pub(in super::super::super::super) fn begin_nested_resume_statement(
        &mut self,
        id: StatementId,
        state: &ValueState,
    ) {
        let parent = self.owner_controls.current();
        let branching =
            self.owner_loop.root_branches_invalid || !self.owner_loop.root_branch_arms.is_empty();
        if let Some(r) = self
            .owner_loop
            .nested
            .as_mut()
            .and_then(|n| n.resume.as_mut())
        {
            r.begin(
                self.checker,
                UnitStatementId::new(self.checker.source_unit, id),
                parent,
                state,
                branching,
            );
        }
    }
    pub(in super::super::super::super) fn finish_nested_resume_statement(
        &mut self,
        id: StatementId,
        state: &ValueState,
    ) {
        if let Some(r) = self
            .owner_loop
            .nested
            .as_mut()
            .and_then(|n| n.resume.as_mut())
        {
            r.finish(UnitStatementId::new(self.checker.source_unit, id), state);
        }
    }
}
impl ControlTransactions {
    pub(in super::super) fn nested_resume_mut(
        &mut self,
        control: UnitStatementId,
    ) -> Option<&mut Resume> {
        self.nested
            .as_mut()
            .filter(|n| n.valid && n.parent == control && n.resume_roots.is_some())
            .and_then(|n| n.resume.as_mut())
    }
}

#[cfg(test)]
thread_local! { pub(super) static FAULT: std::cell::Cell<Option<(UnitStatementId,u8)>> = const { std::cell::Cell::new(None) }; pub(super) static HITS:std::cell::Cell<usize> = const {std::cell::Cell::new(0)}; }
#[cfg(test)]
fn fault(parent: UnitStatementId, phase: u8) -> bool {
    if FAULT.get() == Some((parent, phase)) {
        HITS.set(HITS.get() + 1);
        true
    } else {
        false
    }
}
