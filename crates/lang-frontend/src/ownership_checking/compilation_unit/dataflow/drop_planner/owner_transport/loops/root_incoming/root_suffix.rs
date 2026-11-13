//! Exact immediate Name -> val -> drop suffix; never use legacy owner IDs as phi values.
use super::*;
type Receipt = (UnitSymbolId, crate::ownership_checking::UnitDropFact);
pub(super) struct RootSuffix {
    scope_depth: usize,
    remaining: Vec<UnitSymbolId>,
    pending: Option<Pending>,
    transparent: Option<StatementId>,
    receipts: Vec<Receipt>,
    valid: bool,
}
struct Pending {
    statement: StatementId,
    expression: ExpressionId,
    root: UnitSymbolId,
    binding: UnitSymbolId,
    moved: bool,
    bound: bool,
    dropped: Option<crate::ownership_checking::UnitDropFact>,
}
impl RootSuffix {
    pub(super) fn new(remaining: Vec<UnitSymbolId>, scope_depth: usize) -> Self {
        Self {
            scope_depth,
            remaining,
            pending: None,
            transparent: None,
            receipts: Vec::new(),
            valid: true,
        }
    }
    pub(super) fn completed(self) -> Option<Vec<Receipt>> {
        (self.valid
            && self.remaining.is_empty()
            && self.pending.is_none()
            && self.transparent.is_none())
        .then_some(self.receipts)
    }
    pub(super) fn completed_snapshot(&self) -> Option<Vec<Receipt>> {
        (self.valid
            && self.remaining.is_empty()
            && self.pending.is_none()
            && self.transparent.is_none())
        .then(|| self.receipts.clone())
    }
    fn matches_resources(&self, state: &ValueState) -> bool {
        boundary_values(state).is_some_and(|values| {
            values.len() == self.remaining.len()
                && self
                    .remaining
                    .iter()
                    .all(|root| values.iter().filter(|v| v.symbol == *root).count() == 1)
        })
    }
    fn active(&self) -> bool {
        self.valid && !self.remaining.is_empty()
    }
}
fn transparent_literal(checker: &Checker<'_>, id: StatementId) -> bool {
    let ast = checker.parsed.ast();
    let Ok(statement) = ast.statements().get(id) else {
        return false;
    };
    let Statement::LocalVariable { declaration } = statement.payload() else {
        return false;
    };
    let Ok(item) = ast.items().get(*declaration) else {
        return false;
    };
    let Item::Variable {
        name,
        kind: VariableKind::Val,
        initializer,
        ..
    } = item.payload()
    else {
        return false;
    };
    checker
        .marker_symbol(*name)
        .is_some_and(|symbol| !checker.is_move_only_variable(*symbol))
        && ast
            .expressions()
            .get(*initializer)
            .is_ok_and(|node| matches!(node.payload(), Expression::Literal(_)))
}
impl DropPlanner<'_, '_> {
    pub(in super::super::super::super) fn begin_root_suffix_statement(
        &mut self,
        id: StatementId,
        state: &ValueState,
    ) {
        #[cfg(test)]
        let altered = tests::resource_mismatch(state, 9, transparent_literal(self.checker, id));
        #[cfg(test)]
        let state = altered.as_ref().unwrap_or(state);
        for (_, evidence) in &mut self.owner_loop.root_entries {
            let Some(suffix) = evidence.suffix.as_mut().filter(|suffix| suffix.active()) else {
                continue;
            };
            if suffix.transparent.is_some() {
                suffix.valid = false;
                continue;
            }
            let next = (|| {
                if self.scope_depth != suffix.scope_depth
                    || suffix.pending.is_some()
                    || self.owner_controls.current().is_some()
                    || !self.owner_loop.root_branch_arms.is_empty()
                {
                    return None;
                }
                if transparent_literal(self.checker, id) && suffix.matches_resources(state) {
                    suffix.transparent = Some(id);
                    return None;
                }
                let (binding, expression) =
                    replace_break::variable(self.checker, id, VariableKind::Val)?;
                let node = self
                    .checker
                    .parsed
                    .ast()
                    .expressions()
                    .get(expression)
                    .ok()?;
                if !matches!(node.payload(), Expression::Name)
                    || self.liveness.statement_after[id.index()].contains(&binding)
                {
                    return None;
                }
                let root = self.checker.reference_symbol(node.span())?;
                if !suffix.remaining.contains(&root)
                    || suffix.remaining.contains(&binding)
                    || !boundary_values(state).is_some_and(|values| {
                        values.len() == suffix.remaining.len()
                            && suffix.remaining.iter().all(|root| {
                                values.iter().filter(|v| v.symbol == *root).count() == 1
                            })
                    })
                {
                    return None;
                }
                Some(Pending {
                    statement: id,
                    expression,
                    root,
                    binding,
                    moved: false,
                    bound: false,
                    dropped: None,
                })
            })();
            suffix.valid = next.is_some() || suffix.transparent.is_some();
            suffix.pending = next;
        }
        #[cfg(test)]
        self.inject_transparent_event(id, state);
    }
    pub(in super::super::super::super) fn move_root_suffix(
        &mut self,
        expression: ExpressionId,
        value: Option<&OwnedValue>,
    ) {
        #[cfg(test)]
        if tests::skip(1) {
            return;
        }
        for (_, evidence) in &mut self.owner_loop.root_entries {
            let Some(suffix) = evidence.suffix.as_mut().filter(|suffix| suffix.active()) else {
                continue;
            };
            match &mut suffix.pending {
                Some(p)
                    if p.expression == expression
                        && value.is_some_and(|v| v.symbol == p.root)
                        && !p.moved =>
                {
                    p.moved = true
                }
                _ => suffix.valid = false,
            }
        }
    }
    pub(in super::super::super::super) fn bind_root_suffix(
        &mut self,
        symbol: UnitSymbolId,
        expression: ExpressionId,
    ) {
        #[cfg(test)]
        if tests::skip(2) {
            return;
        }
        for (_, evidence) in &mut self.owner_loop.root_entries {
            let Some(suffix) = evidence.suffix.as_mut().filter(|suffix| suffix.active()) else {
                continue;
            };
            match &mut suffix.pending {
                Some(p)
                    if p.expression == expression && p.binding == symbol && p.moved && !p.bound =>
                {
                    p.bound = true
                }
                _ => suffix.valid = false,
            }
        }
    }
    pub(in super::super::super::super) fn drop_root_suffix(&mut self, fact: PlannerDropFact) {
        #[cfg(test)]
        if tests::skip(3) {
            return;
        }
        for (_, evidence) in &mut self.owner_loop.root_entries {
            let Some(suffix) = evidence.suffix.as_mut().filter(|suffix| suffix.active()) else {
                continue;
            };
            match &mut suffix.pending {
                Some(p)
                    if p.bound
                        && p.dropped.is_none()
                        && fact.target() == PlannerDropTarget::Named(p.binding)
                        && fact.point() == PlannerDropPoint::AfterStatement(p.statement) =>
                {
                    p.dropped = Some(fact.into_unit(self.checker.source_unit))
                }
                _ => suffix.valid = false,
            }
        }
    }
    pub(in super::super::super::super) fn finish_root_suffix_statement(
        &mut self,
        id: StatementId,
        state: &ValueState,
    ) {
        #[cfg(test)]
        if tests::skip(4) {
            return;
        }
        #[cfg(test)]
        let altered = tests::resource_mismatch(
            state,
            10,
            self.owner_loop
                .root_entries
                .iter()
                .any(|(_, e)| e.suffix.as_ref().is_some_and(|s| s.transparent.is_some())),
        );
        #[cfg(test)]
        let state = altered.as_ref().unwrap_or(state);
        for (_, evidence) in &mut self.owner_loop.root_entries {
            let Some(suffix) = evidence.suffix.as_mut().filter(|suffix| suffix.active()) else {
                continue;
            };
            #[cfg(test)]
            if suffix.transparent.is_some() && tests::skip(5) {
                continue;
            }
            if let Some(statement) = suffix.transparent.take() {
                suffix.valid = statement == id && suffix.matches_resources(state);
                continue;
            }
            let complete = (|| {
                let p = suffix.pending.take()?;
                if p.statement != id || !p.moved || !p.bound {
                    return None;
                }
                let drop = p.dropped?;
                suffix.remaining.retain(|root| *root != p.root);
                let values = boundary_values(state)?;
                if values.len() != suffix.remaining.len()
                    || suffix
                        .remaining
                        .iter()
                        .any(|root| values.iter().filter(|v| v.symbol == *root).count() != 1)
                {
                    return None;
                }
                suffix.receipts.push((p.root, drop));
                Some(())
            })();
            if complete.is_none() {
                suffix.valid = false;
            }
        }
    }
}

#[cfg(test)]
impl DropPlanner<'_, '_> {
    fn inject_transparent_event(&mut self, id: StatementId, state: &ValueState) {
        // Failed suffixes retain their transparent marker. Do not inject again at
        // subsequent statements after the intended event has invalidated the suffix.
        if !self.owner_loop.root_entries.iter().any(|(_, e)| {
            e.suffix
                .as_ref()
                .is_some_and(|s| s.active() && s.transparent == Some(id))
        }) {
            return;
        }
        let Some(value) = boundary_values(state)
            .and_then(|values| values.first())
            .cloned()
        else {
            return;
        };
        let Statement::LocalVariable { declaration } = self
            .checker
            .parsed
            .ast()
            .statements()
            .get(id)
            .unwrap()
            .payload()
        else {
            return;
        };
        let Item::Variable { initializer, .. } = self
            .checker
            .parsed
            .ast()
            .items()
            .get(*declaration)
            .unwrap()
            .payload()
        else {
            return;
        };
        let expression = *initializer;
        // Deliberately impossible owner events at a completed scalar-literal step.
        if tests::skip(6) {
            tests::hit();
            self.move_root_suffix(expression, Some(&value));
        }
        if tests::skip(7) {
            tests::hit();
            self.bind_root_suffix(value.symbol, expression);
        }
        if tests::skip(8) {
            tests::hit();
            self.drop_root_suffix(PlannerDropFact::new(
                PlannerDropPoint::AfterStatement(id),
                PlannerDropTarget::Named(value.symbol),
                value.origin,
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ownership_checking::UnitOwnerHeaderSourceV1;
    use crate::ownership_checking::compilation_unit::formed_tests::Fixture;
    thread_local! { static SKIP: std::cell::Cell<u8> = const { std::cell::Cell::new(0) }; }
    thread_local! { static HITS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }
    pub(super) fn hit() {
        HITS.set(HITS.get() + 1);
    }
    pub(super) fn resource_mismatch(
        state: &ValueState,
        phase: u8,
        enabled: bool,
    ) -> Option<ValueState> {
        if !enabled || !skip(phase) {
            return None;
        }
        let symbol = boundary_values(state)?.first()?.symbol;
        let mut altered = state.clone();
        altered.remove_value(symbol)?;
        hit();
        Some(altered)
    }
    pub(super) fn skip(phase: u8) -> bool {
        SKIP.get() == phase
    }
    struct Restore(u8);
    impl Drop for Restore {
        fn drop(&mut self) {
            SKIP.set(self.0);
        }
    }
    #[test]
    fn ci_only_suffix_requires_each_actual_event() {
        let fixture = Fixture::new(&[(
            "suffix-hooks.ko",
            "fun run(flag: Boolean) { var a = \"a\"; var b = \"b\"; while(flag) { val x = replace(&a, \"x\"); val y = replace(&b, \"y\") }; val x = a; val y = b }",
        )]);
        for phase in [0, 1, 2, 3, 4] {
            let restore = Restore(SKIP.replace(phase));
            let owned = fixture.shared();
            drop(restore);
            let boundary = owned.owner_observations()[0]
                .multi_root_boundary_v1()
                .unwrap();
            let incoming: Vec<_> = boundary
                .slots()
                .iter()
                .map(|slot| {
                    (
                        slot.root(),
                        UnitOwnerHeaderSourceV1::Backedge(slot.first_body_output()),
                    )
                })
                .collect();
            assert_eq!(
                boundary
                    .condition_false_suffix_retirements(boundary.statement(), &incoming)
                    .is_some(),
                phase == 0
            );
        }
    }
    #[test]
    fn ci_only_suffix_literal_requires_actual_completion() {
        let fixture = Fixture::new(&[(
            "suffix-literal-hook.ko",
            "fun run(flag: Boolean) { var a = \"a\"; var b = \"b\"; while(flag) { val x = replace(&a, \"x\"); val y = replace(&b, \"y\") }; val n = 1; val x = a; val y = b }",
        )]);
        for phase in [0, 5] {
            let restore = Restore(SKIP.replace(phase));
            let owned = fixture.shared();
            drop(restore);
            let boundary = owned.owner_observations()[0]
                .multi_root_boundary_v1()
                .unwrap();
            let incoming: Vec<_> = boundary
                .slots()
                .iter()
                .map(|slot| {
                    (
                        slot.root(),
                        UnitOwnerHeaderSourceV1::Backedge(slot.first_body_output()),
                    )
                })
                .collect();
            assert_eq!(
                boundary
                    .condition_false_suffix_retirements(boundary.statement(), &incoming)
                    .is_some(),
                phase == 0
            );
        }
    }
    #[test]
    fn ci_only_transparent_suffix_rejects_owner_events_and_resource_changes() {
        let fixture = Fixture::new(&[(
            "suffix-transparent-faults.ko",
            "fun run(flag: Boolean) { var a = \"a\"; var b = \"b\"; while(flag) { val x = replace(&a, \"x\"); val y = replace(&b, \"y\") }; val n = 1; val x = a; val y = b }",
        )]);
        for phase in [0, 6, 7, 8, 9, 10] {
            HITS.set(0);
            let restore = Restore(SKIP.replace(phase));
            let owned = fixture.shared();
            drop(restore);
            assert_eq!(
                HITS.get(),
                usize::from(phase != 0),
                "fault was not reached exactly once: {phase}"
            );
            let boundary = owned.owner_observations()[0]
                .multi_root_boundary_v1()
                .unwrap();
            let incoming: Vec<_> = boundary
                .slots()
                .iter()
                .map(|slot| {
                    (
                        slot.root(),
                        UnitOwnerHeaderSourceV1::Backedge(slot.first_body_output()),
                    )
                })
                .collect();
            assert_eq!(
                boundary
                    .condition_false_suffix_retirements(boundary.statement(), &incoming)
                    .is_some(),
                phase == 0
            );
        }
    }
}
