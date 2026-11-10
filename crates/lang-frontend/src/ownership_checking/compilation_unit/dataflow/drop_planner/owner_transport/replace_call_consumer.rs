//! Preserve the actual P/Q states through the pending Value operand, then transfer once.
use super::replace_consumer::snapshot;
use super::*;
use crate::ownership_checking::compilation_unit::owner_transport::CallConsumerTarget;
use crate::ownership_checking::{
    UnitOwnerReplaceBreakCallActionV1 as Action, UnitOwnerReplaceBreakCallConsumerV1,
    UnitOwnerReplaceBreakObservationV1,
};
use crate::type_checking::{CallableResultSource, UnitCallTarget, UnitCallableTarget};

pub(super) fn admit(
    checker: &Checker<'_>,
    statement: StatementId,
    root: UnitSymbolId,
) -> Option<CallConsumerTarget> {
    let ast = checker.parsed.ast();
    let Statement::Expression { expression } = ast.statements().get(statement).ok()?.payload()
    else {
        return None;
    };
    let Expression::Call {
        callee,
        type_arguments,
        arguments,
        ..
    } = ast.expressions().get(*expression).ok()?.payload()
    else {
        return None;
    };
    let [argument] = arguments.as_slice() else {
        return None;
    };
    let argument_node = ast.expressions().get(argument.value).ok()?;
    if !type_arguments.is_empty()
        || argument.named_prefix.is_some()
        || argument.mode_marker.is_some()
        || !matches!(
            ast.expressions().get(*callee).ok()?.payload(),
            Expression::Name
        )
        || !matches!(argument_node.payload(), Expression::Name)
        || checker.reference_symbol(argument_node.span()) != Some(root)
    {
        return None;
    }
    let call_id = checker.unit_expression(*expression);
    let argument_id = checker.unit_expression(argument.value);
    let call = checker.typed.call(call_id)?;
    let UnitCallTarget::Declaration(declaration) = call.target() else {
        return None;
    };
    let declared = checker.typed.signatures().declaration(declaration)?;
    let signature = declared.callable()?;
    let [parameter] = signature.parameters() else {
        return None;
    };
    let [typed] = call.arguments() else {
        return None;
    };
    let [contract] = checker.contracts_by_call.get(&call_id)?.as_slice() else {
        return None;
    };
    if checker.names.declaration_symbol(declaration) != Some(declared.symbol())
        || signature.target() != UnitCallableTarget::Declaration(declaration)
        || !signature.has_body()
        || signature.receiver().is_some()
        || !signature.type_parameters().is_empty()
        || parameter.mode() != ParameterMode::Value
        || parameter.symbol().is_none()
        || signature.result_source() != CallableResultSource::Owned
        || call.result_source() != CallableResultSource::Owned
        || call.receiver().is_some()
        || call.aborts()
        || !call.instance().type_arguments().is_empty()
        || checker.typed.types().get(signature.return_type())
            != Some(&UnitTypeKind::Builtin(BuiltinType::Unit))
        || call.return_type() != signature.return_type()
        || checker.typed.expression_type(call_id) != Some(call.return_type())
        || checker.typed.types().get(parameter.ty())
            != Some(&UnitTypeKind::Builtin(BuiltinType::String))
        || checker.typed.expression_type(argument_id) != Some(parameter.ty())
        || typed.argument_index() != 0
        || typed.parameter_index() != 0
        || typed.mode() != ParameterMode::Value
        || typed.parameter_type() != parameter.ty()
        || typed.category() != ExpressionCategory::Place
        || typed.crosses_thread()
        || contract.call() != call_id
        || contract.argument() != argument_id
        || contract.parameter_index() != 0
        || contract.parameter_type() != parameter.ty()
        || contract.category() != typed.category()
        || contract.crosses_thread()
        || contract.kind() != UnitCallArgumentOwnershipKind::Value
        || checker.typed.expression_category(argument_id) != Some(typed.category())
        || checker.place(argument.value).ok()??.root() != root
        || !checker.place(argument.value).ok()??.is_root()
    {
        return None;
    }
    Some(CallConsumerTarget {
        call: call_id,
        argument: argument_id,
        statement: UnitStatementId::new(checker.source_unit, statement),
        target: declared.symbol(),
    })
}

#[derive(Clone, Debug)]
pub(in super::super) struct CallConsumerCarrier {
    incoming: [OwnedValue; 2],
    target: CallConsumerTarget,
    actions: Vec<Action>,
}
impl CallConsumerCarrier {
    pub(super) fn handoff(
        condition: &OwnedValue,
        body: OwnedValue,
        target: CallConsumerTarget,
        boundary: &UnitOwnerReplaceBreakObservationV1,
    ) -> Option<Self> {
        if snapshot(condition).as_ref() != Some(boundary.condition_false_exit())
            || snapshot(&body).as_ref() != Some(boundary.body_completion())
            || !boundary.is_consistent()
        {
            return None;
        }
        Some(Self {
            incoming: [condition.clone(), body],
            target,
            actions: Vec::new(),
        })
    }
    pub(super) fn take(
        mut self,
        expression: UnitExpressionId,
        actual: Option<&OwnedValue>,
    ) -> Option<Self> {
        if expression != self.target.argument
            || actual != Some(&self.incoming[0])
            || !self.actions.is_empty()
        {
            return None;
        }
        self.actions
            .push(Action::Take(expression, self.incoming[0].symbol));
        Some(self)
    }
    pub(in super::super) fn matches_pending(
        &self,
        call: UnitExpressionId,
        argument: UnitExpressionId,
        owner: Option<&UnitOwnerId>,
    ) -> bool {
        self.target.call == call
            && self.target.argument == argument
            // The legacy Unsupported observer intentionally clears scalar result identity.
            // Only this carrier retains the real P/Q alternatives after the verified Take.
            && owner.is_none()
            && self.actions == [Action::Take(argument, self.incoming[0].symbol)]
    }
    pub(in super::super) fn transfer(
        mut self,
        call: UnitExpressionId,
        argument: UnitExpressionId,
        owner: Option<&UnitOwnerId>,
    ) -> Option<UnitOwnerReplaceBreakCallConsumerV1> {
        if !self.matches_pending(call, argument, owner) {
            return None;
        }
        self.actions.push(Action::Call {
            call,
            argument,
            target: self.target.target,
        });
        Some(UnitOwnerReplaceBreakCallConsumerV1 {
            incoming: [snapshot(&self.incoming[0])?, snapshot(&self.incoming[1])?],
            target: self.target,
            actions: self.actions,
        })
    }
}
