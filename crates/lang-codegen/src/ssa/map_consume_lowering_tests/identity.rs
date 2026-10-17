//! Read-only identity checks on the original ordinary-source Program and LLVM text.
//! No production consume proof, fact edits, IR edits, or additional lowering are used.
use std::collections::{BTreeMap, BTreeSet};

use lang_frontend::source::SourceMap;

use crate::ssa::model::{
    BlockId, Definition, Edge, EntityId, EntityType, Function, Instruction, InstructionId,
    MapContainerKind, Module, Operation, Ownership, Program, SsaTypeId, SsaTypeKind,
    TerminatorKind, ValueId,
};

pub(super) fn assert_identity(
    label: &str,
    text: &str,
    sources: &SourceMap,
    program: &Program,
    llvm: &str,
) {
    let consumes: Vec<_> = program
        .modules
        .iter()
        .flat_map(|module| {
            module.functions.iter().flat_map(move |function| {
                function.instructions.iter().filter_map(move |instruction| {
                    matches!(instruction.operation, Operation::MapConsume { .. }).then_some((
                        module,
                        function,
                        instruction,
                    ))
                })
            })
        })
        .collect();
    assert_eq!(consumes.len(), 1, "{label}: one actual MapConsume");
    let (module, function, consume) = consumes[0];
    let Operation::MapConsume { owner, map_type } = consume.operation else {
        unreachable!()
    };
    let result = one_value(consume);
    let source_type = value_type(function, owner);
    assert_ne!(
        owner, result,
        "{label}: distinct transferred owner identity"
    );
    assert_ne!(source_type, map_type);
    assert_eq!(value_type(function, result), map_type);
    let (key, value) = map_parts(module, source_type, MapContainerKind::MutableMap);
    assert_eq!(
        map_parts(module, map_type, MapContainerKind::Map),
        (key, value)
    );
    for ty in [source_type, map_type] {
        assert_eq!(module.type_ownership(ty), Some(Ownership::MoveOnly));
    }
    let component = if text.contains("mutableMapOf<String, String>()") {
        SsaTypeKind::StringOwner
    } else {
        SsaTypeKind::Integer {
            bits: 32,
            signed: true,
        }
    };
    assert_eq!(module.type_kind(key), Some(&component));
    assert_eq!(module.type_kind(value), Some(&component));
    let spelling = if text.contains("(source).consume()") {
        "(source).consume()"
    } else {
        "source.consume()"
    };
    assert_eq!(sources.slice(consume.origin.span()).unwrap(), spelling);

    let mut chain = SourceChain::default();
    trace_source(
        function,
        owner,
        source_type,
        sources,
        &mut chain,
        &mut BTreeSet::new(),
    );
    assert_eq!(
        chain.constructs.len(),
        1,
        "{label}: exact original construction"
    );
    assert_eq!(chain.puts.len(), text.matches("source.put(").count());
    for instruction in &function.instructions {
        if matches!(instruction.operation, Operation::MapConstruct { .. }) {
            assert!(chain.constructs.contains(&instruction.id));
        }
        if matches!(instruction.operation, Operation::MapPut { .. }) {
            assert!(chain.puts.contains(&instruction.id));
        }
        let uses = instruction.operation.entities();
        if uses.iter().any(|entity| chain.values.contains(entity)) {
            assert!(
                instruction.id == consume.id || chain.puts.contains(&instruction.id),
                "{label}: source chain has an unexpected use or Drop: {instruction:?}"
            );
            assert_eq!(
                uses.iter()
                    .filter(|entity| chain.values.contains(entity))
                    .count(),
                1
            );
        }
    }

    // No stale MutableMap parameter/result may hide outside the provenance walk.
    assert_eq!(entities_of_type(function, source_type), chain.values);
    let aliases = result_aliases(function, result, map_type);
    assert_eq!(entities_of_type(function, map_type), aliases);
    let drops: Vec<_> = function.instructions.iter().filter(|instruction| {
        matches!(instruction.operation, Operation::Drop { owner } if aliases.contains(&EntityId::Value(owner)))
    }).collect();
    assert_eq!(
        drops.len(),
        1,
        "{label}: one Drop of the actual readonly result"
    );
    let drop = drops[0];
    if !text.contains("val result =") {
        assert_eq!(drop.operation, Operation::Drop { owner: result });
        assert_eq!(drop.block, consume.block);
        let block = function.block(consume.block).unwrap();
        let position = block
            .instructions
            .iter()
            .position(|id| *id == consume.id)
            .unwrap();
        assert_eq!(block.instructions.get(position + 1), Some(&drop.id));
        assert_eq!(drop.origin.span(), consume.origin.span());
    }
    let block = function.block(consume.block).unwrap();
    let position = block
        .instructions
        .iter()
        .position(|id| *id == consume.id)
        .unwrap();
    check_paths(
        function,
        consume.block,
        position + 1,
        true,
        &chain.values,
        &aliases,
        &mut BTreeSet::new(),
    );
    assert_llvm(
        llvm,
        function,
        owner,
        result,
        source_type,
        map_type,
        drop,
        &aliases,
    );
    println!(
        "MAP CONSUME IDENTITY {label}: source={owner:?}, result={result:?}, puts={}, result aliases={}, unique Drop={:?}; six unchanged LLVM fields and pure transfer",
        chain.puts.len(),
        aliases.len(),
        drop.id
    );
}

fn one_value(instruction: &Instruction) -> ValueId {
    let [EntityId::Value(value)] = instruction.results.as_slice() else {
        panic!("one actual Value result: {instruction:?}")
    };
    *value
}

fn value_type(function: &Function, value: ValueId) -> SsaTypeId {
    let EntityType::Value(ty) = function.entity(EntityId::Value(value)).unwrap().ty else {
        panic!("owned Value storage")
    };
    ty
}

fn entities_of_type(function: &Function, ty: SsaTypeId) -> BTreeSet<EntityId> {
    function
        .blocks
        .iter()
        .flat_map(|block| &block.parameters)
        .chain(
            function
                .instructions
                .iter()
                .flat_map(|instruction| &instruction.results),
        )
        .copied()
        .filter(|entity| function.entity(*entity).unwrap().ty == EntityType::Value(ty))
        .collect()
}

fn map_parts(module: &Module, ty: SsaTypeId, expected: MapContainerKind) -> (SsaTypeId, SsaTypeId) {
    let Some(SsaTypeKind::MapContainer { kind, key, value }) = module.type_kind(ty) else {
        panic!("actual nonnullable Map storage")
    };
    assert_eq!(*kind, expected);
    (*key, *value)
}

fn edges(kind: &TerminatorKind) -> Vec<&Edge> {
    match kind {
        TerminatorKind::Branch(edge) => vec![edge],
        TerminatorKind::Conditional {
            when_true,
            when_false,
            ..
        } => vec![when_true, when_false],
        _ => Vec::new(),
    }
}

fn incoming(function: &Function, block: BlockId, slot: usize) -> Vec<EntityId> {
    function
        .blocks
        .iter()
        .flat_map(|predecessor| {
            edges(&predecessor.terminator.as_ref().unwrap().kind)
                .into_iter()
                .filter(move |edge| edge.target == block)
                .map(move |edge| edge.arguments[slot])
        })
        .collect()
}

#[derive(Default)]
struct SourceChain {
    values: BTreeSet<EntityId>,
    constructs: BTreeSet<InstructionId>,
    puts: BTreeSet<InstructionId>,
}

fn trace_source(
    function: &Function,
    value: ValueId,
    ty: SsaTypeId,
    sources: &SourceMap,
    chain: &mut SourceChain,
    visiting: &mut BTreeSet<ValueId>,
) {
    let entity = EntityId::Value(value);
    assert_eq!(value_type(function, value), ty);
    assert!(
        visiting.insert(value),
        "source provenance must reach a real constructor"
    );
    chain.values.insert(entity);
    match function.entity(entity).unwrap().definition {
        Definition::InstructionResult { instruction, index } => {
            let instruction = function.instruction(instruction).unwrap();
            assert_eq!(index, 0);
            assert_eq!(one_value(instruction), value);
            let source = sources.slice(instruction.origin.span()).unwrap();
            match instruction.operation {
                Operation::MapConstruct { map_type } => {
                    assert_eq!(map_type, ty);
                    assert!(matches!(
                        source,
                        "mutableMapOf<Int, Int>()" | "mutableMapOf<String, String>()"
                    ));
                    chain.constructs.insert(instruction.id);
                }
                Operation::MapPut { owner, .. } => {
                    assert!(source.starts_with("source.put("));
                    chain.puts.insert(instruction.id);
                    trace_source(function, owner, ty, sources, chain, visiting);
                }
                _ => {
                    panic!("source must be the original construction/update chain: {instruction:?}")
                }
            }
        }
        Definition::BlockParameter { block, index } => {
            assert_eq!(function.block(block).unwrap().parameters[index], entity);
            let inputs = incoming(function, block, index);
            assert!(!inputs.is_empty());
            for input in inputs {
                let EntityId::Value(input) = input else {
                    panic!("owned source edge")
                };
                trace_source(function, input, ty, sources, chain, visiting);
            }
        }
    }
    visiting.remove(&value);
}

fn result_aliases(function: &Function, result: ValueId, ty: SsaTypeId) -> BTreeSet<EntityId> {
    let mut aliases = BTreeSet::from([EntityId::Value(result)]);
    loop {
        let before = aliases.len();
        for block in &function.blocks {
            for edge in edges(&block.terminator.as_ref().unwrap().kind) {
                let target = function.block(edge.target).unwrap();
                assert_eq!(edge.arguments.len(), target.parameters.len());
                for (argument, parameter) in edge.arguments.iter().zip(&target.parameters) {
                    if aliases.contains(argument) {
                        assert_eq!(
                            function.entity(*parameter).unwrap().ty,
                            EntityType::Value(ty)
                        );
                        aliases.insert(*parameter);
                    }
                }
            }
        }
        if aliases.len() == before {
            break;
        }
    }
    for alias in &aliases {
        if let Definition::BlockParameter { block, index } =
            function.entity(*alias).unwrap().definition
        {
            let inputs = incoming(function, block, index);
            assert!(!inputs.is_empty());
            assert!(
                inputs.iter().all(|input| aliases.contains(input)),
                "readonly owner merge must not mix a different source"
            );
        }
    }
    aliases
}

// Follow every real successor from the consume position, including differing block layouts.
// The fixtures have no loops: a cycle is a changed fixture contract, not proof of cleanup.
fn check_paths(
    function: &Function,
    block: BlockId,
    start: usize,
    mut live: bool,
    sources: &BTreeSet<EntityId>,
    aliases: &BTreeSet<EntityId>,
    path: &mut BTreeSet<BlockId>,
) {
    assert!(
        path.insert(block),
        "unexpected cycle in the six ordinary consume sources"
    );
    let block = function.block(block).unwrap();
    for id in block.instructions.iter().skip(start) {
        let instruction = function.instruction(*id).unwrap();
        let uses = instruction.operation.entities();
        assert!(
            !uses.iter().any(|entity| sources.contains(entity)),
            "consumed MutableMap source reused: {instruction:?}"
        );
        if uses.iter().any(|entity| aliases.contains(entity)) {
            assert!(live, "readonly result reused after Drop: {instruction:?}");
            match instruction.operation {
                Operation::Drop { owner } => {
                    assert!(aliases.contains(&EntityId::Value(owner)));
                    live = false;
                }
                Operation::MapSize { owner } | Operation::MapGet { owner, .. } => {
                    assert!(aliases.contains(&owner));
                }
                _ => panic!("unexpected readonly result use: {instruction:?}"),
            }
        }
    }
    let terminator = &block.terminator.as_ref().unwrap().kind;
    assert!(
        !terminator
            .entities()
            .iter()
            .any(|entity| sources.contains(entity)),
        "source forwarded after consume"
    );
    match terminator {
        TerminatorKind::Branch(_) | TerminatorKind::Conditional { .. } => {
            for edge in edges(terminator) {
                assert_eq!(
                    edge.arguments
                        .iter()
                        .filter(|value| aliases.contains(value))
                        .count(),
                    usize::from(live),
                    "every live result follows one actual edge slot, and none after Drop"
                );
                check_paths(function, edge.target, 0, live, sources, aliases, path);
            }
        }
        TerminatorKind::Return { values } => {
            assert!(!live, "normal exit must have passed the unique result Drop");
            assert!(
                values
                    .iter()
                    .all(|value| !aliases.contains(&EntityId::Value(*value)))
            );
        }
        _ => panic!("unexpected terminal path in the six ordinary sources: {terminator:?}"),
    }
    path.remove(&block.id);
}

// These deliberately small text queries understand only the header representation used by
// this fixture. They compare values, not extract/insert instruction counts: LLVM may fold
// constant fields into a literal aggregate or use a previously inserted scalar directly.
struct LlvmBody<'a> {
    lines: Vec<&'a str>,
    definitions: BTreeMap<&'a str, (usize, &'a str)>,
}

impl<'a> LlvmBody<'a> {
    fn new(llvm: &'a str, function: &Function) -> Self {
        let symbol = format!("@f{}.{}(", function.id.index(), function.name);
        let starts: Vec<_> = llvm
            .lines()
            .enumerate()
            .filter(|(_, line)| line.starts_with("define ") && line.contains(&symbol))
            .map(|(index, _)| index)
            .collect();
        assert_eq!(
            starts.len(),
            1,
            "exact LLVM function for the real SSA function"
        );
        let lines: Vec<_> = llvm
            .lines()
            .skip(starts[0] + 1)
            .take_while(|line| *line != "}")
            .map(str::trim)
            .collect();
        let definitions = lines
            .iter()
            .enumerate()
            .filter_map(|(index, line)| {
                line.split_once(" = ")
                    .map(|(name, rhs)| (name, (index, rhs)))
            })
            .collect();
        Self { lines, definitions }
    }

    fn header_value(&self, value: ValueId, ty: &str) -> &str {
        let prefix = format!("%v{}.", value.index());
        let operation = format!("insertvalue {ty} ");
        self.definitions
            .iter()
            .filter(|(name, (_, rhs))| name.starts_with(&prefix) && rhs.starts_with(&operation))
            .max_by_key(|(_, (index, _))| *index)
            .map(|(name, _)| *name)
            .unwrap_or_else(|| panic!("original header for {value:?} must be observable"))
    }

    fn field(&self, aggregate: &str, field: usize, fields: &[&str], depth: usize) -> String {
        assert!(
            depth <= self.definitions.len() + 6,
            "cyclic LLVM header expression"
        );
        if aggregate == "zeroinitializer" {
            return format!(
                "{} {}",
                fields[field],
                if fields[field] == "ptr" { "null" } else { "0" }
            );
        }
        if let Some(literal) = aggregate
            .strip_prefix("{ ")
            .and_then(|value| value.strip_suffix(" }"))
        {
            let parts = comma_parts(literal);
            assert_eq!(parts.len(), 6);
            return self.scalar(parts[field], fields, depth + 1);
        }
        let (_, rhs) = self
            .definitions
            .get(aggregate)
            .unwrap_or_else(|| panic!("undefined aggregate {aggregate}"));
        let parts = comma_parts(
            rhs.strip_prefix("insertvalue ")
                .unwrap_or_else(|| panic!("unexpected header definition {rhs}")),
        );
        assert_eq!(parts.len(), 3);
        let index: usize = parts[2].parse().expect("header field index");
        if index == field {
            self.scalar(parts[1], fields, depth + 1)
        } else {
            self.field(typed_operand(parts[0]).1, field, fields, depth + 1)
        }
    }

    fn scalar(&self, operand: &str, fields: &[&str], depth: usize) -> String {
        let (_, value) = typed_operand(operand);
        if let Some((_, rhs)) = self.definitions.get(value)
            && let Some(extract) = rhs.strip_prefix("extractvalue ")
        {
            let parts = comma_parts(extract);
            assert_eq!(parts.len(), 2);
            let index: usize = parts[1].parse().expect("extracted field index");
            assert!(index < 6);
            return self.field(typed_operand(parts[0]).1, index, fields, depth + 1);
        }
        assert!(
            !matches!(value, "undef" | "poison"),
            "all six fields must be initialized"
        );
        operand.to_owned()
    }
}

fn typed_operand(text: &str) -> (&str, &str) {
    text.trim().split_once(' ').expect("typed LLVM operand")
}

fn comma_parts(text: &str) -> Vec<&str> {
    let mut depth = 0usize;
    let mut start = 0;
    let mut parts = Vec::new();
    for (index, byte) in text.bytes().enumerate() {
        match byte {
            b'{' | b'[' | b'(' | b'<' => depth += 1,
            b'}' | b']' | b')' | b'>' => {
                depth = depth.checked_sub(1).expect("balanced LLVM operand")
            }
            b',' if depth == 0 => {
                parts.push(text[start..index].trim());
                start = index + 1;
            }
            _ => {}
        }
    }
    assert_eq!(depth, 0);
    parts.push(text[start..].trim());
    parts
}

#[allow(clippy::too_many_arguments)]
fn assert_llvm(
    llvm: &str,
    function: &Function,
    source: ValueId,
    result: ValueId,
    source_type: SsaTypeId,
    result_type: SsaTypeId,
    drop: &Instruction,
    aliases: &BTreeSet<EntityId>,
) {
    let source_ty = format!("%koven.map.t{}", source_type.index());
    let result_ty = format!("%koven.map.t{}", result_type.index());
    let headers: Vec<_> = [&source_ty, &result_ty]
        .into_iter()
        .map(|ty| {
            let prefix = format!("{ty} = type {{ ");
            let matching: Vec<_> = llvm
                .lines()
                .filter_map(|line| {
                    line.strip_prefix(&prefix)
                        .and_then(|line| line.strip_suffix(" }"))
                })
                .collect();
            assert_eq!(matching.len(), 1);
            comma_parts(matching[0])
        })
        .collect();
    assert_eq!(headers[0], headers[1]);
    assert_eq!(headers[0].len(), 6);
    assert_eq!(headers[0][0], "ptr");
    assert!(matches!(headers[0][1], "i32" | "i64"));
    assert!(headers[0][1..].iter().all(|field| *field == headers[0][1]));
    let body = LlvmBody::new(llvm, function);
    let source_header = body.header_value(source, &source_ty);
    let result_header = body.header_value(result, &result_ty);
    for field in 0..6 {
        assert_eq!(
            body.field(source_header, field, &headers[0], 0),
            body.field(result_header, field, &headers[1], 0),
            "readonly header field {field} must preserve the exact source value (field 0 is the same allocation)"
        );
    }
    let drop_prefix = format!(
        "call void @koven.drop.t{}({result_ty} ",
        result_type.index()
    );
    let drops: Vec<_> = body
        .lines
        .iter()
        .filter_map(|line| {
            line.strip_prefix(&drop_prefix)
                .and_then(|line| line.strip_suffix(')'))
        })
        .collect();
    assert_eq!(drops.len(), 1, "exact LLVM readonly Map destructor call");
    assert!(
        !body
            .lines
            .iter()
            .any(|line| line.contains(&format!("@koven.drop.t{}(", source_type.index()))),
        "no MutableMap destructor after ownership transfer"
    );
    let Operation::Drop { owner: dropped } = drop.operation else {
        unreachable!()
    };
    check_llvm_alias(
        &body,
        function,
        dropped,
        drops[0],
        result,
        result_header,
        &result_ty,
        aliases,
        &mut BTreeSet::new(),
    );

    let start = body.definitions[source_header].0;
    let end = body.definitions[result_header].0;
    assert!(start < end);
    let first_read = body
        .lines
        .iter()
        .enumerate()
        .skip(end + 1)
        .find(|(_, line)| llvm_names(line).contains(&result_header))
        .map(|(index, _)| index)
        .expect("real result must be used");
    for line in &body.lines[start + 1..first_read] {
        if line.is_empty() || line.starts_with(';') {
            continue;
        }
        let (_, rhs) = line
            .split_once(" = ")
            .expect("transfer must not branch, call, store, allocate or free");
        assert!(
            rhs.starts_with("extractvalue ") || rhs.starts_with("insertvalue "),
            "conversion must be a pure header transfer, without hashing/reinsertion/entry Drop: {line}"
        );
    }
}

fn llvm_names(line: &str) -> Vec<&str> {
    line.split(|character: char| {
        !(character.is_ascii_alphanumeric() || matches!(character, '%' | '.' | '_' | '-'))
    })
    .filter(|word| word.starts_with('%'))
    .collect()
}

// LLVM can split a source SSA block while implementing put/get. Match the phi's value
// inputs to the actual SSA predecessor parameter column, without guessing LLVM labels.
#[allow(clippy::too_many_arguments)]
fn check_llvm_alias(
    body: &LlvmBody<'_>,
    function: &Function,
    alias: ValueId,
    actual: &str,
    result: ValueId,
    result_header: &str,
    result_ty: &str,
    aliases: &BTreeSet<EntityId>,
    visiting: &mut BTreeSet<ValueId>,
) {
    assert!(visiting.insert(alias));
    if alias == result {
        assert_eq!(actual, result_header);
    } else {
        assert!(aliases.contains(&EntityId::Value(alias)));
        assert_eq!(actual, format!("%v{}", alias.index()));
        let Definition::BlockParameter { block, index } =
            function.entity(EntityId::Value(alias)).unwrap().definition
        else {
            panic!("result alias must be an actual parameter")
        };
        let inputs = incoming(function, block, index);
        let (_, rhs) = body.definitions[actual];
        let prefix = format!("phi {result_ty} ");
        let parts = comma_parts(rhs.strip_prefix(&prefix).expect("actual result phi"));
        let actual_inputs: Vec<_> = parts
            .iter()
            .map(|part| {
                let pair = comma_parts(
                    part.trim()
                        .strip_prefix('[')
                        .and_then(|part| part.strip_suffix(']'))
                        .expect("phi incoming pair"),
                );
                assert_eq!(pair.len(), 2);
                pair[0]
            })
            .collect();
        assert_eq!(inputs.len(), actual_inputs.len());
        let mut expected_inputs = Vec::new();
        for input in inputs {
            let EntityId::Value(input) = input else {
                panic!("owned result input")
            };
            let spelling = if input == result {
                result_header.to_owned()
            } else {
                format!("%v{}", input.index())
            };
            check_llvm_alias(
                body,
                function,
                input,
                &spelling,
                result,
                result_header,
                result_ty,
                aliases,
                visiting,
            );
            expected_inputs.push(spelling);
        }
        expected_inputs.sort();
        let mut actual_inputs = actual_inputs;
        actual_inputs.sort();
        assert_eq!(actual_inputs, expected_inputs);
    }
    visiting.remove(&alias);
}
