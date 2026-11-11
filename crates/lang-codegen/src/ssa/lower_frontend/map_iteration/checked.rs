//! Checked success Map transport and lexical Map ancestry / CFG column transport.
use super::*;
use control::map_frames::Snapshot;

pub(in crate::ssa::lower_frontend::map_iteration) mod nested {
    use super::super::*;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(in crate::ssa::lower_frontend::map_iteration) struct Nesting {
        pub(in crate::ssa::lower_frontend::map_iteration) parent: Option<StatementId>,
    }

    pub(in crate::ssa::lower_frontend::map_iteration) fn nesting(
        parsed: &ParsedFile,
        typed: &TypedFile,
        owned: &OwnershipCheckedFile,
        statement: StatementId,
    ) -> Result<Nesting, LoweringError> {
        let node = parsed
            .ast()
            .statements()
            .get(statement)
            .map_err(|_| LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
        let span = node.span();
        let inside = |outer: Span| {
            outer != span
                && outer.source_id() == span.source_id()
                && outer.start() <= span.start()
                && span.end() <= outer.end()
        };
        let mut ancestors = Vec::new();
        for (id, candidate) in parsed.ast().statements().iter() {
            if let Statement::For { body, .. } = candidate.payload() {
                let body = parsed
                    .ast()
                    .statements()
                    .get(*body)
                    .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
                if id == statement || !inside(body.span()) {
                    continue;
                }
                if crosses_callable(parsed, candidate.span(), span) {
                    return Err(error(LoweringErrorKind::UnsupportedNode, span));
                }
                let Some(descriptor) = typed.map_iteration(id) else {
                    let ordinary = typed
                        .sequential_iteration(id)
                        .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
                    let plan = owned
                        .iteration(id)
                        .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                    if plan.descriptor() != ordinary {
                        return Err(error(LoweringErrorKind::MissingFact, span));
                    }
                    continue;
                };
                let plan = owned
                    .map_iteration(id)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                if plan.descriptor() != descriptor {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                }
                ancestors.push((body.span().end() - body.span().start(), id));
            }
        }
        ancestors.sort_by_key(|(length, _)| *length);
        Ok(Nesting {
            parent: ancestors.first().map(|(_, id)| *id),
        })
    }

    fn crosses_callable(parsed: &ParsedFile, parent: Span, child: Span) -> bool {
        let contains_child = |boundary: Span| {
            boundary.source_id() == child.source_id()
                && parent.start() <= boundary.start()
                && boundary.start() <= child.start()
                && child.end() <= boundary.end()
                && boundary.end() <= parent.end()
        };
        parsed.ast().items().iter().any(|(_, item)| {
            matches!(item.payload(), Item::Function { .. } | Item::Deinit { .. })
                && contains_child(item.span())
        }) || parsed.ast().expressions().iter().any(|(_, expression)| {
            matches!(expression.payload(), Expression::Lambda { .. })
                && contains_child(expression.span())
        })
    }

    impl ExpressionLowerer<'_> {
        pub(in crate::ssa::lower_frontend::map_iteration) fn validate_map_parent(
            &self,
            nesting: Nesting,
            span: Span,
        ) -> Result<(), LoweringError> {
            let mut previous = None;
            for context in &self.loops {
                let Some(state) = context.map_loop.as_ref() else {
                    continue;
                };
                if state.nesting.parent != previous
                    || state.cleaned
                    || state.frame.finished
                    || state.body != self.block
                {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                }
                conditional::transport::roles(self, state, span)?;
                previous = Some(state.frame.statement);
            }
            if previous != nesting.parent {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            Ok(())
        }

        pub(in crate::ssa::lower_frontend::map_iteration) fn active_map_temporary(
            &self,
            key: usize,
            value: ValueId,
        ) -> bool {
            self.loops
                .iter()
                .filter_map(|context| context.map_loop.as_ref())
                .any(|state| {
                    !state.cleaned
                        && !state.frame.finished
                        && state.owner.is_some_and(|(_, source, owner)| {
                            matches!(source, SourceIdentity::Temporary(expression) if expression.index() == key)
                                && owner == value
                        })
                })
        }
    }

    fn edges(function: &Function, from: BlockId) -> Vec<&Edge> {
        match carrier::terminator(function, from) {
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
        }
    }
    pub(in crate::ssa::lower_frontend::map_iteration) fn incoming(
        function: &Function,
        target: BlockId,
    ) -> Vec<(BlockId, &Edge)> {
        function
            .blocks
            .iter()
            .flat_map(|block| {
                edges(function, block.id)
                    .into_iter()
                    .filter(move |edge| edge.target == target)
                    .map(move |edge| (block.id, edge))
            })
            .collect()
    }

    pub(in crate::ssa::lower_frontend::map_iteration) fn transport(
        function: &Function,
        current: EntityId,
        anchor: EntityId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let missing = || error(LoweringErrorKind::MissingFact, span);
        let ty = function.entity(anchor).ok_or_else(missing)?.ty;
        let mut pending = vec![current];
        let mut dependencies = BTreeMap::<EntityId, Vec<EntityId>>::new();
        while let Some(entity) = pending.pop() {
            if entity == anchor || dependencies.contains_key(&entity) {
                continue;
            }
            let data = function.entity(entity).ok_or_else(missing)?;
            if data.ty != ty {
                return Err(missing());
            }
            let Definition::BlockParameter { block, index } = data.definition else {
                return Err(missing());
            };
            let predecessors = incoming(function, block);
            if predecessors.is_empty() {
                return Err(missing());
            }
            let mut sources = Vec::new();
            for (_, edge) in predecessors {
                let parameters = &function.block(block).ok_or_else(missing)?.parameters;
                if edge.arguments.len() != parameters.len()
                    || parameters.get(index) != Some(&entity)
                {
                    return Err(missing());
                }
                let source = *edge.arguments.get(index).ok_or_else(missing)?;
                if function.entity(source).map(|data| data.ty) != Some(ty) {
                    return Err(missing());
                }
                sources.push(source);
                pending.push(source);
            }
            dependencies.insert(entity, sources);
        }
        let mut anchored = std::collections::BTreeSet::from([anchor]);
        loop {
            let previous = anchored.len();
            for (&entity, sources) in &dependencies {
                if sources.iter().any(|source| anchored.contains(source)) {
                    anchored.insert(entity);
                }
            }
            if previous == anchored.len() {
                break;
            }
        }
        if dependencies.keys().any(|entity| !anchored.contains(entity)) {
            return Err(missing());
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
pub(super) struct Roles {
    pub source: LoanId,
    pub cursor: ValueId,
    pub key: LoanId,
    pub value: LoanId,
}

fn same_path(left: Snapshot, right: Snapshot) -> bool {
    left.statement == right.statement
        && left.source == right.source
        && left.components == right.components
        && left.finished == right.finished
        && left.owner == right.owner
        && left.body == right.body
        && left.cursor == right.cursor
        && left.cursor_type == right.cursor_type
        && left.kind == right.kind
        && left.cleaned == right.cleaned
        && left.next == right.next
        && left.returned == right.returned
        && left.cleanup_start == right.cleanup_start
}

impl ExpressionLowerer<'_> {
    pub(in super::super) fn carry_checked_map(
        &self,
        carried: &LinearBindings,
        span: Span,
    ) -> Result<(), LoweringError> {
        let missing = || error(LoweringErrorKind::MissingFact, span);
        let mut active = 0;
        for context in &self.loops {
            let Some(state) = &context.map_loop else {
                continue;
            };
            let actual = state.path_snapshot().ok_or_else(missing)?;
            let before = *carried
                .map_paths
                .get(&actual.statement.index())
                .ok_or_else(missing)?;
            if !same_path(actual, before)
                || before.body != self.block
                || before.statement != state.plan.descriptor().statement()
                || before.cleaned
                || before.finished
                || before.next.is_some()
                || before.returned.is_some()
                || before.cleanup_start.is_some()
            {
                return Err(missing());
            }
            let [(key_symbol, Some(key)), (value_symbol, Some(value))] = before.components else {
                return Err(missing());
            };
            if self.borrow_bindings.get(&key_symbol) != Some(&key)
                || self.borrow_bindings.get(&value_symbol) != Some(&value)
                || state
                    .owner
                    .is_some_and(|(_, identity, owner)| identity.current_owner(self) != Some(owner))
            {
                return Err(missing());
            }
            let entities = [
                Some(EntityId::Loan(before.source.ok_or_else(missing)?)),
                Some(EntityId::Value(before.cursor)),
                Some(EntityId::Loan(key)),
                Some(EntityId::Loan(value)),
                before.owner.map(|(_, owner)| EntityId::Value(owner)),
            ];
            for entity in entities.into_iter().flatten() {
                let slot = unique(carried.slots.iter().filter(|slot| slot.source == entity))
                    .ok_or_else(missing)?;
                if self.function.entity(entity).map(|data| data.ty) != Some(slot.ty) {
                    return Err(missing());
                }
            }
            active += 1;
        }
        if carried.map_paths.len() != active {
            return Err(missing());
        }
        Ok(())
    }

    pub(in super::super) fn resume_checked_map(
        &mut self,
        from: BlockId,
        carried: &LinearBindings,
        span: Span,
    ) -> Result<(), LoweringError> {
        if carried.map_paths.is_empty()
            && self.loops.iter().all(|context| context.map_loop.is_none())
        {
            return Ok(());
        }
        let missing = || error(LoweringErrorKind::MissingFact, span);
        let success = self.function.block(self.block).ok_or_else(missing)?;
        let parameters = &success.parameters;
        let arguments = carried
            .slots
            .iter()
            .map(|slot| slot.source)
            .collect::<Vec<_>>();
        let Some(TerminatorKind::Conditional {
            when_true,
            when_false,
            ..
        }) = carrier::terminator(self.function, from)
        else {
            return Err(missing());
        };
        if when_false.target != self.block
            || when_false.arguments != arguments
            || when_true.arguments != arguments
            || when_true.target == self.block
            || !matches!(
                carrier::terminator(self.function, when_true.target),
                Some(TerminatorKind::Abort)
            )
            || parameters.len() != carried.slots.len()
            || success.origin != Origin::Source(span)
            || !self
                .function
                .block(from)
                .and_then(|block| block.terminator.as_ref())
                .is_some_and(|terminator| terminator.origin == Origin::Source(span))
        {
            return Err(missing());
        }
        let mut incoming = 0;
        for block in &self.function.blocks {
            let Some(terminator) = &block.terminator else {
                continue;
            };
            for _ in terminator
                .kind
                .targets()
                .into_iter()
                .filter(|target| *target == self.block)
            {
                if block.id != from {
                    return Err(missing());
                }
                incoming += 1;
            }
        }
        if incoming != 1 {
            return Err(missing());
        }
        for (index, (slot, &parameter)) in carried.slots.iter().zip(parameters).enumerate() {
            let data = self.function.entity(parameter).ok_or_else(missing)?;
            if self.function.entity(slot.source).map(|source| source.ty) != Some(slot.ty)
                || data.ty != slot.ty
                || data.definition
                    != (Definition::BlockParameter {
                        block: self.block,
                        index,
                    })
            {
                return Err(missing());
            }
        }
        let mapped = |entity| {
            let index = unique(
                carried
                    .slots
                    .iter()
                    .enumerate()
                    .filter_map(|(index, slot)| (slot.source == entity).then_some(index)),
            )
            .ok_or_else(missing)?;
            parameters.get(index).copied().ok_or_else(missing)
        };
        let mapped_loan = |loan| match mapped(EntityId::Loan(loan))? {
            EntityId::Loan(current) => Ok(current),
            _ => Err(missing()),
        };
        let mapped_value = |value| match mapped(EntityId::Value(value))? {
            EntityId::Value(current) => Ok(current),
            _ => Err(missing()),
        };
        let mut resumed = Vec::new();
        for (index, context) in self.loops.iter().enumerate() {
            let Some(state) = &context.map_loop else {
                continue;
            };
            let actual = state.path_snapshot().ok_or_else(missing)?;
            let before = *carried
                .map_paths
                .get(&actual.statement.index())
                .ok_or_else(missing)?;
            if before.body != from || before.statement != state.plan.descriptor().statement() {
                return Err(missing());
            }
            let mut expected = before;
            expected.body = self.block;
            expected.source = Some(mapped_loan(before.source.ok_or_else(missing)?)?);
            expected.cursor = mapped_value(before.cursor)?;
            for (_, loan) in &mut expected.components {
                *loan = Some(mapped_loan(loan.ok_or_else(missing)?)?);
            }
            expected.owner = before
                .owner
                .map(|(root, owner)| mapped_value(owner).map(|current| (root, current)))
                .transpose()?;
            if !same_path(actual, expected)
                || state
                    .owner
                    .is_some_and(|(_, identity, owner)| identity.current_owner(self) != Some(owner))
            {
                return Err(missing());
            }
            let [(key_symbol, Some(key)), (value_symbol, Some(value))] = actual.components else {
                return Err(missing());
            };
            if self.borrow_bindings.get(&key_symbol) != Some(&key)
                || self.borrow_bindings.get(&value_symbol) != Some(&value)
            {
                return Err(missing());
            }
            resumed.push((
                index,
                Roles {
                    source: actual.source.ok_or_else(missing)?,
                    cursor: actual.cursor,
                    key,
                    value,
                },
            ));
        }
        if resumed.len() != carried.map_paths.len() {
            return Err(missing());
        }
        for (index, roles) in resumed {
            self.loops[index]
                .map_loop
                .as_mut()
                .ok_or_else(missing)?
                .resumed = Some(roles);
        }
        Ok(())
    }
}
