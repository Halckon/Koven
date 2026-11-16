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
    pub(super) abi: Vec<SsaTypeId>,
    pub(super) modes: Vec<ParameterMode>,
    pub(super) source_index: usize,
    expression: UnitExpressionId,
    symbol: UnitSymbolId,
    span: Span,
    flag_index: usize,
    unused_index: Option<usize>,
}
impl Source {
    pub(super) fn collect(
        inputs: &RefineInputs<'_>,
        fact: &UnitLoopNoCarryFact,
        types: &RefineTypes<'_>,
    ) -> Option<Self> {
        if !matches!(inputs.parameters.len(), 2 | 3)
            || inputs.modes.len() != inputs.parameters.len()
            || fact.loop_statement().source_unit() != inputs.source_unit
        {
            return None;
        }
        let source_index = inputs.parameters.iter().position(|p| *p == fact.source())?;
        if inputs.modes[source_index] != ParameterMode::Borrow
            || inputs
                .modes
                .iter()
                .any(|m| !matches!(m, ParameterMode::Borrow | ParameterMode::Value))
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
        let flag_index = inputs.parameters.iter().position(|p| *p == symbol)?;
        if flag_index == source_index {
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
            .collect::<Option<Vec<_>>>()?;
        Some(Self {
            abi,
            modes: inputs.modes.clone(),
            source_index,
            expression: UnitExpressionId::new(inputs.source_unit, *condition),
            symbol,
            span: node.span(),
            flag_index,
            unused_index: (0..inputs.parameters.len())
                .find(|i| *i != source_index && *i != flag_index),
        })
    }

    pub(super) fn validates_entry_transport(&self, function: &Function) -> bool {
        let Some(unused) = self.unused_index else {
            return self.abi.len() == 2;
        };
        if self.abi.len() != 3 || unused == self.source_index || unused == self.flag_index {
            return false;
        }
        let Some(entry) = function.entry_block().and_then(|id| function.block(id)) else {
            return false;
        };
        if entry.parameters.len() != 3 {
            return false;
        }
        let Some(term) = &entry.terminator else {
            return false;
        };
        let crate::ssa::model::TerminatorKind::Branch(edge) = &term.kind else {
            return false;
        };
        let expected: BTreeSet<_> = entry
            .parameters
            .iter()
            .enumerate()
            .filter_map(|(i, e)| (i != unused).then_some(*e))
            .collect();
        // Exact role identities here; ordered target correspondence is already checked by Receipt::validate.
        edge.arguments.len() == 2
            && edge.arguments.iter().copied().collect::<BTreeSet<_>>() == expected
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
