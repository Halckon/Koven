//! Direct-call When uses retain the analysis owner's loan or whole-root delivery.
use super::Builder;
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::EntityType,
    refine::{
        body::{ROp, Slot},
        nullable_when,
    },
};
use lang_frontend::{
    ast::ExpressionId,
    ownership_checking::{
        LoanKind, UnitCallArgumentOwnershipKind, UnitDropPoint, UnitLoanTarget,
        UnitNullableWhenUseKind, UnitValueDeliveryKind,
    },
    parser::Expression,
    source::Span,
    type_checking::{
        CallableResultSource, ExpressionCategory, ParameterMode, UnitCallTarget,
        UnitCallableTarget, UnitTypeId,
    },
};

impl Builder<'_, '_> {
    /// The bool distinguishes a shared view from an already-extracted pending Value.
    pub(super) fn nullable_when_argument(
        &mut self,
        call_expression: ExpressionId,
        argument: ExpressionId,
        mode: ParameterMode,
        parameter: UnitTypeId,
        span: Span,
    ) -> Result<Option<(Slot, bool)>, LoweringError> {
        let inputs = self.inputs;
        let call = self.unit(call_expression);
        let argument_id = self.unit(argument);
        let Some(witness) = inputs.typed.nullable_when_use(argument_id) else {
            return Ok(None);
        };
        let missing = || error(LoweringErrorKind::MissingFact, span);
        let fact = inputs
            .owned
            .nullable_when_use(argument_id)
            .ok_or_else(missing)?;
        let source = nullable_when::source(inputs, witness.control())?;
        let descriptor = source.plan.descriptor();
        let call_descriptor = inputs.typed.call(call).ok_or_else(missing)?;
        let UnitCallTarget::Declaration(target) = call_descriptor.target() else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        let route = inputs
            .source_plan
            .call_site(inputs.source_token, call)
            .ok_or_else(missing)?;
        if call_descriptor.receiver().is_some()
            || call_descriptor.result_source() != CallableResultSource::Owned
            || call_descriptor.aborts()
            || call_descriptor.prints_line()
            || !route.delegation().is_empty()
            || !route.key().callable_arguments().is_empty()
            || route.key().target() != UnitCallableTarget::Declaration(target)
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let (Expression::Call { arguments, .. }, call_span) = self.node(call_expression)? else {
            return Err(missing());
        };
        let indices = arguments
            .iter()
            .enumerate()
            .filter(|(_, actual)| actual.value == argument)
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        let [index] = indices.as_slice() else {
            return Err(missing());
        };
        let mappings = call_descriptor
            .arguments()
            .iter()
            .filter(|mapping| mapping.argument_index() == *index)
            .collect::<Vec<_>>();
        let [mapping] = mappings.as_slice() else {
            return Err(missing());
        };
        let contracts = inputs
            .owned
            .call_argument_contracts_for(call)
            .filter(|contract| contract.argument() == argument_id)
            .collect::<Vec<_>>();
        let [contract] = contracts.as_slice() else {
            return Err(missing());
        };
        let expected = match mode {
            ParameterMode::Borrow => UnitCallArgumentOwnershipKind::SharedLoan,
            ParameterMode::Value => UnitCallArgumentOwnershipKind::Value,
            ParameterMode::Inout => return Err(error(LoweringErrorKind::UnsupportedNode, span)),
        };
        let node_span = self.span(argument);
        if fact.call() != call
            || fact.argument() != argument_id
            || fact.witness() != witness
            || witness.expression() != argument_id
            || witness.entry_index() != 1
            || descriptor.stable_symbol() != Some(witness.symbol())
            || descriptor.subject() != witness.subject()
            || descriptor.source_generation() != Some(witness.source_generation())
            || descriptor.nullable_type() != witness.nullable_type()
            || descriptor.inner_type() != witness.inner_type()
            || descriptor.entries()[1].body() != witness.body()
            || source
                .plan
                .uses()
                .iter()
                .filter(|candidate| *candidate == fact)
                .count()
                != 1
            || mapping.mode() != mode
            || mapping.crosses_thread()
            || contract.crosses_thread()
            || mapping.parameter_index() != contract.parameter_index()
            || mapping.parameter_type() != contract.parameter_type()
            || mapping.category() != contract.category()
            || contract.category() != ExpressionCategory::Place
            || contract.kind() != expected
            || contract.call_span() != call_span
            || contract.argument_span() != node_span
            || self.c(parameter) != self.c(witness.inner_type())
        {
            return Err(missing());
        }
        let mut operands = Vec::new();
        let mut current = argument;
        loop {
            let id = self.unit(current);
            let actual = inputs.typed.nullable_when_use(id).ok_or_else(missing)?;
            if actual.expression() != id
                || actual.reference() != witness.reference()
                || actual.control() != witness.control()
                || actual.subject() != witness.subject()
                || actual.entry_index() != witness.entry_index()
                || actual.body() != witness.body()
                || actual.symbol() != witness.symbol()
                || actual.source_generation() != witness.source_generation()
                || actual.nullable_type() != witness.nullable_type()
                || actual.inner_type() != witness.inner_type()
            {
                return Err(missing());
            }
            operands.push(id);
            match self.node(current)?.0 {
                Expression::Group { expression } => current = expression,
                Expression::Name
                    if id == witness.reference()
                        && self.reference(self.span(current))? == witness.symbol() =>
                {
                    break;
                }
                _ => return Err(missing()),
            }
        }
        let owner = *self.bindings.get(&witness.symbol()).ok_or_else(missing)?;
        let view = self
            .active_views
            .iter()
            .rev()
            .find(|view| {
                view.symbol == witness.symbol()
                    && view.when_scope == Some((witness.control(), owner))
            })
            .copied()
            .ok_or_else(missing)?;
        let inner = self.ssa_type(self.c(witness.inner_type()), span)?;
        if mode == ParameterMode::Borrow {
            let loan = fact.loan().ok_or_else(missing)?;
            let loans = inputs
                .owned
                .loans()
                .iter()
                .filter(|loan| loan.call() == call && loan.argument() == argument_id)
                .collect::<Vec<_>>();
            if fact.kind() != UnitNullableWhenUseKind::Observe
                || fact.delivery().is_some()
                || fact.moved_span().is_some()
                || loans.as_slice() != [loan]
                || loan.kind() != LoanKind::Shared
                || loan.begin_span() != contract.loan_begin_span()
                || loan.end_span() != call_span
                || loan.parameter_span() != contract.parameter_span()
                || !matches!(loan.target(), UnitLoanTarget::Place(place) if place.is_root() && place.root() == witness.symbol())
                || inputs
                    .owned
                    .value_deliveries()
                    .iter()
                    .any(|value| value.call() == call && value.argument() == argument_id)
            {
                return Err(missing());
            }
            return Ok(Some((view.slot, true)));
        }
        let delivery = fact.delivery().ok_or_else(missing)?;
        let deliveries = inputs
            .owned
            .value_deliveries()
            .iter()
            .filter(|value| value.call() == call && value.argument() == argument_id)
            .collect::<Vec<_>>();
        if fact.kind() != UnitNullableWhenUseKind::Consume
            || fact.loan().is_some()
            || deliveries.as_slice() != [delivery]
            || delivery.kind() != UnitValueDeliveryKind::Move
            || delivery.span() != node_span
            || delivery.parameter_span() != contract.parameter_span()
            || delivery
                .place()
                .is_none_or(|place| !place.is_root() || place.root() != witness.symbol())
            || fact.moved_span() != Some(self.span(witness.reference().expression()))
            || inputs
                .owned
                .loans()
                .iter()
                .any(|loan| loan.call() == call && loan.argument() == argument_id)
        {
            return Err(missing());
        }
        // Like the original path, every fallible point check precedes the one take.
        for &operand in &operands {
            let point = UnitDropPoint::AfterExpression(operand);
            if inputs
                .owned
                .drops()
                .iter()
                .any(|fact| fact.point() == point)
                || inputs
                    .owned
                    .borrow_results()
                    .ends()
                    .iter()
                    .any(|fact| fact.point() == point)
                || inputs.owned.iteration_cleanup_at(point).is_some()
                || inputs
                    .owned
                    .conditional_receiver_drops()
                    .iter()
                    .any(|fact| fact.point() == point)
            {
                return Err(missing());
            }
        }
        let value = self.value(
            ROp::NullableTake {
                owner,
                proof: view.slot,
            },
            EntityType::Value(inner),
            span,
        )?;
        self.consume_views_of(witness.symbol());
        // The mapping also names the owner on the mutually exclusive null path.
        // Consumption is path-local, like ordinary moves; do not erase that mapping.
        self.dead.insert(witness.symbol());
        self.temporaries.retain(|_, actual| *actual != owner);
        self.register_temporary(argument, value)?;
        Ok(Some((value, false)))
    }

    /// Observe cleanup follows the actual transparent chain, leaf first, after the call.
    pub(super) fn nullable_when_observe_drops(
        &mut self,
        argument: ExpressionId,
    ) -> Result<bool, LoweringError> {
        if self
            .inputs
            .owned
            .nullable_when_use(self.unit(argument))
            .is_none()
        {
            return Ok(false);
        }
        let mut operands = Vec::new();
        let mut current = argument;
        loop {
            operands.push(self.unit(current));
            match self.node(current)?.0 {
                Expression::Group { expression } => current = expression,
                _ => break,
            }
        }
        for operand in operands.into_iter().rev() {
            self.drops(UnitDropPoint::AfterExpression(operand))?;
        }
        Ok(true)
    }
}
