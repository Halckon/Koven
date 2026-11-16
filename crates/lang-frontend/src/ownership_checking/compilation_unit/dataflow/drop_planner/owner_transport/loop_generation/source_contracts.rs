//! Actual local AST/typed/contracts agreement, never a final-owned permission.
use super::*;
impl DropPlanner<'_, '_> {
    pub(super) fn source_contracts<const N: usize>(
        &self,
        allowed_calls: [UnitExpressionId; N],
        expected_count: usize,
    ) -> Option<Vec<crate::ownership_checking::UnitCallArgumentOwnershipContract>> {
        let c = self.checker;
        if allowed_calls
            .iter()
            .any(|call| call.source_unit() != c.source_unit)
            || allowed_calls
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != N
        {
            return None;
        }
        let mut contracts = Vec::new();
        for actual_call in allowed_calls {
            let ast = c.parsed.ast();
            let Expression::Call { arguments, .. } = ast
                .expressions()
                .get(actual_call.expression())
                .ok()?
                .payload()
            else {
                return None;
            };
            let typed = c.typed.call(actual_call)?;
            let actual = c.contracts_by_call.get(&actual_call)?;
            if actual.len() != arguments.len() || typed.arguments().len() != arguments.len() {
                return None;
            }
            for ((contract, argument), descriptor) in
                actual.iter().zip(arguments).zip(typed.arguments())
            {
                let kind = match descriptor.mode() {
                    crate::type_checking::ParameterMode::Value => {
                        crate::ownership_checking::UnitCallArgumentOwnershipKind::Value
                    }
                    crate::type_checking::ParameterMode::Borrow => {
                        crate::ownership_checking::UnitCallArgumentOwnershipKind::SharedLoan
                    }
                    crate::type_checking::ParameterMode::Inout => {
                        crate::ownership_checking::UnitCallArgumentOwnershipKind::ExclusiveLoan
                    }
                };
                if contract.call() != actual_call
                    || contract.argument() != c.unit_expression(argument.value)
                    || contract.parameter_index() != descriptor.parameter_index()
                    || contract.parameter_type() != descriptor.parameter_type()
                    || contract.category() != descriptor.category()
                    || contract.kind() != kind
                    || contract.crosses_thread()
                    || descriptor.crosses_thread()
                    || contract.argument_span()
                        != ast.expressions().get(argument.value).ok()?.span()
                    || contract.call_span()
                        != ast.expressions().get(actual_call.expression()).ok()?.span()
                {
                    return None;
                }
                contracts.push(*contract);
            }
        }
        if contracts.len() != expected_count || c.contracts_by_call.len() != N {
            return None;
        }
        Some(contracts)
    }
}
