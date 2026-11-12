//! Observable constructor results reuse committed container descriptors and owner views.

use super::*;
use crate::ssa::model::{PlaceAccess, ValueId};
use lang_frontend::type_checking::Copyability;

impl ExpressionLowerer<'_> {
    pub(in crate::ssa::lower_frontend) fn lower_container_size(
        &mut self,
        expression: ExpressionId,
    ) -> Result<LoweredValue, LoweringError> {
        let descriptor = self.typed.container_size(expression).ok_or(LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: None,
        })?;
        let span = descriptor.span();
        if self.typed.expression_type(expression) != Some(descriptor.result_type())
            || builtin_type(self.typed, descriptor.result_type()) != Some(BuiltinType::Int)
            || self.typed.expression_type(descriptor.receiver())
                != Some(descriptor.container_type())
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        self.validate_container_identity(
            descriptor.container(),
            descriptor.element_type(),
            descriptor.container_type(),
            span,
        )?;
        let receiver_span = self.expression_span(descriptor.receiver())?;
        let (loan, ending) =
            self.lower_borrow_argument(expression, descriptor.receiver(), receiver_span)?;
        self.pending_call_loans.insert(
            (expression.index(), descriptor.receiver().index()),
            ending.then_some(loan),
        );
        let ty = self.expression_ssa_type(expression, span)?;
        let (_, results) = self.append(
            Operation::ContainerLength {
                owner: EntityId::Loan(loan),
            },
            vec![EntityType::Value(ty)],
            span,
        )?;
        self.finish_borrowed_call(expression, span)?;
        Ok(LoweredValue::Value(value(results[0])))
    }

    /// A plain read copies only a statically Copyable element. MoveOnly elements keep Borrow access.
    pub(in crate::ssa::lower_frontend) fn lower_container_index(
        &mut self,
        expression: ExpressionId,
    ) -> Result<LoweredValue, LoweringError> {
        let span = self.expression_span(expression)?;
        let descriptor = self
            .typed
            .element_place(expression)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let element = self.resolve_type(descriptor.element_type(), span)?;
        if self.typed.copyability(element) != Some(Copyability::Copyable)
            || builtin_type(self.typed, element) == Some(BuiltinType::Unit)
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let owner = self.container_view_operand(descriptor.receiver())?;
        let index = self.require_value(descriptor.index())?;
        let element = self.expression_ssa_type(expression, span)?;
        let (_, results) = self.append(
            Operation::ContainerElementPlace { owner, index },
            vec![EntityType::Place(element)],
            span,
        )?;
        let (_, results) = self.append(
            Operation::Read {
                source: PlaceAccess::Place(place(results[0])),
            },
            vec![EntityType::Value(element)],
            span,
        )?;
        Ok(LoweredValue::Value(value(results[0])))
    }

    fn container_view_operand(
        &mut self,
        expression: ExpressionId,
    ) -> Result<EntityId, LoweringError> {
        let span = self.expression_span(expression)?;
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
        match node.payload() {
            Expression::Group { expression } => self.container_view_operand(*expression),
            Expression::Name => {
                let symbol = self
                    .references
                    .get(&span_key(span))
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                if let Some(LoweredValue::Value(owner)) = self.bindings.get(symbol).copied() {
                    return Ok(EntityId::Value(owner));
                }
                if let Some(&loan) = self.borrow_bindings.get(symbol) {
                    return Ok(EntityId::Loan(loan));
                }
                self.require_value(expression).map(EntityId::Value)
            }
            _ => self.require_value(expression).map(EntityId::Value),
        }
    }

    pub(in crate::ssa::lower_frontend) fn lower_container_append(
        &mut self,
        expression: ExpressionId,
    ) -> Result<LoweredValue, LoweringError> {
        let descriptor = self
            .typed
            .container_append(expression)
            .ok_or(LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
        let span = descriptor.span();
        if descriptor.expression() != expression {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let element_type = self
            .typed
            .expression_type(descriptor.element())
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let element_type = self.resolve_type(element_type, span)?;
        if element_type != self.resolve_type(descriptor.element_type(), span)? {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let copyability = self
            .typed
            .copyability(element_type)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let (owner, symbol) = self.container_owner_for_append(descriptor.receiver())?;
        let element = self.require_value(descriptor.element())?;
        let container_type = self.expression_ssa_type(descriptor.receiver(), span)?;
        let (_, results) = self.append(
            Operation::ContainerAppend { owner, element },
            vec![EntityType::Value(container_type)],
            span,
        )?;
        // Append owns this exact MoveOnly value, including any Group aliases.
        // Unrelated owners and Copyable source bindings remain caller-owned.
        match copyability {
            Copyability::MoveOnly => self.forget_delivered_owners(&[element]),
            Copyability::Copyable => {}
            Copyability::Unknown | Copyability::Error => {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
        }
        let new_owner = value(results[0]);
        if let Some(symbol) = symbol {
            self.bindings.insert(symbol, LoweredValue::Value(new_owner));
        }
        self.emit_drops(
            lang_frontend::ownership_checking::DropPoint::AfterExpression(descriptor.element()),
        )?;
        self.emit_drops(lang_frontend::ownership_checking::DropPoint::CallReturn(
            expression,
        ))?;
        Ok(LoweredValue::Unit)
    }

    pub(in crate::ssa::lower_frontend) fn lower_container_clear(
        &mut self,
        expression: ExpressionId,
    ) -> Result<LoweredValue, LoweringError> {
        let descriptor = self
            .typed
            .container_clear(expression)
            .ok_or(LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
        let span = descriptor.span();
        let (owner, symbol) = self.container_owner_for_append(descriptor.receiver())?;
        let container_type = self.expression_ssa_type(descriptor.receiver(), span)?;
        let (_, results) = self.append(
            Operation::ContainerClear { owner },
            vec![EntityType::Value(container_type)],
            span,
        )?;
        let new_owner = value(results[0]);
        if let Some(symbol) = symbol {
            self.bindings.insert(symbol, LoweredValue::Value(new_owner));
        }
        self.emit_drops(lang_frontend::ownership_checking::DropPoint::CallReturn(
            expression,
        ))?;
        Ok(LoweredValue::Unit)
    }

    pub(in crate::ssa::lower_frontend) fn lower_container_remove_at(
        &mut self,
        expression: ExpressionId,
    ) -> Result<LoweredValue, LoweringError> {
        let descriptor = self
            .typed
            .container_remove_at(expression)
            .ok_or(LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
        let span = descriptor.span();
        let (owner, symbol) = self.container_owner_for_append(descriptor.receiver())?;
        let index = self.require_value(descriptor.index())?;
        let element_type = self.expression_ssa_type(expression, span)?;
        let container_type = self.expression_ssa_type(descriptor.receiver(), span)?;
        let (_, results) = self.append(
            Operation::ContainerRemoveAt { owner, index },
            vec![
                EntityType::Value(element_type),
                EntityType::Value(container_type),
            ],
            span,
        )?;
        let removed_value = value(results[0]);
        let new_owner = value(results[1]);
        if let Some(symbol) = symbol {
            self.bindings.insert(symbol, LoweredValue::Value(new_owner));
        }
        self.emit_drops(
            lang_frontend::ownership_checking::DropPoint::AfterExpression(descriptor.index()),
        )?;
        self.emit_drops(lang_frontend::ownership_checking::DropPoint::CallReturn(
            expression,
        ))?;
        Ok(LoweredValue::Value(removed_value))
    }

    pub(in crate::ssa::lower_frontend) fn lower_container_remove_last(
        &mut self,
        expression: ExpressionId,
    ) -> Result<LoweredValue, LoweringError> {
        let descriptor = self
            .typed
            .container_remove_last(expression)
            .ok_or(LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
        let span = descriptor.span();
        let (owner, symbol) = self.container_owner_for_append(descriptor.receiver())?;
        let element_type = self.expression_ssa_type(expression, span)?;
        let container_type = self.expression_ssa_type(descriptor.receiver(), span)?;
        let (_, results) = self.append(
            Operation::ContainerRemoveLast { owner },
            vec![
                EntityType::Value(element_type),
                EntityType::Value(container_type),
            ],
            span,
        )?;
        let removed_value = value(results[0]);
        let new_owner = value(results[1]);
        if let Some(symbol) = symbol {
            self.bindings.insert(symbol, LoweredValue::Value(new_owner));
        }
        self.emit_drops(lang_frontend::ownership_checking::DropPoint::CallReturn(
            expression,
        ))?;
        Ok(LoweredValue::Value(removed_value))
    }

    pub(in crate::ssa::lower_frontend) fn lower_container_remove_first(
        &mut self,
        expression: ExpressionId,
    ) -> Result<LoweredValue, LoweringError> {
        let descriptor = self
            .typed
            .container_remove_first(expression)
            .ok_or(LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
        let span = descriptor.span();
        let (owner, symbol) = self.container_owner_for_append(descriptor.receiver())?;
        let element_type = self.expression_ssa_type(expression, span)?;
        let container_type = self.expression_ssa_type(descriptor.receiver(), span)?;
        let (_, results) = self.append(
            Operation::ContainerRemoveFirst { owner },
            vec![
                EntityType::Value(element_type),
                EntityType::Value(container_type),
            ],
            span,
        )?;
        let removed_value = value(results[0]);
        let new_owner = value(results[1]);
        if let Some(symbol) = symbol {
            self.bindings.insert(symbol, LoweredValue::Value(new_owner));
        }
        self.emit_drops(lang_frontend::ownership_checking::DropPoint::CallReturn(
            expression,
        ))?;
        Ok(LoweredValue::Value(removed_value))
    }

    pub(in crate::ssa::lower_frontend) fn lower_container_insert_at(
        &mut self,
        expression: ExpressionId,
    ) -> Result<LoweredValue, LoweringError> {
        let descriptor = self
            .typed
            .container_insert_at(expression)
            .ok_or(LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
        let span = descriptor.span();
        let (owner, symbol) = self.container_owner_for_append(descriptor.receiver())?;
        let index = self.require_value(descriptor.index())?;
        let element = self.require_value(descriptor.element())?;
        let container_type = self.expression_ssa_type(descriptor.receiver(), span)?;
        let (_, results) = self.append(
            Operation::ContainerInsertAt {
                owner,
                index,
                element,
            },
            vec![EntityType::Value(container_type)],
            span,
        )?;
        let new_owner = value(results[0]);
        if let Some(symbol) = symbol {
            self.bindings.insert(symbol, LoweredValue::Value(new_owner));
        }
        self.emit_drops(
            lang_frontend::ownership_checking::DropPoint::AfterExpression(descriptor.index()),
        )?;
        self.emit_drops(
            lang_frontend::ownership_checking::DropPoint::AfterExpression(descriptor.element()),
        )?;
        self.emit_drops(lang_frontend::ownership_checking::DropPoint::CallReturn(
            expression,
        ))?;
        Ok(LoweredValue::Unit)
    }

    fn container_owner_for_append(
        &mut self,
        expression: ExpressionId,
    ) -> Result<(ValueId, Option<lang_frontend::name_resolution::SymbolId>), LoweringError> {
        let span = self.expression_span(expression)?;
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
        match node.payload() {
            Expression::Group { expression } => self.container_owner_for_append(*expression),
            Expression::Name => {
                let symbol = self
                    .references
                    .get(&span_key(span))
                    .copied()
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                match self.bindings.get(&symbol).copied() {
                    Some(LoweredValue::Value(owner)) => Ok((owner, Some(symbol))),
                    _ => Err(error(LoweringErrorKind::MissingFact, span)),
                }
            }
            _ => {
                let owner = self.require_value(expression)?;
                Ok((owner, None))
            }
        }
    }

    pub(in crate::ssa::lower_frontend) fn lower_container_expression(
        &mut self,
        expression: ExpressionId,
    ) -> Result<Option<LoweredValue>, LoweringError> {
        if self.typed.range_size(expression).is_some() {
            return self.lower_range_size(expression).map(Some);
        }
        if self.typed.container_size(expression).is_some() {
            return self.lower_container_size(expression).map(Some);
        }
        if self.typed.element_place(expression).is_some() {
            return self.lower_container_index(expression).map(Some);
        }
        if self.typed.container_append(expression).is_some() {
            return self.lower_container_append(expression).map(Some);
        }
        if self.typed.container_clear(expression).is_some() {
            return self.lower_container_clear(expression).map(Some);
        }
        if self.typed.container_remove_at(expression).is_some() {
            return self.lower_container_remove_at(expression).map(Some);
        }
        if self.typed.container_remove_first(expression).is_some() {
            return self.lower_container_remove_first(expression).map(Some);
        }
        if self.typed.container_remove_last(expression).is_some() {
            return self.lower_container_remove_last(expression).map(Some);
        }
        if self.typed.container_insert_at(expression).is_some() {
            return self.lower_container_insert_at(expression).map(Some);
        }
        if self.typed.container_construction(expression).is_some() {
            return self.lower_container_construction(expression).map(Some);
        }
        Ok(None)
    }
}

pub(in crate::ssa::lower_frontend) mod outer_borrowed_source {
    //! Exact nesting and readonly uses, plus simultaneous provider columns across the nullable guard.
    use crate::ssa::lower_frontend::outer_borrowed_while::*;
    use lang_frontend::{
        name_resolution::ReferenceTarget,
        parser::{FunctionBody, FunctionForm, VariableKind},
        type_checking::ContainerConstructionKind,
    };

    pub(in crate::ssa::lower_frontend) mod carrier {
        use crate::ssa::lower_frontend::outer_borrowed_while::*;
        use crate::ssa::model::{Definition, Instruction};
        use std::collections::BTreeSet;
        #[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
        pub(in crate::ssa::lower_frontend) struct Columns {
            pub(in crate::ssa::lower_frontend) block: BlockId,
            pub(in crate::ssa::lower_frontend) entities: Vec<EntityId>,
        }
        pub(in crate::ssa::lower_frontend) struct Guard {
            pub(in crate::ssa::lower_frontend) header: BlockId,
            pub(in crate::ssa::lower_frontend) owner: ValueId,
            pub(in crate::ssa::lower_frontend) view: LoanId,
            pub(in crate::ssa::lower_frontend) when_null: BlockId,
            pub(in crate::ssa::lower_frontend) when_non_null: BlockId,
            pub(in crate::ssa::lower_frontend) non_null: Columns,
        }
        pub(in crate::ssa::lower_frontend) fn parameter(
            function: &Function,
            entity: EntityId,
        ) -> Option<(BlockId, usize)> {
            let Definition::BlockParameter { block, index } = function.entity(entity)?.definition
            else {
                return None;
            };
            (function.block(block)?.parameters.get(index) == Some(&entity)).then_some((block, index))
        }

        pub(in crate::ssa::lower_frontend) fn result(
            function: &Function,
            entity: EntityId,
        ) -> Option<&Instruction> {
            let Definition::InstructionResult { instruction, index } =
                function.entity(entity)?.definition
            else {
                return None;
            };
            let instruction = function.instruction(instruction)?;
            (instruction.results.get(index) == Some(&entity)).then_some(instruction)
        }

        pub(in crate::ssa::lower_frontend) fn instructions(
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

        pub(in crate::ssa::lower_frontend) fn incoming(
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

        pub(in crate::ssa::lower_frontend) fn forward(
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

        pub(in crate::ssa::lower_frontend) fn transport(
            function: &Function,
            current: Columns,
            anchor: &Columns,
            allowed_end: Option<(BlockId, LoanId)>,
            guard: Option<&Guard>,
        ) -> bool {
            transport_states(function, current, anchor, allowed_end, guard).is_some()
        }

        pub(in crate::ssa::lower_frontend) fn transport_states(
            function: &Function,
            current: Columns,
            anchor: &Columns,
            allowed_end: Option<(BlockId, LoanId)>,
            guard: Option<&Guard>,
        ) -> Option<BTreeSet<Columns>> {
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
                    return None;
                }
                let ops = instructions(function, state.block)?;
                for instruction in ops {
                    match instruction.operation {
                        Operation::BorrowEnd { loan }
                            if state.entities.contains(&EntityId::Loan(loan))
                                && allowed_end != Some((state.block, loan)) =>
                        {
                            return None;
                        }
                        Operation::Drop { owner }
                            if state.entities.contains(&EntityId::Value(owner)) =>
                        {
                            return None;
                        }
                        _ => {}
                    }
                }
                if state == *anchor {
                    graph.insert(state, Vec::new());
                    continue;
                }
                let columns = state
                    .entities
                    .iter()
                    .map(|entity| {
                        let (block, index) = parameter(function, *entity)?;
                        (block == state.block).then_some(index)
                    })
                    .collect::<Option<Vec<_>>>()?;
                let predecessors = incoming(function, state.block);
                if predecessors.is_empty() {
                    return None;
                }
                let mut parents = Vec::new();
                for (block, edge) in predecessors {
                    match function
                        .block(block)
                        .and_then(|block| block.terminator.as_ref())
                        .map(|term| &term.kind)
                    {
                        Some(TerminatorKind::Branch(_) | TerminatorKind::Conditional { .. }) => {}
                        Some(TerminatorKind::NullableBranch {
                            owner,
                            view,
                            when_null,
                            when_non_null,
                        }) if guard.is_some_and(|guard| {
                            guard.header == block
                                && guard.owner == *owner
                                && guard.view == *view
                                && guard.when_null == when_null.target
                                && guard.when_non_null == when_non_null.target
                        }) => {}
                        _ => return None,
                    }
                    let entities = columns
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
                        .collect::<Option<Vec<_>>>()?;
                    let parent = Columns { block, entities };
                    parents.push(parent.clone());
                    pending.push(parent);
                }
                graph.insert(state, parents);
            }
            if !graph.contains_key(anchor) {
                return None;
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
                    return (anchored.len() == graph.len()).then_some(anchored);
                }
            }
        }
    }

    pub(in crate::ssa::lower_frontend) fn qualify(
        parsed: &ParsedFile,
        names: &NameResolution,
        typed: &TypedFile,
        outer: StatementId,
        symbols: [SymbolId; 4],
        initializer: ExpressionId,
    ) -> Option<(StatementId, ExpressionId, usize)> {
        let [root, items, item, parameter] = symbols;
        let Statement::For { source, body, .. } = parsed.ast().statements().get(outer).ok()?.payload()
        else {
            return None;
        };
        if name(parsed, names, ungroup(parsed, *source)?)? != items
            || typed.expression_type(*source) != typed.symbol_type(items)
        {
            return None;
        }
        let [inner, tail] = elements(parsed, *body)? else {
            return None;
        };
        let Statement::While {
            condition, body, ..
        } = parsed.ast().statements().get(*inner).ok()?.payload()
        else {
            return None;
        };
        let guard = typed.null_comparison(ungroup(parsed, *condition)?)?;
        if guard.symbol() != root
            || Some(guard.nullable_type()) != typed.symbol_type(root)
            || !guard.non_null_when_true()
        {
            return None;
        }
        let [read, breaking] = elements(parsed, *body)? else {
            return None;
        };
        let Statement::Expression {
            expression: breaking,
        } = parsed.ast().statements().get(*breaking).ok()?.payload()
        else {
            return None;
        };
        if !matches!(expr(parsed, *breaking)?, Expression::Break { .. }) {
            return None;
        }
        for (statement, narrowed) in [(*read, true), (*tail, false)] {
            let Statement::Expression { expression } =
                parsed.ast().statements().get(statement).ok()?.payload()
            else {
                return None;
            };
            let Expression::Assignment {
                target,
                operator: AssignmentOperator::Assign,
                value,
                ..
            } = expr(parsed, *expression)?
            else {
                return None;
            };
            let target = name(parsed, names, *target)?;
            if [root, items, item].contains(&target)
                || builtin_type(typed, typed.symbol_type(target)?) != Some(BuiltinType::Int)
            {
                return None;
            }
            let span = parsed.ast().expressions().get(*value).ok()?.span();
            let mut item_read = false;
            let mut member_read = false;
            for (id, node) in parsed
                .ast()
                .expressions()
                .iter()
                .filter(|(_, node)| contains(span, node.span()))
            {
                readonly(parsed, names, typed, id, root, narrowed)?;
                item_read |= name(parsed, names, id) == Some(item);
                member_read |= matches!(node.payload(), Expression::Member { .. });
            }
            if !item_read || member_read != narrowed {
                return None;
            }
        }
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
            let mut current = false;
            let mut source = false;
            for statement in &statements[..index] {
                let Statement::LocalVariable { declaration } =
                    parsed.ast().statements().get(*statement).ok()?.payload()
                else {
                    return None;
                };
                let Item::Variable {
                    name: NameMarker::Present(span),
                    kind,
                    initializer: value,
                    ..
                } = parsed.ast().items().get(*declaration).ok()?.payload()
                else {
                    return None;
                };
                if *span == names.symbols().get(root.index())?.span() {
                    if current
                        || *kind != VariableKind::Var
                        || name(parsed, names, *value)? != parameter
                    {
                        return None;
                    }
                    current = true;
                } else if *span == names.symbols().get(items.index())?.span() {
                    let leaf = ungroup(parsed, *value)?;
                    let descriptor = typed.container_construction(leaf)?;
                    let Expression::Call { arguments, .. } = expr(parsed, leaf)? else {
                        return None;
                    };
                    if source
                        || *kind != VariableKind::Val
                        || leaf != initializer
                        || descriptor.expression() != leaf
                        || descriptor.kind() != ContainerConstructionKind::ListForm
                        || descriptor.container() != SequentialContainerKind::List
                        || Some(descriptor.container_type()) != typed.symbol_type(items)
                        || builtin_type(typed, descriptor.element_type()) != Some(BuiltinType::Int)
                        || descriptor.parameter_modes().len() != arguments.len()
                        || descriptor
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
                    source = true;
                } else if !matches!(expr(parsed, *value)?, Expression::Literal(_)) {
                    return None;
                }
            }
            let callable = typed.callables().iter().find(|callable| {
                names
                    .symbols()
                    .get(callable.symbol().index())
                    .is_some_and(|symbol| symbol.span() == *function_name)
            })?;
            let slot = callable
                .parameter_symbols()
                .iter()
                .position(|symbol| *symbol == Some(parameter))?;
            if !current
                || !source
                || callable.owner().is_some()
                || callable.receiver().is_some()
                || callable.extension_receiver_symbol().is_some()
                || !callable.type_parameters().is_empty()
                || callable.parameters().get(slot)?.mode != ParameterMode::Value
                || Some(callable.parameters().get(slot)?.ty) != typed.symbol_type(root)
            {
                return None;
            }
            Some(slot)
        });
        let slot = functions.next()?;
        functions
            .next()
            .is_none()
            .then_some((*inner, *breaking, slot))
    }
    fn readonly(
        parsed: &ParsedFile,
        names: &NameResolution,
        typed: &TypedFile,
        id: ExpressionId,
        root: SymbolId,
        narrowed: bool,
    ) -> Option<()> {
        let storage = typed.symbol_type(root)?;
        match expr(parsed, id)? {
            Expression::Name => {
                if name(parsed, names, id)? == root {
                    let fact = typed.non_null_use(id)?;
                    if !narrowed
                        || fact.expression() != id
                        || fact.symbol() != root
                        || fact.declared_type() != storage
                        || typed.types().get(storage) != Some(&TypeKind::Nullable(fact.narrowed_type()))
                        || typed.expression_type(id) != Some(fact.narrowed_type())
                    {
                        return None;
                    }
                } else if builtin_type(typed, typed.expression_type(id)?) != Some(BuiltinType::Int) {
                    return None;
                }
            }
            Expression::Member {
                receiver,
                safe: false,
                ..
            } => {
                if !narrowed
                    || name(parsed, names, *receiver)? != root
                    || builtin_type(typed, typed.expression_type(id)?) != Some(BuiltinType::Int)
                {
                    return None;
                }
            }
            Expression::Literal(_) | Expression::Group { .. } | Expression::Binary { .. } => {
                if builtin_type(typed, typed.expression_type(id)?) != Some(BuiltinType::Int) {
                    return None;
                }
            }
            _ => return None,
        }
        Some(())
    }
    fn expr(parsed: &ParsedFile, id: ExpressionId) -> Option<&Expression> {
        Some(parsed.ast().expressions().get(id).ok()?.payload())
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
    fn ungroup(parsed: &ParsedFile, mut id: ExpressionId) -> Option<ExpressionId> {
        for _ in 0..=parsed.ast().expressions().len() {
            match expr(parsed, id)? {
                Expression::Group { expression } => id = *expression,
                _ => return Some(id),
            }
        }
        None
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
