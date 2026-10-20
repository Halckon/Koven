//! source-qualified 消费式 Map 类型移交；不把非空读取改写为源 storage。
use super::super::{BodyChecker, ExpressionCheck};
use crate::{
    ast::ExpressionId,
    diagnostic::codes,
    name_resolution::{Namespace, SourceUnitId, UnitReferenceTarget, UnitSymbolId},
    parser::{CallArgument, Expression},
    source::Span,
    type_checking::{
        CompilationUnitTypeError, IntrinsicTypeConstructor, TypeCheckingError, UnitExpressionId,
        UnitMapConsumeDescriptor, UnitTypeId, UnitTypeKind,
    },
};

impl BodyChecker<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn check_map_consume_call(
        &mut self,
        source: SourceUnitId,
        expression: ExpressionId,
        receiver: ExpressionId,
        receiver_result: ExpressionCheck,
        constructor: IntrinsicTypeConstructor,
        key_type: UnitTypeId,
        value_type: UnitTypeId,
        arguments: &[CallArgument],
        call_span: Span,
        name_span: Span,
        return_type: UnitTypeId,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        if constructor != IntrinsicTypeConstructor::MutableMap {
            self.emit(
                codes::INVALID_CONTAINER_MEMBER,
                "read-only Map does not support 'consume'",
                name_span,
            )?;
            self.check_construction_operands(source, arguments, return_type)?;
            return Ok(ExpressionCheck {
                ty: self.error_type(),
                falls_through: receiver_result.falls_through,
            });
        }
        if !arguments.is_empty() {
            self.emit(
                codes::CALL_ARGUMENT_ARITY,
                "MutableMap.consume expects no arguments",
                call_span,
            )?;
            self.check_construction_operands(source, arguments, return_type)?;
            return Ok(ExpressionCheck {
                ty: self.error_type(),
                falls_through: receiver_result.falls_through,
            });
        }
        let result_type = self.signatures.types_mut().intern(UnitTypeKind::Intrinsic {
            constructor: IntrinsicTypeConstructor::Map,
            arguments: vec![key_type, value_type],
        });
        let root = self.map_consume_source(source, receiver)?;
        self.parts
            .map_descriptors
            .consumes
            .push(UnitMapConsumeDescriptor::new(
                UnitExpressionId::new(source, expression),
                UnitExpressionId::new(source, receiver),
                receiver_result.ty,
                result_type,
                key_type,
                value_type,
                root,
            ));
        Ok(ExpressionCheck {
            ty: result_type,
            falls_through: receiver_result.falls_through,
        })
    }

    fn map_consume_source(
        &self,
        source: SourceUnitId,
        mut receiver: ExpressionId,
    ) -> Result<Option<(UnitSymbolId, UnitTypeId)>, CompilationUnitTypeError> {
        loop {
            let node = self
                .file(source)
                .ast()
                .expressions()
                .get(receiver)
                .map_err(TypeCheckingError::from)?;
            match node.payload() {
                Expression::Group { expression } => receiver = *expression,
                Expression::Name => {
                    let symbol = match self.reference(source, node.span(), Namespace::Value) {
                        Some(UnitReferenceTarget::Symbol(symbol)) => *symbol,
                        Some(UnitReferenceTarget::Declaration(declaration)) => {
                            let Some(symbol) = self.names.declaration_symbol(*declaration)
                            else {
                                return Ok(None);
                            };
                            symbol
                        }
                        _ => return Ok(None),
                    };
                    let ty = self
                        .parts
                        .symbol_types
                        .get(&symbol)
                        .copied()
                        .or_else(|| self.signatures.symbol_type(symbol));
                    return Ok(ty
                        .filter(|ty| !self.is_error(*ty) && !self.is_deferred(*ty))
                        .map(|ty| (symbol, ty)));
                }
                _ => return Ok(None),
            }
        }
    }
}
