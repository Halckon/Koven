//! Evaluate only qualified, acyclic constants; runtime short-circuit does not alter syntax edges.

use super::super::*;
use crate::parser::Expression;
use crate::type_checking::{
    constant_evaluation::{self, ConstantEvaluationContext},
    constant_value::ConstValue as Value,
};

impl Checker<'_> {
    pub(in crate::type_checking::checker) fn evaluate_constants(
        &mut self,
    ) -> Result<(), TypeCheckingError> {
        let mut remaining = vec![0; self.symbol_kinds.len()];
        let mut reverse = vec![Vec::new(); self.symbol_kinds.len()];
        let mut ready = std::collections::VecDeque::new();
        for (&symbol, dependencies) in &self.constant_dependencies {
            remaining[symbol.index()] = dependencies.len();
            if dependencies.is_empty() {
                ready.push_back(symbol);
            }
            for target in dependencies {
                reverse[target.index()].push(symbol);
            }
        }
        while let Some(symbol) = ready.pop_front() {
            if self.constant_dependencies[&symbol]
                .iter()
                .all(|target| self.constant_values.contains_key(target))
                && let Some(&item) = self.constant_items.get(&symbol)
                && let Item::Constant { initializer, .. } = self.ast().items().get(item)?.payload()
                && let Some(value) = constant_evaluation::evaluate(self, *initializer)?
            {
                self.constant_values.insert(symbol, value);
            }
            for &dependent in &reverse[symbol.index()] {
                remaining[dependent.index()] -= 1;
                if remaining[dependent.index()] == 0 {
                    ready.push_back(dependent);
                }
            }
        }
        Ok(())
    }
}

impl ConstantEvaluationContext for Checker<'_> {
    type Id = ExpressionId;
    type Error = TypeCheckingError;

    fn node(&self, expression: ExpressionId) -> Result<(Expression, Span), TypeCheckingError> {
        let node = self.ast().expressions().get(expression)?;
        Ok((node.payload().clone(), node.span()))
    }

    fn child(&self, _parent: ExpressionId, local: ExpressionId) -> ExpressionId {
        local
    }

    fn text(&self, span: Span) -> Result<&str, TypeCheckingError> {
        Ok(self.sources.slice(span)?)
    }

    fn reference_value(&self, expression: ExpressionId) -> Option<Value> {
        let node = self.ast().expressions().get(expression).ok()?;
        match node.payload() {
            Expression::Name => match self.reference(node.span(), Namespace::Value) {
                Some(ReferenceTarget::Symbol(symbol)) => self.constant_values.get(symbol).cloned(),
                _ => None,
            },
            Expression::Member { .. } => self
                .associated_constant_uses
                .get(&expression.index())
                .and_then(|symbol| self.constant_values.get(symbol))
                .cloned(),
            _ => None,
        }
    }

    fn integer_value(
        &self,
        expression: ExpressionId,
        span: Span,
        kind: crate::parser::IntegerLiteralKind,
        negative: bool,
    ) -> Result<Option<Value>, TypeCheckingError> {
        let Some(TypeKind::Builtin(ty)) =
            self.expression_types[expression.index()].map(|ty| self.kind(ty))
        else {
            return Ok(None);
        };
        Ok(self
            .integer_magnitude(span, kind)?
            .and_then(|magnitude| i128::try_from(magnitude).ok())
            .and_then(|value| Value::integer(*ty, if negative { -value } else { value })))
    }

    fn evaluation_failure(&mut self, span: Span) -> Result<Option<Value>, TypeCheckingError> {
        self.emit(
            self.constant_evaluation_code,
            "constant arithmetic is outside its defined range",
            span,
        )?;
        Ok(None)
    }
}
