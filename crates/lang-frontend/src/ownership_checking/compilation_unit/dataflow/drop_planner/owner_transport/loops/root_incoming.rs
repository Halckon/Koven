//! Actual root entry capture, distinct from a backedge/header equation.
use super::*;
pub(super) type RootRetirements = Vec<(
    UnitSymbolId,
    UnitSymbolId,
    crate::ownership_checking::UnitDropFact,
)>;
pub(super) type RootIncoming = Vec<(UnitSymbolId, UnitOwnerId)>;
pub(super) struct RootEvidence {
    pub(super) roots: RootIncoming,
    pub(super) completed_strings: Vec<(UnitExpressionId, Vec<(UnitExpressionId, usize)>)>,
    pub(super) writes: Vec<RootWriteReceipt>,
    normal_retirements: Option<RootRetirements>,
    normal_write_vector: Option<Vec<(UnitSymbolId, UnitExpressionId)>>,
    after_condition: bool,
    normal_backedge: bool,
    tainted: bool,
}
#[cfg(test)]
impl RootEvidence {
    pub(super) fn fault_fixture(roots: RootIncoming, writes: Vec<RootWriteReceipt>) -> Self {
        Self {
            roots,
            writes,
            completed_strings: Vec::new(),
            normal_retirements: None,
            normal_write_vector: None,
            after_condition: true,
            normal_backedge: false,
            tainted: false,
        }
    }
}
impl ControlTransactions {
    pub(in super::super::super) fn invalidate_root_preservation(&mut self) {
        for (_, evidence) in &mut self.root_entries {
            evidence.tainted = true;
            evidence.normal_backedge = false;
            evidence.normal_write_vector = None;
            evidence.normal_retirements = None;
        }
    }

    #[cfg(test)]
    pub(super) fn root_retirements(&self, control: UnitStatementId) -> Option<RootRetirements> {
        self.root_entries
            .iter()
            .find(|(id, _)| *id == control)
            .and_then(|(_, evidence)| evidence.normal_retirements.clone())
    }
    #[cfg(test)]
    pub(super) fn root_normal_writes(
        &self,
        control: UnitStatementId,
    ) -> Option<Vec<(UnitSymbolId, UnitExpressionId)>> {
        self.root_entries
            .iter()
            .find(|(id, _)| *id == control)
            .and_then(|(_, evidence)| evidence.normal_write_vector.clone())
    }
    #[cfg(test)]
    pub(super) fn root_stages(&self, control: UnitStatementId) -> (bool, bool) {
        self.root_entries
            .iter()
            .find(|(key, _)| *key == control)
            .map_or((false, false), |(_, evidence)| {
                (evidence.after_condition, evidence.normal_backedge)
            })
    }
    pub(super) fn record_root_condition(&mut self, control: UnitStatementId, state: &ValueState) {
        if let Some((_, evidence)) = self
            .root_entries
            .iter_mut()
            .find(|(key, _)| *key == control)
        {
            evidence.after_condition =
                !evidence.tainted && preserves(control, state, &evidence.roots);
            evidence.normal_backedge = false;
            evidence.normal_write_vector = None;
            evidence.normal_retirements = None;
        }
    }
    pub(super) fn record_root_fallthrough(
        &mut self,
        control: UnitStatementId,
        state: &ValueState,
        normal: bool,
    ) {
        if let Some((_, evidence)) = self
            .root_entries
            .iter_mut()
            .find(|(key, _)| *key == control)
        {
            evidence.normal_write_vector = None;
            evidence.normal_retirements = None;
            evidence.normal_backedge = normal
                && evidence.after_condition
                && !evidence.tainted
                && preserves(control, state, &evidence.roots);
        }
    }

    pub(super) fn record_root_normal_writes(
        &mut self,
        checker: &Checker<'_>,
        control: UnitStatementId,
        state: &ValueState,
        normal: bool,
        facts: &[PlannerDropFact],
    ) {
        if let Some((_, evidence)) = self.root_entries.iter_mut().find(|(id, _)| *id == control) {
            evidence.normal_write_vector = if normal && evidence.after_condition {
                completed_root_writes(checker, control, state, evidence)
            } else {
                None
            };
            evidence.normal_retirements = evidence
                .normal_write_vector
                .as_ref()
                .and_then(|_| completed_result_cleanup(checker, control, state, evidence, facts));
        }
    }

    pub(in super::super::super) fn record_root_entry(
        &mut self,
        control: UnitStatementId,
        state: &ValueState,
        observation: Option<&UnitOwnerObservation>,
    ) {
        // Never retain a stale partial capture if this control cannot be recaptured.
        self.root_entries.retain(|(key, _)| *key != control);
        if self.root_branches_invalid {
            return;
        }
        let Some(roots) = capture(control, state, observation) else {
            return;
        };
        self.root_entries.push((
            control,
            RootEvidence {
                roots,
                completed_strings: Vec::new(),
                writes: Vec::new(),
                normal_retirements: None,
                normal_write_vector: None,
                after_condition: false,
                normal_backedge: false,
                tainted: false,
            },
        ));
    }
    #[cfg(test)]
    pub(super) fn root_entry(
        &self,
        control: UnitStatementId,
    ) -> Option<&[(UnitSymbolId, UnitOwnerId)]> {
        self.root_entries
            .iter()
            .find(|(key, _)| *key == control)
            .map(|(_, evidence)| evidence.roots.as_slice())
    }
}
fn capture(
    control: UnitStatementId,
    state: &ValueState,
    observation: Option<&UnitOwnerObservation>,
) -> Option<RootIncoming> {
    let observation = observation?;
    if observation.status != UnitOwnerObservationStatus::Complete {
        return None;
    }
    let UnitOwnerCallable::Function(item) = observation.callable else {
        return None;
    };
    if item.source_unit() != control.source_unit() {
        return None;
    }
    let frame = root_frame::ControlRootFrame::capture(control, state)?;
    let values = frame.roots_for(control)?;
    if values.is_empty() {
        return None;
    }
    values
        .iter()
        .map(|value| {
            let owner = value.owner.as_ref()?;
            if owner.callable != observation.callable
                || !std::sync::Arc::ptr_eq(&owner.identity, &observation.identity)
                || observation.definition(owner.clone()).is_none()
            {
                return None;
            }
            Some((value.symbol, owner.clone()))
        })
        .collect()
}

fn preserves(control: UnitStatementId, state: &ValueState, roots: &RootIncoming) -> bool {
    let Some(frame) = root_frame::ControlRootFrame::capture(control, state) else {
        return false;
    };
    let Some(values) = frame.roots_for(control) else {
        return false;
    };
    values.len() == roots.len()
        && values.iter().zip(roots).all(|(value, (root, owner))| {
            value.symbol == *root && value.owner.as_ref() == Some(owner)
        })
}

// A bounded normal-body producer vector, never a sealed header or owner equation.
fn completed_root_writes(
    checker: &Checker<'_>,
    control: UnitStatementId,
    state: &ValueState,
    evidence: &RootEvidence,
) -> Option<Vec<(UnitSymbolId, UnitExpressionId)>> {
    let ast = checker.parsed.ast();
    let Statement::While {
        condition, body, ..
    } = ast.statements().get(control.statement()).ok()?.payload()
    else {
        return None;
    };
    if !matches!(
        ast.expressions().get(*condition).ok()?.payload(),
        Expression::Name
    ) || !exact_type(checker, *condition, BuiltinType::Boolean)
        || evidence.roots.len() < 2
    {
        return None;
    }
    let Statement::Block { elements } = ast.statements().get(*body).ok()?.payload() else {
        return None;
    };
    if elements.len() != evidence.roots.len() || evidence.writes.len() != elements.len() {
        return None;
    }
    for (statement, receipt) in elements.iter().zip(&evidence.writes) {
        let (_, initializer) = replace_break::variable(checker, *statement, VariableKind::Val)?;
        if receipt.control != control || receipt.call != checker.unit_expression(initializer) {
            return None;
        }
    }
    evidence
        .roots
        .iter()
        .map(|(root, _)| {
            state.position(*root)?;
            let mut writes = evidence
                .writes
                .iter()
                .filter(|receipt| receipt.root == *root);
            let receipt = writes.next()?;
            if writes.next().is_some() {
                return None;
            }
            Some((*root, receipt.producer))
        })
        .collect()
}

// Evidence about actual returned-value bindings, not prior-generation owner identities.
fn completed_result_cleanup(
    checker: &Checker<'_>,
    control: UnitStatementId,
    state: &ValueState,
    evidence: &RootEvidence,
    facts: &[PlannerDropFact],
) -> Option<RootRetirements> {
    let ast = checker.parsed.ast();
    let Statement::While { body, .. } = ast.statements().get(control.statement()).ok()?.payload()
    else {
        return None;
    };
    let Statement::Block { elements } = ast.statements().get(*body).ok()?.payload() else {
        return None;
    };
    if elements.len() != evidence.writes.len() || elements.len() != evidence.roots.len() {
        return None;
    }
    elements
        .iter()
        .zip(&evidence.writes)
        .map(|(statement, receipt)| {
            let (binding, _) = replace_break::variable(checker, *statement, VariableKind::Val)?;
            if state.position(binding).is_some() {
                return None;
            }
            let mut drops = facts
                .iter()
                .filter(|fact| fact.target() == PlannerDropTarget::Named(binding));
            let fact = *drops.next()?;
            if drops.next().is_some()
                || fact.point() != PlannerDropPoint::AfterStatement(*statement)
            {
                return None;
            }
            Some((receipt.root, binding, fact.into_unit(control.source_unit())))
        })
        .collect()
}

#[cfg(test)]
mod cleanup_fault_tests {
    use super::super::super::tests::compare_and_inspect;
    use super::*;
    use crate::ownership_checking::compilation_unit::formed_tests::Fixture;

    // Compile locally, execute injected-fact scenarios only in remote CI.
    #[test]
    fn ci_only_root_result_cleanup_requires_unique_real_drop() {
        let fixture = Fixture::new(&[(
            "cleanup-fault.ko",
            "fun run(flag: Boolean) { var a = \"a\"; var b = \"b\"; while(flag) { val x = replace(&a, \"x\"); val y = replace(&b, \"y\") }; val keptA = a; val keptB = b }",
        )]);
        let owned = fixture.shared();
        compare_and_inspect(&fixture, &fixture.parts(&owned), |checker| {
            let mut planner = DropPlanner::new(
                checker,
                super::super::super::super::liveness::build(checker, true).unwrap(),
            );
            planner.owner_control_probe = Some(Vec::new());
            let planner = planner.run().unwrap();
            let visits = planner.owner_control_probe.as_ref().unwrap();
            let control = visits[0].control;
            let evidence = RootEvidence::fault_fixture(
                visits[0].incoming_roots.clone().unwrap(),
                visits[1].root_writes.clone(),
            );
            let state = ValueState::default();
            let actual =
                completed_result_cleanup(checker, control, &state, &evidence, &planner.facts)
                    .unwrap();
            assert_eq!(actual.len(), 2);
            let binding = actual[0].1;
            let index = planner
                .facts
                .iter()
                .position(|fact| fact.target() == PlannerDropTarget::Named(binding))
                .unwrap();
            let mut missing = planner.facts.clone();
            missing.remove(index);
            assert!(
                completed_result_cleanup(checker, control, &state, &evidence, &missing).is_none()
            );
            let mut duplicate = planner.facts.clone();
            duplicate.push(duplicate[index]);
            assert!(
                completed_result_cleanup(checker, control, &state, &evidence, &duplicate).is_none()
            );
        });
    }
}
