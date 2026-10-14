//! 单文件表达式 value-category；调用检查继续消费同一事实。
use crate::parser::Expression;

use super::*;

impl Checker<'_> {
    pub(super) fn classify_expression_category(
        &self,
        expression: ExpressionId,
        ty: TypeId,
    ) -> ExpressionCategory {
        if self
            .associated_constant_uses
            .contains_key(&expression.index())
        {
            return ExpressionCategory::Temporary;
        }
        let Ok(node) = self.ast().expressions().get(expression) else {
            return ExpressionCategory::Temporary;
        };
        match node.payload() {
            Expression::This => ExpressionCategory::Place,
            Expression::Group { expression } => self.expression_categories[expression.index()],
            Expression::Name => {
                let Some(ReferenceTarget::Symbol(symbol)) =
                    self.reference(node.span(), Namespace::Value)
                else {
                    return ExpressionCategory::Temporary;
                };
                if matches!(
                    self.symbol_kinds[symbol.index()],
                    SymbolKind::Variable
                        | SymbolKind::Field
                        | SymbolKind::ValueParameter
                        | SymbolKind::LambdaParameter
                        | SymbolKind::ForBinding
                        | SymbolKind::DestructuringBinding
                ) {
                    ExpressionCategory::Place
                } else {
                    ExpressionCategory::Temporary
                }
            }
            Expression::Member { .. }
                if self
                    .is_read_only_container_size(expression)
                    .unwrap_or(false) =>
            {
                ExpressionCategory::Temporary
            }
            Expression::Member { .. } | Expression::Index { .. }
                if self.types.get(ty).is_some()
                    && !matches!(self.kind(ty), TypeKind::Error | TypeKind::Deferred(_)) =>
            {
                ExpressionCategory::Place
            }
            _ => ExpressionCategory::Temporary,
        }
    }
}
