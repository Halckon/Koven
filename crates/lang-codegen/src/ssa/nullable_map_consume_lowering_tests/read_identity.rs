//! Independent read-only owner/view/cleanup identity queries over ordinary B sources.
//! Source facts and SSA come from the same pipeline; no fabricated proof or ID fixtures.

use super::*;
use crate::ssa::model::{
    Block, Definition, Edge, EntityId, EntityType, Function, Instruction, LoanKind,
    MapContainerKind, Module, Operation, Origin, Ownership, SsaTypeKind, TerminatorKind, ValueId,
};
use lang_frontend::{
    ast::StatementId,
    parser::{Item, NameMarker, Statement},
};

mod single;
mod unit;

pub(super) use single::inspect as inspect_single;
pub(super) use unit::inspect as inspect_unit;

#[derive(Debug)]
struct ReadFacts {
    root: Span,
    declaration: Span,
    construction: Span,
    control: Span,
    read: Span,
    cleanup: Cleanup,
}

#[derive(Clone, Copy, Debug)]
enum Cleanup {
    InBranches,
    AfterJoin { kept: Span },
}

fn missing(label: &str, detail: impl std::fmt::Debug) -> Failure {
    Failure {
        stage: format!("read identity: {label}"),
        detail: format!("{detail:#?}"),
    }
}

fn one<'a, T: std::fmt::Debug>(values: &'a [T], label: &str) -> Outcome<&'a T> {
    match values {
        [value] => Ok(value),
        _ => Err(missing(label, values)),
    }
}

fn contains(outer: Span, inner: Span) -> bool {
    outer.source_id() == inner.source_id()
        && outer.start() <= inner.start()
        && inner.end() <= outer.end()
}

fn expression_span(parsed: &ParsedFile, expression: ExpressionId) -> Outcome<Span> {
    parsed
        .ast()
        .expressions()
        .get(expression)
        .map(|node| node.span())
        .map_err(|error| missing("source expression", error))
}

fn transparent(parsed: &ParsedFile, mut expression: ExpressionId) -> Outcome<ExpressionId> {
    loop {
        let node = parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|error| missing("source group", error))?;
        match node.payload() {
            Expression::Group { expression: inner } => expression = *inner,
            _ => return Ok(expression),
        }
    }
}

fn declaration(parsed: &ParsedFile, root: Span) -> Outcome<(StatementId, Span, ExpressionId)> {
    let matches: Vec<_> = parsed
        .ast()
        .statements()
        .iter()
        .filter_map(|(id, node)| {
            let Statement::LocalVariable { declaration } = node.payload() else {
                return None;
            };
            let item = parsed.ast().items().get(*declaration).ok()?;
            match item.payload() {
                Item::Variable {
                    name: NameMarker::Present(name),
                    initializer,
                    ..
                } if *name == root => Some((id, node.span(), *initializer)),
                _ => None,
            }
        })
        .collect();
    one(&matches, "unique real local declaration").copied()
}

fn control(
    parsed: &ParsedFile,
    comparison: ExpressionId,
    read: Span,
    non_null_when_true: bool,
) -> Outcome<(ExpressionId, Span)> {
    let mut matches = Vec::new();
    for (id, node) in parsed.ast().expressions().iter() {
        let Expression::If {
            condition,
            then_branch,
            else_branch,
            ..
        } = node.payload()
        else {
            continue;
        };
        if transparent(parsed, *condition)? != comparison {
            continue;
        }
        let body = if non_null_when_true {
            Some(*then_branch)
        } else {
            *else_branch
        };
        let body = body.and_then(|id| parsed.ast().statements().get(id).ok());
        if body.is_some_and(|body| contains(body.span(), read)) {
            matches.push((id, node.span()));
        }
    }
    one(&matches, "descriptor-polarity real If body").copied()
}

fn instructions<'a>(function: &'a Function, block: &Block) -> Outcome<Vec<&'a Instruction>> {
    block
        .instructions
        .iter()
        .map(|id| {
            function
                .instruction(*id)
                .ok_or_else(|| missing("block instruction identity", id))
        })
        .collect()
}

fn outgoing(block: &Block) -> Outcome<&Edge> {
    match block.terminator.as_ref().map(|value| &value.kind) {
        Some(TerminatorKind::Branch(edge)) => Ok(edge),
        other => Err(missing("live ordinary branch exit", other)),
    }
}

fn value_type(function: &Function, owner: ValueId) -> Outcome<crate::ssa::model::SsaTypeId> {
    match function
        .entity(EntityId::Value(owner))
        .map(|value| value.ty)
    {
        Some(EntityType::Value(ty)) => Ok(ty),
        other => Err(missing("real owner type", other)),
    }
}

// Before proof birth, follow every real predecessor of an owner parameter.
// A same-typed alternative is never accepted as the source declaration's owner.
fn trace_owner(
    function: &Function,
    current: ValueId,
    birth: ValueId,
    active: &mut Vec<ValueId>,
    aliases: &mut Vec<ValueId>,
    transports: &mut Vec<(crate::ssa::model::BlockId, ValueId)>,
) -> Outcome<()> {
    if !aliases.contains(&current) {
        aliases.push(current);
    }
    if current == birth {
        return Ok(());
    }
    if active.contains(&current) {
        return Err(missing(
            "cyclic owner lineage outside these sources",
            current,
        ));
    }
    active.push(current);
    let Some(data) = function.entity(EntityId::Value(current)) else {
        return Err(missing("owner lineage entity", current));
    };
    let Definition::BlockParameter { block, index } = data.definition else {
        return Err(missing("owner lineage must end at exact declaration", data));
    };
    let mut incoming = Vec::new();
    for predecessor in &function.blocks {
        let Some(terminator) = &predecessor.terminator else {
            continue;
        };
        let edges: Vec<_> = match &terminator.kind {
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
        for edge in edges.into_iter().filter(|edge| edge.target == block) {
            let Some(EntityId::Value(owner)) = edge.arguments.get(index).copied() else {
                return Err(missing("owner lineage incoming column", edge));
            };
            incoming.push(owner);
            transports.push((predecessor.id, owner));
        }
    }
    gate(
        "read identity: all owner predecessors exist",
        !incoming.is_empty(),
        current,
    )?;
    for owner in incoming {
        trace_owner(function, owner, birth, active, aliases, transports)?;
    }
    active.pop();
    Ok(())
}

fn check(program: &Program, facts: ReadFacts, trace: &mut Trace) -> Outcome<()> {
    let candidates: Vec<_> = program
        .modules
        .iter()
        .flat_map(|module| {
            module
                .functions
                .iter()
                .filter(|function| {
                    function.instructions.iter().any(|instruction| {
                        instruction.origin == Origin::Source(facts.read)
                            && matches!(instruction.operation, Operation::MapSize { .. })
                    })
                })
                .map(move |function| (module, function))
        })
        .collect();
    let &(module, function): &(&Module, &Function) =
        one(&candidates, "source-qualified read function")?;
    let reads: Vec<_> = function
        .instructions
        .iter()
        .filter(|instruction| matches!(instruction.operation, Operation::MapSize { .. }))
        .collect();
    let read = *one(&reads, "unique real MapSize")?;
    gate(
        "read identity: exact descriptor source",
        read.origin == Origin::Source(facts.read),
        read,
    )?;
    let births: Vec<_> = function
        .blocks
        .iter()
        .filter_map(|block| {
            let term = block.terminator.as_ref()?;
            (term.origin == Origin::Source(facts.control)
                && matches!(term.kind, TerminatorKind::NullableBranch { .. }))
            .then_some((block, term))
        })
        .collect();
    let &(header, terminator) = one(&births, "real If nullable branch")?;
    let TerminatorKind::NullableBranch {
        owner,
        when_null,
        when_non_null,
        view,
    } = &terminator.kind
    else {
        return Err(missing("nullable branch", terminator));
    };
    let wraps: Vec<_> = function
        .instructions
        .iter()
        .filter(|instruction| {
            instruction.origin == Origin::Source(facts.declaration)
                && matches!(instruction.operation, Operation::NullableWrap { .. })
        })
        .collect();
    let wrap = *one(&wraps, "source declaration nullable owner")?;
    let [EntityId::Value(birth_owner)] = wrap.results.as_slice() else {
        return Err(missing("source declaration owner result", wrap));
    };
    let mut owner_aliases = Vec::new();
    let mut transports = Vec::new();
    trace_owner(
        function,
        *owner,
        *birth_owner,
        &mut Vec::new(),
        &mut owner_aliases,
        &mut transports,
    )?;
    let nullable = value_type(function, *owner)?;
    gate(
        "read identity: declaration and transported owner storage",
        value_type(function, *birth_owner)? == nullable,
        (&owner_aliases, wrap, header),
    )?;
    let inner = module
        .nullable_maps
        .get(&nullable)
        .copied()
        .ok_or_else(|| missing("registered N to M identity", nullable))?;
    gate(
        "read identity: registered nullable Map storage",
        matches!(
            module.type_kind(nullable),
            Some(SsaTypeKind::Aggregate {
                ownership: Ownership::MoveOnly,
                ..
            })
        ) && matches!(
            module.type_kind(inner),
            Some(SsaTypeKind::MapContainer { .. })
        ) && function.entity(EntityId::Loan(*view)).map(|value| value.ty)
            == Some(EntityType::Loan {
                kind: LoanKind::Shared,
                target: inner,
            }),
        (nullable, inner, view),
    )?;
    let Operation::NullableWrap {
        nullable: wrapped_type,
        owner: inner_owner,
    } = wrap.operation
    else {
        return Err(missing("original inner owner wrap", wrap));
    };
    let inner_data = function
        .entity(EntityId::Value(inner_owner))
        .ok_or_else(|| missing("wrapped inner result", inner_owner))?;
    let Definition::InstructionResult {
        instruction: construct_id,
        index: 0,
    } = inner_data.definition
    else {
        return Err(missing("actual MapConstruct result definition", inner_data));
    };
    let construct = function
        .instruction(construct_id)
        .ok_or_else(|| missing("actual construction instruction", construct_id))?;
    gate(
        "read identity: exact FE construction result feeds N wrapper",
        wrapped_type == nullable
            && inner_data.ty == EntityType::Value(inner)
            && matches!(construct.operation, Operation::MapConstruct { map_type } if map_type == inner)
            && construct.results.as_slice() == [EntityId::Value(inner_owner)]
            && construct.origin == Origin::Source(facts.construction)
            && matches!(module.type_kind(inner), Some(SsaTypeKind::MapContainer {
                kind: MapContainerKind::MutableMap, key, value,
            }) if key == value && matches!(module.type_kind(*key),
                Some(SsaTypeKind::Integer { bits: 32, signed: true }))),
        (construct, wrap, inner_data, module.type_kind(inner)),
    )?;
    let columns: Vec<_> = when_non_null
        .arguments
        .iter()
        .enumerate()
        .filter_map(|(index, entity)| (*entity == EntityId::Value(*owner)).then_some(index))
        .collect();
    let column = *one(&columns, "unique root owner edge column")?;
    let present = function
        .block(when_non_null.target)
        .ok_or_else(|| missing("nonnull block", when_non_null))?;
    let absent = function
        .block(when_null.target)
        .ok_or_else(|| missing("null block", when_null))?;
    let Some(EntityId::Value(present_owner)) = present.parameters.get(column).copied() else {
        return Err(missing("nonnull current owner column", present));
    };
    let Some(EntityId::Value(absent_owner)) = absent.parameters.get(column).copied() else {
        return Err(missing("null current owner column", absent));
    };
    gate(
        "read identity: exact two owner successors and nonnull-only view",
        present.id != absent.id
            && when_null.arguments.get(column) == Some(&EntityId::Value(*owner))
            && present.parameters.len() == when_non_null.arguments.len() + 1
            && present.parameters.last() == Some(&EntityId::Loan(*view))
            && absent.parameters.len() == when_null.arguments.len()
            && absent
                .parameters
                .iter()
                .all(|entity| !matches!(entity, EntityId::Loan(_)))
            && value_type(function, present_owner)? == nullable
            && value_type(function, absent_owner)? == nullable
            && read.block == present.id
            && matches!(read.operation, Operation::MapSize { owner: EntityId::Loan(loan) } if loan == *view),
        (present, absent, read),
    )?;
    let present_instructions = instructions(function, present)?;
    let absent_instructions = instructions(function, absent)?;
    let ends: Vec<_> = function.instructions.iter().filter(|instruction|
        matches!(instruction.operation, Operation::BorrowEnd { loan } if loan == *view)).collect();
    let end = *one(&ends, "one End for actual Map view")?;
    let read_position = present.instructions.iter().position(|id| *id == read.id);
    let end_position = present.instructions.iter().position(|id| *id == end.id);
    gate(
        "read identity: read before current-view End",
        matches!((read_position, end_position), (Some(read), Some(end)) if read < end)
            && absent_instructions.iter().all(|instruction| {
                !matches!(
                    instruction.operation,
                    Operation::MapSize { .. } | Operation::BorrowEnd { .. }
                )
            }),
        (read, end, &absent_instructions),
    )?;
    let present_exit = outgoing(present)?;
    let absent_exit = outgoing(absent)?;
    gate(
        "read identity: same merge",
        present_exit.target == absent_exit.target,
        (present_exit, absent_exit),
    )?;
    let merge = function
        .block(present_exit.target)
        .ok_or_else(|| missing("merge", present_exit))?;
    gate(
        "read identity: no ended view at merge",
        merge
            .parameters
            .iter()
            .all(|entity| !matches!(entity, EntityId::Loan(_)))
            && !present_exit.arguments.contains(&EntityId::Loan(*view))
            && !absent_exit.arguments.contains(&EntityId::Loan(*view)),
        merge,
    )?;
    owner_aliases.extend([present_owner, absent_owner]);
    let drops: Vec<_> = function
        .instructions
        .iter()
        .filter(|instruction| match instruction.operation {
            Operation::Drop { owner } => value_type(function, owner).ok() == Some(nullable),
            _ => false,
        })
        .collect();
    match facts.cleanup {
        Cleanup::InBranches => {
            let present_drops: Vec<_> = drops.iter().copied().filter(|instruction|
                matches!(instruction.operation, Operation::Drop { owner } if owner == present_owner)).collect();
            let absent_drops: Vec<_> = drops.iter().copied().filter(|instruction|
                matches!(instruction.operation, Operation::Drop { owner } if owner == absent_owner)).collect();
            let present_drop = *one(&present_drops, "one original nonnull NamedDrop")?;
            let absent_drop = *one(&absent_drops, "one original null NamedDrop")?;
            let drop_position = present
                .instructions
                .iter()
                .position(|id| *id == present_drop.id);
            gate(
                "read identity: original CallReturn End then Drop and null BranchExit Drop",
                drops.len() == 2
                    && present_drop.block == present.id
                    && absent_drop.block == absent.id
                    && matches!((end_position, drop_position), (Some(end), Some(drop)) if end < drop)
                    && [end, present_drop, absent_drop]
                        .iter()
                        .all(|instruction| instruction.origin == Origin::Source(facts.root))
                    && present_exit.arguments.is_empty()
                    && absent_exit.arguments.is_empty()
                    && merge.parameters.is_empty(),
                (present_drop, absent_drop, merge),
            )?;
        }
        Cleanup::AfterJoin { kept } => {
            let columns: Vec<_> = present_exit
                .arguments
                .iter()
                .enumerate()
                .filter_map(|(index, entity)| {
                    (*entity == EntityId::Value(present_owner)).then_some(index)
                })
                .collect();
            let column = *one(&columns, "live owner merge column")?;
            let Some(EntityId::Value(merged_owner)) = merge.parameters.get(column).copied() else {
                return Err(missing("current joined owner", merge));
            };
            owner_aliases.push(merged_owner);
            let drop = *one(&drops, "one kept owner Drop")?;
            gate(
                "read identity: If only End; exact owner column survives until kept Drop",
                end.origin == Origin::Source(facts.control)
                    && present_instructions
                        .iter()
                        .chain(&absent_instructions)
                        .all(|instruction| {
                            !matches!(instruction.operation, Operation::Drop { .. })
                        })
                    && absent_exit.arguments.get(column) == Some(&EntityId::Value(absent_owner))
                    && value_type(function, merged_owner)? == nullable
                    && drop.block == merge.id
                    && matches!(drop.operation, Operation::Drop { owner } if owner == merged_owner)
                    && drop.origin == Origin::Source(kept),
                (present_exit, absent_exit, merge, drop),
            )?;
        }
    }
    for instruction in &function.instructions {
        for entity in instruction.operation.entities() {
            let allowed = match entity {
                EntityId::Value(value) if value == inner_owner => instruction.id == wrap.id,
                EntityId::Value(value) if owner_aliases.contains(&value) => {
                    matches!(instruction.operation, Operation::Drop { owner } if owner == value)
                        && drops.iter().any(|drop| drop.id == instruction.id)
                }
                EntityId::Loan(loan) if loan == *view => {
                    instruction.id == read.id || instruction.id == end.id
                }
                _ => true,
            };
            gate(
                "read identity: closed owner/view instruction uses",
                allowed,
                instruction,
            )?;
        }
    }
    for block in &function.blocks {
        let Some(terminator) = &block.terminator else {
            continue;
        };
        for entity in terminator.kind.entities() {
            let allowed = match entity {
                EntityId::Value(value) if value == inner_owner => false,
                EntityId::Value(value) if owner_aliases.contains(&value) => {
                    (block.id == header.id && value == *owner)
                        || (block.id == present.id
                            && value == present_owner
                            && matches!(facts.cleanup, Cleanup::AfterJoin { .. }))
                        || (block.id == absent.id
                            && value == absent_owner
                            && matches!(facts.cleanup, Cleanup::AfterJoin { .. }))
                        || transports.contains(&(block.id, value))
                }
                EntityId::Loan(loan) if loan == *view => false,
                _ => true,
            };
            gate(
                "read identity: closed owner/view terminator uses",
                allowed,
                terminator,
            )?;
        }
    }
    gate(
        "read identity: no ownership-consuming Map operation",
        function.instructions.iter().all(|instruction| {
            !matches!(
                instruction.operation,
                Operation::NullableTake { .. }
                    | Operation::MapResultUnwrap { .. }
                    | Operation::MapConsume { .. }
            )
        }),
        &function.instructions,
    )?;
    trace.stages.push(format!("read identity verified: facts={facts:#?}; function={:?}; owner={owner:?}; nullable={nullable:?}; inner={inner:?}; column={column}; current_owner={present_owner:?}; view={view:?}; read={:?}; end={:?}; merge={:?}", function.id, read.id, end.id, merge.id));
    Ok(())
}

#[test]
fn nullable_map_read_owner_view_cleanup_identity() {
    let cases = [&CASES[3], &CASES[4], &super::read_cross_join::CROSS_JOIN];
    let mut observations = Vec::new();
    for case in cases {
        for unit in [false, true] {
            let mut trace = Trace::default();
            let outcome = if unit {
                super::unit::observe_read_identity(case, &mut trace)
            } else {
                super::single::observe_read_identity(case, &mut trace)
            };
            observations.push(Observation {
                label: format!(
                    "{}/{}:read identity",
                    case.id,
                    if unit { "unit" } else { "single" }
                ),
                trace,
                outcome,
            });
        }
    }
    for observation in &observations {
        println!(
            "NULLABLE MAP IDENTITY {}\n{}\ninput failures={:?}\nquery outcome={:#?}",
            observation.label,
            observation.trace.stages.join("\n"),
            observation.trace.input_failures,
            observation.outcome
        );
    }
    assert_eq!(observations.len(), 6);
    assert!(
        observations
            .iter()
            .all(|observation| observation.outcome.is_ok()
                && observation.trace.input_failures.is_empty()),
        "all six ordinary observations must pass source facts, verified LLVM and read identity"
    );
}
