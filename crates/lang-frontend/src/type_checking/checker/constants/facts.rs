//! 先完成常量阶段再检查运行时 bodies；最后一次性发布事实，避免 trial/recovery 半成品。

use super::super::*;
use crate::parser::Expression;
use crate::type_checking::{ConstantDescriptor, ConstantUseDescriptor, ValidatedConstants};

impl Checker<'_> {
    pub(in crate::type_checking::checker) fn check_constant_declarations(
        &mut self,
    ) -> Result<(), TypeCheckingError> {
        let constants = self
            .ast()
            .items()
            .iter()
            .filter_map(|(id, node)| matches!(node.payload(), Item::Constant { .. }).then_some(id))
            .collect::<Vec<_>>();
        for constant in constants {
            self.check_item(constant)?;
        }
        self.recheck_constant_dependencies()?;
        self.check_constant_cycles()?;
        self.evaluate_constants()?;
        self.constants_checked = true;
        Ok(())
    }

    pub(in crate::type_checking::checker) fn build_constant_facts(
        &self,
    ) -> Result<Option<ValidatedConstants>, TypeCheckingError> {
        if !self.constant_inputs_valid
            || !self.diagnostics.is_empty()
            || self.constant_items.len() != self.constant_values.len()
        {
            return Ok(None);
        }
        let mut declarations = Vec::new();
        for (&symbol, value) in &self.constant_values {
            let Some(ty) = self.symbol_type(symbol) else {
                return Ok(None);
            };
            let Some(dependencies) = self.constant_dependencies.get(&symbol) else {
                return Ok(None);
            };
            declarations.push(ConstantDescriptor {
                symbol,
                declaration_span: self.symbol_spans[symbol.index()],
                ty,
                value: value.clone(),
                dependencies: dependencies.clone(),
            });
        }
        let mut uses = Vec::new();
        for (expression, node) in self.ast().expressions().iter() {
            let target = match node.payload() {
                Expression::Name => match self.reference(node.span(), Namespace::Value) {
                    Some(ReferenceTarget::Symbol(symbol))
                        if self.symbol_kinds[symbol.index()] == SymbolKind::Constant =>
                    {
                        Some(*symbol)
                    }
                    _ => None,
                },
                Expression::Member { .. } => self
                    .associated_constant_uses
                    .get(&expression.index())
                    .copied(),
                _ => None,
            };
            let Some(target) = target else {
                continue;
            };
            let Some(value) = self.constant_values.get(&target) else {
                return Ok(None);
            };
            let Some(ty) = self.expression_types[expression.index()] else {
                return Ok(None);
            };
            if self.symbol_type(target) != Some(ty)
                || self.expression_categories[expression.index()] != ExpressionCategory::Temporary
            {
                return Ok(None);
            }
            uses.push(ConstantUseDescriptor {
                expression,
                target,
                ty,
                value: value.clone(),
            });
        }
        uses.sort_by_key(|usage| usage.expression.index());
        Ok(Some(ValidatedConstants {
            analysis_owner: Arc::new(()),
            declarations,
            uses,
        }))
    }
}
