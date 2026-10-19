//! Completed ordinary terminals consume the provider's actual carried owner once.
use super::*;
use crate::ssa::map_source::{
    handoff::Handoff,
    handoff_runtime::{self, Terminal},
};
use lang_frontend::{parser::Expression, type_checking::ExpressionCategory};

pub(super) fn finish(
    lowerer: &UnitExpressionLowerer<'_>,
    proof: map_iteration::handoff::Proof,
    seed: ValueId,
    span: Span,
) -> Result<(), LoweringError> {
    let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
    let terminal = match proof.terminal {
        Handoff::Return {
            expression,
            operand,
        } => {
            if !matches!(lowerer.parsed.ast().expressions().get(expression).map_err(|_| bad())?.payload(),
                Expression::Return { value: Some(actual), .. } if *actual == operand)
            {
                return Err(bad());
            }
            let at = lowerer
                .parsed
                .ast()
                .expressions()
                .get(expression)
                .map_err(|_| bad())?
                .span();
            Terminal::Return(at)
        }
        Handoff::Binding {
            root, initializer, ..
        } => {
            if lowerer
                .typed
                .expression_category(UnitExpressionId::new(lowerer.source_unit, initializer))
                != Some(ExpressionCategory::Place)
            {
                return Err(bad());
            }
            let fact = proof.drop(lowerer.owned, span)?.ok_or_else(bad)?;
            let UnitDropPoint::CallReturn(call) = fact.point() else {
                return Err(bad());
            };
            if lowerer.bindings.contains_key(&root) || lowerer.borrow_bindings.contains_key(&root) {
                return Err(bad());
            }
            let at = lowerer
                .parsed
                .ast()
                .expressions()
                .get(call.expression())
                .map_err(|_| bad())?
                .span();
            let resolved = lowerer
                .source_plan
                .call_site(lowerer.source_token, call)
                .ok_or_else(bad)?;
            let callee = *lowerer.function_ids.get(resolved.key()).ok_or_else(bad)?;
            Terminal::Drop {
                marker: fact.value_origin(),
                call: at,
                callee,
            }
        }
    };
    let (actual, delivered) = handoff_runtime::complete(lowerer.function, span, terminal)?;
    if actual != EntityId::Value(seed)
        || lowerer.pending_operands.contains(&delivered)
        || lowerer
            .temporaries
            .values()
            .any(|v| EntityId::Value(*v) == delivered)
        || lowerer.bindings.values().any(
            |v| matches!(v, LoweredValue::Value(owner) if EntityId::Value(*owner) == delivered),
        )
    {
        return Err(bad());
    }
    Ok(())
}
