//! SPEC-0244：单文件 owned root 原语的 SSA 与 LLVM 闭环。
use super::*;
use crate::test_support::diagnostics::assert_diagnostic;

fn lower_primitive(text: &str) -> crate::ssa::model::Program {
    let analysis = crate::native_tests::boxed_enum_tests::unit_source::UnitSource::analyze(
        "lowering.ko",
        text,
    )
    .unwrap_or_else(|error| panic!("{text}\n{error:?}"));
    analysis.assert_frontend_clean();
    let entry = analysis.declaration("test", lang_frontend::name_resolution::SymbolKind::Function);
    analysis
        .lower_entry(entry)
        .unwrap_or_else(|error| panic!("{text}\n{error:?}"))
        .0
}

#[test]
fn root_primitive_single_replace_swap_string() {
    let program = lower_primitive(
        r#"
        fun test(): String {
            var a = "old" + "a"
            var b = "old" + "b"
            val old = replace(&a, "new" + "a")
            println(old)
            swap(&a, &b)
            println(a)
            return b
        }
    "#,
    );
    let ssa = render_program(&program);
    assert_diagnostic!(ssa.contains("root_replace"), "{ssa}");
    assert_diagnostic!(ssa.contains("root_swap"), "{ssa}");
    render_verified_program(&program).expect("root owners verify through LLVM");
}

#[test]
fn root_primitive_single_existing_storage_families() {
    for (declarations, first, second) in [
        ("", "1", "2"),
        ("class Node(val id: Int)", "Node(1)", "Node(2)"),
        (
            "value class Value(val value: Int)",
            "Box(Value(1))",
            "Box(Value(2))",
        ),
        ("", "Rc(1)", "Rc(2)"),
        ("", "listOf(1)", "listOf(2)"),
        (
            "value class Pair(val text: String)",
            "Pair(\"one\")",
            "Pair(\"two\")",
        ),
        ("value class Pair(val number: Int)", "Pair(1)", "Pair(2)"),
    ] {
        let program = lower_primitive(&format!(
            "{declarations}\nfun test(): Unit {{ var a = {first}\nvar b = {second}\nval old = replace(&a, {second})\nswap(&a, &b) }}"
        ));
        render_verified_program(&program).unwrap_or_else(|error| panic!("{first}: {error:?}"));
    }
}

#[test]
fn root_primitive_single_later_value_cfg_and_control_exit() {
    for value in [
        "if (flag) \"new\" else \"other\"",
        "if (flag) return else \"new\"",
        "error(\"stop\")",
    ] {
        let program = lower_primitive(&format!(
            "fun test(flag: Boolean): Unit {{ var a = \"old\"\nval old = replace(&a, {value}) }}"
        ));
        render_verified_program(&program).unwrap_or_else(|error| panic!("{value}: {error:?}"));
    }
}

#[test]
fn root_primitive_single_grouped_named_nested_and_nullable_owners() {
    for text in [
        "fun test(): Unit { var a = \"a\"\nvar b = \"b\"\nval new = \"new\"\nval old = replace(&(a), new)\nval older = replace(&a, replace(&b, \"last\"))\nprintln(old)\nprintln(older)\nprintln(a)\nprintln(b) }",
        "class Node(val id: Int)\nfun test(): Unit { var a: Node? = Node(1)\nvar b: Node? = Node(2)\nval old = replace(&a, Node(3))\nswap(&a, &b) }",
        "enum class State { One(value: String), Two(value: String) }\nfun test(): Unit { var a: State = State.One(\"a\")\nvar b: State = State.Two(\"b\")\nval old = replace(&a, State.Two(\"new\"))\nswap(&a, &b) }",
    ] {
        let program = lower_primitive(text);
        render_verified_program(&program).unwrap_or_else(|error| panic!("{text}: {error:?}"));
    }
}

#[test]
fn root_primitive_single_unit_storage() {
    let program = lower_primitive(
        "fun unit(): Unit {}\nfun test(): Unit { var a = unit()\nvar b = unit()\nval old = replace(&a, unit())\nswap(&a, &b)\nold\nreturn }",
    );
    render_verified_program(&program).expect("zero-sized root storage verifies");
}

#[test]
fn root_primitive_single_unit_control_exit_keeps_source_binding_shape() {
    let program = lower_primitive(
        "fun unit(): Unit {}\nfun test(flag: Boolean): Unit { var a = unit()\nloop { val old = replace(&a, if (flag) break else unit())\nbreak }\nvar b = unit()\nswap(&a, &b) }",
    );
    render_verified_program(&program).expect("Unit root exits preserve source binding shape");
}

pub(in crate::ssa) mod conditional_verify {
    //! Validate every actual body exit, cleanup suffix and current role provenance.
    use crate::ssa::lower_frontend::map_iteration::carrier::{Evidence, instructions, terminator};
    use crate::ssa::lower_frontend::map_iteration::conditional::*;
    use crate::ssa::lower_frontend::map_iteration::conditional::{State, local};
    use crate::ssa::lower_frontend::map_iteration::*;

    use crate::ssa::model::Definition;
    use std::collections::BTreeSet;

    pub(in crate::ssa) fn validate(e: Evidence<'_>, state: &State) -> Result<(), LoweringError> {
        let missing = || error(LoweringErrorKind::MissingFact, e.span);
        let [preheader, header, body, exhausted] = e.blocks;
        let plan = e
            .owned
            .map_iteration(state.shape.statement)
            .ok_or_else(missing)?;
        let body_parameters = &e.function.block(body).ok_or_else(missing)?.parameters;
        let (source, cursor) = crate::ssa::lower_frontend::map_iteration::roles(
            body_parameters,
            e.source_slot,
            e.cursor_slot,
            e.span,
        )?;
        let expected = plan
            .exits()
            .iter()
            .filter(|exit| exit.kind() != IterationExitKind::Exhaustion)
            .collect::<Vec<_>>();
        if state.exits.len() != expected.len() {
            return Err(missing());
        }
        for exit in &state.exits {
            if state
                .exits
                .iter()
                .filter(|other| other.kind == exit.kind)
                .count()
                != 1
                || !expected.iter().any(|expected| expected.kind() == exit.kind)
            {
                return Err(missing());
            }
            for (actual, anchor) in [
                (EntityId::Loan(exit.roles.source), EntityId::Loan(source)),
                (EntityId::Value(exit.roles.cursor), EntityId::Value(cursor)),
                (EntityId::Loan(exit.roles.key), EntityId::Loan(e.key)),
                (EntityId::Loan(exit.roles.value), EntityId::Loan(e.value)),
            ] {
                nested::transport(e.function, actual, anchor, e.span)?;
            }
            match (exit.roles.owner, e.body_owner) {
                (Some(actual), Some((_, identity, anchor))) if identity == state.shape.source => {
                    nested::transport(
                        e.function,
                        EntityId::Value(actual),
                        EntityId::Value(anchor),
                        e.span,
                    )?
                }
                (None, None) => {}
                _ => return Err(missing()),
            }
            check_exit(e, state, exit)?;
        }
        // The actual graph must have no unrecorded return/backedge/provider advance.
        // Abort is deliberately terminal without cleanup, as in the ordinary emitter.
        let mut pending = vec![body];
        let mut visited = BTreeSet::new();
        while let Some(block) = pending.pop() {
            if !visited.insert(block) {
                continue;
            }
            if state.exits.iter().any(|exit| exit.block == block) {
                continue;
            }
            if [header, exhausted, e.continuation].contains(&block) {
                return Err(missing());
            }
            for instruction in instructions(e.function, block).ok_or_else(missing)? {
                if matches!(
                    instruction.operation,
                    Operation::MapIterationNext { .. }
                        | Operation::MapIterationFirst { .. }
                        | Operation::MapIterationHasNext { .. }
                ) {
                    validate_child(e, instruction)?;
                }
            }
            match terminator(e.function, block).ok_or_else(missing)? {
                TerminatorKind::Branch(edge) => pending.push(edge.target),
                TerminatorKind::Conditional {
                    when_true,
                    when_false,
                    ..
                } => {
                    pending.extend([when_true.target, when_false.target]);
                }
                TerminatorKind::NullableBranch {
                    when_null,
                    when_non_null,
                    ..
                } => {
                    pending.extend([when_null.target, when_non_null.target]);
                }
                TerminatorKind::Abort => {}
                _ => return Err(missing()),
            }
        }
        if state
            .exits
            .iter()
            .any(|exit| !visited.contains(&exit.block))
        {
            return Err(missing());
        }
        for block in &e.function.blocks {
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
            for _edge in edges.into_iter().filter(|edge| edge.target == header) {
                if block.id != preheader
                    && !state.exits.iter().any(|exit| {
                        exit.block == block.id
                            && matches!(
                                exit.kind,
                                IterationExitKind::Continue(_) | IterationExitKind::Fallthrough
                            )
                    })
                {
                    return Err(missing());
                }
            }
        }
        let exhaustion = unique(
            plan.exits()
                .iter()
                .filter(|exit| exit.kind() == IterationExitKind::Exhaustion),
        )
        .ok_or_else(missing)?;
        let parameters = &e.function.block(exhausted).ok_or_else(missing)?.parameters;
        let (source, _) = crate::ssa::lower_frontend::map_iteration::roles(
            parameters,
            e.source_slot,
            e.cursor_slot,
            e.span,
        )?;
        let body = instructions(e.function, exhausted).ok_or_else(missing)?;
        let mut expected = vec![Operation::BorrowEnd { loan: source }];
        if let Some((proof, identity, owner)) = e.owner {
            if identity != state.shape.source {
                return Err(missing());
            }
            nested::transport(e.function, EntityId::Value(owner), e.seed, e.span)?;
            if proof.at_checked(exhaustion.point(), e.owned)?.is_some() {
                expected.push(Operation::Drop { owner });
            }
        } else if !matches!(e.seed, EntityId::Loan(_)) {
            return Err(missing());
        }
        if body.len() != expected.len()
            || body.iter().zip(&expected).any(|(i, op)| &i.operation != op)
        {
            return Err(missing());
        }
        if e.continuation == exhausted {
            // With one live exit ordinary merge reuses the exhaustion block. Its
            // continuation is still open here; the caller emits subsequent code.
            if state
                .exits
                .iter()
                .any(|exit| matches!(exit.kind, IterationExitKind::Break(_)))
                || terminator(e.function, exhausted).is_some()
            {
                return Err(missing());
            }
        } else if !matches!(terminator(e.function, exhausted), Some(TerminatorKind::Branch(edge))
            if edge.target == e.continuation)
        {
            return Err(missing());
        }
        Ok(())
    }

    fn check_exit(e: Evidence<'_>, state: &State, exit: &Exit) -> Result<(), LoweringError> {
        let missing = || error(LoweringErrorKind::MissingFact, e.span);
        let plan = e
            .owned
            .map_iteration(state.shape.statement)
            .ok_or_else(missing)?;
        let expected = unique(
            plan.exits()
                .iter()
                .filter(|actual| actual.kind() == exit.kind),
        )
        .ok_or_else(missing)?;
        let body = instructions(e.function, exit.block).ok_or_else(missing)?;
        if exit.cleanup_start > body.len() {
            return Err(missing());
        }
        let mut at = exit.cleanup_start;
        for action in expected.actions() {
            let frame = |statement| {
                unique(
                    exit.frames
                        .iter()
                        .filter(|frame| frame.statement == statement),
                )
                .ok_or_else(missing)
            };
            let operation = match action {
                IterationCleanupAction::FinishProvider(statement) => {
                    frame(*statement)?;
                    continue;
                }
                IterationCleanupAction::EndSource(statement) => Operation::BorrowEnd {
                    loan: frame(*statement)?.roles.source,
                },
                IterationCleanupAction::EndBinding { statement, symbol } => {
                    let roles = frame(*statement)?.roles;
                    let descriptor = e
                        .owned
                        .map_iteration(*statement)
                        .ok_or_else(missing)?
                        .descriptor();
                    let loan = if *symbol == descriptor.key_binding() {
                        roles.key
                    } else if *symbol == descriptor.value_binding() {
                        roles.value
                    } else {
                        return Err(missing());
                    };
                    Operation::BorrowEnd { loan }
                }
                IterationCleanupAction::Drop(fact) => {
                    let instruction = body.get(at).ok_or_else(missing)?;
                    let Operation::Drop { owner } = instruction.operation else {
                        return Err(missing());
                    };
                    if instruction.origin != Origin::Source(fact.value_origin()) {
                        return Err(missing());
                    }
                    let mut source_owner = None;
                    for frame in &exit.frames {
                        if let Some((proof, _, current)) = frame.owner
                            && proof.at_checked(expected.point(), e.owned)? == Some(*fact)
                            && source_owner.replace(current).is_some()
                        {
                            return Err(missing());
                        }
                    }
                    if let Some(current) = source_owner {
                        if current != owner {
                            return Err(missing());
                        }
                    } else {
                        local::validate(
                            (e.parsed, e.names, e.typed, e.owned),
                            e.function,
                            *fact,
                            owner,
                            e.span,
                        )?;
                    }
                    Operation::Drop { owner }
                }
                _ => return Err(missing()),
            };
            if body.get(at).map(|instruction| &instruction.operation) != Some(&operation) {
                return Err(missing());
            }
            at += 1;
        }
        if at != exit.cleanup_end {
            return Err(missing());
        }
        if exit_shape::terminal(exit.kind) {
            if exit.next.is_some() || at != body.len() {
                return Err(missing());
            }
            match (exit.kind, terminator(e.function, exit.block)) {
                (IterationExitKind::Break(_), Some(TerminatorKind::Branch(edge)))
                    if edge.target == e.continuation && exit.returned.is_none() => {}
                (IterationExitKind::Return(_), Some(TerminatorKind::Return { values })) => {
                    if values.as_slice() != exit.returned.as_slice()
                        || values.len() != e.function.return_types.len()
                        || values
                            .iter()
                            .zip(&e.function.return_types)
                            .any(|(value, ty)| {
                                e.function
                                    .entity(EntityId::Value(*value))
                                    .map(|data| data.ty)
                                    != Some(EntityType::Value(*ty))
                            })
                    {
                        return Err(missing());
                    }
                    if let Some(returned) = exit.returned {
                        let entity = EntityId::Value(returned);
                        if let Some(Definition::InstructionResult { instruction, index }) =
                            e.function.entity(entity).map(|data| data.definition)
                        {
                            let producer =
                                e.function.instruction(instruction).ok_or_else(missing)?;
                            if producer.results.get(index) != Some(&entity)
                                || (producer.block == exit.block
                                    && body[..exit.cleanup_start]
                                        .iter()
                                        .all(|instruction| instruction.id != producer.id))
                            {
                                return Err(missing());
                            }
                        }
                    }
                }
                _ => return Err(missing()),
            }
        } else {
            let next = exit.next.ok_or_else(missing)?;
            if exit.returned.is_some()
                || at + 1 != body.len()
                || body[at].operation
                    != (Operation::MapIterationNext {
                        source: exit.roles.source,
                        cursor: exit.roles.cursor,
                    })
                || body[at].results != [EntityId::Value(next)]
            {
                return Err(missing());
            }
            let Some(TerminatorKind::Branch(edge)) = terminator(e.function, exit.block) else {
                return Err(missing());
            };
            if edge.target != e.blocks[1]
                || edge.arguments.get(e.source_slot) != Some(&EntityId::Loan(exit.roles.source))
                || edge.arguments.get(e.cursor_slot) != Some(&EntityId::Value(next))
            {
                return Err(missing());
            }
            if let Some((_, identity, _)) = e.body_owner {
                let (column, _) = unique(
                    e.carried
                        .slots
                        .iter()
                        .enumerate()
                        .filter(|(_, slot)| identity.matches_slot(slot)),
                )
                .ok_or_else(missing)?;
                if edge.arguments.get(column).copied() != exit.roles.owner.map(EntityId::Value) {
                    return Err(missing());
                }
            }
        }
        if let EntityId::Loan(entry) = e.seed
            && body
                .iter()
                .any(|instruction| instruction.operation == Operation::BorrowEnd { loan: entry })
        {
            return Err(missing());
        }
        Ok(())
    }

    /// A nested provider is already checked by its own carrier. Still bind each
    /// encountered operation to that concrete child's source/cursor in this graph.
    fn validate_child(
        e: Evidence<'_>,
        instruction: &crate::ssa::model::Instruction,
    ) -> Result<(), LoweringError> {
        let missing = || error(LoweringErrorKind::MissingFact, e.span);
        let source = match instruction.operation {
            Operation::MapIterationFirst { source }
            | Operation::MapIterationHasNext { source, .. }
            | Operation::MapIterationNext { source, .. } => source,
            _ => return Err(missing()),
        };
        let candidates = e
            .children
            .iter()
            .filter(|child| {
                nested::transport(
                    e.function,
                    EntityId::Loan(source),
                    EntityId::Loan(child.source),
                    e.span,
                )
                .is_ok()
            })
            .collect::<Vec<_>>();
        let [child] = candidates.as_slice() else {
            return Err(missing());
        };
        if e.children
            .iter()
            .filter(|other| other.statement == child.statement)
            .count()
            != 1
        {
            return Err(missing());
        }
        match instruction.operation {
            Operation::MapIterationFirst { .. } => {
                if instruction.block != child.blocks[0]
                    || instruction.results != [EntityId::Value(child.first)]
                {
                    return Err(missing());
                }
            }
            Operation::MapIterationHasNext { cursor, .. } => {
                let parameters = &e
                    .function
                    .block(child.blocks[1])
                    .ok_or_else(missing)?
                    .parameters;
                if instruction.block != child.blocks[1]
                    || parameters.get(child.source_slot) != Some(&EntityId::Loan(source))
                    || parameters.get(child.cursor_slot) != Some(&EntityId::Value(cursor))
                {
                    return Err(missing());
                }
            }
            Operation::MapIterationNext { cursor, .. } => {
                let parameters = &e
                    .function
                    .block(child.blocks[2])
                    .ok_or_else(missing)?
                    .parameters;
                let anchor = *parameters.get(child.cursor_slot).ok_or_else(missing)?;
                nested::transport(e.function, EntityId::Value(cursor), anchor, e.span)?;
            }
            _ => return Err(missing()),
        }
        Ok(())
    }

    pub(in crate::ssa) mod actions {
        //! Attribute one complete public cleanup sequence to its actual provider statements.
        use crate::ssa::lower_frontend::map_iteration::conditional::*;

        use crate::ssa::lower_frontend::map_iteration::*;

        use lang_frontend::ownership_checking::{DropFact, DropTarget};

        pub(in crate::ssa) fn source_drop(
            owned: &OwnershipCheckedFile,
            plan: &MapIterationOwnershipPlan,
            source: SourceIdentity,
            exit: &IterationExitPlan,
            span: Span,
        ) -> Result<Option<DropFact>, LoweringError> {
            let missing = || error(LoweringErrorKind::MissingFact, span);
            validate_drop_ledger(owned, exit.point(), exit.actions(), span)?;
            if !matches!(
                plan.source_access(),
                MapIterationSourceAccess::Owned | MapIterationSourceAccess::Temporary
            ) {
                return Ok(None);
            }
            let mut ended = false;
            let mut selected = None;
            for action in exit.actions() {
                match action {
                    IterationCleanupAction::EndSource(statement) => {
                        ended = *statement == plan.descriptor().statement()
                    }
                    IterationCleanupAction::Drop(fact)
                        if ended && fact.target() == facts::source_target(source) =>
                    {
                        if selected.replace(*fact).is_some()
                            || fact.point() != exit.point()
                            || !owned.drops().contains(fact)
                        {
                            return Err(missing());
                        }
                    }
                    IterationCleanupAction::Drop(_) => {}
                    _ => ended = false,
                }
            }
            Ok(selected)
        }

        /// Every real Drop at one point occurs once in its complete public sequence.
        pub(in crate::ssa) fn validate_drop_ledger(
            owned: &OwnershipCheckedFile,
            point: DropPoint,
            actions: &[IterationCleanupAction],
            span: Span,
        ) -> Result<(), LoweringError> {
            let missing = || error(LoweringErrorKind::MissingFact, span);
            // The complete point may also clean ancestor providers. Compare the whole
            // published ledger before selecting this frame's source segment, so an
            // absent action cannot be mistaken for a retained owner or staging limit.
            let published = owned
                .drops()
                .iter()
                .filter(|fact| fact.point() == point)
                .collect::<Vec<_>>();
            let emitted = actions
                .iter()
                .filter_map(|action| match action {
                    IterationCleanupAction::Drop(fact) => Some(fact),
                    _ => None,
                })
                .collect::<Vec<_>>();
            if published.len() != emitted.len()
                || published
                    .iter()
                    .any(|fact| emitted.iter().filter(|actual| actual == &fact).count() != 1)
                || emitted
                    .iter()
                    .any(|fact| published.iter().filter(|actual| actual == &fact).count() != 1)
            {
                return Err(missing());
            }
            Ok(())
        }

        pub(in crate::ssa) fn validate(
            frontend: (
                &ParsedFile,
                &NameResolution,
                &TypedFile,
                &OwnershipCheckedFile,
            ),
            plan: &MapIterationOwnershipPlan,
            shape: Shape,
            exit: &IterationExitPlan,
            source_drop: Option<DropFact>,
            span: Span,
        ) -> Result<(), LoweringError> {
            let (parsed, names, typed, owned) = frontend;
            let missing = || error(LoweringErrorKind::MissingFact, span);
            let unsupported = || error(LoweringErrorKind::UnsupportedNode, span);
            let mut local = Vec::new();
            let mut ended = None;
            for (index, action) in exit.actions().iter().enumerate() {
                let statement = match *action {
                    IterationCleanupAction::EndBinding { statement, .. }
                    | IterationCleanupAction::FinishProvider(statement)
                    | IterationCleanupAction::EndSource(statement) => Some(statement),
                    IterationCleanupAction::Drop(fact) => {
                        if fact.point() != exit.point()
                            || !owned.drops().contains(&fact)
                            || fact.capture_slot().is_some()
                            || fact.instance_address().is_some()
                        {
                            return Err(missing());
                        }
                        let is_source = ended
                            .and_then(|statement| owned.map_iteration(statement))
                            .is_some_and(|owner_plan| {
                                matches!(
                                    owner_plan.source_access(),
                                    MapIterationSourceAccess::Owned
                                        | MapIterationSourceAccess::Temporary
                                ) && match (owner_plan.source(), fact.target()) {
                                    (Some(LoanTarget::Place(place)), DropTarget::Named(root)) => {
                                        place.is_root() && place.root() == root
                                    }
                                    (
                                        Some(LoanTarget::Temporary(actual)),
                                        DropTarget::Temporary(root),
                                    ) => *actual == root,
                                    _ => false,
                                }
                            });
                        if is_source {
                            if ended == Some(shape.statement) && source_drop != Some(fact) {
                                return Err(missing());
                            }
                        } else {
                            let DropTarget::Named(root) = fact.target() else {
                                return Err(unsupported());
                            };
                            let symbol = names.symbols().get(root.index()).ok_or_else(missing)?;
                            let live = binding_condition(
                                parsed,
                                names,
                                typed,
                                owned,
                                root,
                                fact.owner().ok_or_else(missing)?,
                                span,
                            )?;
                            if symbol.kind() != SymbolKind::Variable
                                || fact.value_origin() != symbol.span()
                            {
                                return Err(missing());
                            }
                            let matches = match fact.condition() {
                                Some(actual) => source_owner::provenance::equal_on_path(
                                    owned.cleanup_conditions(),
                                    actual,
                                    live,
                                    exit.condition(),
                                ),
                                None => {
                                    owned.cleanup_conditions().get(live)
                                        == Some(&CleanupCondition::Always)
                                }
                            };
                            if !matches {
                                return Err(missing());
                            }
                            // A live local owner is cleaned before the component loans of its
                            // actual enclosing provider, including an outer guard on Return.
                            let mut providers = typed
                                .map_iterations()
                                .iter()
                                .filter_map(|d| {
                                    let node = parsed.ast().statements().get(d.statement()).ok()?;
                                    let Statement::For { body, .. } = node.payload() else {
                                        return None;
                                    };
                                    let node = parsed.ast().statements().get(*body).ok()?;
                                    inside(node.span(), symbol.span()).then_some((
                                        node.span().end() - node.span().start(),
                                        d.statement(),
                                    ))
                                })
                                .collect::<Vec<_>>();
                            providers.sort_by_key(|(size, _)| *size);
                            let (_, provider) = providers.first().ok_or_else(missing)?;
                            let end = exit
                                .actions()
                                .iter()
                                .position(|a| {
                                    matches!(
                                        a,
                                        IterationCleanupAction::EndBinding { statement, .. }
                                            if statement == provider
                                    )
                                })
                                .ok_or_else(missing)?;
                            if index >= end {
                                return Err(missing());
                            }
                        }
                        if source_drop == Some(fact) {
                            local.push(*action);
                        }
                        continue;
                    }
                    _ => return Err(unsupported()),
                };
                if let Some(statement) = statement {
                    if statement == shape.statement {
                        local.push(*action);
                    } else {
                        if !matches!(exit.kind(), IterationExitKind::Return(_)) {
                            return Err(unsupported());
                        }
                        let other = owned.map_iteration(statement).ok_or_else(unsupported)?;
                        let other_exit =
                            unique(other.exits().iter().filter(|e| e.kind() == exit.kind()))
                                .ok_or_else(missing)?;
                        let here = parsed
                            .ast()
                            .statements()
                            .get(shape.statement)
                            .map_err(|_| missing())?
                            .span();
                        let there = parsed
                            .ast()
                            .statements()
                            .get(statement)
                            .map_err(|_| missing())?
                            .span();
                        if !(inside(here, there) || inside(there, here))
                            || other_exit.point() != exit.point()
                            || other_exit.actions() != exit.actions()
                        {
                            return Err(missing());
                        }
                    }
                    ended = match action {
                        IterationCleanupAction::EndSource(statement) => Some(*statement),
                        _ => None,
                    };
                }
            }
            let d = plan.descriptor();
            let mut expected = Vec::new();
            if exit.kind() != IterationExitKind::Exhaustion {
                expected.extend([
                    IterationCleanupAction::EndBinding {
                        statement: shape.statement,
                        symbol: d.value_binding(),
                    },
                    IterationCleanupAction::EndBinding {
                        statement: shape.statement,
                        symbol: d.key_binding(),
                    },
                ]);
            }
            if exit_shape::terminal(exit.kind()) {
                expected.extend([
                    IterationCleanupAction::FinishProvider(shape.statement),
                    IterationCleanupAction::EndSource(shape.statement),
                ]);
                expected.extend(source_drop.map(IterationCleanupAction::Drop));
            }
            if local != expected {
                return Err(missing());
            }
            Ok(())
        }

        fn inside(outer: Span, inner: Span) -> bool {
            outer.source_id() == inner.source_id()
                && outer.start() <= inner.start()
                && inner.end() <= outer.end()
        }

        /// Find the original seed declaration; all actual intermediate phi owners are
        /// checked by the same complete incoming provenance walker as Map source owners.
        pub(in crate::ssa) fn binding_birth(
            parsed: &ParsedFile,
            names: &NameResolution,
            root: SymbolId,
            span: Span,
        ) -> Result<(Option<ExpressionId>, Span), LoweringError> {
            let missing = || error(LoweringErrorKind::MissingFact, span);
            let symbol = names.symbols().get(root.index()).ok_or_else(missing)?;
            if symbol.kind() == SymbolKind::ValueParameter {
                return Ok((None, symbol.span()));
            }
            if symbol.kind() != SymbolKind::Variable {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
            let expression =
                unique(
                    parsed
                        .ast()
                        .items()
                        .iter()
                        .filter_map(|(_, node)| match node.payload() {
                            Item::Variable {
                                name: lang_frontend::parser::NameMarker::Present(marker),
                                initializer,
                                ..
                            } if *marker == symbol.span() => Some(*initializer),
                            _ => None,
                        }),
                )
                .ok_or_else(missing)?;
            let origin = parsed
                .ast()
                .expressions()
                .get(expression)
                .map_err(|_| missing())?
                .span();
            Ok((Some(expression), origin))
        }

        pub(in crate::ssa) fn binding_anchor(
            parsed: &ParsedFile,
            names: &NameResolution,
            typed: &TypedFile,
            owned: &OwnershipCheckedFile,
            root: SymbolId,
            span: Span,
        ) -> Result<crate::ssa::map_source::Birth<SymbolId>, LoweringError> {
            use crate::ssa::map_source::Birth;
            let missing = || error(LoweringErrorKind::MissingFact, span);
            let storage = typed.symbol_type(root).ok_or_else(missing)?;
            let symbol = names.symbols().get(root.index()).ok_or_else(missing)?;
            if symbol.kind() == SymbolKind::Variable
                && symbol.span().end() < span.start()
                && matches!(
                    typed.types().get(storage),
                    Some(lang_frontend::type_checking::TypeKind::Intrinsic {
                        constructor: lang_frontend::type_checking::IntrinsicTypeConstructor::Map
                            | lang_frontend::type_checking::IntrinsicTypeConstructor::MutableMap,
                        ..
                    })
                )
            {
                return Ok(
                    formed::formation(parsed, names, typed, owned, root, storage, span)?.birth,
                );
            }
            let (expression, origin) = binding_birth(parsed, names, root, span)?;
            match expression {
                Some(expression) => Ok(Birth::Call(expression)),
                None => {
                    // Parameter births outside aliases use their real provider Name reference.
                    let reference = parsed
                        .ast()
                        .expressions()
                        .iter()
                        .find_map(|(id, node)| {
                            (node.payload() == &Expression::Name
                                && names.references().iter().any(|r| {
                                    r.span() == node.span()
                                        && r.target()
                                            == &lang_frontend::name_resolution::ReferenceTarget::Symbol(
                                                root,
                                            )
                                }))
                            .then_some(id)
                        })
                        .ok_or_else(missing)?;
                    Ok(Birth::EntryParameter {
                        root,
                        reference,
                        origin,
                    })
                }
            }
        }

        pub(in crate::ssa) fn binding_condition(
            parsed: &ParsedFile,
            names: &NameResolution,
            typed: &TypedFile,
            owned: &OwnershipCheckedFile,
            root: SymbolId,
            owner: lang_frontend::ownership_checking::CleanupOwnerValueId,
            span: Span,
        ) -> Result<lang_frontend::ownership_checking::CleanupConditionId, LoweringError> {
            use lang_frontend::ownership_checking::CleanupOwnerValue;
            let birth = binding_anchor(parsed, names, typed, owned, root, span)?;
            let (expression, origin) = match birth {
                crate::ssa::map_source::Birth::Expression(_) => {
                    return Err(error(LoweringErrorKind::UnsupportedNode, span));
                }
                crate::ssa::map_source::Birth::Call(expression) => (
                    Some(expression),
                    parsed
                        .ast()
                        .expressions()
                        .get(expression)
                        .map_err(|_| error(LoweringErrorKind::MissingFact, span))?
                        .span(),
                ),
                crate::ssa::map_source::Birth::EntryParameter { origin, .. } => (None, origin),
            };
            let missing = || error(LoweringErrorKind::MissingFact, span);
            match owned
                .cleanup_conditions()
                .owner_value(owner)
                .ok_or_else(missing)?
            {
                CleanupOwnerValue::Expression {
                    expression: actual,
                    origin: at,
                } => {
                    if expression != Some(*actual)
                        || origin != *at
                        || !owned.formed_owned_locals().contains(&root)
                    {
                        return Err(missing());
                    }
                    // A body-local birth need never enter a provider header. Its exact
                    // original Expression is live on its cleanup path; use a published
                    // constant-true identity, independently of the Drop's own condition.
                    owned
                        .map_iterations()
                        .iter()
                        .flat_map(|plan| plan.closure_phi_incomings())
                        .map(|incoming| incoming.condition())
                        .find(|condition| {
                            owned.cleanup_conditions().get(*condition)
                                == Some(&CleanupCondition::Always)
                        })
                        .ok_or_else(missing)
                }
                CleanupOwnerValue::IterationPhi { .. } | CleanupOwnerValue::Parameter { .. } => {
                    source_owner::birth_condition(parsed, owned, (root, birth), owner, span, true)
                }
                _ => Err(error(LoweringErrorKind::UnsupportedNode, span)),
            }
        }
    }
}
