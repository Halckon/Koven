//! Source-origin boundaries: local construction/put chain, or real own-parameter
//! plus its two ordinary callers. SSA identities never cross function contexts.
use super::*;

pub(super) fn map_types(
    module: &Module,
    mutable: SsaTypeId,
    readonly: SsaTypeId,
    scalar: BuiltinType,
) -> Outcome<(SsaTypeId, SsaTypeId)> {
    let Some(SsaTypeKind::MapContainer {
        kind: MapContainerKind::MutableMap,
        key,
        value,
    }) = module.type_kind(mutable)
    else {
        return Err(missing("MutableMap storage", mutable));
    };
    gate(
        "consume identity: same concrete K/V on readonly result",
        module.type_kind(readonly)
            == Some(&SsaTypeKind::MapContainer {
                kind: MapContainerKind::Map,
                key: *key,
                value: *value,
            }),
        (mutable, readonly),
    )?;
    let expected = match scalar {
        BuiltinType::Int => SsaTypeKind::Integer {
            bits: 32,
            signed: true,
        },
        BuiltinType::String => SsaTypeKind::StringOwner,
        other => return Err(missing("C scalar domain", other)),
    };
    gate(
        "consume identity: exact frontend scalar K/V storage",
        module.type_kind(*key) == Some(&expected) && module.type_kind(*value) == Some(&expected),
        (module.type_kind(*key), module.type_kind(*value)),
    )?;
    Ok((*key, *value))
}

fn function(program: &Program, id: FunctionId) -> Outcome<(&Module, &Function)> {
    let candidates: Vec<_> = program
        .modules
        .iter()
        .flat_map(|module| {
            module
                .functions
                .iter()
                .filter(move |function| function.id == id)
                .map(move |function| (module, function))
        })
        .collect();
    one(&candidates, "full FunctionId lookup").copied()
}

fn by_origin(program: &Program, origin: Span) -> Outcome<(&Module, &Function)> {
    let candidates: Vec<_> = program
        .modules
        .iter()
        .flat_map(|module| {
            module
                .functions
                .iter()
                .filter(move |function| function.origin == Origin::Source(origin))
                .map(move |function| (module, function))
        })
        .collect();
    one(&candidates, "source-qualified caller callable").copied()
}

fn exact_uses(
    function: &Function,
    values: &BTreeSet<ValueId>,
    allowed: &BTreeSet<InstructionId>,
) -> Outcome<()> {
    for instruction in &function.instructions {
        let uses: Vec<_> = instruction
            .operation
            .entities()
            .into_iter()
            .filter(|entity| matches!(entity, EntityId::Value(value) if values.contains(value)))
            .collect();
        if !uses.is_empty() {
            gate(
                "consume identity: only exact source transfer instructions",
                allowed.contains(&instruction.id) && uses.len() == 1,
                instruction,
            )?;
        }
    }
    for block in &function.blocks {
        let term = block
            .terminator
            .as_ref()
            .ok_or_else(|| missing("source transfer terminator", block.id))?;
        let outside_edges = match &term.kind {
            TerminatorKind::Branch(_) => Vec::new(),
            TerminatorKind::Conditional { condition, .. } => vec![EntityId::Value(*condition)],
            TerminatorKind::NullableBranch { owner, .. } => vec![EntityId::Value(*owner)],
            _ => term.kind.entities(),
        };
        gate(
            "consume identity: source never tested or returned after transfer",
            !outside_edges
                .iter()
                .any(|entity| matches!(entity, EntityId::Value(value) if values.contains(value))),
            term,
        )?;
        for edge in graph::edges(block)? {
            let target = graph::target(function, edge)?;
            let mut seen = BTreeSet::new();
            for (column, entity) in edge.arguments.iter().enumerate() {
                let EntityId::Value(value) = entity else {
                    continue;
                };
                if !values.contains(value) {
                    continue;
                }
                gate(
                    "consume identity: exact source transport once and inside closed chain",
                    seen.insert(*value)
                        && matches!(target.parameters.get(column), Some(EntityId::Value(parameter)) if values.contains(parameter)),
                    (edge, column),
                )?;
            }
        }
    }
    Ok(())
}

fn mutable_chain(
    function: &Function,
    mutable: SsaTypeId,
    last: ValueId,
    construction: Span,
    expected_puts: &[Span],
    wrap: &Instruction,
) -> Outcome<()> {
    let construct = at(function, construction, |op| {
        matches!(op, Operation::MapConstruct { .. })
    })?;
    gate(
        "consume identity: original construction exact type",
        construct.operation == Operation::MapConstruct { map_type: mutable },
        construct,
    )?;
    let birth = value(function, construct)?;
    let mut dependencies = BTreeMap::<ValueId, Vec<ValueId>>::new();
    let mut pending = vec![last];
    let mut puts = BTreeSet::new();
    while let Some(current) = pending.pop() {
        if dependencies.contains_key(&current) {
            continue;
        }
        gate(
            "consume identity: finite construction lineage",
            dependencies.len() < 4096 && ty(function, current)? == mutable,
            current,
        )?;
        let inputs = if current == birth {
            Vec::new()
        } else {
            let data = function
                .entity(EntityId::Value(current))
                .ok_or_else(|| missing("construction lineage entity", current))?;
            match data.definition {
                Definition::BlockParameter { block, index } => {
                    graph::incoming(function, block, index)?
                        .into_iter()
                        .map(|(_, value)| value)
                        .collect()
                }
                Definition::InstructionResult {
                    instruction,
                    index: 0,
                } => {
                    let instruction = function
                        .instruction(instruction)
                        .ok_or_else(|| missing("put lineage instruction", instruction))?;
                    let Operation::MapPut { owner, .. } = instruction.operation else {
                        return Err(missing("foreign mutable source terminal", instruction));
                    };
                    gate(
                        "consume identity: exact source put owner/result",
                        expected_puts.contains(&instruction.origin.span())
                            && instruction.origin == Origin::Source(instruction.origin.span())
                            && value(function, instruction)? == current,
                        instruction,
                    )?;
                    puts.insert(instruction.id);
                    vec![owner]
                }
                _ => return Err(missing("unexpected mutable source definition", data)),
            }
        };
        pending.extend(inputs.iter().copied());
        dependencies.insert(current, inputs);
    }
    let mut closed = BTreeSet::from([birth]);
    loop {
        let previous = closed.len();
        for (&current, inputs) in &dependencies {
            if inputs.iter().any(|value| closed.contains(value)) {
                closed.insert(current);
            }
        }
        if previous == closed.len() {
            break;
        }
    }
    gate(
        "consume identity: construction SCC and complete actual put set",
        dependencies.keys().all(|value| closed.contains(value))
            && puts.len() == expected_puts.len()
            && expected_puts.iter().all(|origin| {
                puts.iter().any(|id| {
                    function
                        .instruction(*id)
                        .is_some_and(|instruction| instruction.origin == Origin::Source(*origin))
                })
            }),
        (&dependencies, &puts, expected_puts),
    )?;
    let aliases: BTreeSet<_> = dependencies.into_keys().collect();
    puts.insert(wrap.id);
    exact_uses(function, &aliases, &puts)
}

pub(super) struct SourceQuery<'a> {
    pub program: &'a Program,
    pub entry: FunctionId,
    pub module: &'a Module,
    pub consumer: &'a Function,
    pub header: &'a Block,
    pub owner: ValueId,
}

pub(super) fn check(query: SourceQuery<'_>, facts: &Facts, trace: &mut Trace) -> Outcome<()> {
    let SourceQuery {
        program,
        entry,
        module,
        consumer,
        header,
        owner,
    } = query;
    let nullable = ty(consumer, owner)?;
    let birth = match &facts.source {
        Source::Local { construction, puts } => {
            let wrap = at(consumer, facts.declaration, |op| {
                matches!(op, Operation::NullableWrap { .. })
            })?;
            let Operation::NullableWrap {
                nullable: actual,
                owner: mutable_owner,
            } = wrap.operation
            else {
                return Err(missing("source NullableWrap", wrap));
            };
            gate(
                "consume identity: source declaration registered wrapper",
                actual == nullable
                    && module.nullable_maps.get(&nullable) == Some(&ty(consumer, mutable_owner)?),
                wrap,
            )?;
            mutable_chain(
                consumer,
                ty(consumer, mutable_owner)?,
                mutable_owner,
                *construction,
                puts,
                wrap,
            )?;
            if entry != consumer.id {
                let (_, selected) = function(program, entry)?;
                let calls: Vec<_> = selected.instructions.iter().filter(|instruction| matches!(instruction.operation, Operation::DirectCall { callee, receiver: None, ref arguments } if callee == consumer.id && arguments.is_empty())).collect();
                let call = *one(&calls, "selected entry reaches actual consuming callable")?;
                gate(
                    "consume identity: real entry call source",
                    matches!(call.origin, Origin::Source(_)),
                    call,
                )?;
            }
            value(consumer, wrap)?
        }
        Source::Parameter {
            parameter,
            index,
            calls,
        } => {
            gate(
                "consume identity: input and local source belong to real consuming callable",
                contains(facts.callable, *parameter)
                    && contains(facts.callable, facts.root)
                    && contains(facts.callable, facts.declaration),
                facts,
            )?;
            let block = consumer
                .entry_block()
                .and_then(|id| consumer.block(id))
                .ok_or_else(|| missing("own input entry block", consumer.id))?;
            let Some(EntityId::Value(birth)) = block.parameters.get(*index).copied() else {
                return Err(missing("real own input parameter column", (block, index)));
            };
            gate(
                "consume identity: exact entry parameter definition/storage",
                consumer.entity(EntityId::Value(birth)).is_some_and(|data| {
                    data.definition
                        == Definition::BlockParameter {
                            block: block.id,
                            index: *index,
                        }
                }) && ty(consumer, birth)? == nullable,
                birth,
            )?;
            gate(
                "consume identity: present and absent caller observations",
                calls.len() == 2
                    && calls
                        .iter()
                        .filter(|call| matches!(call.initializer, CallerInit::Present { .. }))
                        .count()
                        == 1
                    && calls
                        .iter()
                        .filter(|call| matches!(call.initializer, CallerInit::Absent { .. }))
                        .count()
                        == 1,
                calls,
            )?;
            for call in calls {
                caller(program, entry, consumer.id, *index, call, facts.scalar)?;
            }
            birth
        }
    };
    let aliases = graph::lineage(consumer, owner, birth)?;
    graph::before_branch(consumer, &aliases, header.id, owner)?;
    trace.stages.push(format!("consume identity source lineage: consumer={:?}; exact birth={birth:?}; complete pre-branch aliases={aliases:?}", consumer.id));
    Ok(())
}

fn caller(
    program: &Program,
    entry: FunctionId,
    target: FunctionId,
    column: usize,
    facts: &CallSite,
    scalar: BuiltinType,
) -> Outcome<()> {
    let (module, function) = by_origin(program, facts.callable)?;
    gate(
        "consume identity: C3 caller is actual selected entry",
        function.id == entry && contains(facts.callable, facts.root),
        (function.id, entry, facts),
    )?;
    let call = at(function, facts.call, |op| {
        matches!(op, Operation::DirectCall { .. })
    })?;
    let Operation::DirectCall {
        callee,
        receiver: None,
        arguments,
    } = &call.operation
    else {
        return Err(missing("real own argument call", call));
    };
    let Some(EntityId::Value(argument)) = arguments.get(column) else {
        return Err(missing("real own argument column", call));
    };
    gate(
        "consume identity: full callee FunctionId and actual argument mapping",
        *callee == target && arguments.len() == 1,
        call,
    )?;
    let nullable = ty(function, *argument)?;
    let mutable = *module
        .nullable_maps
        .get(&nullable)
        .ok_or_else(|| missing("caller local registered N/M", nullable))?;
    // Caller storage is checked inside its own module; never compare bare type IDs
    // with the callee's module-local table.
    let Some(SsaTypeKind::MapContainer {
        kind: MapContainerKind::MutableMap,
        key,
        value: value_type,
    }) = module.type_kind(mutable)
    else {
        return Err(missing("caller mutable storage", mutable));
    };
    let component = match scalar {
        BuiltinType::Int => SsaTypeKind::Integer {
            bits: 32,
            signed: true,
        },
        BuiltinType::String => SsaTypeKind::StringOwner,
        _ => return Err(missing("caller scalar", scalar)),
    };
    gate(
        "consume identity: caller concrete pair and ownership",
        module.type_kind(*key) == Some(&component)
            && module.type_kind(*value_type) == Some(&component)
            && module.type_ownership(nullable) == Some(Ownership::MoveOnly),
        (nullable, mutable),
    )?;
    let birth = match facts.initializer {
        CallerInit::Present { construction } => {
            let wrap = at(function, facts.declaration, |op| {
                matches!(op, Operation::NullableWrap { .. })
            })?;
            let Operation::NullableWrap {
                nullable: actual,
                owner,
            } = wrap.operation
            else {
                return Err(missing("caller present wrap", wrap));
            };
            gate(
                "consume identity: caller present actual wrapper storage",
                actual == nullable,
                wrap,
            )?;
            mutable_chain(function, mutable, owner, construction, &[], wrap)?;
            value(function, wrap)?
        }
        CallerInit::Absent { null } => {
            let instruction = at(function, null, |op| {
                matches!(op, Operation::NullableNull { .. })
            })?;
            gate(
                "consume identity: caller actual null storage",
                instruction.operation == Operation::NullableNull { nullable },
                instruction,
            )?;
            value(function, instruction)?
        }
    };
    let aliases = graph::lineage(function, *argument, birth)?;
    exact_uses(function, &aliases, &BTreeSet::from([call.id]))?;
    // Every instruction-level use is counted as well as classified. In particular
    // two arguments or repeated calls cannot reuse a once-delivered caller owner.
    let uses = function
        .instructions
        .iter()
        .flat_map(|instruction| instruction.operation.entities())
        .filter(|entity| matches!(entity, EntityId::Value(value) if aliases.contains(value)))
        .count();
    gate(
        "consume identity: caller owner delivered exactly once without post-call Drop",
        uses == 1,
        (call, aliases, uses),
    )
}
