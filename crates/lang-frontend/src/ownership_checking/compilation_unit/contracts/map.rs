//! 消费 typed Map 描述符，发布与普通调用相同的 receiver/argument 所有权合同。
use super::*;

type MapContracts = (
    Vec<UnitCallReceiverOwnershipContract>,
    Vec<UnitCallArgumentOwnershipContract>,
);

pub(super) fn collect(
    inputs: &[SourceUnitInput<'_>],
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
) -> Result<MapContracts, OwnershipCheckingError> {
    let mut calls = Vec::new();
    for descriptor in typed.map_contains_calls() {
        calls.push((
            descriptor.expression(),
            descriptor.receiver(),
            ParameterMode::Borrow,
            vec![(
                descriptor.key(),
                descriptor.key_type(),
                ParameterMode::Borrow,
            )],
        ));
    }
    for descriptor in typed.map_require_values() {
        calls.push((
            descriptor.expression(),
            descriptor.receiver(),
            ParameterMode::Borrow,
            vec![(
                descriptor.key(),
                descriptor.key_type(),
                ParameterMode::Borrow,
            )],
        ));
    }
    for descriptor in typed.map_with_values() {
        calls.push((
            descriptor.expression(),
            descriptor.receiver(),
            ParameterMode::Borrow,
            vec![
                (
                    descriptor.key(),
                    descriptor.key_type(),
                    ParameterMode::Borrow,
                ),
                (
                    descriptor.action(),
                    descriptor.action_type(),
                    ParameterMode::Borrow,
                ),
            ],
        ));
    }
    for descriptor in typed.map_gets() {
        calls.push((
            descriptor.expression(),
            descriptor.receiver(),
            ParameterMode::Borrow,
            vec![(
                descriptor.key(),
                descriptor.key_type(),
                ParameterMode::Borrow,
            )],
        ));
    }
    for descriptor in typed.map_puts() {
        calls.push((
            descriptor.expression(),
            descriptor.receiver(),
            ParameterMode::Inout,
            vec![
                (
                    descriptor.key(),
                    descriptor.key_type(),
                    ParameterMode::Value,
                ),
                (
                    descriptor.value(),
                    descriptor.value_type(),
                    ParameterMode::Value,
                ),
            ],
        ));
    }
    for descriptor in typed.map_removes() {
        calls.push((
            descriptor.expression(),
            descriptor.receiver(),
            ParameterMode::Inout,
            vec![(
                descriptor.key(),
                descriptor.key_type(),
                ParameterMode::Borrow,
            )],
        ));
    }
    let mut receivers = Vec::new();
    let mut arguments = Vec::new();
    for (call, receiver, mode, operands) in calls {
        let parsed = parsed_for_call(inputs, names, call)?;
        let node = parsed.ast().expressions().get(call.expression())?;
        // Index expressions/assignments have their own place traversal; they are not calls.
        if !matches!(node.payload(), Expression::Call { .. }) {
            continue;
        }
        let receiver_span = parsed
            .ast()
            .expressions()
            .get(receiver.expression())?
            .span();
        let receiver_type = typed
            .expression_type(receiver)
            .ok_or_else(|| invalid_unit_call(call))?;
        let category = typed
            .expression_category(receiver)
            .ok_or_else(|| invalid_unit_call(call))?;
        receivers.push(UnitCallReceiverOwnershipContract::new(
            call,
            UnitCallTarget::FunctionValue,
            UnitCallReceiverOrigin::Expression(receiver),
            receiver_type,
            category,
            ownership_kind(mode),
            receiver_span,
            node.span(),
            None,
        ));
        for (parameter_index, (argument, parameter_type, mode)) in operands.into_iter().enumerate()
        {
            let argument_span = parsed
                .ast()
                .expressions()
                .get(argument.expression())?
                .span();
            let category = typed
                .expression_category(argument)
                .ok_or_else(|| invalid_unit_call(call))?;
            arguments.push(UnitCallArgumentOwnershipContract {
                call,
                argument,
                parameter_index,
                parameter_type,
                category,
                kind: ownership_kind(mode),
                crosses_thread: false,
                argument_span,
                call_span: node.span(),
                parameter_span: None,
                loan_begin_span: argument_span,
            });
        }
    }
    Ok((receivers, arguments))
}
