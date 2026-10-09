//! 只验证生产者已检查的同轮模板与退出记录，不从AST生成ownership事实。
use super::{
    CompilationUnitOwnership, OwnershipBindingKind, UnitDropTarget,
    UnitIterationCleanupAction as Action, UnitIterationExitKind as Exit, UnitLoanTarget,
};

impl CompilationUnitOwnership {
    pub(super) fn iterations_are_valid(&self) -> bool {
        if self.iterations.len() != self.iteration_templates.len() {
            return false;
        }
        for (index, plan) in self.iterations.iter().enumerate() {
            let statement = plan.descriptor().statement();
            if plan.descriptor().provider() == crate::type_checking::IterationProvider::RangeView
                && matches!(plan.source(), UnitLoanTarget::Temporary(_))
            {
                let Some(fact) = self.borrow_results.range_uses.iter().find(|fact| {
                    fact.expression() == plan.descriptor().source()
                        && fact.site() == crate::ownership_checking::RangeUseSite::Iteration
                }) else {
                    return false;
                };
                let source = fact.source_loan();
                if fact.origin() != plan.source()
                    || source.call() != fact.expression()
                    || self
                        .loans
                        .iter()
                        .find(|loan| {
                            loan.call() == source.call() && loan.argument() == source.argument()
                        })
                        .is_none_or(|loan| {
                            loan.target() != fact.origin()
                                || loan.kind() != crate::ownership_checking::LoanKind::Shared
                        })
                {
                    return false;
                }
            }
            if self.iterations[..index]
                .iter()
                .any(|other| other.descriptor().statement() == statement)
            {
                return false;
            }
            let Some(template) = self
                .iteration_templates
                .iter()
                .find(|template| template.descriptor().statement() == statement)
            else {
                return false;
            };
            if plan.descriptor() != template.descriptor()
                || plan.source() != template.source()
                || plan.source_access() != template.source_access()
                || plan.bindings() != template.bindings()
                || plan
                    .bindings()
                    .iter()
                    .any(|binding| binding.kind() != OwnershipBindingKind::Shared)
                || plan
                    .bindings()
                    .iter()
                    .map(|binding| binding.symbol())
                    .ne(plan.descriptor().binding().symbols())
            {
                return false;
            }
            let expected = self
                .iteration_required_exits
                .iter()
                .filter(|(owner, _, _)| *owner == statement)
                .collect::<Vec<_>>();
            if expected.len() != plan.exits().len()
                || !plan
                    .exits()
                    .iter()
                    .any(|exit| exit.kind() == Exit::Exhaustion)
            {
                return false;
            }
            for (exit, (_, kind, point)) in plan.exits().iter().zip(expected) {
                if exit.kind() != *kind || exit.point() != *point {
                    return false;
                }
                let actions = exit.actions();
                let mut required_actions = Vec::new();
                // Required exits were recorded by traversal at each lexical frame unwind.
                // Checking their complete sequence rejects foreign/extra providers and
                // outer-before-inner cleanup even when each individual provider is ordered.
                for (owner, kind, point) in &self.iteration_required_exits {
                    if *point != exit.point() {
                        continue;
                    }
                    let Some(owner_plan) = self
                        .iteration_templates
                        .iter()
                        .find(|candidate| candidate.descriptor().statement() == *owner)
                    else {
                        return false;
                    };
                    if *kind != Exit::Exhaustion {
                        for binding in owner_plan.bindings().iter().rev() {
                            required_actions.push(Action::EndBinding {
                                statement: *owner,
                                symbol: binding.symbol(),
                            });
                        }
                        required_actions.push(Action::EndElement(*owner));
                    }
                    if matches!(kind, Exit::Break(_) | Exit::Return(_) | Exit::Exhaustion) {
                        required_actions.push(Action::FinishProvider(*owner));
                        required_actions.push(Action::EndSource(*owner));
                    }
                }
                if actions
                    .iter()
                    .filter(|action| {
                        !matches!(
                            action,
                            Action::Drop(_)
                                | Action::DropConditionalReceiver(_)
                                | Action::EndCallLoan(_)
                                | Action::EndReceiverLoan(_)
                                | Action::EndCaptureLoan { .. }
                        )
                    })
                    .ne(required_actions.iter())
                {
                    return false;
                }
                if actions
                    .iter()
                    .filter(|action| {
                        matches!(
                            action,
                            Action::EndCallLoan(_)
                                | Action::EndReceiverLoan(_)
                                | Action::EndCaptureLoan { .. }
                        )
                    })
                    .ne(self
                        .iteration_loan_ends
                        .iter()
                        .filter(|(point, _)| *point == exit.point())
                        .map(|(_, action)| action))
                {
                    return false;
                }
                // 同一点的多个provider必须指向同一完整序列，消费者按point执行一次。
                if self.iteration_cleanup_at(exit.point()) != Some(actions) {
                    return false;
                }
                let positions = |wanted: &Action| {
                    actions
                        .iter()
                        .enumerate()
                        .filter(|(_, action)| *action == wanted)
                        .map(|(i, _)| i)
                        .collect::<Vec<_>>()
                };
                let elements = positions(&Action::EndElement(statement));
                let finish = positions(&Action::FinishProvider(statement));
                let source = positions(&Action::EndSource(statement));
                let exhausted = exit.kind() == Exit::Exhaustion;
                if elements.len() != usize::from(!exhausted) {
                    return false;
                }
                for binding in plan.bindings() {
                    let ends = positions(&Action::EndBinding {
                        statement,
                        symbol: binding.symbol(),
                    });
                    if exhausted && !ends.is_empty()
                        || !exhausted && (ends.len() != 1 || ends[0] >= elements[0])
                    {
                        return false;
                    }
                }
                let terminal = matches!(
                    exit.kind(),
                    Exit::Break(_) | Exit::Return(_) | Exit::Exhaustion
                );
                if finish.len() != usize::from(terminal) || source.len() != usize::from(terminal) {
                    return false;
                }
                if terminal && (finish[0] >= source[0] || !exhausted && elements[0] >= finish[0]) {
                    return false;
                }
                for (position, action) in actions.iter().enumerate() {
                    // A loan must end before destruction of the owner it protects,
                    // not merely before the later element/provider boundary.
                    if actions[..position].iter().any(|earlier| {
                        let target = match earlier {
                            Action::Drop(drop) => drop.target(),
                            Action::DropConditionalReceiver(drop) => {
                                UnitDropTarget::This(drop.owner())
                            }
                            _ => return false,
                        };
                        match action {
                            Action::EndCallLoan(loan) => match loan.target() {
                                UnitLoanTarget::Place(place) => {
                                    target == UnitDropTarget::Named(place.root())
                                }
                                UnitLoanTarget::Temporary(owner) => {
                                    target == UnitDropTarget::Temporary(*owner)
                                }
                                UnitLoanTarget::This(owner) => {
                                    target == UnitDropTarget::This(*owner)
                                }
                            },
                            Action::EndReceiverLoan(loan) => match loan.target() {
                                super::UnitReceiverOwnershipTarget::Place(place) => {
                                    target == UnitDropTarget::Named(place.root())
                                }
                                super::UnitReceiverOwnershipTarget::Temporary(owner) => {
                                    target == UnitDropTarget::Temporary(*owner)
                                }
                                super::UnitReceiverOwnershipTarget::This(owner) => {
                                    target == UnitDropTarget::This(*owner)
                                }
                            },
                            Action::EndCaptureLoan {
                                source: super::UnitClosureCaptureSource::Symbol(symbol),
                                ..
                            } => target == UnitDropTarget::Named(*symbol),
                            Action::EndCaptureLoan {
                                source: super::UnitClosureCaptureSource::This,
                                ..
                            } => matches!(target, UnitDropTarget::This(_)),
                            _ => false,
                        }
                    }) {
                        return false;
                    }
                    match action {
                        Action::EndCallLoan(fact) => {
                            if !self.loans().contains(fact)
                                || actions[..position]
                                    .iter()
                                    .any(|action| matches!(action, Action::EndElement(_)))
                            {
                                return false;
                            }
                        }
                        Action::EndReceiverLoan(fact) => {
                            if !self.receiver_facts().contains(fact)
                                || !matches!(
                                    fact.kind(),
                                    super::UnitReceiverOwnershipKind::SharedLoan
                                        | super::UnitReceiverOwnershipKind::ExclusiveLoan
                                )
                                || actions[..position]
                                    .iter()
                                    .any(|action| matches!(action, Action::EndElement(_)))
                            {
                                return false;
                            }
                        }
                        Action::EndCaptureLoan {
                            closure,
                            source: capture_source,
                        } => {
                            if !self.captures().iter().any(|capture| {
                                capture.lambda() == *closure
                                    && capture.source() == *capture_source
                                    && capture.mode()
                                        == crate::ownership_checking::ClosureCaptureMode::Shared
                            }) {
                                return false;
                            }
                            if let super::UnitClosureCaptureSource::Symbol(symbol) = capture_source
                                && plan
                                    .bindings()
                                    .iter()
                                    .any(|binding| binding.symbol() == *symbol)
                                && !exhausted
                                && position >= elements[0]
                            {
                                return false;
                            }
                        }
                        Action::DropConditionalReceiver(fact) => {
                            let Some((_, scopes)) = self
                                .iteration_conditional_scopes
                                .iter()
                                .find(|(expected, _)| expected == fact)
                            else {
                                return false;
                            };
                            let preceding = actions[..position]
                                .iter()
                                .filter(|action| matches!(action, Action::Drop(_)))
                                .count();
                            if preceding != fact.preceding_drops()
                                || (scopes.contains(&statement)
                                    && (exhausted || position >= elements[0]))
                                || (!scopes.contains(&statement)
                                    && terminal
                                    && position <= source[0])
                            {
                                return false;
                            }
                        }
                        Action::Drop(fact) => {
                            // Compare the owner's lexical scope to the provider's scope,
                            // independently of the producer's action ordering.
                            if let Some((_, _, inside)) =
                                self.iteration_owner_scopes
                                    .iter()
                                    .find(|(owner, candidate, _)| {
                                        *owner == statement && *candidate == fact.target()
                                    })
                                && ((!exhausted && *inside && position >= elements[0])
                                    || (matches!(exit.kind(), Exit::Return(_) | Exit::Break(_))
                                        && !*inside
                                        && position <= source[0]))
                            {
                                return false;
                            }
                            if terminal
                                && let UnitLoanTarget::Place(place) = plan.source()
                                && fact.target() == UnitDropTarget::Named(place.root())
                                && position <= source[0]
                            {
                                return false;
                            }
                        }
                        _ => {}
                    }
                }
                if let UnitLoanTarget::Temporary(temporary) = plan.source() {
                    let drops = actions
                        .iter()
                        .enumerate()
                        .filter_map(|(i, action)| match action {
                            Action::Drop(fact)
                                if fact.target() == UnitDropTarget::Temporary(*temporary) =>
                            {
                                Some(i)
                            }
                            _ => None,
                        })
                        .collect::<Vec<_>>();
                    if drops.len() != usize::from(terminal) || terminal && drops[0] <= source[0] {
                        return false;
                    }
                }
                let facts = self
                    .drops()
                    .iter()
                    .filter(|fact| fact.point() == exit.point())
                    .collect::<Vec<_>>();
                let action_drops = actions
                    .iter()
                    .filter_map(|action| match action {
                        Action::Drop(fact) => Some(fact),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                if facts != action_drops {
                    return false;
                }
                if actions
                    .iter()
                    .filter_map(|action| match action {
                        Action::DropConditionalReceiver(fact) => Some(fact),
                        _ => None,
                    })
                    .ne(self
                        .conditional_receiver_drops()
                        .iter()
                        .filter(|fact| fact.point() == exit.point()))
                {
                    return false;
                }
            }
        }
        true
    }
}
