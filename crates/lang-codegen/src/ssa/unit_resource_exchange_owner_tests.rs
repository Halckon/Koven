//! Keep exact resource instance lineage in the Unit SSA and native code, never in diagnostic spans.
use super::{
    model::{
        AbortReason, EntityId, FunctionId, Instruction, Operation, Program, Terminator,
        TerminatorKind, ValueId,
    },
    unit_field_replace_tests::link_and_run,
    unit_lower::lower_scalar_unit_with_entry,
    unit_lower_test_support::{analyze, parsed},
};
use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};

#[test]
fn unit_lexical_resources_keep_exact_replaced_and_swapped_owners() {
    let text = include_str!(
        "../../../lang-frontend/tests/ownership_resource_deinit/fixtures/root-resource-exchange.ko"
    );
    for_each_run(text, |sources, program, entry| {
        assert_lexical_exchange(sources, program, entry);
        let (instructions, _) = straight_line_run(program, entry);
        let constructor_origins = allocations(sources, &instructions)
            .into_iter()
            .map(|(_, origin)| origin)
            .collect::<Vec<_>>();
        assert_eq!(
            constructor_origins,
            ["Resource(1)", "Resource(2)", "Resource(3)"]
        );
        let llvm = crate::llvm::render_verified_program_with_entry(program, entry)
            .unwrap()
            .replace("@malloc(", "@recorded_malloc(")
            .replace("@free(", "@recorded_free(");
        let counter = COUNTER;
        link_and_run(&llvm, counter, b"");
    });
}

fn for_each_run(text: &str, mut inspect: impl FnMut(&SourceMap, &Program, FunctionId)) {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(&mut sources, "resource-drop.ko", text);
    let (name_environment, environment) = standard_environments();
    for input in [
        SourceUnitInput::memory_document(source, &file),
        SourceUnitInput::new("root", "resource-drop.ko", source, &file),
    ] {
        let inputs = [input];
        let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &environment);
        let entry = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|candidate| candidate.name() == "run")
            .expect("original function")
            .id();
        let (program, entry) = lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &environment,
            &typed,
            &owned,
            entry,
        )
        .expect("exact original resource fixture lowers through Unit");
        crate::ssa::verify::verify_program(&program).unwrap();
        inspect(&sources, &program, entry);
    }
}

const COUNTER: &str = r#"
#include <assert.h>
#include <stdlib.h>
static void *live[3];
static int allocated, released;
void *recorded_malloc(size_t bytes) {
    assert(bytes && allocated < 3);
    void *p = malloc(bytes);
    assert(p);
    for (int i = 0; i < allocated; ++i) assert(live[i] != p);
    live[allocated++] = p;
    return p;
}
void recorded_free(void *p) {
    static const int expected[] = {0, 2, 1};
    assert(p && released < 3);
    int id = expected[released++];
    assert(live[id] == p);
    live[id] = NULL;
    free(p);
}
__attribute__((destructor)) static void verify(void) {
    assert(allocated == 3 && released == 3);
}
"#;

fn assert_lexical_exchange(sources: &SourceMap, program: &Program, entry: FunctionId) {
    let function = program.modules[entry.module().index()]
        .function(entry)
        .unwrap();
    let [block] = function.blocks.as_slice() else {
        panic!("original straight-line run must have one block");
    };
    assert!(matches!(
        block.terminator.as_ref().unwrap().kind,
        TerminatorKind::Return { .. }
    ));
    let mut actions = Vec::new();
    for instruction in &block.instructions {
        let instruction = &function.instructions[instruction.index()];
        match instruction.operation {
            Operation::RootReplace { .. } => actions.push("replace"),
            Operation::RootSwap { .. } => actions.push("swap"),
            Operation::DirectCall { callee, .. } => {
                let callee = program.modules[callee.module().index()]
                    .function(callee)
                    .unwrap();
                if sources.slice(callee.origin.span()).unwrap() == "fun tick(): Unit {}" {
                    actions.push("tick");
                }
            }
            Operation::Drop { .. } => actions.push("drop"),
            _ => {}
        }
    }
    assert_eq!(actions, ["replace", "swap", "tick", "drop", "drop", "drop"]);
}

#[test]
fn unit_abandoned_calls_use_exact_return_owners_and_never_unwind_abort() {
    for (template, field) in [
        (
            include_str!(
                "../../../lang-frontend/tests/ownership_resource_deinit/fixtures/abandoned-root-resource-exchange.ko.in"
            ),
            false,
        ),
        (
            include_str!(
                "../../../lang-frontend/tests/ownership_resource_deinit/fixtures/abandoned-field-resource-exchange.ko.in"
            ),
            true,
        ),
    ] {
        for exit in ["return", "error(\"stop\")"] {
            let text = template.replace("$EXIT", exit);
            for_each_run(&text, |sources, program, entry| {
                let (instructions, terminator) = straight_line_run(program, entry);
                assert!(
                    !instructions.iter().any(|instruction| matches!(
                        instruction.operation,
                        Operation::DirectCall { .. }
                    )),
                    "abandoned outer accept must not dispatch"
                );
                let (commit_index, old, live) = if field {
                    let (index, original_parent, replacement, exchanged) =
                        field_exchange(&instructions);
                    let allocations = allocations(sources, &instructions);
                    let [
                        (original_field, _),
                        (parent, "Holder(Guard())"),
                        (incoming, _),
                    ] = allocations.as_slice()
                    else {
                        panic!("three distinct allocations: old field, original parent, incoming");
                    };
                    assert_eq!((original_parent, replacement), (*parent, *incoming));
                    assert_ne!(exchanged, *original_field);
                    (index, exchanged, *parent)
                } else {
                    let matches = instructions
                        .iter()
                        .enumerate()
                        .filter(|(_, i)| matches!(i.operation, Operation::RootReplace { .. }))
                        .collect::<Vec<_>>();
                    let [(index, exchange)] = matches.as_slice() else {
                        panic!("one nested root commit before abandoned outer call");
                    };
                    let Operation::RootReplace {
                        owner, replacement, ..
                    } = exchange.operation
                    else {
                        unreachable!()
                    };
                    let allocations = allocations(sources, &instructions);
                    let [(initial, "Guard()"), (incoming, "Guard()")] = allocations.as_slice()
                    else {
                        panic!("two distinct constructor occurrences");
                    };
                    assert_eq!((owner, replacement), (*initial, *incoming));
                    let [EntityId::Value(live), EntityId::Value(old)] = exchange.results.as_slice()
                    else {
                        panic!("new root and detached old owner");
                    };
                    (*index, *old, *live)
                };
                assert_ne!(old, live);
                let drops = drops(&instructions);
                if exit == "return" {
                    let [(old_index, old_drop), (live_index, live_drop)] = drops.as_slice() else {
                        panic!("exactly two distinct cleanup owners");
                    };
                    assert!(commit_index < *old_index && old_index < live_index);
                    assert_eq!((*old_drop, *live_drop), (old, live));
                    assert_eq!(sources.slice(terminator.origin.span()), Ok("return"));
                    assert!(
                        matches!(&terminator.kind, TerminatorKind::Return { values } if values.is_empty())
                    );
                    assert!(terminator.abort.is_none());
                } else {
                    assert!(drops.is_empty(), "Abort does not unwind either owner");
                    assert_eq!(sources.slice(terminator.origin.span()), Ok(exit));
                    assert_eq!(terminator.kind, TerminatorKind::Abort);
                    assert!(matches!(terminator.abort, Some(AbortReason::Error { .. })));
                }
            });
        }
    }
}

#[test]
fn unit_field_exchange_drops_detached_field_then_sibling_and_original_parent() {
    let template = include_str!(
        "../../../lang-frontend/tests/ownership_resource_deinit/fixtures/lexical-field-resource-exchange.ko.in"
    );
    for binding in ["val", "var"] {
        for new in ["Guard()", "incoming"] {
            let incoming = if new == "incoming" {
                "val incoming = Guard(); "
            } else {
                ""
            };
            let text = template
                .replace("$BINDING", binding)
                .replace("$INCOMING", incoming)
                .replace("$NEW", new);
            for_each_run(&text, |sources, program, entry| {
                let (instructions, terminator) = straight_line_run(program, entry);
                let allocations = allocations(sources, &instructions);
                let [
                    (_, "Guard()"),
                    (parent, "Holder(Guard())"),
                    (sibling, "Guard()"),
                    (incoming, "Guard()"),
                ] = allocations.as_slice()
                else {
                    panic!("old field and parent, sibling, then incoming construction");
                };
                let (commit, actual_parent, actual_replacement, old) =
                    field_exchange(&instructions);
                assert_eq!((actual_parent, actual_replacement), (*parent, *incoming));
                assert_ne!(old, *parent);
                let calls = instructions
                    .iter()
                    .enumerate()
                    .filter(|(_, instruction)| {
                        matches!(instruction.operation, Operation::DirectCall { .. })
                    })
                    .map(|(index, instruction)| {
                        (index, sources.slice(instruction.origin.span()).unwrap())
                    })
                    .collect::<Vec<_>>();
                let [
                    (inspect_field, "inspect(holder.state)"),
                    (inspect_old, "inspect(old)"),
                    (tick, "tick()"),
                ] = calls.as_slice()
                else {
                    panic!("both views and tick remain reachable after commit: {calls:?}");
                };
                let drops = drops(&instructions);
                let [(drop_old, old_owner), (_, sibling_owner), (_, parent_owner)] =
                    drops.as_slice()
                else {
                    panic!("three lexical owner identities");
                };
                assert!(
                    commit < *inspect_field
                        && inspect_field < inspect_old
                        && inspect_old < tick
                        && tick < drop_old
                );
                assert_eq!(
                    [*old_owner, *sibling_owner, *parent_owner],
                    [old, *sibling, *parent]
                );
                assert!(
                    matches!(&terminator.kind, TerminatorKind::Return { values } if values.is_empty())
                );
            });
        }
    }
}

fn straight_line_run(program: &Program, entry: FunctionId) -> (Vec<&Instruction>, &Terminator) {
    let function = program.modules[entry.module().index()]
        .function(entry)
        .unwrap();
    let [block] = function.blocks.as_slice() else {
        panic!("exact original fixture has only one executable block");
    };
    (
        block
            .instructions
            .iter()
            .map(|id| &function.instructions[id.index()])
            .collect(),
        block.terminator.as_ref().unwrap(),
    )
}

fn field_exchange(instructions: &[&Instruction]) -> (usize, ValueId, ValueId, ValueId) {
    let commits = instructions
        .iter()
        .enumerate()
        .filter_map(|(index, instruction)| {
            let Operation::HeapFieldExchange {
                owner,
                field,
                replacement,
                ..
            } = instruction.operation
            else {
                return None;
            };
            assert_eq!(field, 0, "Holder.state is the only projection");
            let [EntityId::Value(old)] = instruction.results.as_slice() else {
                panic!("field commit yields the moved old field and preserves the parent");
            };
            Some((index, owner, replacement, *old))
        })
        .collect::<Vec<_>>();
    let [commit] = commits.as_slice() else {
        panic!("one exact field exchange");
    };
    *commit
}

fn drops(instructions: &[&Instruction]) -> Vec<(usize, ValueId)> {
    instructions
        .iter()
        .enumerate()
        .filter_map(|(index, instruction)| match instruction.operation {
            Operation::Drop { owner } => Some((index, owner)),
            _ => None,
        })
        .collect()
}

fn allocations<'a>(
    sources: &'a SourceMap,
    instructions: &[&Instruction],
) -> Vec<(ValueId, &'a str)> {
    instructions
        .iter()
        .filter_map(|instruction| {
            if !matches!(instruction.operation, Operation::HeapAllocate { .. }) {
                return None;
            }
            let [EntityId::Value(value)] = instruction.results.as_slice() else {
                panic!("single fresh allocation");
            };
            Some((*value, sources.slice(instruction.origin.span()).unwrap()))
        })
        .collect()
}
