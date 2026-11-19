//! Original conditional primitive sources: actual SSA exchange and cleanup paths.
use super::super::model::{Definition, EntityId, TerminatorKind, ValueId};
use super::*;

#[test]
fn conditional_root_cleanup_preserves_exchange_identity_on_both_paths() {
    for (case, text) in [
        "fun run(flag: Boolean): Unit {\nvar a = \"old\"\nval old = replace(&a, if (flag) \"new\" else \"other\")\n}",
        "fun run(flag: Boolean): Unit {\nvar a = if (flag) \"first\" else \"second\"\nvar b = \"other\"\nswap(&a, &b)\n}",
    ].into_iter().enumerate() {
        let mut sources = SourceMap::new();
        let (source, file) = parsed(&mut sources, "primitive.ko", text);
        for input in [
            SourceUnitInput::new("root", "primitive.ko", source, &file),
            SourceUnitInput::memory_document(source, &file),
        ] {
            let inputs = [input];
            let (environment, types) = standard_environments();
            let (names, typed, owned) = analyze(&sources, &inputs, &environment, &types);
            assert_eq!(owned.ownership().drops().len(), 2);
            let entry = names.names().index().declarations().iter()
                .find(|declaration| declaration.name() == "run").unwrap().id();
            let result = lower_scalar_unit_with_entry(&sources, &inputs, &names, &types, &typed, &owned, entry);
            let (program, entry) = result.expect("original legal source lowers");
            super::super::verify::verify_program(&program).unwrap();
            assert_cleanup(program.modules[0].function(entry).unwrap(), case);
        }
    }
}

fn literals(function: &Function, value: ValueId, depth: usize) -> Vec<Vec<u8>> {
    assert!(
        depth <= function.blocks.len(),
        "bounded acyclic original source"
    );
    match function.values[value.index()].definition {
        Definition::InstructionResult { instruction, .. } => {
            match &function.instructions[instruction.index()].operation {
                Operation::StringLiteral { bytes, .. } => vec![bytes.clone()],
                other => panic!("expected original literal producer, got {other:?}"),
            }
        }
        Definition::BlockParameter { block, index } => {
            let mut result = Vec::new();
            for predecessor in &function.blocks {
                let edges = match &predecessor.terminator.as_ref().unwrap().kind {
                    TerminatorKind::Branch(edge) => vec![edge],
                    TerminatorKind::Conditional {
                        when_true,
                        when_false,
                        ..
                    } => vec![when_true, when_false],
                    TerminatorKind::Return { .. } => vec![],
                    other => panic!("unexpected original source terminator: {other:?}"),
                };
                for edge in edges.into_iter().filter(|edge| edge.target == block) {
                    let EntityId::Value(incoming) = edge.arguments[index] else {
                        panic!("String value");
                    };
                    result.extend(literals(function, incoming, depth + 1));
                }
            }
            result.sort();
            result.dedup();
            result
        }
    }
}

fn assert_cleanup(function: &Function, case: usize) {
    let commits = function
        .instructions
        .iter()
        .filter(|i| {
            matches!(
                i.operation,
                Operation::RootReplace { .. } | Operation::RootSwap { .. }
            )
        })
        .collect::<Vec<_>>();
    let [commit] = commits.as_slice() else {
        panic!("one original exchange");
    };
    let expect = |value, bytes: &[&[u8]]| {
        let mut expected = bytes.iter().map(|bytes| bytes.to_vec()).collect::<Vec<_>>();
        expected.sort();
        assert_eq!(literals(function, value, 0), expected);
    };
    match (&commit.operation, case) {
        (
            Operation::RootReplace {
                owner, replacement, ..
            },
            0,
        ) => {
            expect(*owner, &[b"old"]);
            expect(*replacement, &[b"new", b"other"]);
        }
        (Operation::RootSwap { owners, .. }, 1) => {
            expect(owners[0], &[b"first", b"second"]);
            expect(owners[1], &[b"other"]);
        }
        other => panic!("original primitive kind: {other:?}"),
    }
    let [EntityId::Value(first), EntityId::Value(second)] = commit.results.as_slice() else {
        panic!("two post-exchange owner identities");
    };
    assert_ne!(first, second);
    let drops = function
        .instructions
        .iter()
        .filter_map(|i| match i.operation {
            Operation::Drop { owner } => Some(owner),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(drops.len(), 2);
    assert!(drops.contains(first) && drops.contains(second));
    // Follow both real edges, not instruction-arena order: no drop precedes the
    // commit and each normal path consumes both post-exchange identities once.
    let mut pending = vec![(function.blocks[0].id, Vec::new(), 0usize)];
    let mut paths = 0;
    while let Some((block, mut trace, depth)) = pending.pop() {
        assert!(depth < function.blocks.len(), "no loop in original sources");
        let block = &function.blocks[block.index()];
        for id in &block.instructions {
            if *id == commit.id {
                trace.push(0);
            }
            match function.instructions[id.index()].operation {
                Operation::Drop { owner } if owner == *first => trace.push(1),
                Operation::Drop { owner } if owner == *second => trace.push(2),
                _ => {}
            }
        }
        match &block.terminator.as_ref().unwrap().kind {
            TerminatorKind::Branch(edge) => pending.push((edge.target, trace, depth + 1)),
            TerminatorKind::Conditional {
                when_true,
                when_false,
                ..
            } => {
                pending.push((when_true.target, trace.clone(), depth + 1));
                pending.push((when_false.target, trace, depth + 1));
            }
            TerminatorKind::Return { .. } => {
                assert_eq!(trace.first(), Some(&0));
                assert_eq!(trace.len(), 3);
                trace.sort();
                assert_eq!(trace, [0, 1, 2]);
                paths += 1;
            }
            other => panic!("unexpected original source terminator: {other:?}"),
        }
    }
    assert_eq!(paths, 2);
}
