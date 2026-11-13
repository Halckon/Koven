//! Actual implicit-false loop-exit cleanup, without modifying legacy owner state.
use super::*;
impl DropPlanner<'_, '_> {
    pub(in super::super::super) fn drop_root_loop_exit(
        &mut self,
        id: StatementId,
        condition: ExpressionId,
        state: &mut ValueState,
    ) {
        let start = self.facts.len();
        self.drop_loop_exit(id, state);
        let control = UnitStatementId::new(self.checker.source_unit, id);
        let condition_id = self.checker.unit_expression(condition);
        let false_possible = self.checker.ordinary_condition_outcomes(condition).1;
        self.owner_loop.parent_false_exit(
            control,
            condition_id,
            state,
            &self.facts[start..],
            false_possible,
        );
        self.owner_loop.nested_false_exit(
            control,
            condition_id,
            state,
            &self.facts[start..],
            false_possible,
        );
        if let Some(body) = self.owner_loop.bridge_body_mut(control) {
            body.false_exit(
                control,
                condition_id,
                state,
                &self.facts[start..],
                false_possible,
            );
            body.begin_suffix(self.scope_depth);
        }
        let Some((_, evidence)) = self
            .owner_loop
            .root_entries
            .iter_mut()
            .find(|(key, _)| *key == control)
        else {
            return;
        };
        evidence.exit_drops = None;
        evidence.exit_partition = None;
        evidence.suffix = None;
        // Body Replace intentionally taints legacy preservation. Keep that sticky bit;
        // the exact normal transaction proves these writes, while after_condition
        // records the uncontaminated pure Name condition boundary.
        if !self.checker.ordinary_condition_outcomes(condition).1
            || !evidence.after_condition
            || evidence.normal_equation.is_none()
            || evidence.completed_condition != Some(self.checker.unit_expression(condition))
        {
            return;
        }
        let drops = &self.facts[start..];
        let roots = evidence
            .roots
            .iter()
            .map(|(root, _)| *root)
            .collect::<Vec<_>>();
        let Some((live, drops)) = actual_partition(control, &roots, state, drops) else {
            return;
        };
        evidence.exit_partition = Some((live.clone(), drops.clone()));
        if !live.is_empty() {
            evidence.suffix = Some(super::root_suffix::RootSuffix::new(
                live.clone(),
                self.scope_depth,
            ));
        }
        // Keep the older all-dead capability strictly unchanged.
        if !live.is_empty() {
            return;
        }

        if drops.len() != evidence.roots.len()
            || evidence.roots.iter().any(|(root, _)| {
                drops
                    .iter()
                    .filter(|fact| {
                        fact.target() == crate::ownership_checking::UnitDropTarget::Named(*root)
                            && fact.point()
                                == crate::ownership_checking::UnitDropPoint::LoopExit(control)
                    })
                    .count()
                    != 1
            })
        {
            return;
        }
        evidence.exit_drops = Some(drops);
    }
}

// Read-only common verifier over the post-drop state and only freshly appended facts.
pub(in super::super) fn actual_partition(
    control: UnitStatementId,
    roots: &[UnitSymbolId],
    state: &ValueState,
    drops: &[PlannerDropFact],
) -> Option<(
    Vec<UnitSymbolId>,
    Vec<crate::ownership_checking::UnitDropFact>,
)> {
    let values = boundary_values(state)?;
    let live: Vec<_> = values.iter().map(|value| value.symbol).collect();
    if live
        .iter()
        .enumerate()
        .any(|(i, root)| live[..i].contains(root) || !roots.contains(root))
        || live.len() + drops.len() != roots.len()
        || roots.iter().any(|root| {
            usize::from(live.contains(root))
                + drops
                    .iter()
                    .filter(|fact| {
                        fact.target() == PlannerDropTarget::Named(*root)
                            && fact.point() == PlannerDropPoint::LoopExit(control.statement())
                    })
                    .count()
                != 1
        })
    {
        return None;
    }
    Some((
        live,
        drops
            .iter()
            .map(|fact| fact.into_unit(control.source_unit()))
            .collect(),
    ))
}
