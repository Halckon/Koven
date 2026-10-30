//! Fixed storage signatures reuse ordinary argument mapping and final call checking.
use super::*;
use crate::type_checking::{
    IntrinsicTypeConstructor, argument_mapping::map_arguments, text_primitive::TextOperandType,
};

impl Checker<'_> {
    pub(super) fn check_text_primitive_call(
        &mut self,
        expression: ExpressionId,
        span: Span,
        callee: ExpressionId,
        type_arguments: &[TypeRefId],
        arguments: &[CallArgument],
    ) -> Result<Option<ExprCheck>, TypeCheckingError> {
        let node = self.ast().expressions().get(callee)?;
        if !matches!(node.payload(), Expression::Name) {
            return Ok(None);
        }
        let external = match self.reference(node.span(), Namespace::Value) {
            Some(ReferenceTarget::External(external)) => Some(*external),
            Some(ReferenceTarget::ExternalOverloadSet(externals)) => externals
                .iter()
                .copied()
                .find(|id| self.environment.text_primitive_kind(*id).is_some()),
            _ => None,
        };
        let Some((external, kind)) = external.and_then(|id| {
            self.environment
                .text_primitive_kind(id)
                .map(|kind| (id, kind))
        }) else {
            return Ok(None);
        };
        if !self
            .environment
            .is_authorized_text_source(self.parsed.source_id())
        {
            self.emit(
                codes::catalog()?.resolve(codes::UNSUPPORTED_BORROW_FLOW)?,
                "text storage operation requires an authorized standard source",
                span,
            )?;
            return Ok(Some(ExprCheck {
                ty: self.error_type(),
                falls_through: true,
            }));
        }
        if !type_arguments.is_empty() {
            self.emit(
                self.type_argument_arity_code,
                "text storage operation has no type parameters",
                span,
            )?;
            return Ok(Some(ExprCheck {
                ty: self.error_type(),
                falls_through: true,
            }));
        }
        let (parameters, result) = kind.signature();
        let parameters = parameters
            .iter()
            .map(|shape| MappedParameter {
                name: None,
                mode: ParameterMode::Borrow,
                ty: self.text_operand_type(*shape),
                span: None,
            })
            .collect::<Vec<_>>();
        let mapping =
            match map_arguments(self.sources, &parameters, arguments, span, |_| Ok(false))? {
                Ok(mapping) => mapping,
                Err(error) => {
                    self.emit_mapping_error(error)?;
                    return Ok(Some(ExprCheck {
                        ty: self.error_type(),
                        falls_through: true,
                    }));
                }
            };
        let candidate = CallCandidate {
            target: CallableTarget::External(external),
            declaration_span: None,
            type_parameters: Vec::new(),
            instance_arguments: Vec::new(),
            receiver: None,
            parameters,
            return_type: self.text_operand_type(result),
            result_source: crate::type_checking::CallableResultSource::Owned,
            cross_thread_parameters: BTreeSet::new(),
            aborts: false,
            prints_line: false,
        };
        let call_count = self.calls.len();
        let result = self.finish_unique_call(expression, callee, arguments, candidate, mapping)?;
        if let Some(call) = self.calls[call_count..]
            .iter_mut()
            .find(|call| call.expression() == expression)
        {
            call.set_text_primitive(kind);
        }
        Ok(Some(result))
    }

    fn text_operand_type(&mut self, shape: TextOperandType) -> TypeId {
        match shape {
            TextOperandType::Builtin(builtin) => self.builtin(builtin),
            TextOperandType::Bytes => {
                let element = self.builtin(BuiltinType::UByte);
                self.types.intern(TypeKind::Intrinsic {
                    constructor: IntrinsicTypeConstructor::Array,
                    arguments: vec![element],
                })
            }
        }
    }
}
