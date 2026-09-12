//! Const expression 资格与语法依赖；值求值不得在此处执行或裁掉短路 RHS。

use super::super::*;
use crate::parser::{BinaryOperator, Expression, LiteralKind, PrefixOperator, StringPart};

impl Checker<'_> {
    pub(in crate::type_checking::checker) fn record_constant_dependencies(
        &mut self,
        name: NameMarker,
        initializer: ExpressionId,
    ) -> Result<(), TypeCheckingError> {
        let NameMarker::Present(span) = name else {
            return Ok(());
        };
        let Some(symbol) = self.symbol_at(span) else {
            return Ok(());
        };
        let companion =
            self.scope_kinds[self.symbol_scopes[symbol.index()].index()] == ScopeKind::Companion;
        let mut pending = vec![initializer];
        let mut dependencies = BTreeSet::new();
        let mut expressions = Vec::new();
        let mut invalid_expression = None;
        while let Some(expression) = pending.pop() {
            expressions.push(expression);
            let node = self.ast().expressions().get(expression)?;
            let span = node.span();
            let valid = match node.payload() {
                Expression::Error => return Ok(()),
                Expression::Literal(
                    LiteralKind::Integer(_) | LiteralKind::Boolean(_) | LiteralKind::Char,
                ) => true,
                Expression::Group { expression } => {
                    pending.push(*expression);
                    true
                }
                Expression::String { parts } => {
                    parts.iter().all(|part| matches!(part, StringPart::Text(_)))
                }
                Expression::Name => match self.reference(span, Namespace::Value) {
                    Some(ReferenceTarget::Symbol(target))
                        if self.symbol_kinds[target.index()] == SymbolKind::Constant =>
                    {
                        dependencies.insert(*target);
                        true
                    }
                    Some(ReferenceTarget::Unresolved | ReferenceTarget::LaterLocal(_)) | None => {
                        return Ok(());
                    }
                    _ => false,
                },
                Expression::Member { .. } => {
                    if let Some(target) = self.associated_constant_uses.get(&expression.index()) {
                        dependencies.insert(*target);
                        true
                    } else {
                        false
                    }
                }
                Expression::Prefix {
                    operator, operand, ..
                } => {
                    let valid = self.const_operand_allows(*operand, |ty| match operator {
                        PrefixOperator::Not => ty == BuiltinType::Boolean,
                        PrefixOperator::Plus | PrefixOperator::Minus => const_integer(ty),
                    });
                    pending.push(*operand);
                    valid
                }
                Expression::Binary {
                    left,
                    operator,
                    right,
                    ..
                } => {
                    let supported = matches!(
                        operator,
                        BinaryOperator::Add
                            | BinaryOperator::Subtract
                            | BinaryOperator::Multiply
                            | BinaryOperator::Divide
                            | BinaryOperator::Remainder
                            | BinaryOperator::Less
                            | BinaryOperator::Greater
                            | BinaryOperator::LessEqual
                            | BinaryOperator::GreaterEqual
                            | BinaryOperator::Equal
                            | BinaryOperator::NotEqual
                            | BinaryOperator::LogicalAnd
                            | BinaryOperator::LogicalOr
                    );
                    let accepts = |ty| match operator {
                        BinaryOperator::LogicalAnd | BinaryOperator::LogicalOr => {
                            ty == BuiltinType::Boolean
                        }
                        BinaryOperator::Add | BinaryOperator::Equal | BinaryOperator::NotEqual => {
                            const_integer(ty) || ty == BuiltinType::String
                        }
                        _ => const_integer(ty),
                    };
                    // 逆序入栈，确保先检查左侧；短路只属于后续值求值。
                    pending.push(*right);
                    pending.push(*left);
                    supported
                        && (self.const_operand_unresolved(*left)
                            || self.const_operand_unresolved(*right)
                            || (self.const_operand_allows(*left, accepts)
                                && self.const_operand_allows(*right, accepts)))
                }
                Expression::This if companion => {
                    invalid_expression.get_or_insert((span, true));
                    false
                }
                _ => false,
            };
            if !valid {
                invalid_expression.get_or_insert((span, false));
            }
        }
        if let Some(error) = invalid_expression {
            if !self.rechecking_constants
                && expressions
                    .iter()
                    .any(|expression| self.const_operand_unresolved(*expression))
            {
                self.pending_constant_errors.insert(symbol, error);
            } else {
                self.emit_constant_expression_error(error)?;
                return Ok(());
            }
        }
        self.constant_dependencies
            .insert(symbol, dependencies.into_iter().collect());
        self.constant_expressions.insert(symbol, expressions);
        Ok(())
    }

    pub(in crate::type_checking::checker) fn emit_constant_expression_error(
        &mut self,
        (span, context): (Span, bool),
    ) -> Result<(), TypeCheckingError> {
        if context {
            self.emit(
                self.invalid_constant_context_code,
                "companion constant initializer has no instance receiver",
                span,
            )
        } else {
            self.emit(
                self.invalid_constant_expression_code,
                "expression is not permitted in a constant initializer",
                span,
            )
        }
    }

    fn const_operand_unresolved(&self, expression: ExpressionId) -> bool {
        matches!(
            self.expression_types[expression.index()].map(|ty| self.kind(ty)),
            Some(TypeKind::Deferred(_)) | None
        )
    }

    fn const_operand_allows(
        &self,
        expression: ExpressionId,
        accepts: impl Fn(BuiltinType) -> bool,
    ) -> bool {
        match self.expression_types[expression.index()].map(|ty| self.kind(ty)) {
            Some(TypeKind::Builtin(ty)) => accepts(*ty),
            // 前向依赖尚未定型；语法图仍须保留，后继求值按拓扑次序完成类型。
            Some(TypeKind::Deferred(_) | TypeKind::IntegerLiteral(_)) | None => true,
            _ => false,
        }
    }
}

fn const_integer(ty: BuiltinType) -> bool {
    matches!(
        ty,
        BuiltinType::Byte
            | BuiltinType::Short
            | BuiltinType::Int
            | BuiltinType::Long
            | BuiltinType::UByte
            | BuiltinType::UShort
            | BuiltinType::UInt
            | BuiltinType::ULong
    )
}
