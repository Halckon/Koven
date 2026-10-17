//! 返回的真实 entry ID、源码 Origin 与从 entry block 出发的 CFG/DirectCall 可达身份。
use super::fixture::{Exit, Failure, Fixture, failure, location, loop_site, need};
use crate::ssa::model::{EntityId, EntityType, FunctionId, Operation, Origin, Program};
use lang_frontend::{
    parser::{Item, NameMarker},
    source::{SourceMap, Span},
};

pub(super) fn source_functions(fixture: &Fixture) -> Result<Vec<(&'static str, Span)>, Failure> {
    let parsed = &fixture.parsed;
    let root_count = if fixture.exit == Exit::Return { 6 } else { 5 };
    if parsed.roots().len() != root_count {
        return Err(failure(
            "source function identity query",
            (
                "expected Guard plus actual top-level function roots",
                parsed.roots().len(),
            ),
        ));
    }
    let factory = if fixture.map {
        "makeMap"
    } else {
        "makeMutable"
    };
    let mut functions = Vec::new();
    for expected in ["entry", "scan", "read", factory]
        .into_iter()
        .chain((fixture.exit == Exit::Return).then_some("operand"))
    {
        let mut spans = Vec::new();
        for root in parsed.roots() {
            let node = parsed
                .ast()
                .items()
                .get(*root)
                .map_err(|error| failure("source function identity query", error))?;
            if let Item::Function {
                name: NameMarker::Present(name),
                ..
            } = node.payload()
                && fixture.sources.slice(*name).ok() == Some(expected)
            {
                if node.span().source_id() != parsed.source_id()
                    || fixture.sources.source_name(node.span().source_id()) != Ok(fixture.path)
                {
                    return Err(failure(
                        "source function identity query",
                        location(&fixture.sources, node.span()),
                    ));
                }
                spans.push(node.span());
            }
        }
        let [span] = spans.as_slice() else {
            return Err(failure("source function identity query", (expected, spans)));
        };
        functions.push((expected, *span));
    }
    let guards = parsed.roots().iter().filter_map(|root| {
        let node = parsed.ast().items().get(*root).ok()?;
        let Item::Classifier(class) = node.payload() else { return None };
        matches!(class.name, NameMarker::Present(name) if fixture.sources.slice(name) == Ok("Guard")).then_some((node, class))
    }).collect::<Vec<_>>();
    let [(node, guard)] = guards.as_slice() else {
        return Err(failure("AST unique actual Guard", guards));
    };
    let body = guard
        .body
        .as_ref()
        .ok_or_else(|| failure("AST Guard body", node))?;
    let [deinit] = body.members.as_slice() else {
        return Err(failure("AST sole Guard deinit member", body));
    };
    let deinit = parsed
        .ast()
        .items()
        .get(*deinit)
        .map_err(|e| failure("AST Guard deinit", e))?;
    need(
        matches!(deinit.payload(), Item::Deinit { .. })
            && fixture.sources.slice(deinit.span()) == Ok("deinit() { println(\"local-drop\") }"),
        "AST real local-drop deinit body",
        deinit,
    )?;
    functions.push(("Guard.deinit", deinit.span()));
    Ok(functions)
}

pub(super) fn entry_sources(
    sources: &SourceMap,
    program: &Program,
    entry: FunctionId,
    expected: &[(&str, Span)],
) -> Result<(), Failure> {
    let [module] = program.modules.as_slice() else {
        return Err(failure(
            "SSA source identity query",
            "expected one actual source module",
        ));
    };
    let mut required = Vec::new();
    for (name, span) in expected {
        let matches = module
            .functions
            .iter()
            .filter(|function| matches!(&function.origin, Origin::Source(actual) if actual == span))
            .collect::<Vec<_>>();
        let [function] = matches.as_slice() else {
            return Err(failure(
                "SSA source identity query",
                (name, location(sources, *span), matches.len()),
            ));
        };
        println!(
            "SPEC0331 PROVIDER EXIT FUNCTION source_name={name} id={:?} lowered_name={:?} {}",
            function.id,
            function.name,
            location(sources, *span)
        );
        if *name == "entry" && function.id != entry {
            return Err(failure(
                "SSA entry identity query",
                (
                    "lower returned a different source function",
                    entry,
                    function.id,
                ),
            ));
        }
        required.push(function.id);
    }
    let mut pending = vec![entry];
    let mut reached = Vec::new();
    while let Some(id) = pending.pop() {
        if reached.contains(&id) {
            continue;
        }
        let function = module.function(id).ok_or_else(|| {
            failure(
                "SSA entry call graph query",
                ("missing actual function", id),
            )
        })?;
        reached.push(id);
        let entry_block = function
            .entry_block()
            .ok_or_else(|| failure("SSA entry call graph query", ("missing entry block", id)))?;
        let mut blocks = vec![entry_block];
        let mut reached_blocks = Vec::new();
        while let Some(block_id) = blocks.pop() {
            if reached_blocks.contains(&block_id) {
                continue;
            }
            let block = function.block(block_id).ok_or_else(|| {
                failure(
                    "SSA entry call graph query",
                    ("missing actual block", block_id),
                )
            })?;
            reached_blocks.push(block_id);
            // 只读当前可达 block 引用的 instructions，不扫描 function instruction arena。
            for instruction_id in &block.instructions {
                let instruction = function.instruction(*instruction_id).ok_or_else(|| {
                    failure(
                        "SSA entry call graph query",
                        ("missing referenced instruction", instruction_id),
                    )
                })?;
                if let Operation::DirectCall { callee, .. } = &instruction.operation {
                    println!(
                        "SPEC0331 PROVIDER EXIT CALL caller={id:?} block={block_id:?} callee={callee:?} origin={:?}",
                        instruction.origin
                    );
                    pending.push(*callee);
                }
                // Drop invokes the registered deinit implicitly; this is a real call graph edge.
                if let Operation::Drop { owner } = instruction.operation {
                    let entity = function
                        .entity(EntityId::Value(owner))
                        .ok_or_else(|| failure("SSA Drop owner entity", owner))?;
                    if let EntityType::Value(ty) = entity.ty
                        && let Some(deinit) = module.deinits.get(&ty)
                    {
                        println!(
                            "SPEC0331 EXIT DEINIT EDGE caller={id:?} block={block_id:?} owner={owner:?} type={ty:?} deinit={deinit:?} origin={:?}",
                            instruction.origin
                        );
                        pending.push(*deinit);
                    }
                }
            }
            let terminator = block.terminator.as_ref().ok_or_else(|| {
                failure(
                    "SSA entry call graph query",
                    ("missing actual terminator", block_id),
                )
            })?;
            blocks.extend(terminator.kind.targets());
        }
    }
    if !required.iter().all(|id| reached.contains(id)) {
        return Err(failure("SSA entry call graph query", (required, reached)));
    }
    Ok(())
}

/// No loop-carrier shape is guessed here. Inspect actual reachable construction, Drop and operand.
pub(super) fn guard_and_operand(
    f: &Fixture,
    program: &Program,
    expected: &[(&str, Span)],
) -> Result<(), Failure> {
    let site = loop_site(f)?;
    let [module] = program.modules.as_slice() else {
        return Err(failure("SSA exit entity query", "expected one module"));
    };
    let function = |name: &str| -> Result<_, Failure> {
        let span = expected
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, s)| *s)
            .ok_or_else(|| failure("SSA expected source function", name))?;
        let found = module
            .functions
            .iter()
            .filter(|f| matches!(f.origin, Origin::Source(actual) if actual == span))
            .collect::<Vec<_>>();
        let [function] = found.as_slice() else {
            return Err(failure("SSA unique source function", (name, found.len())));
        };
        Ok(*function)
    };
    let scan = function("scan")?;
    let deinit = function("Guard.deinit")?;
    let consumer = function(f.exit.callee())?;
    let mut pending = vec![
        scan.entry_block()
            .ok_or_else(|| failure("SSA scan entry block", scan.id))?,
    ];
    let mut blocks = Vec::new();
    let mut reachable = Vec::new();
    while let Some(id) = pending.pop() {
        if blocks.contains(&id) {
            continue;
        }
        let block = scan
            .block(id)
            .ok_or_else(|| failure("SSA actual reachable block", id))?;
        blocks.push(id);
        for instruction in &block.instructions {
            reachable.push(
                scan.instruction(*instruction)
                    .ok_or_else(|| failure("SSA reachable instruction", instruction))?,
            );
        }
        let terminator = block
            .terminator
            .as_ref()
            .ok_or_else(|| failure("SSA reachable terminator", id))?;
        pending.extend(terminator.kind.targets());
    }
    let allocations = reachable
        .iter()
        .filter_map(|i| match i.operation {
            Operation::HeapAllocate { owner, .. }
                if i.origin.span() == site.guard_span
                    && module.deinits.get(&owner) == Some(&deinit.id) =>
            {
                Some((*i, owner))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(allocation, guard_type)] = allocations.as_slice() else {
        return Err(failure(
            "SSA actual Guard allocation and registered deinit",
            allocations,
        ));
    };
    let [EntityId::Value(guard_owner)] = allocation.results.as_slice() else {
        return Err(failure(
            "SSA Guard allocation unique actual Value result",
            &allocation.results,
        ));
    };
    let guard_drops = reachable
        .iter()
        .filter(|i| {
            let Operation::Drop { owner } = i.operation else {
                return false;
            };
            scan.entity(EntityId::Value(owner))
                .is_some_and(|e| e.ty == EntityType::Value(*guard_type))
        })
        .collect::<Vec<_>>();
    need(
        matches!(guard_drops.as_slice(), [drop] if matches!(drop.operation, Operation::Drop { owner } if owner == *guard_owner)),
        "SSA unique Guard Drop consumes exact allocation result",
        &guard_drops,
    )?;
    let calls = reachable
        .iter()
        .filter(|i| {
            matches!(&i.operation, Operation::DirectCall { callee, .. } if *callee == consumer.id)
                && i.origin.span() == site.call_span
        })
        .collect::<Vec<_>>();
    let [call] = calls.as_slice() else {
        return Err(failure("SSA reachable actual component call", calls));
    };
    println!(
        "SPEC0331 EXIT ACTUAL ENTITIES scan={:?} allocation={allocation:#?} guard_type={guard_type:?} drops={guard_drops:#?} body_call={call:#?}",
        scan.id
    );
    if f.exit == Exit::Return {
        use crate::ssa::model::{Definition, TerminatorKind as T};
        let [EntityId::Value(result)] = call.results.as_slice() else {
            return Err(failure("SSA operand actual scalar result", &call.results));
        };
        let returned = blocks
            .iter()
            .filter_map(|id| scan.block(*id))
            .filter_map(|b| b.terminator.as_ref())
            .filter(|t| t.origin.span() == site.transfer_span)
            .filter_map(|t| {
                if let T::Return { values } = &t.kind {
                    Some((t, values))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        let [(terminator, values)] = returned.as_slice() else {
            return Err(failure("SSA unique actual transfer Return", returned));
        };
        let [returned_value] = values.as_slice() else {
            return Err(failure(
                "SSA actual transfer unique returned Value",
                terminator,
            ));
        };
        // A has no body CFG: require the operand result itself or a real backwards Copy chain.
        // A block parameter is not accepted from a single matching incoming; no phi colouring.
        let mut cursor = *returned_value;
        let mut chain = Vec::new();
        while cursor != *result {
            need(
                !chain.contains(&cursor),
                "SSA return Copy chain is rooted, not cyclic",
                &chain,
            )?;
            chain.push(cursor);
            let entity = scan
                .entity(EntityId::Value(cursor))
                .ok_or_else(|| failure("SSA return Copy actual entity", cursor))?;
            let Definition::InstructionResult { instruction, index } = entity.definition else {
                return Err(failure(
                    "SSA A return requires direct result/Copy, no unproved phi transport",
                    (cursor, entity),
                ));
            };
            let copy = scan
                .instruction(instruction)
                .ok_or_else(|| failure("SSA return Copy actual instruction", instruction))?;
            need(
                index == 0
                    && copy.results.as_slice() == [EntityId::Value(cursor)]
                    && reachable.iter().any(|i| i.id == instruction),
                "SSA actual reachable Copy result definition",
                copy,
            )?;
            let Operation::Copy { source } = copy.operation else {
                return Err(failure(
                    "SSA returned value must be actual operand result or Copy",
                    copy,
                ));
            };
            cursor = source;
        }
        println!(
            "SPEC0331 EXIT RETURN ROOT operand_result={result:?} returned={returned_value:?} backwards_copies={chain:?} terminator={terminator:#?}"
        );
    }
    Ok(())
}
