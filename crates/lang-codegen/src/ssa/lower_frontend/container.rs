//! 已完成 typed container descriptor 到顺序容器 SSA 的窄化 lowering。

mod access;
mod runtime;

use lang_frontend::{
    ast::ExpressionId,
    name_resolution::NameResolution,
    ownership_checking::OwnershipPlace,
    parser::Expression,
    source::Span,
    type_checking::{
        BuiltinType, ContainerConstructionKind, IntrinsicTypeConstructor,
        SequentialContainerKind as FrontendContainerKind, TypeId, TypedFile,
    },
};

use super::{
    ExpressionLowerer, LoweredValue, LoweringError, LoweringErrorKind, builtin_type, error,
    nominal::NominalTypeMapper, place, span_key, value,
};
use crate::ssa::model::{
    EntityId, EntityType, Module, Operation, PlaceId, ScalarConstant, SequentialContainerKind,
    SsaTypeId,
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
        self.validate_container_identity(
            descriptor.container(),
            descriptor.element_type(),
            descriptor.container_type(),
            span,
        )?;

        let elements = match descriptor.kind() {
            ContainerConstructionKind::ListForm => {
                if arguments.len() != descriptor.parameter_modes().len()
                    || descriptor
                        .parameter_modes()
                        .iter()
                        .any(|mode| *mode != lang_frontend::type_checking::ParameterMode::Value)
                {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                }
                let mut elements = Vec::with_capacity(arguments.len());
                let move_only = self.typed.copyability(descriptor.element_type())
                    == Some(lang_frontend::type_checking::Copyability::MoveOnly);
                // AST call arguments retain source order; ContainerConstruct consumes them in that
                // same order, which is the repeated Value-delivery contract.
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
                            // The canonical builtin identity was interned by NominalTypeMapper.
                            // Materialize storage only after this operand has executed once;
                            // ordinary calls and returns retain their no-result Unit ABI.
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
        // Operation verifier independently revalidates the SSA identity and element operands. This
        // frontend-side check binds descriptor facts to the pre-interned identity before emission.
        if !matches {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        Ok(())
    }
}
