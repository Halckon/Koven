//! 已完成 typed container descriptor 到顺序容器 SSA 的窄化 lowering。

pub(in crate::ssa) mod access;

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
    pub(in crate::ssa) fn intern_container(
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
    pub(in crate::ssa) fn lower_borrowed_container_element(
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

    pub(in crate::ssa) fn runtime_operand_exits(
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

#[cfg(test)]
pub(in crate::ssa) mod cleanup_guard_harness {
    //! Private guard tests start with ordinary, complete source and verified lowering.
    //! Recovered checkpoints use actual SSA identities; no frontend fact is synthesized.
    use crate::ssa::lower_frontend::*;
    use crate::ssa::model::{Definition, Module, Program, SsaTypeKind};
    use lang_frontend::{
        lexer::lex,
        name_resolution::{ReferenceTarget, resolve_names},
        ownership_checking::check_ownership,
        parser::parse_file,
        source::SourceMap,
        type_checking::{
            IntrinsicTypeConstructor, check_types, complete_file_types, standard_environments,
        },
    };

    pub(in crate::ssa) struct Fixture {
        pub(in crate::ssa) sources: SourceMap,
        pub(in crate::ssa) parsed: ParsedFile,
        pub(in crate::ssa) names: NameResolution,
        pub(in crate::ssa) typed: TypedFile,
        pub(in crate::ssa) owned: OwnershipCheckedFile,
        pub(in crate::ssa) program: Program,
        pub(in crate::ssa) type_ids: BTreeMap<TypeId, SsaTypeId>,
    }

    impl Fixture {
        pub(in crate::ssa) fn analyze(text: &str) -> Self {
            let mut sources = SourceMap::new();
            let source = sources
                .add_source("private-cleanup-guard.ko", text)
                .unwrap();
            let lexed = lex(&sources, source).unwrap();
            assert!(lexed.diagnostics().is_empty());
            let parsed = parse_file(&sources, &lexed).unwrap();
            assert!(parsed.diagnostics().is_empty());
            let (environment, types) = standard_environments();
            let names = resolve_names(&sources, &parsed, &environment).unwrap();
            assert!(names.diagnostics().is_empty());
            let typed = check_types(&sources, &parsed, &names, &types).unwrap();
            assert!(typed.diagnostics().is_empty());
            complete_file_types(&sources, &parsed, &names, &types, &typed).unwrap();
            let owned = check_ownership(&sources, &parsed, &names, &typed).unwrap();
            assert!(owned.diagnostics().is_empty());
            let program = orchestrate::lower_scalar_file(&sources, &parsed, &names, &typed, &owned)
                .expect("real complete/ownership/lowering baseline");
            crate::ssa::verify::verify_program(&program).unwrap();
            let mut type_ids = BTreeMap::new();
            for ty in names
                .symbols()
                .iter()
                .filter_map(|symbol| typed.symbol_type(symbol.id()))
                .chain(
                    parsed
                        .ast()
                        .expressions()
                        .iter()
                        .filter_map(|(id, _)| typed.expression_type(id)),
                )
            {
                map_type(&program.modules[0], &names, &typed, ty, &mut type_ids);
            }
            Self {
                sources,
                parsed,
                names,
                typed,
                owned,
                program,
                type_ids,
            }
        }

        pub(in crate::ssa) fn entry(&self) -> &Function {
            self.program.modules[0]
                .functions
                .iter()
                .find(|function| function.name == "entry")
                .unwrap()
        }

        pub(in crate::ssa) fn symbol(&self, name: &str) -> SymbolId {
            let mut matches = self
                .names
                .symbols()
                .iter()
                .filter(|symbol| symbol.name() == name);
            let symbol = matches.next().unwrap().id();
            assert!(
                matches.next().is_none(),
                "fixture symbol must be unambiguous: {name}"
            );
            symbol
        }

        /// Empty runtime state is intentional. Each test installs its observed checkpoint.
        pub(in crate::ssa) fn with_lowerer<T>(
            &self,
            function: &mut Function,
            check: impl FnOnce(&mut ExpressionLowerer<'_>) -> T,
        ) -> T {
            let module = &self.program.modules[0];
            let references = self
                .names
                .references()
                .iter()
                .filter_map(|reference| match reference.target() {
                    ReferenceTarget::Symbol(symbol) => Some((span_key(reference.span()), *symbol)),
                    _ => None,
                })
                .collect();
            let heap_payloads = module
                .types
                .iter()
                .enumerate()
                .filter_map(|(index, kind)| match kind {
                    SsaTypeKind::HeapOwner {
                        payload: Some(payload),
                        ..
                    } => Some((
                        SsaTypeId {
                            module: module.id,
                            index,
                        },
                        *payload,
                    )),
                    _ => None,
                })
                .collect();
            let function_ids = BTreeMap::new();
            let source_closures = source_closure::CallableLayouts::default();
            let instance_plan = instances::FunctionInstancePlan::default();
            let enum_payloads = BTreeMap::new();
            let substitutions = BTreeMap::new();
            let block = function.entry_block().unwrap();
            let mut lowerer = ExpressionLowerer {
                map_loop_owners: map_loop_owner::Carriers::default(),
                map_iteration_state: crate::ssa::lower_frontend::map_iteration::State::default(),
                parsed: &self.parsed,
                names: &self.names,
                typed: &self.typed,
                owned: &self.owned,
                source_text: self.sources.source_text(self.parsed.source_id()).unwrap(),
                references: &references,
                function_ids: &function_ids,
                source_closures: &source_closures,
                instance_plan: &instance_plan,
                source_token: None,
                thunk_expression: None,
                capture_loans: BTreeMap::new(),
                type_ids: &self.type_ids,
                heap_payloads: &heap_payloads,
                map_results: &module.map_results,
                nullable_maps: &module.nullable_maps,
                ssa_types: &module.types,
                enum_payloads: &enum_payloads,
                substitutions: &substitutions,
                function,
                block,
                bindings: BTreeMap::new(),
                borrow_bindings: BTreeMap::new(),
                result_source_loans: BTreeMap::new(),
                non_null_bindings: BTreeMap::new(),
                temporaries: BTreeMap::new(),
                pending_call_loans: PendingCallLoans::default(),
                return_type: self.typed.types().builtin(BuiltinType::Int).unwrap(),
                readonly_receiver: None,
                loops: Vec::new(),
            };
            check(&mut lowerer)
        }
    }

    // Read only the real module's already-emitted types, including canonical nominal names.
    fn map_type(
        module: &Module,
        names: &NameResolution,
        typed: &TypedFile,
        ty: TypeId,
        mapped: &mut BTreeMap<TypeId, SsaTypeId>,
    ) -> Option<SsaTypeId> {
        if let Some(id) = mapped.get(&ty) {
            return Some(*id);
        }
        let kind = match typed.types().get(ty)? {
            TypeKind::Builtin(builtin) => match builtin {
                BuiltinType::Int => SsaTypeKind::Integer {
                    bits: 32,
                    signed: true,
                },
                BuiltinType::Long => SsaTypeKind::Integer {
                    bits: 64,
                    signed: true,
                },
                BuiltinType::Boolean => SsaTypeKind::Boolean,
                BuiltinType::String => SsaTypeKind::StringOwner,
                BuiltinType::Unit => SsaTypeKind::Unit,
                _ => return None,
            },
            TypeKind::Nullable(inner) => SsaTypeKind::NullableHandle {
                inner: map_type(module, names, typed, *inner, mapped)?,
            },
            TypeKind::Nominal { nominal, .. } => {
                let name = format!(
                    "{}#t{}",
                    names.symbols()[nominal.symbol().index()].name(),
                    ty.index()
                );
                let id = *module.named_type_ids.get(&name)?;
                mapped.insert(ty, id);
                return Some(id);
            }
            TypeKind::Intrinsic {
                constructor,
                arguments,
            } => match constructor {
                IntrinsicTypeConstructor::List
                | IntrinsicTypeConstructor::Array
                | IntrinsicTypeConstructor::MutableList => {
                    let [element] = arguments.as_slice() else {
                        return None;
                    };
                    let kind = match constructor {
                        IntrinsicTypeConstructor::List => {
                            crate::ssa::model::SequentialContainerKind::List
                        }
                        IntrinsicTypeConstructor::Array => {
                            crate::ssa::model::SequentialContainerKind::Array
                        }
                        _ => crate::ssa::model::SequentialContainerKind::MutableList,
                    };
                    SsaTypeKind::SequentialContainer {
                        kind,
                        element: map_type(module, names, typed, *element, mapped)?,
                    }
                }
                IntrinsicTypeConstructor::Box | IntrinsicTypeConstructor::Rc => {
                    let prefix = if *constructor == IntrinsicTypeConstructor::Box {
                        "Box"
                    } else {
                        "Rc"
                    };
                    let id = *module
                        .named_type_ids
                        .get(&format!("{prefix}#t{}", ty.index()))?;
                    mapped.insert(ty, id);
                    return Some(id);
                }
                _ => return None,
            },
            _ => return None,
        };
        let id = *module.type_ids.get(&kind)?;
        mapped.insert(ty, id);
        Some(id)
    }

    pub(in crate::ssa) fn parameter(function: &Function, entity: EntityId) -> (BlockId, usize) {
        let Definition::BlockParameter { block, index } =
            function.entity(entity).unwrap().definition
        else {
            panic!("actual block parameter required")
        };
        assert_eq!(function.block(block).unwrap().parameters[index], entity);
        (block, index)
    }

    pub(in crate::ssa) fn edges(kind: &TerminatorKind) -> Vec<&Edge> {
        match kind {
            TerminatorKind::Branch(edge) => vec![edge],
            TerminatorKind::Conditional {
                when_true,
                when_false,
                ..
            } => vec![when_true, when_false],
            TerminatorKind::NullableBranch {
                when_null,
                when_non_null,
                ..
            } => vec![when_null, when_non_null],
            _ => Vec::new(),
        }
    }

    pub(in crate::ssa) fn edges_mut(kind: &mut TerminatorKind) -> Vec<&mut Edge> {
        match kind {
            TerminatorKind::Branch(edge) => vec![edge],
            TerminatorKind::Conditional {
                when_true,
                when_false,
                ..
            } => vec![when_true, when_false],
            TerminatorKind::NullableBranch {
                when_null,
                when_non_null,
                ..
            } => vec![when_null, when_non_null],
            _ => Vec::new(),
        }
    }

    pub(in crate::ssa) fn incoming(function: &Function, target: BlockId) -> Vec<(BlockId, Edge)> {
        function
            .blocks
            .iter()
            .flat_map(|block| {
                block
                    .terminator
                    .iter()
                    .flat_map(|term| edges(&term.kind))
                    .filter(move |edge| edge.target == target)
                    .map(move |edge| (block.id, edge.clone()))
            })
            .collect()
    }

    pub(in crate::ssa) fn unsupported<T: std::fmt::Debug>(
        result: Result<T, LoweringError>,
        span: Span,
    ) {
        let error = result.expect_err("private guard must reject before final verification");
        assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
        assert_eq!(error.span, Some(span));
    }

    /// Recover only the active outer context queried by these cleanup guards. Every slot
    /// is the actual preheader operand; current/items/total are the fixture's live locals.
    pub(in crate::ssa) fn outer_context(
        lowerer: &mut ExpressionLowerer<'_>,
        root: SymbolId,
        statement: StatementId,
    ) -> NonNullProof {
        let span = lowerer
            .parsed
            .ast()
            .statements()
            .get(statement)
            .unwrap()
            .span();
        let (header, owner, view, body) = lowerer
            .function
            .blocks
            .iter()
            .find_map(|block| {
                let term = block.terminator.as_ref()?;
                if term.origin != Origin::Source(span) {
                    return None;
                }
                let TerminatorKind::NullableBranch {
                    owner,
                    view,
                    when_non_null,
                    ..
                } = &term.kind
                else {
                    return None;
                };
                Some((block.id, *owner, *view, when_non_null.target))
            })
            .unwrap();
        let owner_column = parameter(lowerer.function, EntityId::Value(owner)).1;
        let incoming = incoming(lowerer.function, header);
        let [(preheader, edge)] = incoming.as_slice() else {
            panic!("bounded source has one real preheader")
        };
        assert_eq!(*preheader, lowerer.function.entry_block().unwrap());
        let mut carried = lowerer
            .linear_binding_slots(&BTreeMap::new(), span)
            .unwrap();
        carried.slots = edge
            .arguments
            .iter()
            .enumerate()
            .map(|(column, &source)| {
                let ty = lowerer.function.entity(source).unwrap().ty;
                assert_eq!(
                    lowerer
                        .function
                        .entity(lowerer.function.block(header).unwrap().parameters[column])
                        .unwrap()
                        .ty,
                    ty
                );
                let symbol = if column == owner_column {
                    root
                } else {
                    let matches = lowerer
                        .names
                        .symbols()
                        .iter()
                        .filter(|symbol| {
                            matches!(symbol.name(), "items" | "total")
                                && lowerer
                                    .typed
                                    .symbol_type(symbol.id())
                                    .and_then(|ty| lowerer.type_ids.get(&ty))
                                    .copied()
                                    == Some(ty.semantic_type())
                        })
                        .collect::<Vec<_>>();
                    assert_eq!(
                        matches.len(),
                        1,
                        "real preheader column has one fixture local"
                    );
                    matches[0].id()
                };
                control::LinearBindingSlot::new(Some(symbol), source, ty)
            })
            .collect();
        lowerer.loops.push(loop_control::LoopContext {
            entry_views: BTreeMap::new(),
            outer_borrowed_while: None,
            map_loop: None,
            header,
            carried,
            continues: Vec::new(),
            breaks: Vec::new(),
            for_loop: None,
            numeric_loop: None,
        });
        let EntityId::Value(body_owner) =
            lowerer.function.block(body).unwrap().parameters[owner_column]
        else {
            panic!("real owner column")
        };
        let EntityType::Loan {
            target: inner,
            kind: LoanKind::Shared,
        } = lowerer.function.entity(EntityId::Loan(view)).unwrap().ty
        else {
            panic!("real nullable branch view")
        };
        assert_eq!(parameter(lowerer.function, EntityId::Loan(view)).0, body);
        NonNullProof::introduced(
            view,
            body_owner,
            inner,
            non_null_proof::ProofScope::While(statement),
        )
    }

    /// Stop immediately before the real adjacent end/drop pair, retaining the complete
    /// earlier instruction prefix and every other block/edge. No predecessor is removed.
    pub(in crate::ssa) fn before_cleanup(
        lowerer: &mut ExpressionLowerer<'_>,
        block: BlockId,
        root: SymbolId,
        span: Span,
        inherited: Option<NonNullProof>,
    ) -> ValueId {
        let instructions = lowerer.function.block(block).unwrap().instructions.clone();
        let drops = instructions
            .iter()
            .enumerate()
            .filter_map(|(index, id)| {
                let instruction = lowerer.function.instruction(*id).unwrap();
                let Operation::Drop { owner } = instruction.operation else {
                    return None;
                };
                (instruction.origin == Origin::Source(span)
                    && lowerer.function.entity(EntityId::Value(owner)).unwrap().ty
                        == EntityType::Value(
                            lowerer.type_ids[&lowerer.typed.symbol_type(root).unwrap()],
                        ))
                .then_some((index, owner))
            })
            .collect::<Vec<_>>();
        let [(index, owner)] = drops.as_slice() else {
            panic!("one real current-owner cleanup")
        };
        let mut prefix = *index;
        if let Some(mut proof) = inherited {
            assert!(*index > 0);
            let end = lowerer
                .function
                .instruction(instructions[index - 1])
                .unwrap();
            let Operation::BorrowEnd { loan } = end.operation else {
                panic!("actual owner cleanup must first retire its view")
            };
            assert_eq!(end.origin, Origin::Source(span));
            assert_eq!(parameter(lowerer.function, EntityId::Loan(loan)).0, block);
            proof.owner = *owner;
            proof.loan = loan;
            lowerer.non_null_bindings.insert(root, proof);
            prefix -= 1;
        }
        assert_eq!(
            parameter(lowerer.function, EntityId::Value(*owner)).0,
            block
        );
        lowerer.function.blocks[block.index()]
            .instructions
            .truncate(prefix);
        lowerer.function.blocks[block.index()].terminator = None;
        lowerer.block = block;
        lowerer.bindings.insert(root, LoweredValue::Value(*owner));
        // Borrow Boolean parameters are independent, caller-owned, and remain live.
        let entry = lowerer.function.entry_block().unwrap();
        for &entity in &lowerer.function.block(entry).unwrap().parameters {
            if let EntityId::Loan(loan) = entity {
                let parameter = lowerer
                    .names
                    .symbols()
                    .iter()
                    .find(|symbol| symbol.name() == "flag")
                    .unwrap()
                    .id();
                assert_eq!(
                    lowerer.function.entity(entity).unwrap().ty,
                    EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: lowerer.type_ids[&lowerer.typed.symbol_type(parameter).unwrap()]
                    }
                );
                lowerer.borrow_bindings.insert(parameter, loan);
            }
        }
        *owner
    }
}
