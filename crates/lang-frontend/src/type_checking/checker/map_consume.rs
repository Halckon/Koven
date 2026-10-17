//! 消费式移交保留原 K/V 与来源 storage；所有权资格留给 Phase 3。
use super::super::{Checker, ExprCheck};
use crate::{
    ast::ExpressionId,
    name_resolution::{Namespace, ReferenceTarget, SymbolId},
    parser::{CallArgument, Expression},
    source::Span,
    type_checking::{
        IntrinsicTypeConstructor, MapConsumeDescriptor, TypeCheckingError, TypeId, TypeKind,
    },
};

impl Checker<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn check_map_consume_call(
        &mut self,
        expression: ExpressionId,
        receiver: ExpressionId,
        receiver_result: ExprCheck,
        constructor: IntrinsicTypeConstructor,
        key_type: TypeId,
        value_type: TypeId,
        arguments: &[CallArgument],
        call_span: Span,
        name_span: Span,
    ) -> Result<ExprCheck, TypeCheckingError> {
        if constructor != IntrinsicTypeConstructor::MutableMap {
            self.emit(
                self.invalid_container_member_code,
                "read-only Map does not support 'consume'",
                name_span,
            )?;
            self.check_construction_operands(arguments)?;
            return Ok(ExprCheck {
                ty: self.error_type(),
                falls_through: receiver_result.falls_through,
            });
        }
        if !arguments.is_empty() {
            self.emit(
                self.call_argument_arity_code,
                "MutableMap.consume expects no arguments",
                call_span,
            )?;
            self.check_construction_operands(arguments)?;
            return Ok(ExprCheck {
                ty: self.error_type(),
                falls_through: receiver_result.falls_through,
            });
        }
        let result_type = self.types.intern(TypeKind::Intrinsic {
            constructor: IntrinsicTypeConstructor::Map,
            arguments: vec![key_type, value_type],
        });
        let source = self.map_consume_source(receiver)?;
        self.map_descriptors
            .consumes
            .push(MapConsumeDescriptor::new(
                expression,
                receiver,
                receiver_result.ty,
                result_type,
                key_type,
                value_type,
                source,
            ));
        Ok(ExprCheck {
            ty: result_type,
            falls_through: receiver_result.falls_through,
        })
    }

    fn map_consume_source(
        &self,
        mut receiver: ExpressionId,
    ) -> Result<Option<(SymbolId, TypeId)>, TypeCheckingError> {
        loop {
            let node = self.ast().expressions().get(receiver)?;
            match node.payload() {
                Expression::Group { expression } => receiver = *expression,
                Expression::Name => {
                    let Some(ReferenceTarget::Symbol(symbol)) =
                        self.reference(node.span(), Namespace::Value)
                    else {
                        return Ok(None);
                    };
                    let source = self
                        .symbol_type(*symbol)
                        .filter(|ty| !self.is_error(*ty) && !self.is_deferred(*ty))
                        .map(|ty| (*symbol, ty));
                    return Ok(source);
                }
                _ => return Ok(None),
            }
        }
    }
}
