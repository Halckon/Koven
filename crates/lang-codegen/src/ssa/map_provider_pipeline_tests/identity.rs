//! 返回的真实 entry ID、源码 Origin 与从 entry block 出发的 CFG/DirectCall 可达身份。
use super::fixture::{Failure, Fixture, failure, location};
use crate::ssa::model::{FunctionId, Operation, Origin, Program};
use lang_frontend::{
    parser::{Item, NameMarker},
    source::{SourceMap, Span},
};

pub(super) fn source_functions(fixture: &Fixture) -> Result<Vec<(&'static str, Span)>, Failure> {
    let parsed = &fixture.parsed;
    if parsed.roots().len() != 4 {
        return Err(failure(
            "source function identity query",
            ("expected four function roots", parsed.roots().len()),
        ));
    }
    let factory = if fixture.map {
        "makeMap"
    } else {
        "makeMutable"
    };
    let mut functions = Vec::new();
    for expected in ["entry", "scan", "read", factory] {
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
            "SPEC0331 PROVIDER FUNCTION source_name={name} id={:?} lowered_name={:?} {}",
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
                        "SPEC0331 PROVIDER CALL caller={id:?} block={block_id:?} callee={callee:?} origin={:?}",
                        instruction.origin
                    );
                    pending.push(*callee);
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
