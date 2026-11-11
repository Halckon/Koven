//! Observe actual Take -> delivery -> cleanup, then require the body's real Ok(false).
use super::*;
use crate::ownership_checking::compilation_unit::owner_transport::{
    ReplaceBreakSuffix, ReturnConsumerTarget,
};
use crate::ownership_checking::{
    UnitOwnerOpenLoopReturnV1, UnitOwnerReturnActionV1 as A, UnitOwnerReturnEndpointV1,
};

pub(super) struct ReturnInput {
    body_target: ReturnConsumerTarget,
    false_target: ReturnConsumerTarget,
    taken: Option<UnitOwnerReturnEndpointV1>,
    body_return: Option<UnitOwnerReturnEndpointV1>,
    false_return: Option<UnitOwnerReturnEndpointV1>,
    body_stopped: bool,
    prefix: Option<UnitOwnerOpenLoopPrefixV1>,
    valid: bool,
}
impl ReturnInput {
    pub(super) fn admit(
        checker: &Checker<'_>,
        statement: StatementId,
        condition: ExpressionId,
        root: UnitSymbolId,
        suffix: &ReplaceBreakSuffix,
    ) -> Option<Self> {
        // This terminal variant deliberately admits only the original parameter Name.
        if !matches!(
            checker
                .parsed
                .ast()
                .expressions()
                .get(condition)
                .ok()?
                .payload(),
            Expression::Name
        ) {
            return None;
        }
        let ReplaceBreakSuffix::Return(false_target) = suffix else {
            return None;
        };
        let body_target =
            super::super::super::replace_return_consumer::admit(checker, statement, root)?;
        if [false_target, &body_target]
            .iter()
            .any(|target| target.binding.is_some() || target.replacement.is_some())
        {
            return None;
        }
        Some(Self {
            body_target,
            false_target: false_target.clone(),
            taken: None,
            body_return: None,
            false_return: None,
            body_stopped: false,
            prefix: None,
            valid: true,
        })
    }
    fn take(
        &mut self,
        expression: UnitExpressionId,
        actual: UnitOwnerReplaceBreakStateV1,
        body: bool,
    ) {
        let target = if body {
            &self.body_target
        } else {
            &self.false_target
        };
        if !self.valid
            || self.taken.is_some()
            || expression != target.payload
            || (body && (self.body_return.is_some() || self.body_stopped || self.prefix.is_some()))
            || (!body
                && (!self.body_stopped || self.prefix.is_none() || self.false_return.is_some()))
        {
            self.valid = false;
            return;
        }
        self.taken = Some(UnitOwnerReturnEndpointV1 {
            target: target.clone(),
            actions: vec![A::Take {
                expression,
                root: actual.symbol,
                owner: actual.owner(),
            }],
            taken: actual,
            cleaned: false,
        });
    }
    fn deliver(
        &mut self,
        expression: UnitExpressionId,
        payload: UnitExpressionId,
        owner: Option<&UnitOwnerId>,
        state: &ValueState,
        body: bool,
    ) {
        let Some(taken) = &mut self.taken else {
            self.valid = false;
            return;
        };
        let expected = if body {
            &self.body_target
        } else {
            &self.false_target
        };
        if !self.valid
            || &taken.target != expected
            || expression != expected.expression
            || payload != expected.payload
            || taken.actions.len() != 1
            || taken.cleaned
            || !empty_boundary(state)
            || (body && owner != Some(&taken.taken.owner))
            || (!body && owner.is_some())
        {
            self.valid = false;
            return;
        }
        taken.actions.push(A::Return {
            expression,
            payload,
            owner: taken.owner(),
        });
    }
    fn cleanup(&mut self, expression: UnitExpressionId, state: &ValueState, body: bool) {
        let Some(mut taken) = self.taken.take() else {
            self.valid = false;
            return;
        };
        let expected = if body {
            &self.body_target
        } else {
            &self.false_target
        };
        if !self.valid
            || &taken.target != expected
            || expression != expected.expression
            || !empty_boundary(state)
            || taken.cleaned
        {
            self.valid = false;
            return;
        }
        taken.cleaned = true;
        if !taken.is_consistent() {
            self.valid = false;
            return;
        }
        let destination = if body {
            &mut self.body_return
        } else {
            &mut self.false_return
        };
        if destination.is_some() {
            self.valid = false;
            return;
        }
        *destination = Some(taken);
    }
    pub(super) fn complete_body(&mut self, state: &ValueState) -> bool {
        if !self.valid
            || self.body_stopped
            || self.taken.is_some()
            || self.body_return.is_none()
            || self.false_return.is_some()
            || !empty_boundary(state)
        {
            return false;
        }
        self.body_stopped = true;
        true
    }
    pub(super) fn body_snapshot(&self) -> Option<UnitOwnerReplaceBreakStateV1> {
        (self.valid && self.body_stopped)
            .then(|| {
                self.body_return
                    .as_ref()
                    .map(|endpoint| endpoint.taken.clone())
            })
            .flatten()
    }
    pub(super) fn save_prefix(&mut self, prefix: UnitOwnerOpenLoopPrefixV1) {
        if self.prefix.is_some() {
            self.valid = false;
            return;
        }
        self.prefix = Some(prefix);
    }
    fn finish(&mut self) -> Option<UnitOwnerOpenLoopObservationV1> {
        if !self.valid || !self.body_stopped || self.taken.is_some() {
            return None;
        }
        let value = UnitOwnerOpenLoopReturnV1 {
            prefix: self.prefix.take()?,
            body_return: self.body_return.take()?,
            condition_false_return: self.false_return.take()?,
            body_stopped: self.body_stopped,
        };
        value
            .is_consistent()
            .then_some(UnitOwnerOpenLoopObservationV1::ReturnDelivered(Box::new(
                value,
            )))
    }
}
impl OpenLoopInput {
    pub(in super::super) fn return_take(
        &mut self,
        expression: UnitExpressionId,
        actual: Option<&OwnedValue>,
        identity: Option<&std::sync::Arc<()>>,
    ) {
        let Some(returning) = &self.returning else {
            return;
        };
        let body = identity.is_some();
        let target = if body {
            &returning.body_target
        } else {
            &returning.false_target
        };
        if expression != target.payload {
            return;
        }
        let snapshot = actual.and_then(super::super::super::replace_consumer::snapshot);
        let expected = self.expected.as_ref();
        let valid = match (actual, &snapshot, expected) {
            (Some(actual), Some(snapshot), Some(expected)) => {
                actual.symbol == expected.symbol
                    && actual.declaration == expected.declaration
                    && actual.scope_depth == expected.scope_depth
                    && match identity {
                        Some(identity) => {
                            snapshot.origin == self.call_origin
                                && actual.owner != expected.owner
                                && std::sync::Arc::ptr_eq(&snapshot.owner.identity, identity)
                        }
                        None => actual == expected,
                    }
            }
            _ => false,
        };
        let returning = self.returning.as_mut().expect("checked terminal candidate");
        if valid {
            returning.take(
                expression,
                snapshot.expect("validated actual snapshot"),
                body,
            );
        } else {
            returning.valid = false;
        }
    }
    pub(in super::super) fn return_deliver(
        &mut self,
        expression: UnitExpressionId,
        payload: UnitExpressionId,
        owner: Option<&UnitOwnerId>,
        state: &ValueState,
        body: bool,
    ) {
        if let Some(returning) = &mut self.returning {
            returning.deliver(expression, payload, owner, state, body);
        }
    }
    pub(in super::super) fn return_cleanup(
        &mut self,
        expression: UnitExpressionId,
        state: &ValueState,
        body: bool,
    ) -> Option<UnitOwnerOpenLoopObservationV1> {
        let returning = self.returning.as_mut()?;
        returning.cleanup(expression, state, body);
        if body { None } else { returning.finish() }
    }
}
