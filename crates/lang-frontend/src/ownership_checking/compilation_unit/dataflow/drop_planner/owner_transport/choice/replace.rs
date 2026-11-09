//! One checked root replace: selected old value out, literal producer into the root.
use super::*;
use crate::ownership_checking::UnitDropPoint;

pub(super) struct Admission {
    call: UnitExpressionId,
    argument: Vec<UnitExpressionId>,
    symbol: UnitSymbolId,
}

#[derive(Default)]
pub(super) struct Frame {
    incoming: super::Frame,
    owner: Option<UnitOwnerId>,
    pending: bool,
    committed: bool,
    returned: Option<UnitExpressionId>,
    completed: bool,
    bound: bool,
    root_dropped: bool,
}

pub(super) fn admit(
    checker: &Checker<'_>,
    root: UnitSymbolId,
    symbol: UnitSymbolId,
    initializer: ExpressionId,
) -> Option<Admission> {
    let call = checker.unit_expression(initializer);
    if checker.is_constant_use(initializer)
        || checker
            .typed
            .expression_type(call)
            .and_then(|ty| checker.typed.types().get(ty))
            != Some(&UnitTypeKind::Builtin(BuiltinType::String))
    {
        return None;
    }
    let Expression::Call { arguments, .. } = checker
        .parsed
        .ast()
        .expressions()
        .get(initializer)
        .ok()?
        .payload()
    else {
        return None;
    };
    let plan = checker.ownership_primitives.iter().find(|plan| {
        plan.descriptor().expression() == call
            && plan.descriptor().kind() == OwnershipPrimitiveKind::Replace
            && plan.places().len() == 1
            && plan.places()[0].is_root()
            && plan.places()[0].root() == root
            && checker.typed.types().get(plan.descriptor().value_type())
                == Some(&UnitTypeKind::Builtin(BuiltinType::String))
    })?;
    if arguments.len() != 2
        || plan.descriptor().operands().len() != 2
        || arguments
            .iter()
            .zip(plan.descriptor().operands())
            .any(|(argument, operand)| checker.unit_expression(argument.value) != operand)
    {
        return None;
    }
    let argument = chain(checker, arguments[1].value, None)?;
    if argument.iter().any(|id| {
        checker
            .typed
            .expression_type(*id)
            .and_then(|ty| checker.typed.types().get(ty))
            != Some(&UnitTypeKind::Builtin(BuiltinType::String))
    }) {
        return None;
    }
    Some(Admission {
        call,
        argument,
        symbol,
    })
}

impl ChoiceTransaction {
    pub(super) fn replacement_string(&mut self, expression: UnitExpressionId) -> bool {
        if self
            .admission
            .replacement
            .as_ref()
            .is_none_or(|replacement| replacement.argument.first() != Some(&expression))
        {
            return false;
        }
        let frame = &self.replacement;
        if !self.bound || self.dropped || frame.owner.is_some() || frame.pending || frame.committed
        {
            self.abort();
            return true;
        }
        let owner = UnitOwnerId {
            identity: self.catalog.identity.clone(),
            callable: self.catalog.callable,
            index: self.catalog.definitions.len(),
        };
        self.catalog.definitions.push(UnitOwnerDefinition {
            id: owner.clone(),
            origin: UnitOwnerOrigin::String(expression),
        });
        self.suffix.push(Event::Create(expression, owner.clone()));
        self.replacement.owner = Some(owner.clone());
        self.replacement.incoming.result = Some((expression, owner));
        true
    }

    pub(super) fn replacement_expression(
        &mut self,
        expression: UnitExpressionId,
        consuming: bool,
    ) -> bool {
        let Some(admission) = &self.admission.replacement else {
            return false;
        };
        if expression == admission.call {
            let frame = &mut self.replacement;
            if !consuming
                || !frame.committed
                || !frame.root_dropped
                || frame.completed
                || frame.bound
                || frame.returned != Some(expression)
                || self.dropped
            {
                self.abort();
            } else {
                frame.completed = true;
            }
            return true;
        }
        let Some(position) = admission.argument.iter().position(|id| *id == expression) else {
            return false;
        };
        let frame = &mut self.replacement;
        let from = if position == 0 {
            expression
        } else {
            admission.argument[position - 1]
        };
        if !consuming
            || !self.bound
            || self.dropped
            || frame.pending
            || frame.committed
            || frame.incoming.completed != position
            || frame.incoming.result.as_ref().map(|(id, _)| *id) != Some(from)
        {
            self.abort();
            return true;
        }
        let Some(owner) = frame.owner.clone() else {
            self.abort();
            return true;
        };
        if position != 0 {
            self.suffix.push(Event::Forward(
                from,
                expression,
                Value::Definition(owner.clone()),
            ));
            frame.incoming.result = Some((expression, owner));
        }
        frame.incoming.completed += 1;
        true
    }

    pub(in super::super::super) fn replacement_pending(
        &mut self,
        call: UnitExpressionId,
        argument: UnitExpressionId,
        registered: bool,
    ) {
        if !self.valid {
            return;
        }
        let Some(admission) = &self.admission.replacement else {
            self.abort();
            return;
        };
        let frame = &mut self.replacement;
        if !registered
            || !self.bound
            || self.dropped
            || call != admission.call
            || admission.argument.last() != Some(&argument)
            || frame.pending
            || frame.committed
            || frame.incoming.completed != admission.argument.len()
            || frame.incoming.result.as_ref().map(|(id, _)| *id) != Some(argument)
        {
            self.abort();
            return;
        }
        let Some((_, owner)) = frame.incoming.result.take() else {
            self.abort();
            return;
        };
        self.suffix
            .push(Event::Pending(call, argument, Value::Definition(owner)));
        frame.pending = true;
    }

    pub(in super::super::super) fn replacement_committed(
        &mut self,
        call: UnitExpressionId,
        root: Option<UnitSymbolId>,
        argument: Option<UnitExpressionId>,
        pending: bool,
    ) {
        if !self.valid {
            return;
        }
        let Some(admission) = &self.admission.replacement else {
            self.abort();
            return;
        };
        let frame = &self.replacement;
        if !pending
            || !self.bound
            || self.dropped
            || call != admission.call
            || root != Some(self.admission.symbol)
            || argument != admission.argument.last().copied()
            || !frame.pending
            || frame.committed
            || frame.incoming.result.is_some()
            || frame.returned.is_some()
        {
            self.abort();
            return;
        }
        let (Some(owner), Some(argument)) = (frame.owner.clone(), argument) else {
            self.abort();
            return;
        };
        self.suffix.push(Event::Replace {
            call,
            argument,
            root: self.admission.symbol,
            previous: Value::Choice(self.choice_id()),
            incoming: Value::Definition(owner),
        });
        self.replacement.pending = false;
        self.replacement.committed = true;
        self.replacement.returned = Some(call);
    }

    pub(super) fn replacement_bound(
        &mut self,
        symbol: UnitSymbolId,
        initializer: UnitExpressionId,
    ) -> bool {
        let Some(admission) = &self.admission.replacement else {
            return false;
        };
        if symbol != admission.symbol {
            return false;
        }
        let frame = &self.replacement;
        if !self.bound
            || self.dropped
            || !frame.committed
            || !frame.root_dropped
            || !frame.completed
            || frame.bound
            || frame.returned != Some(initializer)
            || initializer != admission.call
        {
            self.abort();
            return true;
        }
        self.suffix.push(Event::Bind(
            symbol,
            initializer,
            Value::Choice(self.choice_id()),
        ));
        self.replacement.returned = None;
        self.replacement.bound = true;
        true
    }

    pub(super) fn replacement_dropped(
        &mut self,
        point: UnitDropPoint,
        symbol: UnitSymbolId,
    ) -> bool {
        let Some(admission) = &self.admission.replacement else {
            return false;
        };
        let frame = &self.replacement;
        if symbol == self.admission.symbol {
            if !self.bound
                || self.dropped
                || !frame.committed
                || frame.pending
                || frame.root_dropped
                || frame.completed
                || frame.bound
                || frame.returned != Some(admission.call)
                || point != UnitDropPoint::CallReturn(admission.call)
            {
                self.abort();
                return true;
            }
            let Some(owner) = frame.owner.clone() else {
                self.abort();
                return true;
            };
            self.suffix
                .push(Event::Drop(point, symbol, Value::Definition(owner)));
            self.replacement.root_dropped = true;
        } else if symbol == admission.symbol
            && !self.dropped
            && frame.bound
            && frame.root_dropped
            && frame.returned.is_none()
            && point == self.admission.drop_point
        {
            self.suffix
                .push(Event::Drop(point, symbol, Value::Choice(self.choice_id())));
            self.dropped = true;
        } else {
            self.abort();
        }
        true
    }
}
