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
        let Some(values) = boundary_values(state) else {
            return;
        };
        let live: Vec<_> = values.iter().map(|value| value.symbol).collect();
        if live.iter().enumerate().any(|(i, root)| {
            live[..i].contains(root) || !evidence.roots.iter().any(|(known, _)| known == root)
        }) || live.len() + drops.len() != evidence.roots.len()
            || evidence.roots.iter().any(|(root, _)| {
                usize::from(live.contains(root))
                    + drops
                        .iter()
                        .filter(|fact| {
                            fact.target() == PlannerDropTarget::Named(*root)
                                && fact.point() == PlannerDropPoint::LoopExit(id)
                        })
                        .count()
                    != 1
            })
        {
            return;
        }
        evidence.exit_partition = Some((
            live.clone(),
            drops
                .iter()
                .map(|fact| fact.into_unit(control.source_unit()))
                .collect(),
        ));
        // Keep the older all-dead capability strictly unchanged.
        if !live.is_empty() {
            return;
        }

        if drops.len() != evidence.roots.len()
            || evidence.roots.iter().any(|(root, _)| {
                drops
                    .iter()
                    .filter(|fact| {
                        fact.target() == PlannerDropTarget::Named(*root)
                            && fact.point() == PlannerDropPoint::LoopExit(id)
                    })
                    .count()
                    != 1
            })
        {
            return;
        }
        evidence.exit_drops = Some(
            drops
                .iter()
                .map(|fact| fact.into_unit(control.source_unit()))
                .collect(),
        );
    }
}
