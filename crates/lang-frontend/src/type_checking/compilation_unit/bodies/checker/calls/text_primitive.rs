//! Unit text storage calls share fixed signatures and the ordinary final call checker.
use super::*;
use crate::{
    source::Span,
    type_checking::{IntrinsicTypeConstructor, text_primitive::TextOperandType},
};

impl BodyChecker<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn check_text_primitive_call(
        &mut self,
        source: SourceUnitId,
        expression: ExpressionId,
        span: Span,
        callee: ExpressionId,
        type_arguments: &[TypeRefId],
        arguments: &[CallArgument],
        return_type: UnitTypeId,
    ) -> Result<Option<ExpressionCheck>, CompilationUnitTypeError> {
        let node = self
            .file(source)
            .ast()
            .expressions()
            .get(callee)
            .map_err(TypeCheckingError::from)?;
        if !matches!(node.payload(), Expression::Name) {
            return Ok(None);
        }
        let external = match self.reference(source, node.span(), Namespace::Value) {
            Some(UnitReferenceTarget::External(external)) => Some(*external),
            Some(UnitReferenceTarget::ExternalOverloadSet(externals)) => externals
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
            .is_authorized_text_source(self.file(source).source_id())
        {
            self.emit(
                codes::UNSUPPORTED_BORROW_FLOW,
                "text storage operation requires an authorized standard source",
                span,
            )?;
            return Ok(Some(ExpressionCheck {
                ty: self.error_type(),
                falls_through: true,
            }));
        }
        if !type_arguments.is_empty() {
            self.emit(
                codes::TYPE_ARGUMENT_ARITY,
                "text storage operation has no type parameters",
                span,
            )?;
            return Ok(Some(ExpressionCheck {
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
                    return Ok(Some(ExpressionCheck {
                        ty: self.error_type(),
                        falls_through: true,
                    }));
                }
            };
        let candidate = CallCandidate {
            target: UnitCallTarget::External(external),
            declaration_span: None,
            type_parameters: Vec::new(),
            move_only: false,
            parameters,
            return_type: self.text_operand_type(result),
            result_source: crate::type_checking::CallableResultSource::Owned,
            instance_arguments: Vec::new(),
            receiver: None,
            owner_substitutions: BTreeMap::new(),
            cross_thread_parameters: BTreeSet::new(),
            aborts: false,
            prints_line: false,
        };
        let call_count = self.parts.calls.len();
        let result = self.finish_candidate(
            source,
            expression,
            callee,
            arguments,
            return_type,
            &candidate,
            &mapping,
            None,
        )?;
        if let Some(call) = self.parts.calls[call_count..]
            .iter_mut()
            .find(|call| call.expression() == UnitExpressionId::new(source, expression))
        {
            call.text_primitive = Some(kind);
        }
        Ok(Some(result))
    }

    fn text_operand_type(&mut self, shape: TextOperandType) -> UnitTypeId {
        match shape {
            TextOperandType::Builtin(builtin) => self.builtin(builtin),
            TextOperandType::Bytes => {
                let element = self.builtin(BuiltinType::UByte);
                self.signatures.types_mut().intern(UnitTypeKind::Intrinsic {
                    constructor: IntrinsicTypeConstructor::Array,
                    arguments: vec![element],
                })
            }
        }
    }
}
