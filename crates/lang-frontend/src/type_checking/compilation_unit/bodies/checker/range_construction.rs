//! source-qualified 范围构造；权限来自已校验 SourceId，实际操作数保持 unit identity。
use super::*;
use crate::ast::TypeRefId;
use crate::parser::CallArgument;
use crate::type_checking::{
    CallableResultSource, CarrierReturnContract, IntrinsicCallable, IntrinsicTypeConstructor,
    RangeConstructionDescriptor, RangeSourceKind, UnitCallArgumentDescriptor, UnitCallDescriptor,
    UnitCallTarget, UnitCallableInstanceKey,
};

impl BodyChecker<'_> {
    pub(super) fn clear_range_facts(&mut self) {
        self.parts.range_sizes.clear();
        for call in &mut self.parts.calls {
            call.range_construction = None;
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn check_range_construction_call(
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
            Some(UnitReferenceTarget::ExternalOverloadSet(externals)) => {
                externals.iter().copied().find(|&id| {
                    self.environment.binding(id)
                        == Some(&ExternalTypeBinding::IntrinsicCallable(
                            IntrinsicCallable::RangeView,
                        ))
                })
            }
            _ => None,
        };
        let Some(external) = external else {
            return Ok(None);
        };
        if self.environment.binding(external)
            != Some(&ExternalTypeBinding::IntrinsicCallable(
                IntrinsicCallable::RangeView,
            ))
        {
            return Ok(None);
        }
        if !self
            .environment
            .is_authorized_range_source(self.file(source).source_id())
        {
            self.emit(
                codes::UNSUPPORTED_BORROW_FLOW,
                "range construction requires an authorized standard source",
                span,
            )?;
            return Ok(Some(ExpressionCheck {
                ty: self.error_type(),
                falls_through: true,
            }));
        }
        let mut valid = true;
        if !type_arguments.is_empty() {
            self.emit(
                codes::TYPE_ARGUMENT_ARITY,
                "range construction infers its element type from source",
                span,
            )?;
            valid = false;
        }
        if arguments.len() != 3 {
            self.emit(
                codes::CALL_ARGUMENT_ARITY,
                "range construction requires source, begin and end",
                span,
            )?;
            for argument in arguments {
                self.check_expression(source, argument.value, None, None, return_type)?;
            }
            return Ok(Some(ExpressionCheck {
                ty: self.error_type(),
                falls_through: true,
            }));
        }
        for argument in arguments {
            if let Some(prefix) = argument.named_prefix {
                self.emit(
                    codes::INVALID_NAMED_ARGUMENT,
                    "range construction uses positional operands",
                    prefix.name_span,
                )?;
                valid = false;
            }
        }
        let operand = arguments[0].value;
        let source_span = self
            .file(source)
            .ast()
            .expressions()
            .get(operand)
            .map_err(TypeCheckingError::from)?
            .span();
        let source_type = self
            .check_expression(source, operand, None, None, return_type)?
            .ty;
        let (kind, element) = match self.signatures.types().get(source_type) {
            Some(UnitTypeKind::Intrinsic {
                constructor: IntrinsicTypeConstructor::List,
                arguments,
            }) if arguments.len() == 1 => (RangeSourceKind::List, arguments[0]),
            Some(UnitTypeKind::Intrinsic {
                constructor: IntrinsicTypeConstructor::View,
                arguments,
            }) if arguments.len() == 1 => (RangeSourceKind::View, arguments[0]),
            _ => {
                self.emit(
                    codes::INVALID_BORROW_CONTRACT,
                    "range source must be a compiler-bound List or View",
                    source_span,
                )?;
                return Ok(Some(ExpressionCheck {
                    ty: self.error_type(),
                    falls_through: true,
                }));
            }
        };
        let int = self.builtin(BuiltinType::Int);
        for argument in &arguments[1..] {
            let value =
                self.check_expression(source, argument.value, Some(int), Some(span), return_type)?;
            valid &= self.assignable(value.ty, int)
                && !self.is_error(value.ty)
                && !self.is_deferred(value.ty);
        }
        if !valid {
            return Ok(Some(ExpressionCheck {
                ty: self.error_type(),
                falls_through: true,
            }));
        }
        let view = self.signatures.types_mut().intern(UnitTypeKind::Intrinsic {
            constructor: IntrinsicTypeConstructor::View,
            arguments: vec![element],
        });
        let function = self.signatures.types_mut().intern(UnitTypeKind::Function {
            move_only: false,
            parameters: vec![
                UnitFunctionParameterType::new(ParameterMode::Borrow, source_type),
                UnitFunctionParameterType::new(ParameterMode::Borrow, int),
                UnitFunctionParameterType::new(ParameterMode::Borrow, int),
            ],
            return_type: view,
        });
        self.parts
            .expression_types
            .insert(UnitExpressionId::new(source, callee), function);
        self.parts.expression_categories.insert(
            UnitExpressionId::new(source, callee),
            crate::type_checking::ExpressionCategory::Temporary,
        );
        let descriptor = RangeConstructionDescriptor {
            expression: UnitExpressionId::new(source, expression),
            intrinsic: external,
            source: UnitExpressionId::new(source, operand),
            begin: UnitExpressionId::new(source, arguments[1].value),
            end: UnitExpressionId::new(source, arguments[2].value),
            source_kind: kind,
            element_type: element,
            result_type: view,
            source_span,
        };
        self.parts.calls.push(UnitCallDescriptor {
            expression: UnitExpressionId::new(source, expression),
            instance: UnitCallableInstanceKey {
                target: UnitCallTarget::External(external),
                type_arguments: vec![element],
            },
            return_type: view,
            receiver: None,
            result_source: CallableResultSource::Carrier(CarrierReturnContract::range_primitive(
                span,
                source_span,
            )),
            range_construction: Some(descriptor),
            arguments: arguments
                .iter()
                .enumerate()
                .map(|(index, a)| UnitCallArgumentDescriptor {
                    argument_index: index,
                    parameter_index: index,
                    category: self.expression_category(source, a.value),
                    mode: ParameterMode::Borrow,
                    parameter_type: if index == 0 { source_type } else { int },
                    cross_thread: false,
                })
                .collect(),
            aborts: false,
            prints_line: false,
        });
        Ok(Some(ExpressionCheck {
            ty: view,
            falls_through: true,
        }))
    }
}
