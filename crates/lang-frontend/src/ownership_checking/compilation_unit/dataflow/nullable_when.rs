//! 绑定同轮 typed witness 与已完成的 ownership contract，不复制 move/loan 状态机。
use std::collections::BTreeMap;

use super::{AccessKind, Checker, ExpressionUse, Flows, State, UnitCallArgumentOwnershipContract};
use crate::{
    ast::{ExpressionId, StatementId},
    ownership_checking::{
        LoanKind, OwnershipBindingKind, OwnershipCheckingError, UnitCallArgumentOwnershipKind,
        UnitLoanTarget, UnitNullableWhenEntryOwnershipFact, UnitNullableWhenOwnershipPlan,
        UnitNullableWhenUseFact, UnitNullableWhenUseKind, UnitValueDeliveryKind,
        UnitValueDeliverySource,
    },
    parser::Expression,
    type_checking::{
        NullableWhenSubjectCategory, UnitCallTarget, UnitExpressionId,
        UnitNullableWhenUseWitness, UnitStatementId,
    },
};

/// 独立于旧九长度 rollback API 的完整 collector checkpoint。
#[derive(Clone, Default)]
pub(super) struct Collection {
    plans: BTreeMap<UnitExpressionId, UnitNullableWhenOwnershipPlan>,
}

fn invalid(expression: UnitExpressionId) -> OwnershipCheckingError {
    OwnershipCheckingError::InvalidUnitArgumentPlace {
        source_unit: expression.source_unit().index(),
        expression: expression.expression().index(),
    }
}

impl Collection {
    pub(super) fn finish(self) -> Result<Vec<UnitNullableWhenOwnershipPlan>, OwnershipCheckingError> {
        for plan in self.plans.values() {
            if plan.entries.len() != plan.descriptor.entries().len() {
                return Err(invalid(plan.descriptor.expression()));
            }
        }
        Ok(self.plans.into_values().collect())
    }

    fn record_use(&mut self, fact: UnitNullableWhenUseFact) -> Result<(), OwnershipCheckingError> {
        let plan = self
            .plans
            .get_mut(&fact.witness.control())
            .ok_or_else(|| invalid(fact.argument))?;
        if let Some(prior) = plan.uses.iter().find(|prior| prior.argument == fact.argument) {
            if prior != &fact {
                return Err(invalid(fact.argument));
            }
        } else {
            plan.uses.push(fact);
            plan.uses.sort_by_key(|fact| (fact.call, fact.argument));
        }
        Ok(())
    }
}

impl Checker<'_> {
    /// 只读判别不把真实 field/element subject 当成 Value 交付。
    pub(super) fn check_nullable_when_subject(
        &mut self,
        control: ExpressionId,
        subject: ExpressionId,
        mut flows: Flows,
    ) -> Result<Flows, OwnershipCheckingError> {
        let unit_control = self.unit_expression(control);
        let unit_subject = self.unit_expression(subject);
        let Some(descriptor) = self.typed.nullable_when(unit_control) else {
            return self.chain_expression(flows, subject, ExpressionUse::Read);
        };
        if descriptor.expression().source_unit() != self.source_unit
            || descriptor.subject().source_unit() != self.source_unit
            || descriptor.expression() != unit_control
            || descriptor.subject() != unit_subject
        {
            return Err(invalid(unit_control));
        }
        if !matches!(
            descriptor.source_category(),
            NullableWhenSubjectCategory::OrdinaryField
                | NullableWhenSubjectCategory::ContainerElement
        ) {
            return self.chain_expression(flows, subject, ExpressionUse::Read);
        }
        let diagnostic_count = self.diagnostics.len();
        flows = self.chain_expression(
            flows,
            subject,
            ExpressionUse::Place {
                parameter_span: None,
            },
        )?;
        if self.diagnostics.len() != diagnostic_count {
            return Ok(flows);
        }
        if let Some(state) = flows.next.as_mut()
            && let Some(place) = self.place(subject)?
        {
            let span = self.parsed.ast().expressions().get(subject)?.span();
            self.access_place(&place, AccessKind::Read, span, None, state)?;
        }
        Ok(flows)
    }

    pub(super) fn begin_nullable_when(
        &mut self,
        control: ExpressionId,
    ) -> Result<(), OwnershipCheckingError> {
        let control = self.unit_expression(control);
        let Some(descriptor) = self.typed.nullable_when(control) else {
            return Ok(());
        };
        let source_place = self.place(descriptor.subject().expression())?;
        if descriptor.subject().source_unit() != self.source_unit
            || (descriptor.source_category() == NullableWhenSubjectCategory::Temporary
                && source_place.is_some())
        {
            return Err(invalid(control));
        }
        if let Some(prior) = self.nullable_whens.plans.get(&control) {
            if prior.descriptor != *descriptor || prior.source_place != source_place {
                return Err(invalid(control));
            }
        } else {
            self.nullable_whens.plans.insert(
                control,
                UnitNullableWhenOwnershipPlan {
                    descriptor: descriptor.clone(),
                    source_place,
                    entries: Vec::new(),
                    uses: Vec::new(),
                },
            );
        }
        Ok(())
    }

    pub(super) fn check_nullable_when_body(
        &mut self,
        control: ExpressionId,
        index: usize,
        body: StatementId,
        state: State,
        usage: ExpressionUse,
        escaping: bool,
    ) -> Result<Flows, OwnershipCheckingError> {
        let flows = self.check_control_body(body, state, usage, escaping)?;
        let control = self.unit_expression(control);
        if let Some(plan) = self.nullable_whens.plans.get_mut(&control) {
            let body = UnitStatementId::new(control.source_unit(), body);
            if plan.descriptor.entries().get(index).is_none_or(|entry| {
                entry.index() != index || entry.body() != body
            }) {
                return Err(invalid(control));
            }
            let fact = UnitNullableWhenEntryOwnershipFact {
                index,
                body,
                falls_through: flows.next.is_some(),
                breaks: flows.breaks.is_some(),
                continues: flows.continues.is_some(),
            };
            if let Some(prior) = plan.entries.iter().find(|prior| prior.index == index) {
                if prior != &fact {
                    return Err(invalid(control));
                }
            } else {
                plan.entries.push(fact);
                plan.entries.sort_by_key(|fact| fact.index);
            }
        }
        Ok(flows)
    }

    fn nullable_when_call_witness(
        &self,
        contract: UnitCallArgumentOwnershipContract,
    ) -> Result<Option<UnitNullableWhenUseWitness>, OwnershipCheckingError> {
        let Some(call) = self.typed.call(contract.call()) else {
            return Ok(None);
        };
        if !matches!(call.target(), UnitCallTarget::Declaration(_)) || call.receiver().is_some() {
            return Ok(None);
        }
        let Some(witness) = self.typed.nullable_when_use(contract.argument()) else {
            return Ok(None);
        };
        let Some(plan) = self.nullable_whens.plans.get(&witness.control()) else {
            return Ok(None);
        };
        let descriptor = &plan.descriptor;
        if descriptor.source_category() != NullableWhenSubjectCategory::OwnedRoot {
            return Ok(None);
        }
        if witness.expression() != contract.argument()
            || witness.subject() != descriptor.subject()
            || Some(witness.symbol()) != descriptor.stable_symbol()
            || Some(witness.source_generation()) != descriptor.source_generation()
            || witness.nullable_type() != descriptor.nullable_type()
            || witness.inner_type() != descriptor.inner_type()
            || descriptor.entries().get(witness.entry_index()).is_none_or(|entry| {
                entry.body() != witness.body()
            })
            || plan.source_place.as_ref().is_none_or(|place| {
                !place.is_root() || place.root() != witness.symbol()
            })
        {
            return Err(invalid(contract.argument()));
        }
        let mut current = contract.argument();
        loop {
            let Some(chain) = self.typed.nullable_when_use(current) else {
                return Err(invalid(current));
            };
            if chain.expression() != current
                || chain.reference() != witness.reference()
                || chain.control() != witness.control()
                || chain.subject() != witness.subject()
                || chain.entry_index() != witness.entry_index()
                || chain.body() != witness.body()
                || chain.symbol() != witness.symbol()
                || chain.source_generation() != witness.source_generation()
                || chain.nullable_type() != witness.nullable_type()
                || chain.inner_type() != witness.inner_type()
            {
                return Err(invalid(current));
            }
            let node = self.parsed.ast().expressions().get(current.expression())?;
            match node.payload() {
                Expression::Group { expression } => current = self.unit_expression(*expression),
                Expression::Name if current == witness.reference() => break,
                _ => return Err(invalid(current)),
            }
        }
        Ok(Some(witness))
    }

    pub(super) fn record_nullable_when_delivery(
        &mut self,
        contract: UnitCallArgumentOwnershipContract,
        state: &State,
    ) -> Result<(), OwnershipCheckingError> {
        if contract.kind() != UnitCallArgumentOwnershipKind::Value {
            return Ok(());
        }
        let Some(witness) = self.nullable_when_call_witness(contract)? else {
            return Ok(());
        };
        let Some(delivery) = self.value_deliveries.last().filter(|delivery| {
            delivery.call() == contract.call() && delivery.argument() == contract.argument()
        }) else {
            return Err(invalid(contract.argument()));
        };
        if delivery.kind() != UnitValueDeliveryKind::Move {
            return Ok(());
        }
        let UnitValueDeliverySource::Place(place) = delivery.source() else {
            return Ok(());
        };
        let owned = self.bindings.get(&place.root()).map_or_else(
            || self.formed_owned_locals.contains(&place.root()),
            |binding| binding.kind() == OwnershipBindingKind::Owned,
        );
        let reference_span = self
            .parsed
            .ast()
            .expressions()
            .get(witness.reference().expression())?
            .span();
        if !place.is_root()
            || place.root() != witness.symbol()
            || !owned
            || state.non_owning.contains_key(&place.root())
            || state.moved.get(&place.root()) != Some(&reference_span)
        {
            return Ok(());
        }
        self.nullable_whens.record_use(UnitNullableWhenUseFact {
            call: contract.call(),
            argument: contract.argument(),
            witness,
            kind: UnitNullableWhenUseKind::Consume,
            delivery: Some(delivery.clone()),
            loan: None,
            moved_span: Some(reference_span),
        })
    }

    pub(super) fn record_nullable_when_loan(
        &mut self,
        contract: UnitCallArgumentOwnershipContract,
    ) -> Result<(), OwnershipCheckingError> {
        if contract.kind() != UnitCallArgumentOwnershipKind::SharedLoan {
            return Ok(());
        }
        let Some(witness) = self.nullable_when_call_witness(contract)? else {
            return Ok(());
        };
        let Some(loan) = self.loans.last().filter(|loan| {
            loan.call() == contract.call()
                && loan.argument() == contract.argument()
                && loan.kind() == LoanKind::Shared
        }) else {
            return Err(invalid(contract.argument()));
        };
        if !matches!(loan.target(), UnitLoanTarget::Place(place)
            if place.is_root() && place.root() == witness.symbol())
        {
            return Ok(());
        }
        self.nullable_whens.record_use(UnitNullableWhenUseFact {
            call: contract.call(),
            argument: contract.argument(),
            witness,
            kind: UnitNullableWhenUseKind::Observe,
            delivery: None,
            loan: Some(loan.clone()),
            moved_span: None,
        })
    }
}
