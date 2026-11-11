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
mod tests {
    use crate::ownership_checking::UnitOwnerHeaderSourceV1;
    use crate::ownership_checking::compilation_unit::formed_tests::Fixture;
    thread_local! { static SKIP: std::cell::Cell<u8> = const { std::cell::Cell::new(0) }; }
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
}
