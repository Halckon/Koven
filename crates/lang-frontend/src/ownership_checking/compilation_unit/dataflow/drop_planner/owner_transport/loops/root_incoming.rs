//! Actual root entry capture, distinct from a backedge/header equation.
use super::*;
use crate::ownership_checking::UnitOwnerProducerCatalogV1;
#[cfg(test)]
pub(in super::super) use root_tokens::StaticRootPredecessor;
mod guard_reset;
mod root_exit;
mod root_suffix;
pub(super) mod root_tokens;
pub(super) type RootRetirements = Vec<(
    UnitSymbolId,
    UnitSymbolId,
    crate::ownership_checking::UnitDropFact,
)>;
type RootExitPartition = (
    Vec<UnitSymbolId>,
    Vec<crate::ownership_checking::UnitDropFact>,
);
pub(super) type RootIncoming = Vec<(UnitSymbolId, UnitOwnerId)>;
pub(super) struct RootEvidence {
    normal_equation: Option<root_tokens::RootBoundaryEquation>,
    exit_drops: Option<Vec<crate::ownership_checking::UnitDropFact>>,
    exit_partition: Option<RootExitPartition>,
    suffix: Option<root_suffix::RootSuffix>,
    pub(super) entry_catalog: Option<UnitOwnerProducerCatalogV1>,
    pub(super) token_transaction: Option<root_tokens::RootTokenTransaction>,
    pub(super) roots: RootIncoming,
    pub(super) completed_strings: Vec<(UnitExpressionId, Vec<(UnitExpressionId, usize)>)>,
    pub(super) writes: Vec<RootWriteReceipt>,
    normal_retirements: Option<RootRetirements>,
    normal_write_vector: Option<Vec<(UnitSymbolId, UnitExpressionId)>>,
    after_condition: bool,
    completed_condition: Option<UnitExpressionId>,
    pub(super) entered_body: Option<(UnitExpressionId, UnitStatementId)>,
    normal_backedge: bool,
    tainted: bool,
}
#[cfg(test)]
impl RootEvidence {
    pub(super) fn fault_fixture(roots: RootIncoming, writes: Vec<RootWriteReceipt>) -> Self {
        Self {
            normal_equation: None,
            exit_drops: None,
            exit_partition: None,
            suffix: None,
            entry_catalog: None,
            token_transaction: None,
            roots,
            writes,
            completed_strings: Vec::new(),
            normal_retirements: None,
            normal_write_vector: None,
            after_condition: true,
            completed_condition: None,
            entered_body: None,
            normal_backedge: false,
            tainted: false,
        }
    }
}
impl ControlTransactions {
    pub(in super::super::super) fn take_root_boundary(
        &mut self,
    ) -> Option<crate::ownership_checking::UnitOwnerMultiRootBoundaryV1> {
        if self.root_branches_invalid || self.root_entries.len() != 1 {
            return None;
        }
        let evidence = &mut self.root_entries[0].1;
        let boundary = evidence.normal_equation.take()?.into_boundary()?;
        let boundary = match evidence.exit_partition.take() {
            Some((live, drops)) => boundary.with_condition_false_partition(live, drops)?,
            None => boundary,
        };
        let boundary = match evidence.suffix.take().and_then(|suffix| suffix.completed()) {
            Some(receipts) => boundary.with_condition_false_suffix(receipts)?,
            None => boundary,
        };
        match evidence.exit_drops.take() {
            Some(drops) => boundary.with_condition_false_drops(drops),
            None => Some(boundary),
        }
    }

    pub(in super::super::super) fn invalidate_root_preservation(&mut self) {
        for (_, evidence) in &mut self.root_entries {
            evidence.tainted = true;
            evidence.normal_backedge = false;
            evidence.normal_write_vector = None;
            evidence.normal_retirements = None;
            evidence.normal_equation = None;
            evidence.exit_drops = None;
            evidence.exit_partition = None;
            evidence.suffix = None;
        }
    }

    #[cfg(test)]
    pub(super) fn root_equation(
        &self,
        control: UnitStatementId,
    ) -> Option<root_tokens::RootBoundaryEquation> {
        self.root_entries
            .iter()
            .find(|(id, _)| *id == control)
            .and_then(|(_, evidence)| evidence.normal_equation.clone())
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
    pub(super) fn record_root_condition(
        &mut self,
        control: UnitStatementId,
        condition: UnitExpressionId,
        state: &ValueState,
    ) {
        if let Some((_, evidence)) = self
            .root_entries
            .iter_mut()
            .find(|(key, _)| *key == control)
        {
            evidence.completed_condition = Some(condition);
            evidence.entered_body = None;
            evidence.after_condition =
                !evidence.tainted && preserves(control, state, &evidence.roots);
            evidence.normal_backedge = false;
            evidence.normal_write_vector = None;
            evidence.normal_retirements = None;
            evidence.normal_equation = None;
            evidence.exit_drops = None;
            evidence.exit_partition = None;
            evidence.suffix = None;
        }
    }
    // Called at the actual planner body-entry edge, not reconstructed from the AST.
    pub(super) fn record_root_body_entry(
        &mut self,
        control: UnitStatementId,
        condition: UnitExpressionId,
        body: UnitStatementId,
        state: &ValueState,
    ) {
        #[cfg(test)]
        if root_tokens::skip_body_entry() {
            return;
        }
        if let Some((_, evidence)) = self.root_entries.iter_mut().find(|(id, _)| *id == control) {
            evidence.entered_body = (evidence.after_condition
                && !evidence.tainted
                && evidence.completed_condition == Some(condition)
                && clean_root_shape(state, &evidence.roots))
            .then_some((condition, body));
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
            evidence.normal_equation = None;
            evidence.exit_drops = None;
            evidence.exit_partition = None;
            evidence.suffix = None;
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
            if !normal
                || !evidence
                    .token_transaction
                    .as_ref()
                    .is_some_and(|transaction| transaction.is_complete())
            {
                for write in &mut evidence.writes {
                    write.token_link = None;
                    write.token_retirement = None;
                }
            }
            evidence.normal_write_vector = if normal && evidence.after_condition {
                completed_root_writes(checker, control, state, evidence)
            } else {
                None
            };
            evidence.normal_retirements = evidence
                .normal_write_vector
                .as_ref()
                .and_then(|_| completed_result_cleanup(checker, control, state, evidence, facts));
            evidence.normal_equation = evidence
                .normal_retirements
                .as_ref()
                .filter(|_| clean_root_shape(state, &evidence.roots))
                .and_then(|_| evidence.token_transaction.as_ref()?.boundary_equation());
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
            self.counter_entered(control, false);
            return;
        }
        let roots = capture(control, state, observation);
        self.counter_entered(control, roots.is_some());
        let Some(roots) = roots else {
            return;
        };
        let entry_catalog = observation.and_then(|observation| {
            let catalog = UnitOwnerProducerCatalogV1 {
                identity: observation.identity.clone(),
                callable: observation.callable,
                definitions: observation.definitions.clone(),
            };
            (catalog.is_consistent()
                && roots
                    .iter()
                    .all(|(_, owner)| catalog.definition(owner.clone()).is_some()))
            .then_some(catalog)
        });
        self.root_entries.push((
            control,
            RootEvidence {
                normal_equation: None,
                exit_drops: None,
                exit_partition: None,
                suffix: None,
                entry_catalog,
                token_transaction: None,
                roots,
                completed_strings: Vec::new(),
                writes: Vec::new(),
                normal_retirements: None,
                normal_write_vector: None,
                after_condition: false,
                completed_condition: None,
                entered_body: None,
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
// Shape/auxiliary-state obligation only. Stale ValueState owner IDs are not evidence.
fn clean_root_shape(state: &ValueState, roots: &RootIncoming) -> bool {
    boundary_values(state).is_some_and(|values| {
        values.len() == roots.len()
            && roots
                .iter()
                .all(|(root, _)| values.iter().filter(|value| value.symbol == *root).count() == 1)
    })
}

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
    let guard = checker.reference_symbol(ast.expressions().get(*condition).ok()?.span())?;
    let (elements, _) = guard_reset::body_parts(checker, elements, evidence.roots.len(), guard)?;
    if evidence.writes.len() != elements.len() {
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
    let Statement::While {
        condition, body, ..
    } = ast.statements().get(control.statement()).ok()?.payload()
    else {
        return None;
    };
    let Statement::Block { elements } = ast.statements().get(*body).ok()?.payload() else {
        return None;
    };
    let guard = checker.reference_symbol(ast.expressions().get(*condition).ok()?.span())?;
    let (elements, _) = guard_reset::body_parts(checker, elements, evidence.roots.len(), guard)?;
    if elements.len() != evidence.writes.len() {
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

pub(super) fn counter_prefixes(
    checker: &Checker<'_>,
    item: ItemId,
) -> Vec<(UnitStatementId, UnitExpressionId)> {
    guard_reset::counter::prefixes(checker, item).unwrap_or_default()
}

impl RootEvidence {
    pub(super) fn counter_tail_evidence(
        &self,
        initializer: UnitExpressionId,
    ) -> Option<crate::ownership_checking::UnitOwnerCounterTailEvidenceV1> {
        let mut result = self
            .normal_equation
            .as_ref()?
            .counter_evidence(initializer)?;
        if let Some((live, drops)) = &self.exit_partition {
            result.roots = result
                .roots
                .with_condition_false_partition(live.clone(), drops.clone())?;
        }
        if let Some(receipts) = self.suffix.as_ref().and_then(|s| s.completed_snapshot()) {
            result.roots = result.roots.with_condition_false_suffix(receipts)?;
        }
        result.is_consistent().then_some(result)
    }
}

#[cfg(test)]
mod cleanup_fault_tests {
    use super::super::super::tests::compare_and_inspect;
    use super::*;
    use crate::ownership_checking::compilation_unit::formed_tests::Fixture;

    #[test]
    fn ci_only_header_shape_rejects_missing_duplicate_and_borrow_state() {
        let fixture = Fixture::new(&[(
            "header-shape-fault.ko",
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
            let roots = visits[0].incoming_roots.as_ref().unwrap();
            assert!(visits[1].normal_equation.is_some());
            let span = checker
                .parsed
                .ast()
                .statements()
                .get(visits[0].control.statement())
                .unwrap()
                .span();
            // Deliberately assembled shape model, not a real planner boundary snapshot.
            // No owner IDs: this check must not mistake stale IDs for generation evidence.
            let state = ValueState {
                values: roots
                    .iter()
                    .map(|(root, _)| OwnedValue {
                        symbol: *root,
                        owner: None,
                        origin: span,
                        declaration: span,
                        scope_depth: 1,
                    })
                    .collect(),
                ..ValueState::default()
            };
            assert!(clean_root_shape(&state, roots));
            let mut reordered = state.clone();
            reordered.values.reverse();
            assert!(clean_root_shape(&reordered, roots));
            let mut missing = state.clone();
            missing.values.pop();
            assert!(!clean_root_shape(&missing, roots));
            let mut duplicate = state.clone();
            duplicate.values[1] = duplicate.values[0].clone();
            assert!(!clean_root_shape(&duplicate, roots));
            let mut borrowed = state.clone();
            borrowed.borrow_bindings.push(roots[0].0);
            assert!(!clean_root_shape(&borrowed, roots));
            let mut replacing = state.clone();
            replacing.replacements.push(roots[0].0);
            assert!(!clean_root_shape(&replacing, roots));
            let mut extra = state.clone();
            extra.values.push(extra.values[0].clone());
            assert!(!clean_root_shape(&extra, roots));
        });
    }

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
