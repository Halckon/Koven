//! 单 MemoryDocument 的严格 Unit 接入；single 标签保留原 source shape 含义。
use lang_frontend::name_resolution::SymbolKind;

use super::{LoweringError, model::Program};
use crate::native_tests::boxed_enum_tests::unit_source::UnitSource;

pub(super) fn single(text: &str) -> Result<Program, LoweringError> {
    let analysis = UnitSource::analyze("result.ko", text)
        .expect("borrow source must pass strict Unit analysis");
    analysis.assert_frontend_clean();
    let entry = analysis.declaration("entry", SymbolKind::Function);
    analysis.lower_entry(entry).map(|(program, _)| program)
}

pub(in crate::ssa) mod borrowed_cleanup_carrier {
    //! Read-only simultaneous CFG-column checks, grounded in the actual outer branch.
    use crate::ssa::lower_frontend::nullable_borrowed_cleanup::*;
    use crate::ssa::lower_frontend::*;
    use crate::ssa::model::{Definition, Instruction, SsaTypeKind};
    use std::collections::BTreeSet;

    #[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
    pub(in crate::ssa) struct Columns {
        pub(in crate::ssa) block: BlockId,
        pub(in crate::ssa) entities: Vec<EntityId>,
    }

    pub(in crate::ssa) struct Birth {
        pub(in crate::ssa) pair: Columns,
        pub(in crate::ssa) source_owner: ValueId,
    }

    pub(in crate::ssa) fn parameter(
        function: &Function,
        entity: EntityId,
    ) -> Option<(BlockId, usize)> {
        let Definition::BlockParameter { block, index } = function.entity(entity)?.definition else {
            return None;
        };
        (function.block(block)?.parameters.get(index) == Some(&entity)).then_some((block, index))
    }

    pub(in crate::ssa) fn result(
        function: &Function,
        entity: EntityId,
    ) -> Option<&Instruction> {
        let Definition::InstructionResult { instruction, index } = function.entity(entity)?.definition
        else {
            return None;
        };
        let instruction = function.instruction(instruction)?;
        (instruction.results.get(index) == Some(&entity)).then_some(instruction)
    }

    pub(in crate::ssa) fn instructions(
        function: &Function,
        block: BlockId,
    ) -> Option<Vec<&Instruction>> {
        function
            .block(block)?
            .instructions
            .iter()
            .map(|id| function.instruction(*id))
            .collect()
    }

    pub(in crate::ssa) fn incoming(
        function: &Function,
        target: BlockId,
    ) -> Vec<(BlockId, &Edge)> {
        function
            .blocks
            .iter()
            .flat_map(|block| {
                let edges = match block.terminator.as_ref().map(|term| &term.kind) {
                    Some(TerminatorKind::Branch(edge)) => vec![edge],
                    Some(TerminatorKind::Conditional {
                        when_true,
                        when_false,
                        ..
                    }) => vec![when_true, when_false],
                    Some(TerminatorKind::NullableBranch {
                        when_null,
                        when_non_null,
                        ..
                    }) => vec![when_null, when_non_null],
                    _ => Vec::new(),
                };
                edges
                    .into_iter()
                    .filter(move |edge| edge.target == target)
                    .map(move |edge| (block.id, edge))
            })
            .collect()
    }

    pub(in crate::ssa) fn forward(
        function: &Function,
        edge: &Edge,
        entities: &[EntityId],
    ) -> Option<Columns> {
        let parameters = &function.block(edge.target)?.parameters;
        let entities = entities
            .iter()
            .map(|entity| {
                let index = unique(
                    edge.arguments
                        .iter()
                        .enumerate()
                        .filter_map(|(index, value)| (value == entity).then_some(index)),
                )?;
                let target = *parameters.get(index)?;
                (function.entity(target)?.ty == function.entity(*entity)?.ty).then_some(target)
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Columns {
            block: edge.target,
            entities,
        })
    }

    pub(in crate::ssa) fn birth(
        lowerer: &ExpressionLowerer<'_>,
        plan: &Plan,
        proof: NonNullProof,
    ) -> Option<Birth> {
        let (statement, view) = proof.while_origin()?;
        let [outer, rest @ ..] = lowerer.loops.as_slice() else {
            return None;
        };
        if statement != plan.outer
            || rest.len() > 1
            || rest.iter().any(|context| {
                context.numeric_loop.is_some()
                    || context
                        .for_loop
                        .as_ref()
                        .is_none_or(|data| data.statement != plan.statement)
            })
            || outer.for_loop.is_some()
            || outer.numeric_loop.is_some()
            || !outer.entry_views.is_empty()
            || !outer.breaks.is_empty()
            || !outer.continues.is_empty()
        {
            return None;
        }
        let term = lowerer.function.block(outer.header)?.terminator.as_ref()?;
        let TerminatorKind::NullableBranch {
            owner,
            view: actual_view,
            when_non_null,
            when_null,
        } = &term.kind
        else {
            return None;
        };
        let span = lowerer
            .parsed
            .ast()
            .statements()
            .get(statement)
            .ok()?
            .span();
        let (block, owner_column) = parameter(lowerer.function, EntityId::Value(*owner))?;
        let source_column = unique(
            outer
                .carried
                .slots
                .iter()
                .enumerate()
                .filter_map(|(index, slot)| (slot.symbol == Some(plan.source_root)).then_some(index)),
        )?;
        let slot = outer.carried.slots.get(owner_column)?;
        if *actual_view != view
            || term.origin != Origin::Source(span)
            || when_non_null.target == when_null.target
            || incoming(lowerer.function, when_non_null.target).as_slice()
                != [(outer.header, when_non_null)]
            || block != outer.header
            || slot.symbol != Some(plan.root)
            || outer
                .carried
                .slots
                .iter()
                .filter(|slot| slot.symbol == Some(plan.root))
                .count()
                != 1
            || when_non_null.arguments.get(owner_column) != Some(&EntityId::Value(*owner))
            || source_column == owner_column
        {
            return None;
        }
        let header_source = *lowerer
            .function
            .block(outer.header)?
            .parameters
            .get(source_column)?;
        if when_non_null.arguments.get(source_column) != Some(&header_source) {
            return None;
        }
        let target = lowerer.function.block(when_non_null.target)?;
        let EntityId::Value(owner) = *target.parameters.get(owner_column)? else {
            return None;
        };
        let EntityId::Value(source_owner) = *target.parameters.get(source_column)? else {
            return None;
        };
        let source_type = *lowerer
            .type_ids
            .get(&lowerer.typed.symbol_type(plan.source_root)?)?;
        let nullable = *lowerer.type_ids.get(&plan.storage)?;
        if owner == source_owner
            || lowerer.ssa_types.get(nullable.index())
                != Some(&SsaTypeKind::NullableHandle { inner: proof.inner })
            || !matches!(
                lowerer.ssa_types.get(proof.inner.index()),
                Some(SsaTypeKind::HeapOwner { .. })
            )
            || lowerer.function.entity(EntityId::Value(owner))?.ty != EntityType::Value(nullable)
            || lowerer.function.entity(EntityId::Value(source_owner))?.ty
                != EntityType::Value(source_type)
            || lowerer.function.entity(EntityId::Loan(view))?.ty
                != (EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: proof.inner,
                })
            || parameter(lowerer.function, EntityId::Loan(view))?.0 != target.id
        {
            return None;
        }
        Some(Birth {
            pair: Columns {
                block: target.id,
                entities: vec![EntityId::Value(owner), EntityId::Loan(view)],
            },
            source_owner,
        })
    }

    /// All columns cross each predecessor together. Closed ungrounded SCCs fail.
    /// Only the caller's exact terminal element end may consume a tracked loan.
    pub(in crate::ssa) fn transport(
        function: &Function,
        current: Columns,
        anchor: &Columns,
        allowed_end: Option<(BlockId, LoanId)>,
    ) -> bool {
        let mut graph = BTreeMap::<Columns, Vec<Columns>>::new();
        let mut pending = vec![current];
        while let Some(state) = pending.pop() {
            if graph.contains_key(&state) {
                continue;
            }
            if state.entities.len() != anchor.entities.len()
                || state.entities.iter().enumerate().any(|(index, entity)| {
                    state.entities[..index].contains(entity)
                        || function.entity(*entity).map(|data| data.ty)
                            != function.entity(anchor.entities[index]).map(|data| data.ty)
                })
            {
                return false;
            }
            let Some(ops) = instructions(function, state.block) else {
                return false;
            };
            for instruction in ops {
                match instruction.operation {
                    Operation::BorrowEnd { loan }
                        if state.entities.contains(&EntityId::Loan(loan))
                            && allowed_end != Some((state.block, loan)) =>
                    {
                        return false;
                    }
                    Operation::Drop { owner } if state.entities.contains(&EntityId::Value(owner)) => {
                        return false;
                    }
                    _ => {}
                }
            }
            if state == *anchor {
                graph.insert(state, Vec::new());
                continue;
            }
            let Some(columns) = state
                .entities
                .iter()
                .map(|entity| {
                    let (block, index) = parameter(function, *entity)?;
                    (block == state.block).then_some(index)
                })
                .collect::<Option<Vec<_>>>()
            else {
                return false;
            };
            let predecessors = incoming(function, state.block);
            if predecessors.is_empty() {
                return false;
            }
            let mut parents = Vec::new();
            for (block, edge) in predecessors {
                if !matches!(
                    function
                        .block(block)
                        .and_then(|block| block.terminator.as_ref())
                        .map(|term| &term.kind),
                    Some(TerminatorKind::Branch(_) | TerminatorKind::Conditional { .. })
                ) {
                    return false;
                }
                let Some(entities) = columns
                    .iter()
                    .map(|column| {
                        let entity = *edge.arguments.get(*column)?;
                        (edge
                            .arguments
                            .iter()
                            .filter(|value| **value == entity)
                            .count()
                            == 1)
                            .then_some(entity)
                    })
                    .collect::<Option<Vec<_>>>()
                else {
                    return false;
                };
                let parent = Columns { block, entities };
                parents.push(parent.clone());
                pending.push(parent);
            }
            graph.insert(state, parents);
        }
        if !graph.contains_key(anchor) {
            return false;
        }
        let mut anchored = BTreeSet::from([anchor.clone()]);
        loop {
            let before = anchored.len();
            for (state, parents) in &graph {
                if parents.iter().any(|parent| anchored.contains(parent)) {
                    anchored.insert(state.clone());
                }
            }
            if before == anchored.len() {
                return anchored.len() == graph.len();
            }
        }
    }

    pub(in crate::ssa) mod source {
        //! Bounded AST and typed-use qualification, independent of runtime SSA transport.
        use super::super::*;
        use lang_frontend::{
            name_resolution::ReferenceTarget,
            parser::{FunctionBody, FunctionForm, VariableKind},
            type_checking::{ContainerConstructionKind, SequentialContainerKind},
        };

        #[allow(clippy::too_many_arguments)]
        pub(in crate::ssa) fn qualify(
            parsed: &ParsedFile,
            names: &NameResolution,
            typed: &TypedFile,
            statement: StatementId,
            member: ExpressionId,
            root: SymbolId,
            seed: SymbolId,
            source_root: SymbolId,
            element: SymbolId,
            source_seed: ExpressionId,
        ) -> Option<StatementId> {
            let storage = typed.symbol_type(root)?;
            let Statement::For { source, body, .. } =
                parsed.ast().statements().get(statement).ok()?.payload()
            else {
                return None;
            };
            let leaf = ungroup(parsed, *source)?;
            if name(parsed, names, leaf)? != source_root
                || typed.expression_type(*source) != typed.symbol_type(source_root)
                || typed.expression_type(leaf) != typed.symbol_type(source_root)
            {
                return None;
            }
            let body_span = parsed.ast().statements().get(*body).ok()?.span();
            for (_, node) in parsed
                .ast()
                .statements()
                .iter()
                .filter(|(_, node)| contains(body_span, node.span()))
            {
                if !matches!(
                    node.payload(),
                    Statement::Block { .. }
                        | Statement::ControlBody { .. }
                        | Statement::Expression { .. }
                ) {
                    return None;
                }
            }
            for (id, node) in parsed
                .ast()
                .expressions()
                .iter()
                .filter(|(_, node)| contains(body_span, node.span()))
            {
                match node.payload() {
                    Expression::If { .. } | Expression::Continue { .. } => {}
                    Expression::Assignment {
                        target,
                        operator: AssignmentOperator::Assign,
                        ..
                    } => {
                        let symbol = name(parsed, names, *target)?;
                        if [root, source_root, element].contains(&symbol)
                            || !scalar(typed, typed.symbol_type(symbol)?)
                        {
                            return None;
                        }
                    }
                    _ => {
                        readonly_expression(parsed, names, typed, id, root, storage)?;
                    }
                }
            }
            let Expression::Member {
                receiver,
                safe: false,
                ..
            } = expr(parsed, member)?
            else {
                return None;
            };
            let use_fact = typed.non_null_use(*receiver)?;
            if name(parsed, names, *receiver)? != root
                || use_fact.expression() != *receiver
                || use_fact.symbol() != root
                || use_fact.declared_type() != storage
                || typed.types().get(storage) != Some(&TypeKind::Nullable(use_fact.narrowed_type()))
            {
                return None;
            }
            let mut loops = parsed.ast().statements().iter().filter_map(|(id, node)| {
                let Statement::While {
                    condition, body, ..
                } = node.payload()
                else {
                    return None;
                };
                let [borrowed, tail] = elements(parsed, *body)? else {
                    return None;
                };
                let Statement::Expression { expression } =
                    parsed.ast().statements().get(*tail).ok()?.payload()
                else {
                    return None;
                };
                let Expression::Return {
                    value: Some(value), ..
                } = expr(parsed, *expression)?
                else {
                    return None;
                };
                let guard = typed.null_comparison(ungroup(parsed, *condition)?)?;
                if *borrowed != statement
                    || guard.symbol() != root
                    || guard.nullable_type() != storage
                    || !guard.non_null_when_true()
                    || !contains(
                        parsed.ast().expressions().get(*value).ok()?.span(),
                        parsed.ast().expressions().get(member).ok()?.span(),
                    )
                {
                    return None;
                }
                let tail_span = parsed.ast().expressions().get(*value).ok()?.span();
                for (id, _) in parsed
                    .ast()
                    .expressions()
                    .iter()
                    .filter(|(_, node)| contains(tail_span, node.span()))
                {
                    readonly_expression(parsed, names, typed, id, root, storage)?;
                }
                Some(id)
            });
            let outer = loops.next()?;
            if loops.next().is_some() {
                return None;
            }
            // Tie the frontend parameter seed to the real source binding before this outer loop.
            let mut functions = parsed.ast().items().iter().filter_map(|(_, node)| {
                let Item::Function {
                    name: NameMarker::Present(function_name),
                    form,
                    ..
                } = node.payload()
                else {
                    return None;
                };
                let body = match form {
                    FunctionForm::ImplicitUnitBlock(body)
                    | FunctionForm::Explicit {
                        body: FunctionBody::Block(body),
                        ..
                    } => *body,
                    _ => return None,
                };
                let statements = elements(parsed, body)?;
                let index = statements.iter().position(|id| *id == outer)?;
                let mut initialized = false;
                let mut source_initialized = false;
                for id in &statements[..index] {
                    let Statement::LocalVariable { declaration } =
                        parsed.ast().statements().get(*id).ok()?.payload()
                    else {
                        return None;
                    };
                    let Item::Variable {
                        name: NameMarker::Present(span),
                        initializer,
                        kind,
                        ..
                    } = parsed.ast().items().get(*declaration).ok()?.payload()
                    else {
                        return None;
                    };
                    if *span == names.symbols().get(root.index())?.span() {
                        if initialized
                            || *kind != VariableKind::Var
                            || name(parsed, names, *initializer)? != seed
                        {
                            return None;
                        }
                        initialized = true;
                    } else if *span == names.symbols().get(source_root.index())?.span() {
                        let leaf = ungroup(parsed, *initializer)?;
                        let construction = typed.container_construction(leaf)?;
                        let Expression::Call { arguments, .. } = expr(parsed, leaf)? else {
                            return None;
                        };
                        if source_initialized
                            || leaf != source_seed
                            || *kind != VariableKind::Val
                            || construction.expression() != leaf
                            || construction.kind() != ContainerConstructionKind::ListForm
                            || construction.container() != SequentialContainerKind::List
                            || Some(construction.container_type()) != typed.symbol_type(source_root)
                            || builtin_type(typed, construction.element_type())
                                != Some(BuiltinType::Int)
                            || construction.parameter_modes().len() != arguments.len()
                            || construction
                                .parameter_modes()
                                .iter()
                                .any(|mode| *mode != ParameterMode::Value)
                            || arguments.iter().any(|argument| {
                                !matches!(
                                    expr(parsed, argument.value),
                                    Some(Expression::Literal(LiteralKind::Integer(_)))
                                )
                            })
                        {
                            return None;
                        }
                        source_initialized = true;
                    } else if !matches!(expr(parsed, *initializer)?, Expression::Literal(_)) {
                        return None;
                    }
                }
                let callable = typed.callables().iter().find(|callable| {
                    names
                        .symbols()
                        .get(callable.symbol().index())
                        .is_some_and(|symbol| symbol.span() == *function_name)
                })?;
                let index = callable
                    .parameter_symbols()
                    .iter()
                    .position(|symbol| *symbol == Some(seed))?;
                let parameter = callable.parameters().get(index)?;
                (initialized
                    && source_initialized
                    && callable.owner().is_none()
                    && callable.receiver().is_none()
                    && callable.extension_receiver_symbol().is_none()
                    && callable.type_parameters().is_empty()
                    && parameter.mode == ParameterMode::Value
                    && parameter.ty == storage)
                    .then_some(())
            });
            functions.next()?;
            functions.next().is_none().then_some(outer)
        }

        fn readonly_expression(
            parsed: &ParsedFile,
            names: &NameResolution,
            typed: &TypedFile,
            id: ExpressionId,
            root: SymbolId,
            storage: TypeId,
        ) -> Option<()> {
            match expr(parsed, id)? {
                Expression::Name => {
                    let symbol = name(parsed, names, id)?;
                    if symbol == root {
                        let fact = typed.non_null_use(id)?;
                        if fact.expression() != id
                            || fact.symbol() != root
                            || fact.declared_type() != storage
                            || typed.types().get(storage)
                                != Some(&TypeKind::Nullable(fact.narrowed_type()))
                            || typed.expression_type(id) != Some(fact.narrowed_type())
                        {
                            return None;
                        }
                    } else if !scalar(typed, typed.expression_type(id)?) {
                        return None;
                    }
                }
                Expression::Member {
                    receiver,
                    safe: false,
                    ..
                } => {
                    if name(parsed, names, *receiver)? != root
                        || builtin_type(typed, typed.expression_type(id)?) != Some(BuiltinType::Int)
                    {
                        return None;
                    }
                }
                Expression::Literal(_) | Expression::Group { .. } | Expression::Binary { .. } => {
                    if !scalar(typed, typed.expression_type(id)?) {
                        return None;
                    }
                }
                _ => return None,
            }
            Some(())
        }
        fn scalar(typed: &TypedFile, ty: TypeId) -> bool {
            matches!(
                builtin_type(typed, ty),
                Some(BuiltinType::Int | BuiltinType::Long | BuiltinType::Boolean)
            )
        }
        pub(in crate::ssa) fn expr(
            parsed: &ParsedFile,
            id: ExpressionId,
        ) -> Option<&Expression> {
            Some(parsed.ast().expressions().get(id).ok()?.payload())
        }
        fn ungroup(parsed: &ParsedFile, mut id: ExpressionId) -> Option<ExpressionId> {
            for _ in 0..=parsed.ast().expressions().len() {
                match expr(parsed, id)? {
                    Expression::Group { expression } => id = *expression,
                    _ => return Some(id),
                }
            }
            None
        }
        fn name(parsed: &ParsedFile, names: &NameResolution, id: ExpressionId) -> Option<SymbolId> {
            let node = parsed.ast().expressions().get(id).ok()?;
            if !matches!(node.payload(), Expression::Name) {
                return None;
            }
            names
                .references()
                .iter()
                .find_map(|reference| match reference.target() {
                    ReferenceTarget::Symbol(symbol) if reference.span() == node.span() => Some(*symbol),
                    _ => None,
                })
        }
        fn elements(parsed: &ParsedFile, id: StatementId) -> Option<&[StatementId]> {
            match parsed.ast().statements().get(id).ok()?.payload() {
                Statement::Block { elements, .. } | Statement::ControlBody { elements, .. } => {
                    Some(elements)
                }
                _ => None,
            }
        }
        fn contains(outer: Span, inner: Span) -> bool {
            outer.source_id() == inner.source_id()
                && outer.start() <= inner.start()
                && inner.end() <= outer.end()
        }
    }
}
