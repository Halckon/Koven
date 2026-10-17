//! Small identity graph queries. Every actual incoming column is checked; cycles
//! close only if their complete dependency component reaches the exact birth.
use super::*;

pub(super) fn edges(block: &Block) -> Outcome<Vec<&Edge>> {
    let term = block
        .terminator
        .as_ref()
        .ok_or_else(|| missing("missing terminator", block.id))?;
    Ok(match &term.kind {
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
        TerminatorKind::Return { .. }
        | TerminatorKind::BorrowReturn { .. }
        | TerminatorKind::RangeReturn { .. }
        | TerminatorKind::Abort => Vec::new(),
    })
}

pub(super) fn target<'a>(function: &'a Function, edge: &Edge) -> Outcome<&'a Block> {
    function
        .block(edge.target)
        .ok_or_else(|| missing("source-qualified edge target", edge))
}

pub(super) fn incoming(
    function: &Function,
    block: BlockId,
    column: usize,
) -> Outcome<Vec<(BlockId, ValueId)>> {
    let mut result = Vec::new();
    for predecessor in &function.blocks {
        for edge in edges(predecessor)? {
            if edge.target != block {
                continue;
            }
            let target = target(function, edge)?;
            gate(
                "consume identity: incoming column exists in real target",
                column < target.parameters.len(),
                (edge, column),
            )?;
            let Some(EntityId::Value(value)) = edge.arguments.get(column) else {
                return Err(missing(
                    "every incoming owner column",
                    (predecessor.id, edge, column),
                ));
            };
            result.push((predecessor.id, *value));
        }
    }
    gate(
        "consume identity: owner parameter has predecessors",
        !result.is_empty(),
        (block, column),
    )?;
    Ok(result)
}

pub(super) fn only_incoming(
    function: &Function,
    block: BlockId,
    header: BlockId,
    expected: &Edge,
) -> Outcome<()> {
    let mut found = Vec::new();
    for predecessor in &function.blocks {
        for edge in edges(predecessor)? {
            if edge.target == block {
                found.push((predecessor.id, edge));
            }
        }
    }
    gate(
        "consume identity: original direct branch is the entire successor incoming set",
        found.as_slice() == [(header, expected)],
        found,
    )
}

/// Reverse all incoming columns, retain cycles, then close a fixed point from the
/// exact birth. A carrier SCC without that birth, or any foreign terminal, fails.
pub(super) fn lineage(
    function: &Function,
    current: ValueId,
    birth: ValueId,
) -> Outcome<BTreeSet<ValueId>> {
    let storage = ty(function, birth)?;
    let mut pending = vec![current];
    let mut dependencies = BTreeMap::<ValueId, Vec<ValueId>>::new();
    while let Some(value) = pending.pop() {
        if dependencies.contains_key(&value) {
            continue;
        }
        gate(
            "consume identity: finite lineage guard",
            dependencies.len() < 4096,
            function.id,
        )?;
        gate(
            "consume identity: exact lineage storage",
            ty(function, value)? == storage,
            value,
        )?;
        if value == birth {
            dependencies.insert(value, Vec::new());
            continue;
        }
        let data = function
            .entity(EntityId::Value(value))
            .ok_or_else(|| missing("lineage entity", value))?;
        let Definition::BlockParameter { block, index } = data.definition else {
            return Err(missing("lineage foreign terminal", (value, data)));
        };
        let inputs: Vec<_> = incoming(function, block, index)?
            .into_iter()
            .map(|(_, value)| value)
            .collect();
        pending.extend(inputs.iter().copied());
        dependencies.insert(value, inputs);
    }
    let mut closed = BTreeSet::from([birth]);
    loop {
        let previous = closed.len();
        for (&value, inputs) in &dependencies {
            if inputs.iter().any(|input| closed.contains(input)) {
                closed.insert(value);
            }
        }
        if previous == closed.len() {
            break;
        }
    }
    gate(
        "consume identity: every complete lineage SCC reaches exact birth",
        dependencies.keys().all(|value| closed.contains(value)),
        (&dependencies, &closed),
    )?;
    Ok(dependencies.into_keys().collect())
}

/// Enumerate all forward transports, then validate each target against *all* its
/// predecessors. Same-typed foreign values at joins never become source aliases.
pub(super) fn aliases(function: &Function, birth: ValueId) -> Outcome<BTreeSet<ValueId>> {
    let mut result = BTreeSet::from([birth]);
    loop {
        let previous = result.len();
        for block in &function.blocks {
            for edge in edges(block)? {
                let target = target(function, edge)?;
                for (column, argument) in edge.arguments.iter().enumerate() {
                    let EntityId::Value(argument) = argument else {
                        continue;
                    };
                    if !result.contains(argument) {
                        continue;
                    }
                    let Some(EntityId::Value(parameter)) = target.parameters.get(column) else {
                        return Err(missing("transport owner target column", (edge, column)));
                    };
                    result.insert(*parameter);
                }
            }
        }
        gate(
            "consume identity: finite alias guard",
            result.len() <= 4096,
            function.id,
        )?;
        if previous == result.len() {
            break;
        }
    }
    for &alias in &result {
        lineage(function, alias, birth)?;
    }
    Ok(result)
}

/// The current wrapper, original view, and extracted inner may each have exactly
/// their one designated use. This also rejects stale edge transports and returns.
pub(super) fn only_instruction_use(
    function: &Function,
    entity: EntityId,
    allowed: InstructionId,
) -> Outcome<()> {
    let mut uses = Vec::new();
    for instruction in &function.instructions {
        for used in instruction.operation.entities() {
            if used == entity {
                uses.push(instruction.id);
            }
        }
    }
    gate(
        "consume identity: one exact instruction use",
        uses.as_slice() == [allowed],
        (entity, uses),
    )?;
    for block in &function.blocks {
        let term = block
            .terminator
            .as_ref()
            .ok_or_else(|| missing("owner use terminator", block.id))?;
        gate(
            "consume identity: no stale transport or return",
            !term.kind.entities().contains(&entity),
            (entity, block.id, term),
        )?;
    }
    Ok(())
}

/// Before the selected branch, only the complete reverse lineage transports and
/// the actual branch are permitted. The two successor owners are fresh identities.
pub(super) fn before_branch(
    function: &Function,
    aliases: &BTreeSet<ValueId>,
    header: BlockId,
    owner: ValueId,
) -> Outcome<()> {
    for instruction in &function.instructions {
        gate(
            "consume identity: pre-branch wrapper has no extra instruction use",
            !instruction
                .operation
                .entities()
                .iter()
                .any(|entity| matches!(entity, EntityId::Value(value) if aliases.contains(value))),
            instruction,
        )?;
    }
    for block in &function.blocks {
        let term = block
            .terminator
            .as_ref()
            .ok_or_else(|| missing("pre-branch terminator", block.id))?;
        if block.id == header {
            gate(
                "consume identity: actual branch exact owner",
                matches!(term.kind, TerminatorKind::NullableBranch { owner: actual, .. } if actual == owner),
                term,
            )?;
        } else {
            let non_edge = match &term.kind {
                TerminatorKind::Branch(_) => Vec::new(),
                TerminatorKind::Conditional { condition, .. } => vec![EntityId::Value(*condition)],
                TerminatorKind::NullableBranch { owner, .. } => vec![EntityId::Value(*owner)],
                _ => term.kind.entities(),
            };
            gate(
                "consume identity: pre-branch only edge uses",
                !non_edge.iter().any(
                    |entity| matches!(entity, EntityId::Value(value) if aliases.contains(value)),
                ),
                (block.id, term),
            )?;
        }
        for edge in edges(block)? {
            let target = target(function, edge)?;
            let mut seen = BTreeSet::new();
            for (column, argument) in edge.arguments.iter().enumerate() {
                let EntityId::Value(argument) = argument else {
                    continue;
                };
                if !aliases.contains(argument) {
                    continue;
                }
                gate(
                    "consume identity: one owner delivery per edge",
                    seen.insert(*argument),
                    edge,
                )?;
                if block.id == header {
                    gate(
                        "consume identity: branch transports only its current owner",
                        *argument == owner,
                        edge,
                    )?;
                } else {
                    gate(
                        "consume identity: no unobserved source escape",
                        matches!(target.parameters.get(column), Some(EntityId::Value(value)) if aliases.contains(value)),
                        (edge, column),
                    )?;
                }
            }
        }
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct State {
    block: BlockId,
    start: usize,
    current: Option<ValueId>,
    read: bool,
    drops: usize,
}

/// Explore both/all outgoing edges with owner state in the visited key. A guard
/// failure rejects the query; it never prunes unknown paths or counts them as green.
pub(super) fn cleanup(
    function: &Function,
    block: BlockId,
    start: usize,
    owner: ValueId,
    aliases: &BTreeSet<ValueId>,
    cleanup: &Cleanup,
    consume: InstructionId,
) -> Outcome<()> {
    let drops: Vec<_> = function.instructions.iter().filter(|instruction| matches!(instruction.operation, Operation::Drop { owner } if aliases.contains(&owner))).collect();
    let drop = *one(&drops, "unique exact owner Drop instruction")?;
    gate(
        "consume identity: Drop uses real frontend origin",
        drop.origin == Origin::Source(cleanup.origin),
        drop,
    )?;
    if start > 0 && cleanup.read.is_none() {
        gate(
            "consume identity: consume TemporaryDrop immediately follows expression",
            function
                .block(block)
                .and_then(|block| block.instructions.get(start))
                == Some(&drop.id),
            drop,
        )?;
    }
    let read = match cleanup.read {
        Some(origin) => Some(
            at(function, origin, |op| {
                matches!(op, Operation::MapSize { .. })
            })?
            .id,
        ),
        None => None,
    };
    for instruction in &function.instructions {
        let uses: Vec<_> = instruction
            .operation
            .entities()
            .into_iter()
            .filter(|entity| matches!(entity, EntityId::Value(value) if aliases.contains(value)))
            .collect();
        if !uses.is_empty() {
            gate(
                "consume identity: exact result/null use graph",
                uses.len() == 1 && (instruction.id == drop.id || Some(instruction.id) == read),
                instruction,
            )?;
        }
    }
    let initial = State {
        block,
        start,
        current: Some(owner),
        read: false,
        drops: 0,
    };
    let mut pending = vec![initial.clone()];
    let mut transitions = BTreeMap::<State, Vec<State>>::new();
    let mut returns = BTreeSet::new();
    while let Some(state) = pending.pop() {
        if transitions.contains_key(&state) {
            continue;
        }
        gate(
            "consume identity: finite path-state guard",
            transitions.len() < 8192,
            function.id,
        )?;
        let block = function
            .block(state.block)
            .ok_or_else(|| missing("cleanup block", &state))?;
        gate(
            "consume identity: valid start instruction",
            state.start <= block.instructions.len(),
            &state,
        )?;
        let mut next = state.clone();
        for id in &block.instructions[state.start..] {
            let instruction = function
                .instruction(*id)
                .ok_or_else(|| missing("cleanup instruction", id))?;
            gate(
                "consume identity: no reentry to consume on either cleanup path",
                *id != consume,
                instruction,
            )?;
            for used in instruction.operation.entities() {
                let EntityId::Value(value) = used else {
                    continue;
                };
                if !aliases.contains(&value) {
                    continue;
                }
                gate(
                    "consume identity: use current live owner",
                    next.current == Some(value),
                    (instruction, &next),
                )?;
                if Some(*id) == read {
                    gate(
                        "consume identity: exact one size read before Drop",
                        !next.read
                            && matches!(instruction.operation, Operation::MapSize { owner: EntityId::Value(actual) } if actual == value),
                        instruction,
                    )?;
                    next.read = true;
                } else if *id == drop.id {
                    gate(
                        "consume identity: read fact precedes result Drop",
                        read.is_none() || next.read,
                        (&next, instruction),
                    )?;
                    next.current = None;
                    next.drops += 1;
                } else {
                    return Err(missing("unexpected owner use", instruction));
                }
            }
        }
        let term = block
            .terminator
            .as_ref()
            .ok_or_else(|| missing("cleanup terminator", block.id))?;
        let mut successors = Vec::new();
        match &term.kind {
            TerminatorKind::Return { values } => {
                gate(
                    "consume identity: normal return has exactly one Drop and no live owner",
                    next.current.is_none()
                        && next.drops == 1
                        && !values.iter().any(|value| aliases.contains(value)),
                    (&state, &next, term),
                )?;
                returns.insert(state.clone());
            }
            TerminatorKind::Abort
            | TerminatorKind::BorrowReturn { .. }
            | TerminatorKind::RangeReturn { .. } => {
                return Err(missing(
                    "unexpected nonordinary cleanup exit",
                    (block.id, term),
                ));
            }
            _ => {
                if let TerminatorKind::NullableBranch { owner, .. } = &term.kind {
                    gate(
                        "consume identity: no second test of tracked owner",
                        !aliases.contains(owner),
                        term,
                    )?;
                }
                if let TerminatorKind::Conditional { condition, .. } = &term.kind {
                    gate(
                        "consume identity: owner not condition",
                        !aliases.contains(condition),
                        term,
                    )?;
                }
                for edge in edges(block)? {
                    let target = target(function, edge)?;
                    let deliveries: Vec<_> = edge
                        .arguments
                        .iter()
                        .enumerate()
                        .filter_map(|(column, entity)| match entity {
                            EntityId::Value(value) if aliases.contains(value) => {
                                Some((column, *value))
                            }
                            _ => None,
                        })
                        .collect();
                    let current = match next.current {
                        Some(value) => {
                            let &(column, actual) =
                                one(&deliveries, "one current live obligation per actual edge")?;
                            gate(
                                "consume identity: live edge transports current owner",
                                actual == value,
                                edge,
                            )?;
                            let Some(EntityId::Value(parameter)) = target.parameters.get(column)
                            else {
                                return Err(missing("cleanup parameter column", edge));
                            };
                            gate(
                                "consume identity: target belongs to complete same-birth lineage",
                                aliases.contains(parameter),
                                (edge, parameter),
                            )?;
                            Some(*parameter)
                        }
                        None => {
                            gate(
                                "consume identity: dead owner cannot escape edge",
                                deliveries.is_empty(),
                                edge,
                            )?;
                            None
                        }
                    };
                    successors.push(State {
                        block: target.id,
                        start: 0,
                        current,
                        read: next.read,
                        drops: next.drops,
                    });
                }
            }
        }
        pending.extend(successors.iter().cloned());
        transitions.insert(state, successors);
    }
    // Close the entire state graph, including SCCs. No missing/unexplained sink
    // can be hidden by the visited set; all reachable states have normal cleanup.
    let mut closed = returns;
    loop {
        let previous = closed.len();
        for (state, successors) in &transitions {
            if successors.iter().any(|next| closed.contains(next)) {
                closed.insert(state.clone());
            }
        }
        if previous == closed.len() {
            break;
        }
    }
    gate(
        "consume identity: all path-state SCCs close to checked normal return",
        transitions.keys().all(|state| closed.contains(state)),
        (&transitions, &closed),
    )
}
