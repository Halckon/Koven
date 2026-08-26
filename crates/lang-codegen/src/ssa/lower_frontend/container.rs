//! 已完成 typed container descriptor 到顺序容器 SSA 的窄化 lowering。

use lang_frontend::{
    ast::ExpressionId,
    name_resolution::NameResolution,
    parser::Expression,
    source::Span,
    type_checking::{
        ContainerConstructionKind, IntrinsicTypeConstructor,
        SequentialContainerKind as FrontendContainerKind, TypeId, TypedFile,
    },
};

use super::{
    ExpressionLowerer, LoweredValue, LoweringError, LoweringErrorKind, error,
    nominal::NominalTypeMapper, value,
};
use crate::ssa::model::{EntityType, Module, Operation, SequentialContainerKind, SsaTypeId};

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
        let element = self.intern(module, names, typed, *element, span)?;
        module
            .add_sequential_container_type(kind, element)
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))
    }
}

impl ExpressionLowerer<'_> {
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
                // AST call arguments retain source order; ContainerConstruct consumes them in that
                // same order, which is the repeated Value-delivery contract.
                for argument in arguments {
                    match self.lower(argument.value)? {
                        LoweredValue::Value(element) => elements.push(element),
                        LoweredValue::Diverged => return Ok(LoweredValue::Diverged),
                        LoweredValue::Unit => {
                            return Err(error(LoweringErrorKind::MissingFact, argument.span));
                        }
                    }
                }
                elements
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
            // Runtime-length construction depends on the still-unimplemented callable initializer
            // bridge and remains outside this source-construction slice.
            ContainerConstructionKind::RuntimeLength => {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
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
