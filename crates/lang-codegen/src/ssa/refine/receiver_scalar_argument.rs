//! Source-bound scalar evidence for the optional receiver receipt; no new actions.
use super::*;
use crate::ssa::model::{Definition, EntityId, Function, Operation, Origin};
use lang_frontend::parser::{Expression, IntegerLiteralKind, LiteralKind};

/// Private call-operand evidence; not a lifetime action or a public receipt field.
#[derive(Clone, Debug)]
pub(super) struct IntArgument {
    pub(super) span: Span,
    pub(super) value: i128,
    pub(super) slot: Option<Slot>,
}

impl IntArgument {
    pub(super) fn try_for_call(
        inputs: &super::super::RefineInputs<'_>,
        call: UnitExpressionId,
    ) -> Result<Option<Self>, LoweringError> {
        let fail = || error(LoweringErrorKind::MissingFact, inputs.span);
        let expressions = inputs.parsed.ast().expressions();
        let Expression::Call { arguments, .. } = expressions
            .get(call.expression())
            .map_err(|_| fail())?
            .payload()
        else {
            return Err(fail());
        };
        Ok(match arguments.as_slice() {
            [] => None,
            [argument] => {
                let node = expressions.get(argument.value).map_err(|_| fail())?;
                let Expression::Literal(LiteralKind::Integer(IntegerLiteralKind::Unsuffixed)) =
                    node.payload()
                else {
                    return Err(fail());
                };
                let span = node.span();
                let value = inputs
                    .sources
                    .slice(span)
                    .ok()
                    .and_then(|text| {
                        lang_frontend::type_checking::integer_literal_magnitude(
                            text,
                            IntegerLiteralKind::Unsuffixed,
                        )
                    })
                    .and_then(|value| i128::try_from(value).ok())
                    .ok_or_else(fail)?;
                Some(IntArgument {
                    span,
                    value,
                    slot: None,
                })
            }
            _ => return Err(fail()),
        })
    }
}

pub(super) fn matches_emitted(
    argument: Option<&IntArgument>,
    function: &Function,
    call: &crate::ssa::model::Instruction,
    arguments: &[EntityId],
    entity: &impl Fn(Option<Slot>) -> Result<EntityId, LoweringError>,
) -> Result<bool, LoweringError> {
    let Some(argument) = argument else {
        return Ok(arguments.is_empty());
    };
    let value = entity(argument.slot)?;
    let [EntityId::Value(_)] = arguments else {
        return Ok(false);
    };
    if arguments != [value] {
        return Ok(false);
    }
    let Some(Definition::InstructionResult {
        instruction,
        index: 0,
    }) = function.entity(value).map(|data| data.definition)
    else {
        return Ok(false);
    };
    let Some(producer) = function.instruction(instruction) else {
        return Ok(false);
    };
    Ok(producer.block == call.block
        && producer.origin == Origin::Source(argument.span)
        && producer.results == [value]
        && producer.operation == Operation::Constant(ScalarConstant::Integer(argument.value))
        && function.block(call.block).is_some_and(|block| {
            block
                .instructions
                .windows(2)
                .any(|pair| pair == [instruction, call.id])
        }))
}
