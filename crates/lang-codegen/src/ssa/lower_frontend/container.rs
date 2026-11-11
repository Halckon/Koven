//! 已完成 typed container descriptor 到顺序容器 SSA 的窄化 lowering。

mod access;

use lang_frontend::{
    ast::ExpressionId,
    name_resolution::NameResolution,
    ownership_checking::OwnershipPlace,
    parser::{CallArgument, Expression},
    source::Span,
    type_checking::{
        BuiltinType, ContainerConstructionKind, IntrinsicTypeConstructor, ParameterMode,
        SequentialContainerKind as FrontendContainerKind, TypeId, TypedFile,
    },
};

use super::{
    ExpressionLowerer, LoweredValue, LoweringError, LoweringErrorKind, builtin_type, error,
    nominal::NominalTypeMapper, place, span_key, value,
};
use crate::ssa::model::{
    ComparisonOperator, Edge, EntityId, EntityType, Module, Operation, Origin, PlaceAccess,
    PlaceId, ScalarConstant, SequentialContainerKind, SsaTypeId, TerminatorKind,
};

impl NominalTypeMapper {
    /// 将 frontend 的封闭顺序容器 identity 递归映射为保留元素类型的 SSA identity。
    pub(super) fn intern_container(
        &mut self,
        module: &mut Module,
        names: &NameResolution,
        typed: &TypedFile,
        constructor: IntrinsicTypeConstructor,
        arguments: &[TypeId],
        span: Span,
    ) -> Result<SsaTypeId, LoweringError> {
        let [element] = arguments else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        let kind = match constructor {
            IntrinsicTypeConstructor::Array => SequentialContainerKind::Array,
            IntrinsicTypeConstructor::List => SequentialContainerKind::List,
            IntrinsicTypeConstructor::MutableList => SequentialContainerKind::MutableList,
            _ => return Err(error(LoweringErrorKind::MissingFact, span)),
        };
        let element = self.intern_inner(module, names, typed, *element, span)?;
        module
            .add_sequential_container_type(kind, element)
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))
    }
}

impl ExpressionLowerer<'_> {
    /// Lower a typed element-place used only as a shared Borrow call argument.
    pub(super) fn lower_borrowed_container_element(
        &mut self,
        expression: ExpressionId,
        target: &OwnershipPlace,
        span: Span,
    ) -> Result<Option<PlaceId>, LoweringError> {
        let Some(descriptor) = self.typed.element_place(expression) else {
            return Ok(None);
        };
        if target.element().is_none() {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let owner = self.container_borrow_owner(descriptor.receiver(), target.root(), span)?;
        let index = self.require_value(descriptor.index())?;
        self.expression_ssa_type(descriptor.receiver(), span)?;
        let element = self.expression_ssa_type(expression, span)?;
        let (_, results) = self.append(
            Operation::ContainerElementPlace { owner, index },
            vec![EntityType::Place(element)],
            span,
        )?;
        Ok(Some(place(results[0])))
    }

    fn container_borrow_owner(
        &self,
        expression: ExpressionId,
        expected_root: lang_frontend::name_resolution::SymbolId,
        span: Span,
    ) -> Result<EntityId, LoweringError> {
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
        match node.payload() {
            Expression::Group { expression } => {
                self.container_borrow_owner(*expression, expected_root, span)
            }
            Expression::Name => {
                let symbol = self
                    .references
                    .get(&span_key(node.span()))
                    .copied()
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, node.span()))?;
                if symbol != expected_root {
                    return Err(error(LoweringErrorKind::MissingFact, node.span()));
                }
                if let Some(LoweredValue::Value(owner)) = self.bindings.get(&symbol).copied() {
                    return Ok(EntityId::Value(owner));
                }
                self.borrow_bindings
                    .get(&symbol)
                    .copied()
                    .map(EntityId::Loan)
                    .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, node.span()))
            }
            _ => Err(error(LoweringErrorKind::UnsupportedNode, node.span())),
        }
    }

    /// Lower 已由 frontend 固定种类、元素类型和 delivery mode 的顺序容器构造。
    pub(super) fn lower_container_construction(
        &mut self,
        expression: ExpressionId,
    ) -> Result<LoweredValue, LoweringError> {
        let descriptor = self
            .typed
            .container_construction(expression)
            .ok_or(LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
        let span = node.span();
        let Expression::Call { arguments, .. } = node.payload() else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        if self.typed.expression_type(expression) != Some(descriptor.container_type()) {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        if descriptor.kind() != ContainerConstructionKind::RuntimeLength {
            self.validate_container_identity(
                descriptor.container(),
                descriptor.element_type(),
                descriptor.container_type(),
                span,
            )?;
        }

        let elements = match descriptor.kind() {
            ContainerConstructionKind::ListForm => {
                if arguments.len() != descriptor.parameter_modes().len()
                    || descriptor
                        .parameter_modes()
                        .iter()
                        .any(|mode| *mode != ParameterMode::Value)
                {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                }
                let mut elements = Vec::with_capacity(arguments.len());
                let move_only = self.typed.copyability(descriptor.element_type())
                    == Some(lang_frontend::type_checking::Copyability::MoveOnly);
                for argument in arguments {
                    match self.lower(argument.value)? {
                        LoweredValue::Value(element) => {
                            let pending = if move_only {
                                self.forget_delivered_owners(&[element]);
                                let key = argument.value.index();
                                self.temporaries.insert(key, element);
                                Some(key)
                            } else {
                                None
                            };
                            elements.push((element, pending));
                        }
                        LoweredValue::Diverged => return Ok(LoweredValue::Diverged),
                        LoweredValue::Unit => {
                            let element = self.resolve_type(descriptor.element_type(), span)?;
                            let argument_type =
                                self.typed.expression_type(argument.value).ok_or_else(|| {
                                    error(LoweringErrorKind::MissingFact, argument.span)
                                })?;
                            let argument_type = self.resolve_type(argument_type, argument.span)?;
                            if argument_type != element
                                || builtin_type(self.typed, element) != Some(BuiltinType::Unit)
                            {
                                return Err(error(LoweringErrorKind::MissingFact, argument.span));
                            }
                            let target = self.expression_ssa_type(argument.value, argument.span)?;
                            let (_, results) = self.append(
                                Operation::Constant(ScalarConstant::Unit),
                                vec![EntityType::Value(target)],
                                argument.span,
                            )?;
                            elements.push((value(results[0]), None));
                        }
                    }
                }
                elements
                    .into_iter()
                    .map(|(element, pending)| {
                        pending.map_or(Ok(element), |key| {
                            self.temporaries
                                .remove(&key)
                                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?
            }
            ContainerConstructionKind::EmptyMutableList => {
                if !arguments.is_empty()
                    || !descriptor.parameter_modes().is_empty()
                    || descriptor.container() != FrontendContainerKind::MutableList
                {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                }
                Vec::new()
            }
            ContainerConstructionKind::RuntimeLength => {
                return self.lower_runtime_container(expression, arguments, span);
            }
        };
        let container = self.expression_ssa_type(expression, span)?;
        let (_, results) = self.append(
            Operation::ContainerConstruct {
                container,
                elements,
            },
            vec![EntityType::Value(container)],
            span,
        )?;
        Ok(LoweredValue::Value(value(results[0])))
    }

    pub(super) fn lower_runtime_container(
        &mut self,
        expression: ExpressionId,
        arguments: &[CallArgument],
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let descriptor = self
            .typed
            .container_construction(expression)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let [size, initializer] = arguments else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        if descriptor.parameter_modes() != [ParameterMode::Borrow, ParameterMode::Borrow] {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        if self.runtime_operand_exits(size.value, size.span)? {
            return Ok(LoweredValue::Diverged);
        }
        let (size_loan, ends_after_call) =
            self.lower_borrow_argument(expression, size.value, size.span)?;
        self.pending_call_loans.insert(
            (expression.index(), size.value.index()),
            ends_after_call.then_some(size_loan),
        );
        let size_type = self.expression_ssa_type(size.value, size.span)?;
        let (_, results) = self.append(
            Operation::Read {
                source: PlaceAccess::Loan(size_loan),
            },
            vec![EntityType::Value(size_type)],
            size.span,
        )?;
        let length = value(results[0]);
        self.guard_nonnegative_length(length, size_type, span)?;
        if self.runtime_operand_exits(initializer.value, initializer.span)? {
            return Ok(LoweredValue::Diverged);
        }
        let initializer_type = self
            .typed
            .expression_type(initializer.value)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, initializer.span))?;
        if matches!(
            self.typed.types().get(initializer_type),
            Some(lang_frontend::type_checking::TypeKind::Deferred(_))
        ) {
            return Err(error(LoweringErrorKind::UnsupportedNode, initializer.span));
        }
        let (initializer_loan, ends_after_call) =
            self.lower_borrow_argument(expression, initializer.value, initializer.span)?;
        self.pending_call_loans.insert(
            (expression.index(), initializer.value.index()),
            ends_after_call.then_some(initializer_loan),
        );
        self.validate_container_identity(
            descriptor.container(),
            descriptor.element_type(),
            descriptor.container_type(),
            span,
        )?;
        let container = self.expression_ssa_type(expression, span)?;
        let (_, results) = self.append(
            Operation::ContainerGenerateBorrowed {
                container,
                length,
                initializer: initializer_loan,
            },
            vec![EntityType::Value(container)],
            span,
        )?;
        self.finish_borrowed_call(expression, span)?;
        Ok(LoweredValue::Value(value(results[0])))
    }

    pub(in crate::ssa::lower_frontend) fn runtime_operand_exits(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<bool, LoweringError> {
        let ty = self
            .typed
            .expression_type(expression)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if builtin_type(self.typed, ty) != Some(BuiltinType::Nothing) {
            return Ok(false);
        }
        match self.lower(expression)? {
            LoweredValue::Diverged => Ok(true),
            _ => Err(error(LoweringErrorKind::InvalidModel, span)),
        }
    }

    fn guard_nonnegative_length(
        &mut self,
        length: crate::ssa::model::ValueId,
        int: SsaTypeId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let boolean = self
            .type_ids
            .iter()
            .find_map(|(frontend, ssa)| {
                (builtin_type(self.typed, *frontend) == Some(BuiltinType::Boolean)).then_some(*ssa)
            })
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let (_, zero) = self.append(
            Operation::Constant(ScalarConstant::Integer(0)),
            vec![EntityType::Value(int)],
            span,
        )?;
        let (_, negative) = self.append(
            Operation::Compare {
                operator: ComparisonOperator::LessThan,
                left: length,
                right: value(zero[0]),
            },
            vec![EntityType::Value(boolean)],
            span,
        )?;
        let baseline = self.bindings.clone();
        let carried = self.linear_binding_slots(&baseline, span)?;
        let parameter_types = carried.slots.iter().map(|slot| slot.ty).collect::<Vec<_>>();
        let arguments = carried
            .slots
            .iter()
            .map(|slot| slot.source)
            .collect::<Vec<_>>();
        let abort = self
            .function
            .add_block(parameter_types.clone(), Origin::Source(span))
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        let success = self
            .function
            .add_block(parameter_types, Origin::Source(span))
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        self.function
            .set_terminator(
                self.block,
                TerminatorKind::Conditional {
                    condition: value(negative[0]),
                    when_true: Edge {
                        target: abort,
                        arguments: arguments.clone(),
                    },
                    when_false: Edge {
                        target: success,
                        arguments,
                    },
                },
                Origin::Source(span),
            )
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        self.function
            .set_terminator(abort, TerminatorKind::Abort, Origin::Source(span))
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        self.block = success;
        self.bindings = self.rebind_linear_bindings(&baseline, success, &carried, span)?;
        Ok(())
    }

    fn validate_container_identity(
        &self,
        frontend_kind: FrontendContainerKind,
        element: TypeId,
        container: TypeId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let expected_constructor = match frontend_kind {
            FrontendContainerKind::Array => IntrinsicTypeConstructor::Array,
            FrontendContainerKind::List => IntrinsicTypeConstructor::List,
            FrontendContainerKind::MutableList => IntrinsicTypeConstructor::MutableList,
        };
        let container = self.resolve_type(container, span)?;
        let element = self.resolve_type(element, span)?;
        let matches = match self.typed.types().get(container) {
            Some(lang_frontend::type_checking::TypeKind::Intrinsic {
                constructor,
                arguments,
            }) => {
                *constructor == expected_constructor
                    && arguments.as_slice() == [element]
                    && self.type_ids.contains_key(&container)
                    && self.type_ids.contains_key(&element)
            }
            _ => false,
        };
        if !matches {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        Ok(())
    }
}
