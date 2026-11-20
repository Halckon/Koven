//! Keep exact resource instance lineage in the Unit SSA and native code, never in diagnostic spans.
use super::{
    model::{FunctionId, Operation, Program, TerminatorKind},
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
        assert_lexical_exchange(&sources, &program, entry);
        let llvm = crate::llvm::render_verified_program_with_entry(&program, entry)
            .unwrap()
            .replace("@malloc(", "@recorded_malloc(")
            .replace("@free(", "@recorded_free(");
        let counter = r#"
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
        link_and_run(&llvm, counter, b"");
    }
}

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
