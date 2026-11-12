//! Unit 原始退出表；旧 owner/condition/phi 行为由同轮 SSA 全列和完整库存证明。
#[allow(unused_imports)] pub(in crate::ssa) use crate::ssa::map_provider_exits_pipeline_tests::local_exits::boundaries::inlined_unit_cleanup_1 as unit_cleanup;
use super::{Failure, failure, need, one};
pub(super) use unit_cleanup::{caller as unit_caller, check as unit};

pub(in crate::ssa) mod map_iteration_conditional {
    //! Ordinary body CFG uses a plan-bound source identity and append-only exit evidence.
    use verify::actions;
    pub(in crate::ssa) use crate::ssa::borrow_result_boundary_tests::conditional_facts as facts;
    pub(in crate::ssa) use retained as successor;
    pub(in crate::ssa) use crate::ssa::lower_frontend_tests::root_primitive_tests::conditional_verify as verify;

    use crate::ssa::lower_frontend::map_iteration::*;
    use crate::ssa::lower_frontend::*;
    use lang_frontend::{
        ownership_checking::{DropFact, DropTarget},
        type_checking::MapIterationDescriptor,
    };

    pub(in crate::ssa) use self::{local_validate as local, transport_roles as transport};
    pub(in crate::ssa) use actions::validate_drop_ledger;
    pub(in crate::ssa) use facts::{qualify, source_drop};
    pub(in crate::ssa) use verify::validate;

    #[derive(Clone, Copy)]
    pub(in crate::ssa) struct Shape {
        pub(in crate::ssa) statement: StatementId,
        pub(in crate::ssa) body: StatementId,
        pub(in crate::ssa) source: SourceIdentity,
    }

    pub(in crate::ssa) struct State {
        pub(in crate::ssa) shape: Shape,
        pub(in crate::ssa) exits: Vec<Exit>,
    }

    impl State {
        pub(in crate::ssa) fn new(shape: Shape) -> Self {
            Self {
                shape,
                exits: Vec::new(),
            }
        }
    }

    /// These are the actual live entities immediately before this path's cleanup.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(in crate::ssa) struct Roles {
        pub(in crate::ssa) source: LoanId,
        pub(in crate::ssa) cursor: ValueId,
        pub(in crate::ssa) key: LoanId,
        pub(in crate::ssa) value: LoanId,
        pub(in crate::ssa) owner: Option<ValueId>,
    }

    #[derive(Clone, Copy)]
    pub(in crate::ssa) struct CleanupFrame {
        pub(in crate::ssa) statement: StatementId,
        pub(in crate::ssa) roles: Roles,
        pub(in crate::ssa) owner: Option<(source_owner::Proof, SourceIdentity, ValueId)>,
    }

    pub(in crate::ssa) struct Exit {
        pub(in crate::ssa) kind: IterationExitKind,
        pub(in crate::ssa) block: BlockId,
        pub(in crate::ssa) roles: Roles,
        pub(in crate::ssa) frames: Vec<CleanupFrame>,
        pub(in crate::ssa) cleanup_start: usize,
        pub(in crate::ssa) cleanup_end: usize,
        pub(in crate::ssa) next: Option<ValueId>,
        pub(in crate::ssa) returned: Option<ValueId>,
    }

    /// Conditional local drops are admitted only when the complete source-qualified
    /// plan has proved their original expression owner and ordering.
    pub(in crate::ssa) fn supported_local_drop(
        parsed: &ParsedFile,
        names: &NameResolution,
        typed: &TypedFile,
        owned: &OwnershipCheckedFile,
        fact: DropFact,
    ) -> bool {
        typed.map_iterations().iter().any(|descriptor| {
            super::qualify::qualify(parsed, names, typed, owned, *descriptor)
                .ok()
                .is_some_and(|qualified| {
                    qualified.conditional.is_some()
                        && qualified
                            .plan
                            .exits()
                            .iter()
                            .any(|exit| exit.actions().contains(&IterationCleanupAction::Drop(fact)))
                })
        })
    }

    pub(in crate::ssa) fn shape(
        parsed: &ParsedFile,
        _names: &NameResolution,
        _typed: &TypedFile,
        descriptor: MapIterationDescriptor,
        body: StatementId,
        source: SourceIdentity,
        span: Span,
    ) -> Result<Option<Shape>, LoweringError> {
        let missing = || error(LoweringErrorKind::MissingFact, span);
        let node = parsed
            .ast()
            .statements()
            .get(descriptor.statement())
            .map_err(|_| missing())?;
        if !matches!(node.payload(), Statement::For { source, body: actual, .. }
            if *source == descriptor.source() && *actual == body)
        {
            return Err(missing());
        }
        // Every acquired provider uses the same full plan/body proof. A normal body
        // does not require a different lifetime or transport representation.
        Ok(Some(Shape {
            statement: descriptor.statement(),
            body,
            source,
        }))
    }

    pub(in crate::ssa) mod transport_roles {
        use crate::ssa::lower_frontend::map_iteration::*;
    use crate::ssa::lower_frontend::*;

        pub(in super::super) fn roles(
            lowerer: &ExpressionLowerer<'_>,
            state: &Context,
            span: Span,
        ) -> Result<Roles, LoweringError> {
            let missing = || error(LoweringErrorKind::MissingFact, span);
            if state.conditional.is_none()
                || state.cleaned
                || state.frame.finished
                || state.body != lowerer.block
            {
                return Err(missing());
            }
            let [(key_symbol, Some(key)), (value_symbol, Some(value))] = state.frame.components else {
                return Err(missing());
            };
            if lowerer.borrow_bindings.get(&key_symbol) != Some(&key)
                || lowerer.borrow_bindings.get(&value_symbol) != Some(&value)
                || state
                    .owner
                    .is_some_and(|(_, source, owner)| source.current_owner(lowerer) != Some(owner))
            {
                return Err(missing());
            }
            Ok(Roles {
                source: state.frame.source.ok_or_else(missing)?,
                cursor: state.cursor,
                key,
                value,
                owner: state.owner.map(|(_, _, owner)| owner),
            })
        }
    }

    pub(in crate::ssa) mod local_validate {
        use crate::ssa::lower_frontend::map_iteration::*;
    use crate::ssa::lower_frontend::*;

        pub(in super::super) fn validate(
            frontend: (
                &ParsedFile,
                &NameResolution,
                &TypedFile,
                &OwnershipCheckedFile,
            ),
            function: &Function,
            fact: DropFact,
            owner: ValueId,
            span: Span,
        ) -> Result<(), LoweringError> {
            let (parsed, names, typed, owned) = frontend;
            let missing = || error(LoweringErrorKind::MissingFact, span);
            let DropTarget::Named(root) = fact.target() else {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            };
            let symbol = names.symbols().get(root.index()).ok_or_else(missing)?;
            if fact.value_origin() != symbol.span()
                || !owned.drops().contains(&fact)
                || fact.capture_slot().is_some()
                || fact.instance_address().is_some()
            {
                return Err(missing());
            }
            actions::binding_condition(
                parsed,
                names,
                typed,
                owned,
                root,
                fact.owner().ok_or_else(missing)?,
                span,
            )?;
            let (expression, origin) = actions::binding_birth(parsed, names, root, span)?;
            if expression.is_none() {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
            let ty = function
                .entity(EntityId::Value(owner))
                .ok_or_else(missing)?
                .ty;
            let anchor = unique(function.instructions.iter().filter_map(|instruction| {
                let [EntityId::Value(value)] = instruction.results.as_slice() else {
                    return None;
                };
                (instruction.origin == Origin::Source(origin)
                    && function.entity(EntityId::Value(*value)).map(|data| data.ty) == Some(ty))
                .then_some(*value)
            }))
            .ok_or_else(missing)?;
            nested::transport(
                function,
                EntityId::Value(owner),
                EntityId::Value(anchor),
                span,
            )
        }
    }

    pub(in crate::ssa) mod retained {
        //! A nested source may retain the exact owner whose lifetime belongs to an acquired ancestor.
        use crate::ssa::lower_frontend::map_iteration::*;
    use crate::ssa::lower_frontend::*;
        use crate::ssa::map_source::Formation;
        use lang_frontend::ownership_checking::{
            CleanupOwnerValue, DropFact, DropTarget, IterationPhiBoundary,
        };

        pub(in super::super) fn ancestor_fact(
            frontend: (
                &ParsedFile,
                &NameResolution,
                &TypedFile,
                &OwnershipCheckedFile,
            ),
            flow: source_owner::Flow,
            fact: DropFact,
            span: Span,
        ) -> Result<bool, LoweringError> {
            let (parsed, names, typed, owned) = frontend;
            let missing = || error(LoweringErrorKind::MissingFact, span);
            let current = owned.map_iteration(flow.statement).ok_or_else(missing)?;
            let child = parsed
                .ast()
                .statements()
                .get(flow.statement)
                .map_err(|_| missing())?
                .span();
            let DropTarget::Named(root) = facts::source_target(flow.source) else {
                return Ok(false);
            };
            if fact.target() != DropTarget::Named(root) {
                return Err(missing());
            }
            for ancestor in owned.map_iterations() {
                let statement = ancestor.descriptor().statement();
                if statement == flow.statement
                    || ancestor.source_access() != MapIterationSourceAccess::Owned
                    || ancestor.source() != current.source()
                {
                    continue;
                }
                let node = parsed
                    .ast()
                    .statements()
                    .get(statement)
                    .map_err(|_| missing())?;
                let Statement::For { body, .. } = node.payload() else {
                    return Err(missing());
                };
                let body = parsed
                    .ast()
                    .statements()
                    .get(*body)
                    .map_err(|_| missing())?
                    .span();
                if body.source_id() != child.source_id()
                    || body.start() > child.start()
                    || child.end() > body.end()
                {
                    continue;
                }
                if ancestor.descriptor().source_type() != current.descriptor().source_type() {
                    return Err(missing());
                }
                // A terminal Drop belongs to the ancestor's actual EndSource segment,
                // even when this same point also retires the nested provider.
                let mut exits = ancestor
                    .exits()
                    .iter()
                    .filter(|exit| exit.point() == fact.point());
                let exit = exits.next();
                if exits.next().is_some() {
                    return Err(missing());
                }
                if let Some(exit) = exit
                    && actions::source_drop(owned, ancestor, flow.source, exit, span)? == Some(fact)
                {
                    return Ok(true);
                }
                if typed.is_resource_type(ancestor.descriptor().source_type()) != Some(true) {
                    continue;
                }
                let Some(CleanupOwnerValue::IterationPhi {
                    statement: owner_statement,
                    boundary: IterationPhiBoundary::Exit,
                    symbol,
                    origin,
                }) = fact
                    .owner()
                    .and_then(|owner| owned.cleanup_conditions().owner_value(owner))
                else {
                    continue;
                };
                if *owner_statement != statement {
                    continue;
                }
                if *symbol != root || *origin != node.span() {
                    return Err(missing());
                }
                // Lexical Resource cleanup follows this ancestor's live Exit owner,
                // not the nested provider's different Exit phi or a same-typed owner.
                let phi = source_owner::phi(
                    owned.cleanup_conditions(),
                    ancestor,
                    root,
                    IterationPhiBoundary::Exit,
                    node.span(),
                )?;
                let origin = names
                    .symbols()
                    .get(root.index())
                    .ok_or_else(missing)?
                    .span();
                let scope = resource::scope(parsed, statement, origin, node.span())?;
                let live = source_owner::flow_condition(
                    parsed,
                    names,
                    owned,
                    source_owner::Flow {
                        statement,
                        source: flow.source,
                        span: node.span(),
                        late: None,
                    },
                    phi.owner(),
                    span,
                )?;
                if fact.owner() != Some(phi.owner())
                    || fact.condition() != Some(live)
                    || !resource::normal_point(parsed, scope, node.span(), fact.point())
                {
                    return Err(missing());
                }
                let cleanup = owned
                    .cleanup_steps()
                    .iter()
                    .filter_map(|(point, action)| (*point == fact.point()).then_some(*action))
                    .collect::<Vec<_>>();
                actions::validate_drop_ledger(owned, fact.point(), &cleanup, span)?;
                if cleanup
                    .iter()
                    .filter(|action| **action == IterationCleanupAction::Drop(fact))
                    .count()
                    != 1
                {
                    return Err(missing());
                }
                return Ok(true);
            }
            Ok(false)
        }

        /// Both full birth graphs are validated by the caller. Additionally require every
        /// later incoming path to pass the current Exit, rather than only share a birth.
        pub(in super::super) fn terminal(
            frontend: (
                &ParsedFile,
                &NameResolution,
                &TypedFile,
                &OwnershipCheckedFile,
            ),
            flow: source_owner::Flow,
            fact: DropFact,
            formation: Option<&Formation<SymbolId>>,
            span: Span,
        ) -> Result<bool, LoweringError> {
            let (parsed, names, typed, owned) = frontend;
            let missing = || error(LoweringErrorKind::MissingFact, span);
            let DropTarget::Named(root) = facts::source_target(flow.source) else {
                return Ok(false);
            };
            let conditions = owned.cleanup_conditions();
            let owner = fact.owner().ok_or_else(missing)?;
            let Some(CleanupOwnerValue::IterationPhi {
                statement,
                boundary: IterationPhiBoundary::Exit,
                symbol,
                origin,
            }) = conditions.owner_value(owner)
            else {
                return Ok(false);
            };
            if *statement == flow.statement || *symbol != root {
                return Ok(false);
            }
            let current = owned.map_iteration(flow.statement).ok_or_else(missing)?;
            let later = owned.map_iteration(*statement).ok_or_else(missing)?;
            let node = parsed
                .ast()
                .statements()
                .get(*statement)
                .map_err(|_| missing())?;
            if !matches!(node.payload(), Statement::For { .. })
                || node.span() != *origin
                || origin.source_id() != span.source_id()
                || origin.start() < span.end()
                || later.source_access() != MapIterationSourceAccess::Owned
                || later.source() != current.source()
                || !matches!(later.source(), Some(LoanTarget::Place(place)) if place.is_root() && place.root() == root)
                || later.descriptor().source_type() != current.descriptor().source_type()
                || typed.map_iteration(*statement) != Some(later.descriptor())
            {
                return Err(missing());
            }
            let declaration = names
                .symbols()
                .get(root.index())
                .ok_or_else(missing)?
                .span();
            let scope = resource::scope(parsed, flow.statement, declaration, span)?;
            if resource::scope(parsed, *statement, declaration, *origin)? != scope
                || !resource::normal_point(parsed, scope, *origin, fact.point())
            {
                return Err(missing());
            }
            if let Some(before) = formation {
                let after = formed::formation(
                    parsed,
                    names,
                    typed,
                    owned,
                    root,
                    later.descriptor().source_type(),
                    *origin,
                )?;
                if before.birth != after.birth
                    || before.binding != after.binding
                    || before.origin != after.origin
                    || before.declaration != after.declaration
                    || before.roots != after.roots
                    || before.updates != after.updates
                    || !before
                        .history
                        .iter()
                        .map(|event| (event.rhs, event.origin))
                        .eq(after.history.iter().map(|event| (event.rhs, event.origin)))
                {
                    return Err(missing());
                }
            }
            let exit = source_owner::phi(conditions, current, root, IterationPhiBoundary::Exit, span)?;
            let terminal =
                source_owner::phi(conditions, later, root, IterationPhiBoundary::Exit, *origin)?;
            let live = source_owner::provenance::from_exit(owned, root, owner, exit.owner(), span)?;
            if terminal.owner() != owner
                || fact.condition() != Some(live)
                || conditions.get(live) == Some(&CleanupCondition::Never)
            {
                return Err(missing());
            }
            let ledger = owned
                .cleanup_steps()
                .iter()
                .filter_map(|(point, action)| (*point == fact.point()).then_some(*action))
                .collect::<Vec<_>>();
            actions::validate_drop_ledger(owned, fact.point(), &ledger, span)?;
            if ledger
                .iter()
                .filter(|action| **action == IterationCleanupAction::Drop(fact))
                .count()
                != 1
            {
                return Err(missing());
            }
            Ok(true)
        }
    }

    pub(in crate::ssa) mod checked {
        //! Checked success Map transport and lexical Map ancestry / CFG column transport.
        use super::super::*;
        use control::map_frames::Snapshot;

        pub(in crate::ssa) mod nested {
            use super::super::super::*;

            #[derive(Clone, Copy, Debug, PartialEq, Eq)]
            pub(in crate::ssa) struct Nesting {
                pub(in crate::ssa) parent: Option<StatementId>,
            }

            pub(in crate::ssa) fn nesting(
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
                pub(in crate::ssa) fn validate_map_parent(
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

                pub(in crate::ssa) fn active_map_temporary(&self, key: usize, value: ValueId) -> bool {
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
            pub(in crate::ssa) fn incoming(
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

            pub(in crate::ssa) fn transport(
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
        pub(in crate::ssa) struct Roles {
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
            pub(in crate::ssa::lower_frontend) fn carry_checked_map(
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
                    let [(key_symbol, Some(key)), (value_symbol, Some(value))] = before.components
                    else {
                        return Err(missing());
                    };
                    if self.borrow_bindings.get(&key_symbol) != Some(&key)
                        || self.borrow_bindings.get(&value_symbol) != Some(&value)
                        || state.owner.is_some_and(|(_, identity, owner)| {
                            identity.current_owner(self) != Some(owner)
                        })
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

            pub(in crate::ssa::lower_frontend) fn resume_checked_map(
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
                        || state.owner.is_some_and(|(_, identity, owner)| {
                            identity.current_owner(self) != Some(owner)
                        })
                    {
                        return Err(missing());
                    }
                    let [(key_symbol, Some(key)), (value_symbol, Some(value))] = actual.components
                    else {
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
    }
}
