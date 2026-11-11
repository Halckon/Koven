//! Private observed primitive commits, not public owner tokens or cleanup authority.
use super::*;
#[derive(Clone, Debug)]
pub(in super::super::super) struct RootWriteReceipt {
    pub(in super::super::super) control: UnitStatementId,
    pub(in super::super::super) root: UnitSymbolId,
    pub(in super::super::super) call: UnitExpressionId,
    pub(in super::super::super) producer: UnitExpressionId,
    pub(in super::super::super) token_retirement:
        Option<(UnitSymbolId, crate::ownership_checking::UnitDropFact)>,
    pub(in super::super::super) token_link: Option<(UnitOwnerId, UnitOwnerId)>,
    // Lexical If/When body and short-circuit RHS ancestry only, not a complete path or runtime trace.
    pub(in super::super::super) branch_arms: Vec<(UnitExpressionId, usize)>,
}
impl RootWriteReceipt {
    fn valid_branch_ancestry(&self, checker: &Checker<'_>) -> bool {
        self.branch_arms
            .iter()
            .enumerate()
            .all(|(index, (id, arm))| {
                id.source_unit() == self.control.source_unit()
                    && match checker
                        .parsed
                        .ast()
                        .expressions()
                        .get(id.expression())
                        .ok()
                        .map(|node| node.payload())
                    {
                        Some(Expression::If { .. }) => *arm < 2,
                        Some(Expression::When { entries, .. }) => *arm < entries.len(),
                        Some(Expression::Binary { .. }) => checker
                            .short_circuit_plan(id.expression())
                            .ok()
                            .flatten()
                            .is_some_and(|plan| *arm == plan.rhs_branch),
                        _ => false,
                    }
                    && !self.branch_arms[..index]
                        .iter()
                        .any(|(prior, _)| prior == id)
            })
    }
}
impl ControlTransactions {
    #[cfg(test)]
    pub(super) fn root_writes(&self, control: UnitStatementId) -> &[RootWriteReceipt] {
        self.root_entries
            .iter()
            .find(|(key, _)| *key == control)
            .map_or(&[], |(_, evidence)| evidence.writes.as_slice())
    }
}
fn direct_string(checker: &Checker<'_>, expression: ExpressionId) -> bool {
    matches!(checker.parsed.ast().expressions().get(expression).ok().map(|node| node.payload()),
        Some(Expression::String { parts }) if parts.iter().all(|part| !matches!(part, StringPart::Interpolation { .. })))
        && exact_type(checker, expression, BuiltinType::String)
}
impl DropPlanner<'_, '_> {
    pub(in super::super::super) fn enter_root_branch_arm(&mut self, id: ExpressionId, arm: usize) {
        let id = self.checker.unit_expression(id);
        if self.owner_loop.root_branches_invalid {
            return;
        }
        if self
            .owner_loop
            .root_branch_arms
            .iter()
            .any(|(parent, _)| *parent == id || parent.source_unit() != id.source_unit())
        {
            self.invalidate_root_branch_stack();
            return;
        }
        self.owner_loop.root_branch_arms.push((id, arm));
    }
    pub(in super::super::super) fn leave_root_branch_arm(&mut self, id: ExpressionId, arm: usize) {
        let expected = (self.checker.unit_expression(id), arm);
        if self.owner_loop.root_branches_invalid {
            return;
        }
        if self.owner_loop.root_branch_arms.last() != Some(&expected) {
            self.invalidate_root_branch_stack();
            return;
        }
        self.owner_loop.root_branch_arms.pop();
    }
    fn invalidate_root_branch_stack(&mut self) {
        self.owner_loop.root_branches_invalid = true;
        self.owner_loop.root_branch_arms.clear();
        self.owner_loop.root_entries.clear();
    }

    pub(in super::super::super) fn bind_root_old_token(
        &mut self,
        symbol: UnitSymbolId,
        initializer: ExpressionId,
    ) {
        let Some(control) = self.owner_controls.current() else {
            return;
        };
        let Some((_, evidence)) = self
            .owner_loop
            .root_entries
            .iter_mut()
            .find(|(id, _)| *id == control)
        else {
            return;
        };
        if let Some(transaction) = &mut evidence.token_transaction {
            transaction.bind(self.checker.unit_expression(initializer), symbol);
            if !transaction.is_valid() {
                for write in &mut evidence.writes {
                    write.token_link = None;
                    write.token_retirement = None;
                }
            }
        }
    }
    pub(in super::super::super) fn drop_root_old_token(&mut self, fact: PlannerDropFact) {
        let Some(control) = self.owner_controls.current() else {
            return;
        };
        let Some((_, evidence)) = self
            .owner_loop
            .root_entries
            .iter_mut()
            .find(|(id, _)| *id == control)
        else {
            return;
        };
        if let Some(transaction) = &mut evidence.token_transaction {
            let fact = fact.into_unit(self.checker.source_unit);
            if let Some((call, binding, old)) = transaction.drop_result(fact) {
                if let Some(write) = evidence.writes.iter_mut().find(|write| {
                    write.call == call
                        && write
                            .token_link
                            .as_ref()
                            .is_some_and(|(previous, _)| *previous == old)
                        && write.token_retirement.is_none()
                }) {
                    write.token_retirement = Some((binding, fact));
                } else {
                    transaction.invalidate();
                }
            }
            if !transaction.is_valid() {
                for write in &mut evidence.writes {
                    write.token_link = None;
                    write.token_retirement = None;
                }
            }
        }
    }

    // Called only after the actual String evaluation has reached its completion hook.
    pub(in super::super::super) fn record_root_literal(&mut self, expression: ExpressionId) {
        if self.owner_loop.root_branches_invalid {
            return;
        }
        let Some(control) = self.owner_controls.current() else {
            return;
        };
        if !direct_string(self.checker, expression) {
            return;
        }
        let expression = self.checker.unit_expression(expression);
        if let Some((_, evidence)) = self
            .owner_loop
            .root_entries
            .iter_mut()
            .find(|(key, _)| *key == control)
            && !evidence.completed_strings.iter().any(|(known, arms)| {
                *known == expression && *arms == self.owner_loop.root_branch_arms
            })
        {
            if self.owner_loop.root_branch_arms.is_empty() {
                if evidence.token_transaction.is_none()
                    && evidence.writes.is_empty()
                    && evidence.completed_strings.is_empty()
                {
                    evidence.token_transaction =
                        root_incoming::root_tokens::RootTokenTransaction::begin(
                            self.checker,
                            control,
                            &evidence.roots,
                            evidence.entry_catalog.as_ref(),
                            evidence.entered_body,
                        );
                }
                if let Some(transaction) = &mut evidence.token_transaction {
                    transaction.complete(expression);
                    if !transaction.is_valid() {
                        for write in &mut evidence.writes {
                            write.token_link = None;
                            write.token_retirement = None;
                        }
                    }
                }
            }
            evidence
                .completed_strings
                .push((expression, self.owner_loop.root_branch_arms.clone()));
        }
    }
    // Called at the real checked primitive commit boundary even when legacy observation declined.
    pub(in super::super::super) fn record_root_primitive_write(
        &mut self,
        call: ExpressionId,
        state: &ValueState,
    ) {
        if self.owner_loop.root_branches_invalid {
            return;
        }
        let Some(control) = self.owner_controls.current() else {
            return;
        };
        let call_id = self.checker.unit_expression(call);
        let Some(plan) = self.checker.ownership_primitives.iter().find(|plan| {
            plan.descriptor().expression() == call_id
                && plan.descriptor().kind() == OwnershipPrimitiveKind::Replace
                && plan.places().len() == 1
                && plan.places()[0].is_root()
                && plan.descriptor().operands().len() == 2
                && self
                    .checker
                    .typed
                    .types()
                    .get(plan.descriptor().value_type())
                    == Some(&UnitTypeKind::Builtin(BuiltinType::String))
        }) else {
            return;
        };
        let root = plan.places()[0].root();
        let producer = plan.descriptor().operands()[1];
        if state.position(root).is_none()
            || !direct_string(self.checker, producer.expression())
            || !self.pending_value_registered(call, producer.expression(), state)
        {
            return;
        }
        let Some((_, evidence)) = self
            .owner_loop
            .root_entries
            .iter_mut()
            .find(|(key, _)| *key == control)
        else {
            return;
        };
        if !evidence.roots.iter().any(|(known, _)| *known == root)
            || evidence.writes.iter().any(|write| {
                write.control == control
                    && (write.call == call_id
                        || write.producer == producer
                        || write.root.source_unit() != root.source_unit())
            })
        {
            return;
        }
        let Some(index) = evidence
            .completed_strings
            .iter()
            .position(|(completed, arms)| {
                *completed == producer && *arms == self.owner_loop.root_branch_arms
            })
        else {
            return;
        };
        evidence.completed_strings.remove(index);
        let mut receipt = RootWriteReceipt {
            control,
            root,
            call: call_id,
            producer,
            branch_arms: self.owner_loop.root_branch_arms.clone(),
            token_link: None,
            token_retirement: None,
        };
        if receipt.valid_branch_ancestry(self.checker) {
            if receipt.branch_arms.is_empty() {
                receipt.token_link = evidence
                    .token_transaction
                    .as_mut()
                    .and_then(|transaction| transaction.commit(call_id, root, producer));
            }
            if evidence
                .token_transaction
                .as_ref()
                .is_some_and(|transaction| !transaction.is_valid())
            {
                for write in &mut evidence.writes {
                    write.token_link = None;
                    write.token_retirement = None;
                }
                receipt.token_link = None;
            }
            evidence.writes.push(receipt);
        }
    }
}

#[cfg(test)]
mod scope_fault_tests {
    use super::super::super::tests::compare_and_inspect;
    use super::*;
    use crate::ownership_checking::compilation_unit::formed_tests::Fixture;

    // Fault injection is compiled locally but executed only by remote CI.
    #[test]
    fn ci_only_root_branch_scope_mismatch_is_sticky() {
        let fixture = Fixture::new(&[(
            "scope-fault.ko",
            "fun run(flag: Boolean) { var root = \"old\"; while(flag) { val old = replace(&root, \"new\"); break }; val kept = root }",
        )]);
        let owned = fixture.shared();
        compare_and_inspect(&fixture, &fixture.parts(&owned), |checker| {
            for duplicate in [false, true] {
                let mut planner = DropPlanner::new(
                    checker,
                    super::super::super::super::liveness::build(checker, true).unwrap(),
                );
                planner.owner_control_probe = Some(Vec::new());
                let mut planner = planner.run().unwrap();
                let id = planner.owner_control_probe.as_ref().unwrap()[1].root_writes[0]
                    .call
                    .expression();
                let visits = planner.owner_control_probe.as_ref().unwrap();
                let control = visits[0].control;
                let evidence = super::super::root_incoming::RootEvidence::fault_fixture(
                    visits[0].incoming_roots.clone().unwrap(),
                    visits[1].root_writes.clone(),
                );
                assert!(!evidence.roots.is_empty());
                assert!(!evidence.writes.is_empty());
                planner.owner_loop.root_entries.push((control, evidence));
                assert_eq!(planner.owner_loop.root_entries.len(), 1);
                planner.enter_root_branch_arm(id, 0);
                if duplicate {
                    planner.enter_root_branch_arm(id, 0);
                } else {
                    planner.leave_root_branch_arm(id, 1);
                }
                assert!(planner.owner_loop.root_branches_invalid);
                assert!(planner.owner_loop.root_entries.is_empty());
                planner.enter_root_branch_arm(id, 0);
                planner.leave_root_branch_arm(id, 0);
                assert!(planner.owner_loop.root_branches_invalid);
                assert!(planner.owner_loop.root_branch_arms.is_empty());
            }
        });
    }
}
