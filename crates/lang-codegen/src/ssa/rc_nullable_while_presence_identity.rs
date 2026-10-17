//! Exact read-only owner/cleanup queries derived from the eight real presence SSA products.
use crate::ssa::model::{
    Block, BlockId, ComparisonOperator, Definition, Edge, EntityId, EntityType, Function,
    FunctionId, Instruction, InstructionId, Module, Operation, Origin, Ownership, Program,
    ScalarConstant, SsaTypeId, SsaTypeKind, TerminatorKind, ValueId,
};
use lang_frontend::source::Span;

use super::identity_facts::Facts;

type Query<T> = Result<T, String>;

fn require(valid: bool, label: &str) -> Query<()> {
    if valid { Ok(()) } else { Err(label.to_owned()) }
}

fn one<T>(mut values: impl Iterator<Item = T>, label: &str) -> Query<T> {
    let value = values.next().ok_or_else(|| format!("{label}: absent"))?;
    require(values.next().is_none(), &format!("{label}: ambiguous"))?;
    Ok(value)
}

fn term(block: &Block) -> Query<&TerminatorKind> {
    block
        .terminator
        .as_ref()
        .map(|term| &term.kind)
        .ok_or("missing actual terminator".into())
}

fn block(function: &Function, id: BlockId) -> Query<&Block> {
    function
        .block(id)
        .ok_or("block does not belong to the actual function".into())
}

fn first(function: &Function) -> Query<&Block> {
    block(
        function,
        function.entry_block().ok_or("actual entry block absent")?,
    )
}

fn branch(block: &Block) -> Query<&Edge> {
    match term(block)? {
        TerminatorKind::Branch(edge) => Ok(edge),
        _ => Err("expected observed ordinary branch".into()),
    }
}

fn value_type(function: &Function, value: ValueId) -> Query<SsaTypeId> {
    match function.entity(EntityId::Value(value)).map(|data| data.ty) {
        Some(EntityType::Value(ty)) => Ok(ty),
        _ => Err("actual Value entity absent".into()),
    }
}

fn parameter(function: &Function, block: &Block, ty: SsaTypeId) -> Query<ValueId> {
    let EntityId::Value(value) = one(block.parameters.iter().copied(), "sole owned parameter")?
    else {
        return Err("owned parameter is not Value".into());
    };
    let data = function
        .entity(EntityId::Value(value))
        .ok_or("parameter entity absent")?;
    require(
        data.ty == EntityType::Value(ty)
            && data.definition
                == Definition::BlockParameter {
                    block: block.id,
                    index: 0,
                },
        "owned parameter exact definition and nullable type",
    )?;
    Ok(value)
}

fn result(function: &Function, instruction: &Instruction, ty: SsaTypeId) -> Query<ValueId> {
    let EntityId::Value(value) = one(
        instruction.results.iter().copied(),
        "sole actual instruction result",
    )?
    else {
        return Err("instruction result is not Value".into());
    };
    let data = function
        .entity(EntityId::Value(value))
        .ok_or("result entity absent")?;
    require(
        data.ty == EntityType::Value(ty)
            && data.definition
                == Definition::InstructionResult {
                    instruction: instruction.id,
                    index: 0,
                },
        "result exact instruction/column/type identity",
    )?;
    Ok(value)
}

fn instruction_at(
    function: &Function,
    span: Span,
    matches: impl Fn(&Operation) -> bool,
) -> Query<&Instruction> {
    one(
        function.instructions.iter().filter(|instruction| {
            instruction.origin == Origin::Source(span) && matches(&instruction.operation)
        }),
        "source-anchored actual operation",
    )
}

fn before(function: &Function, left: &Instruction, right: &Instruction) -> Query<()> {
    require(
        left.block == right.block,
        "observed ordered operations belong to the same block",
    )?;
    let instructions = &block(function, left.block)?.instructions;
    let left = instructions
        .iter()
        .position(|id| *id == left.id)
        .ok_or("left instruction absent")?;
    let right = instructions
        .iter()
        .position(|id| *id == right.id)
        .ok_or("right instruction absent")?;
    require(left < right, "actual source/cleanup instruction order")
}

fn incoming(function: &Function, target: BlockId) -> Query<Vec<(BlockId, &Edge)>> {
    let mut result = Vec::new();
    for block in &function.blocks {
        let edges: Vec<&Edge> = match term(block)? {
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
        };
        result.extend(
            edges
                .into_iter()
                .filter(|edge| edge.target == target)
                .map(|edge| (block.id, edge)),
        );
    }
    Ok(result)
}

fn exact_incoming(
    function: &Function,
    target: BlockId,
    expected: &[(BlockId, ValueId)],
) -> Query<()> {
    let actual = incoming(function, target)?;
    require(
        actual.len() == expected.len(),
        "all owner incoming edges are accounted for",
    )?;
    for (predecessor, owner) in expected {
        let (_, edge) = one(
            actual.iter().filter(|(id, _)| id == predecessor),
            "unique actual predecessor",
        )?;
        require(
            edge.arguments == [EntityId::Value(*owner)],
            "edge transports exact predecessor owner",
        )?;
    }
    Ok(())
}

fn block_set(function: &Function, expected: &[BlockId]) -> Query<()> {
    require(
        expected
            .iter()
            .enumerate()
            .all(|(index, id)| !expected[..index].contains(id))
            && function.blocks.len() == expected.len()
            && function
                .blocks
                .iter()
                .all(|block| expected.contains(&block.id)),
        "observed whole-function block closure; no hidden normal exit or extra cycle",
    )
}

fn returned(block: &Block, value: ValueId, span: Span) -> Query<()> {
    require(
        matches!(term(block)?, TerminatorKind::Return { values } if values == &[value])
            && block.terminator.as_ref().map(|term| &term.origin) == Some(&Origin::Source(span)),
        "source Return transfers the exact expected owner/result",
    )
}

fn owned_function(function: &Function, returns: &[SsaTypeId]) -> Query<()> {
    require(
        function.return_types == returns
            && function.receiver().is_none()
            && function.borrow_return.is_none()
            && function.carrier_return.is_none(),
        "real callable uses the source owned-return ABI",
    )
}

// The admitted operations are exact identities observed below, not any same-typed Drop.
fn owner_operations(
    function: &Function,
    nullable: SsaTypeId,
    inner: SsaTypeId,
    allowed: &[InstructionId],
) -> Query<()> {
    for instruction in &function.instructions {
        let touches_owner = instruction
            .operation
            .entities()
            .iter()
            .chain(&instruction.results)
            .any(|entity| {
                function
                    .entity(*entity)
                    .is_some_and(|data| [nullable, inner].contains(&data.ty.semantic_type()))
            });
        if touches_owner {
            require(
                allowed.contains(&instruction.id),
                "no extra creation, retain, extraction, loan or consume of the Rc owner",
            )?;
        }
    }
    Ok(())
}

fn nullable_drop(
    function: &Function,
    nullable: SsaTypeId,
    owner: ValueId,
    origin: Span,
) -> Query<&Instruction> {
    let drop = one(function.instructions.iter().filter(|instruction| {
        matches!(instruction.operation, Operation::Drop { owner } if value_type(function, owner).ok() == Some(nullable))
    }), "unique actual nullable Rc Drop")?;
    require(
        matches!(drop.operation, Operation::Drop { owner: actual } if actual == owner)
            && drop.origin == Origin::Source(origin)
            && drop.results.is_empty(),
        "Drop consumes exact transported owner and real source DropFact value_origin",
    )?;
    Ok(drop)
}

fn inspect(
    function: &Function,
    nullable: SsaTypeId,
    inner: SsaTypeId,
    boolean: SsaTypeId,
    case: &str,
    facts: &Facts,
) -> Query<()> {
    owned_function(function, &[nullable])?;
    let entry = first(function)?;
    let seed = parameter(function, entry, nullable)?;
    let test = instruction_at(function, facts.guard, |op| {
        matches!(op, Operation::NullableIsNull { .. })
    })?;
    let header = block(function, test.block)?;
    let current = parameter(function, header, nullable)?;
    require(
        matches!(test.operation, Operation::NullableIsNull { owner } if owner == current),
        "guard reads actual current loop owner",
    )?;
    let tested = result(function, test, boolean)?;
    let invert = instruction_at(
        function,
        facts.guard,
        |op| matches!(op, Operation::BooleanNot { operand } if *operand == tested),
    )?;
    let inverted = result(function, invert, boolean)?;
    require(
        header.instructions == [test.id, invert.id]
            && invert.block == header.id
            && entry.instructions.is_empty(),
        "observed entry and guard operations",
    )?;
    require(
        branch(entry)?.target == header.id,
        "real seed enters actual header",
    )?;
    let TerminatorKind::Conditional {
        condition,
        when_true,
        when_false,
    } = term(header)?
    else {
        return Err("guard is not the observed ordinary conditional".into());
    };
    require(
        *condition == inverted
            && header.terminator.as_ref().map(|term| &term.origin)
                == Some(&Origin::Source(facts.while_span)),
        "while branches on the actual != null result",
    )?;
    let body = block(function, when_true.target)?;
    let exit = block(function, when_false.target)?;
    let body_owner = parameter(function, body, nullable)?;
    let exit_owner = parameter(function, exit, nullable)?;
    exact_incoming(function, entry.id, &[])?;
    exact_incoming(function, body.id, &[(header.id, current)])?;
    exact_incoming(function, exit.id, &[(header.id, current)])?;
    require(
        exit.instructions.is_empty(),
        "false edge preserves whole nullable owner",
    )?;
    let mut allowed = vec![test.id];
    if case == "break-presence" {
        require(
            facts.inspect_drop_origin.is_none() && facts.body_null.is_none(),
            "break source transfers its whole owner",
        )?;
        exact_incoming(function, header.id, &[(entry.id, seed)])?;
        let joined = block(function, branch(body)?.target)?;
        require(
            branch(exit)?.target == joined.id && joined.instructions.is_empty(),
            "break and false exit join without owner operations",
        )?;
        let output = parameter(function, joined, nullable)?;
        exact_incoming(
            function,
            joined.id,
            &[(body.id, body_owner), (exit.id, exit_owner)],
        )?;
        returned(joined, output, facts.tail_return)?;
        block_set(
            function,
            &[entry.id, header.id, body.id, exit.id, joined.id],
        )?;
    } else {
        let null = instruction_at(
            function,
            facts.body_null.ok_or("body null source absent")?,
            |op| matches!(op, Operation::NullableNull { nullable: actual } if *actual == nullable),
        )?;
        let replacement = result(function, null, nullable)?;
        let drop = nullable_drop(
            function,
            nullable,
            body_owner,
            facts
                .inspect_drop_origin
                .ok_or("real inspect DropFact absent")?,
        )?;
        require(
            null.block == body.id && drop.block == body.id && replacement != body_owner,
            "new null and cleaned current owner are distinct real body identities",
        )?;
        before(function, null, drop)?;
        allowed.extend([null.id, drop.id]);
        returned(exit, exit_owner, facts.tail_return)?;
        if case == "return-presence" {
            exact_incoming(function, header.id, &[(entry.id, seed)])?;
            returned(
                body,
                replacement,
                facts.body_return.ok_or("body return source absent")?,
            )?;
        } else {
            require(
                matches!(case, "normal-presence" | "continue-presence"),
                "known bounded presence case",
            )?;
            require(
                branch(body)?.target == header.id
                    && body.terminator.as_ref().map(|term| &term.origin)
                        == facts
                            .backedge_origin
                            .as_ref()
                            .map(|span| Origin::Source(*span))
                            .as_ref(),
                "observed normal/continue backedge source identity",
            )?;
            // Cut at the actual header: its two incoming values have different births.
            exact_incoming(
                function,
                header.id,
                &[(entry.id, seed), (body.id, replacement)],
            )?;
        }
        block_set(function, &[entry.id, header.id, body.id, exit.id])?;
    }
    owner_operations(function, nullable, inner, &allowed)
}

fn reader(
    function: &Function,
    nullable: SsaTypeId,
    inner: SsaTypeId,
    boolean: SsaTypeId,
    facts: &Facts,
) -> Query<()> {
    owned_function(function, &[boolean])?;
    let entry = first(function)?;
    let owner = parameter(function, entry, nullable)?;
    let test = instruction_at(
        function,
        facts.reader_comparison,
        |op| matches!(op, Operation::NullableIsNull { owner: actual } if *actual == owner),
    )?;
    let result = result(function, test, boolean)?;
    let drop = nullable_drop(function, nullable, owner, facts.reader_drop_origin)?;
    require(
        entry.instructions == [test.id, drop.id]
            && test.block == entry.id
            && drop.block == entry.id
            && function.instructions.len() == 2
            && function.places.is_empty()
            && function.loans.is_empty(),
        "reader reads same owner, drops it, and preserves only its Boolean observation",
    )?;
    returned(entry, result, facts.reader_return)?;
    exact_incoming(function, entry.id, &[])?;
    block_set(function, &[entry.id])?;
    owner_operations(function, nullable, inner, &[test.id, drop.id])
}

fn call(
    function: &Function,
    span: Span,
    callee_id: FunctionId,
    argument: ValueId,
    result_type: SsaTypeId,
) -> Query<(&Instruction, ValueId)> {
    let instruction = instruction_at(
        function,
        span,
        |op| matches!(op, Operation::DirectCall { callee, .. } if *callee == callee_id),
    )?;
    require(
        matches!(&instruction.operation, Operation::DirectCall { receiver: None, arguments, .. }
        if arguments == &[EntityId::Value(argument)]),
        "call Value-delivers exactly the real owner to the actual callee",
    )?;
    Ok((instruction, result(function, instruction, result_type)?))
}

fn sole_instruction_use(function: &Function, owner: ValueId, consumer: InstructionId) -> Query<()> {
    let users = function
        .instructions
        .iter()
        .filter(|instruction| {
            instruction
                .operation
                .entities()
                .contains(&EntityId::Value(owner))
        })
        .map(|instruction| instruction.id)
        .collect::<Vec<_>>();
    require(
        users == [consumer],
        "caller owner/result has only its exact authorized consumer",
    )?;
    for block in &function.blocks {
        require(
            !term(block)?.entities().contains(&EntityId::Value(owner)),
            "delivered caller owner never survives into an edge or return",
        )?;
    }
    Ok(())
}

fn marker<'a>(
    function: &'a Function,
    block: &'a Block,
    observed: ValueId,
    expected: bool,
    boolean: SsaTypeId,
) -> Query<(&'a Block, &'a Block)> {
    let comparison = one(function.instructions.iter().filter(|instruction| {
        instruction.block == block.id && matches!(instruction.operation,
            Operation::Compare { operator: ComparisonOperator::Equal, left, .. } if left == observed)
    }), "marker compares exact reader observation")?;
    let Operation::Compare { right, .. } = comparison.operation else {
        return Err("selected marker is not an actual comparison".into());
    };
    let constant = one(
        function
            .instructions
            .iter()
            .filter(|instruction| instruction.results == [EntityId::Value(right)]),
        "marker expectation source",
    )?;
    require(
        matches!(constant.operation, Operation::Constant(ScalarConstant::Boolean(actual)) if actual == expected),
        "actual marker Boolean expectation",
    )?;
    require(
        result(function, constant, boolean)? == right,
        "marker expectation exact result definition",
    )?;
    before(function, constant, comparison)?;
    let condition = result(function, comparison, boolean)?;
    let TerminatorKind::Conditional {
        condition: actual,
        when_true,
        when_false,
    } = term(block)?
    else {
        return Err("marker does not branch normally".into());
    };
    require(*actual == condition, "marker uses exact comparison result")?;
    Ok((
        self::block(function, when_true.target)?,
        self::block(function, when_false.target)?,
    ))
}

fn scalar_incoming(
    function: &Function,
    target: &Block,
    expected: &[BlockId],
    boolean: SsaTypeId,
) -> Query<()> {
    let actual = incoming(function, target.id)?;
    require(
        actual.len() == expected.len(),
        "all ordinary caller predecessors accounted for",
    )?;
    for predecessor in expected {
        let (_, edge) = one(
            actual.iter().filter(|(id, _)| id == predecessor),
            "actual marker predecessor",
        )?;
        require(
            edge.arguments.len() == target.parameters.len(),
            "caller edge column arity",
        )?;
        for (index, (argument, parameter)) in
            edge.arguments.iter().zip(&target.parameters).enumerate()
        {
            let arg = function
                .entity(*argument)
                .ok_or("caller argument entity absent")?;
            let param = function
                .entity(*parameter)
                .ok_or("caller parameter entity absent")?;
            require(
                arg.ty == EntityType::Value(boolean)
                    && param.ty == arg.ty
                    && param.definition
                        == Definition::BlockParameter {
                            block: target.id,
                            index,
                        },
                "marker edges transport only real Boolean columns, never consumed owners",
            )?;
        }
    }
    Ok(())
}

struct StorageTypes {
    nullable: SsaTypeId,
    inner: SsaTypeId,
    payload: SsaTypeId,
    boolean: SsaTypeId,
}

fn entry(
    function: &Function,
    inspect: &Function,
    reader: &Function,
    storage: StorageTypes,
    case: &str,
    facts: &Facts,
) -> Query<()> {
    let StorageTypes {
        nullable,
        inner,
        payload,
        boolean,
    } = storage;
    owned_function(function, &[])?;
    let first = first(function)?;
    require(
        first.parameters.is_empty(),
        "entry is the real no-argument source function",
    )?;
    let payload_instruction = instruction_at(function, facts.entry_payload, |op| {
        matches!(op, Operation::Constant(ScalarConstant::Integer(41)))
    })?;
    let payload_value = result(function, payload_instruction, payload)?;
    let allocation = instruction_at(
        function,
        facts.entry_construction,
        |op| matches!(op, Operation::SharedAllocate { owner, payload } if *owner == inner && *payload == payload_value),
    )?;
    let shared = result(function, allocation, inner)?;
    let wrap = instruction_at(
        function,
        facts.entry_present_statement,
        |op| matches!(op, Operation::NullableWrap { nullable: actual, owner } if *actual == nullable && *owner == shared),
    )?;
    let present = result(function, wrap, nullable)?;
    let absent_instruction = instruction_at(
        function,
        facts.entry_null,
        |op| matches!(op, Operation::NullableNull { nullable: actual } if *actual == nullable),
    )?;
    let absent = result(function, absent_instruction, nullable)?;
    let mut reads = Vec::new();
    let mut allowed = vec![allocation.id, wrap.id, absent_instruction.id];
    for (index, input) in [present, absent].into_iter().enumerate() {
        let (inspect_call, result_owner) = call(
            function,
            facts.calls[index].inspect,
            inspect.id,
            input,
            nullable,
        )?;
        let (reader_call, observation) = call(
            function,
            facts.calls[index].reader,
            reader.id,
            result_owner,
            boolean,
        )?;
        before(function, inspect_call, reader_call)?;
        sole_instruction_use(function, input, inspect_call.id)?;
        sole_instruction_use(function, result_owner, reader_call.id)?;
        reads.push((inspect_call, reader_call, observation));
        allowed.extend([inspect_call.id, reader_call.id]);
    }
    sole_instruction_use(function, shared, wrap.id)?;
    before(function, payload_instruction, allocation)?;
    before(function, allocation, wrap)?;
    before(function, wrap, reads[0].0)?;
    before(function, absent_instruction, reads[1].0)?;
    require(
        reads[0].0.block == first.id,
        "present input is actually inspected at entry",
    )?;
    let second = block(function, absent_instruction.block)?;
    let (present_yes, present_no) = marker(
        function,
        first,
        reads[0].2,
        case != "break-presence",
        boolean,
    )?;
    require(
        branch(present_yes)?.target == second.id && branch(present_no)?.target == second.id,
        "both present outcomes proceed to the actual absent input",
    )?;
    let (absent_yes, absent_no) = marker(function, second, reads[1].2, true, boolean)?;
    let exit = block(function, branch(absent_yes)?.target)?;
    require(
        branch(absent_no)?.target == exit.id
            && matches!(term(exit)?, TerminatorKind::Return { values } if values.is_empty()),
        "both absent outcomes reach the sole normal Unit return",
    )?;
    scalar_incoming(function, first, &[], boolean)?;
    scalar_incoming(function, present_yes, &[first.id], boolean)?;
    scalar_incoming(function, present_no, &[first.id], boolean)?;
    scalar_incoming(function, second, &[present_yes.id, present_no.id], boolean)?;
    scalar_incoming(function, absent_yes, &[second.id], boolean)?;
    scalar_incoming(function, absent_no, &[second.id], boolean)?;
    scalar_incoming(function, exit, &[absent_yes.id, absent_no.id], boolean)?;
    block_set(
        function,
        &[
            first.id,
            present_yes.id,
            present_no.id,
            second.id,
            absent_yes.id,
            absent_no.id,
            exit.id,
        ],
    )?;
    owner_operations(function, nullable, inner, &allowed)
}

fn source_function(module: &Module, span: Span) -> Query<&Function> {
    one(
        module
            .functions
            .iter()
            .filter(|function| function.origin == Origin::Source(span)),
        "actual source function",
    )
}

pub(super) fn inspect_program(
    program: &Program,
    selected: FunctionId,
    case: &str,
    facts: &Facts,
) -> Query<()> {
    let module = one(program.modules.iter(), "actual presence module")?;
    let inspect_function = source_function(module, facts.inspect)?;
    let reader_function = source_function(module, facts.reader)?;
    let entry_function = source_function(module, facts.entry)?;
    require(
        entry_function.id == selected
            && module.functions.len() == 3
            && inspect_function.id != reader_function.id
            && inspect_function.id != selected
            && reader_function.id != selected,
        "lowered source entry and three distinct actual functions",
    )?;
    let nullable = one(
        inspect_function.return_types.iter().copied(),
        "inspect actual return type",
    )?;
    let Some(SsaTypeKind::NullableHandle { inner }) = module.type_kind(nullable) else {
        return Err("real inspect result is not NullableHandle".into());
    };
    let Some(SsaTypeKind::SharedOwner {
        payload: Some(payload),
        ..
    }) = module.type_kind(*inner)
    else {
        return Err("real nullable inner is not SharedOwner".into());
    };
    require(
        module.type_kind(*payload)
            == Some(&SsaTypeKind::Integer {
                bits: 32,
                signed: true,
            })
            && module.type_ownership(nullable) == Some(Ownership::MoveOnly)
            && module.type_ownership(*inner) == Some(Ownership::MoveOnly),
        "actual Rc<Int>? storage identity",
    )?;
    let boolean = one(
        reader_function.return_types.iter().copied(),
        "reader Boolean return",
    )?;
    require(
        module.type_kind(boolean) == Some(&SsaTypeKind::Boolean),
        "actual reader Boolean type",
    )?;
    inspect(inspect_function, nullable, *inner, boolean, case, facts)?;
    reader(reader_function, nullable, *inner, boolean, facts)?;
    entry(
        entry_function,
        inspect_function,
        reader_function,
        StorageTypes {
            nullable,
            inner: *inner,
            payload: *payload,
            boolean,
        },
        case,
        facts,
    )
}

#[test]
fn presence_rc_nullable_while_preserves_owner_cleanup_identity() {
    let mut observations = Vec::new();
    for (case, source, _) in super::presence_cases() {
        for unit in [false, true] {
            observations.push(super::observe_with_identity(case, source, unit, true));
        }
    }
    for (observation, identity) in &observations {
        println!(
            "SPEC0332 PRESENCE IDENTITY case={} unit={} query={identity:?} pipeline={}",
            observation.case,
            observation.unit,
            if observation.outcome.is_ok() {
                "ok"
            } else {
                "err"
            }
        );
        if let Err(error) = &observation.outcome {
            println!(
                "SPEC0332 PRESENCE IDENTITY PIPELINE ERROR case={} unit={} stage={} detail={} lower={:?} span_text={:?}",
                observation.case,
                observation.unit,
                error.stage,
                error.detail,
                error.lower,
                error.span_text
            );
        }
    }
    assert_eq!(observations.len(), 8);
    assert!(
        observations.iter().all(|(observation, identity)| {
            observation.complete_ok
                && observation.ownership_ok
                && observation.lower_attempted
                && observation.entry_identity_ok
                && observation.outcome.is_ok()
                && identity.is_ok()
        }),
        "all eight real presence sources independently preserve source/result owners and normal cleanup identities"
    );
}
