//! 受宿主来源能力限制的可复用 View 构造，发布真实操作数而非算法名。
use super::*;
use crate::parser::{CallArgument, Expression};
use crate::type_checking::{
    CallArgumentDescriptor, CallableResultSource, CallableTarget, CarrierReturnContract,
    IntrinsicCallable, RangeConstructionDescriptor, RangeSourceKind,
};

impl Checker<'_> {
    pub(super) fn clear_range_facts(&mut self) {
        self.range_sizes.clear();
        for call in &mut self.calls {
            call.clear_range_construction();
        }
    }

    pub(super) fn check_range_construction_call(
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
            Some(ReferenceTarget::ExternalOverloadSet(externals)) => {
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
            .is_authorized_range_source(self.parsed.source_id())
        {
            self.emit(
                codes::catalog()?.resolve(codes::UNSUPPORTED_BORROW_FLOW)?,
                "range construction requires an authorized standard source",
                span,
            )?;
            return Ok(Some(ExprCheck {
                ty: self.error_type(),
                falls_through: true,
            }));
        }
        let mut valid = true;
        if !type_arguments.is_empty() {
            self.emit(
                self.type_argument_arity_code,
                "range construction infers its element type from source",
                span,
            )?;
            valid = false;
        }
        if arguments.len() != 3 {
            self.emit(
                self.call_argument_arity_code,
                "range construction requires source, begin and end",
                span,
            )?;
            for argument in arguments {
                self.check_expression(argument.value, None, None)?;
            }
            return Ok(Some(ExprCheck {
                ty: self.error_type(),
                falls_through: true,
            }));
        }
        for argument in arguments {
            if let Some(prefix) = argument.named_prefix {
                self.emit(
                    self.invalid_named_argument_code,
                    "range construction uses positional operands",
                    prefix.name_span,
                )?;
                valid = false;
            }
        }
        let source = arguments[0].value;
        let source_span = self.ast().expressions().get(source)?.span();
        let source_type = self.check_expression(source, None, None)?.ty;
        let (kind, element) = match self.kind(source_type) {
            TypeKind::Intrinsic {
                constructor: IntrinsicTypeConstructor::List,
                arguments,
            } if arguments.len() == 1 => (RangeSourceKind::List, arguments[0]),
            TypeKind::Intrinsic {
                constructor: IntrinsicTypeConstructor::View,
                arguments,
            } if arguments.len() == 1 => (RangeSourceKind::View, arguments[0]),
            _ => {
                self.emit(
                    codes::catalog()?.resolve(codes::INVALID_BORROW_CONTRACT)?,
                    "range source must be a compiler-bound List or View",
                    source_span,
                )?;
                return Ok(Some(ExprCheck {
                    ty: self.error_type(),
                    falls_through: true,
                }));
            }
        };
        let int = self.builtin(BuiltinType::Int);
        for argument in &arguments[1..] {
            let value = self.check_expression(argument.value, Some(int), Some(span))?;
            valid &= self.assignable(value.ty, int)
                && !self.is_error(value.ty)
                && !self.is_deferred(value.ty);
        }
        if !valid {
            return Ok(Some(ExprCheck {
                ty: self.error_type(),
                falls_through: true,
            }));
        }
        let view = self.types.intern(TypeKind::Intrinsic {
            constructor: IntrinsicTypeConstructor::View,
            arguments: vec![element],
        });
        let function = self.types.intern(TypeKind::Function {
            move_only: false,
            parameters: vec![
                FunctionParameterType {
                    mode: ParameterMode::Borrow,
                    ty: source_type,
                },
                FunctionParameterType {
                    mode: ParameterMode::Borrow,
                    ty: int,
                },
                FunctionParameterType {
                    mode: ParameterMode::Borrow,
                    ty: int,
                },
            ],
            return_type: view,
        });
        self.set_expression(callee, function);
        self.set_expression_category(callee, ExpressionCategory::Temporary);
        let descriptor = RangeConstructionDescriptor {
            expression,
            intrinsic: external,
            source,
            begin: arguments[1].value,
            end: arguments[2].value,
            source_kind: kind,
            element_type: element,
            result_type: view,
            source_span,
        };
        self.calls.push(
            CallDescriptor::new(
                expression,
                CallableTarget::External(external),
                vec![element],
                view,
                None,
                arguments
                    .iter()
                    .enumerate()
                    .map(|(index, a)| {
                        CallArgumentDescriptor::new(
                            index,
                            index,
                            self.expression_categories[a.value.index()],
                            ParameterMode::Borrow,
                            if index == 0 { source_type } else { int },
                            false,
                        )
                    })
                    .collect(),
                false,
                false,
            )
            .with_result_source(CallableResultSource::Carrier(
                CarrierReturnContract::range_primitive(span, source_span),
            ))
            .with_range_construction(descriptor),
        );
        Ok(Some(ExprCheck {
            ty: view,
            falls_through: true,
        }))
    }
}
