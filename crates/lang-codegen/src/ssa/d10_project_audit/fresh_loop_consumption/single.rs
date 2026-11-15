//! Independent typed ABI and original condition identity for one fresh scope.
use super::*;
use lang_frontend::{
    name_resolution::UnitSymbolId,
    parser::{Expression, Statement},
    source::Span,
    type_checking::{BuiltinType, UnitTypeKind},
};
#[derive(Clone)]
pub(super) struct Source {
    pub(super) abi: [SsaTypeId; 2],
    pub(super) modes: [ParameterMode; 2],
    pub(super) source_index: usize,
    expression: UnitExpressionId,
    symbol: UnitSymbolId,
    span: Span,
    flag_index: usize,
}
impl Source {
    pub(super) fn collect(
        inputs: &RefineInputs<'_>,
        fact: &UnitLoopNoCarryFact,
        types: &RefineTypes<'_>,
    ) -> Option<Self> {
        if inputs.parameters.len() != 2
            || inputs.modes.len() != 2
            || fact.loop_statement().source_unit() != inputs.source_unit
        {
            return None;
        }
        let source_index = inputs.parameters.iter().position(|p| *p == fact.source())?;
        let flag_index = 1 - source_index;
        if inputs.modes[source_index] != ParameterMode::Borrow
            || !matches!(
                inputs.modes[flag_index],
                ParameterMode::Borrow | ParameterMode::Value
            )
        {
            return None;
        }
        let ast = inputs.parsed.ast();
        let Statement::While { condition, .. } = ast
            .statements()
            .get(fact.loop_statement().statement())
            .ok()?
            .payload()
        else {
            return None;
        };
        let node = ast.expressions().get(*condition).ok()?;
        if !matches!(node.payload(), Expression::Name) {
            return None;
        }
        let symbol = *inputs
            .references
            .get(&(node.span().start(), node.span().end()))?;
        if symbol != inputs.parameters[flag_index] {
            return None;
        }
        let abi = inputs
            .parameters
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let ty = inputs.typed.symbol_type(*s)?;
                let expected = if i == source_index {
                    BuiltinType::String
                } else {
                    BuiltinType::Boolean
                };
                if inputs.typed.types().get(ty) != Some(&UnitTypeKind::Builtin(expected)) {
                    return None;
                }
                types.type_ids.get(&ty).copied()
            })
            .collect::<Option<Vec<_>>>()?
            .try_into()
            .ok()?;
        Some(Self {
            abi,
            modes: inputs.modes.as_slice().try_into().ok()?,
            source_index,
            expression: UnitExpressionId::new(inputs.source_unit, *condition),
            symbol,
            span: node.span(),
            flag_index,
        })
    }
    pub(super) fn matches(&self, receipt: &Receipt, fact: &UnitLoopNoCarryFact) -> bool {
        self.expression.source_unit() == fact.loop_statement().source_unit()
            && self.symbol != fact.source()
            && receipt.matches_flag_roles(&[(
                self.flag_index,
                self.span,
                self.modes[self.flag_index],
            )])
    }
}
