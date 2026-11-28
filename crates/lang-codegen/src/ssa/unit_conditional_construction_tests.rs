//! Check the actual Unit CFG and allocation identities for the unchanged Single source.
use lang_frontend::{
    name_resolution::{SourceUnitInput, SourceUnitKey},
    source::SourceMap,
    type_checking::standard_environments,
};

use super::{
    model::{
        Block, Definition, Edge, EntityId, Function, Instruction, LoanKind, Operation, PlaceAccess,
        Program, ScalarConstant, TerminatorKind, ValueId,
    },
    unit_lower::lower_scalar_unit_with_entry,
    unit_lower_test_support::{analyze, parsed},
};
use crate::native_tests::boxed_enum_tests::{assert_success, run_counted_allocations_in_order};

const ORIGINAL: &str = include_str!(
    "../../../lang-frontend/tests/ownership_construction/fixtures/conditional-argument-return.ko"
);

#[test]
fn unit_conditional_construction_releases_selected_argument_once() {
    each_native_case(ORIGINAL, |program, hold, llvm, _, early| {
        assert_exact_holder_flow(program, hold);
        // A literal String has static storage; only normal completion allocates the Holder.
        let order: &[usize] = if early { &[] } else { &[0] };
        assert_success(&run_counted_allocations_in_order(llvm, order), b"");
    });
}

#[test]
fn unit_conditional_construction_frees_only_the_selected_heap_string() {
    // Separate runtime-strengthened control; this is not the original Single source.
    let cloned = ORIGINAL
        .replace("\"a\"", "\"a\".clone()")
        .replace("\"b\"", "\"b\".clone()");
    each_native_case(&cloned, |_, _, llvm, select, early| {
        let selected = if select { 'a' } else { 'b' };
        let count = if early { 1 } else { 2 };
        let counter = format!(
            "#define EXPECTED_COUNT {count}\n#define EXPECTED_BYTE '{selected}'\n{COUNTER}"
        );
        super::unit_field_replace_tests::link_and_run(
            &llvm
                .replace("@malloc(", "@recorded_malloc(")
                .replace("@free(", "@recorded_free("),
            &counter,
            b"",
        );
    });
}

fn each_native_case(
    original: &str,
    mut inspect: impl FnMut(&Program, &Function, &str, bool, bool),
) {
    for rooted in [false, true] {
        for select in [false, true] {
            for early in [false, true] {
                let text = format!("{original}\nfun entry(): Unit {{ hold({select}, {early}) }}");
                let mut sources = SourceMap::new();
                let (source, file) = parsed(&mut sources, "conditional-construction.ko", &text);
                assert_eq!(
                    &sources.source_text(source).unwrap()[..original.len()],
                    original
                );
                let input = if rooted {
                    SourceUnitInput::new("root", "conditional-construction.ko", source, &file)
                } else {
                    SourceUnitInput::memory_document(source, &file)
                };
                let inputs = [input];
                let (name_environment, environment) = standard_environments();
                let (names, typed, owned) =
                    analyze(&sources, &inputs, &name_environment, &environment);
                let index = names.names().index();
                let [source_entry] = index.source_units() else {
                    panic!("one actual source");
                };
                assert_eq!(source_entry.source_id(), source);
                assert_eq!(
                    matches!(source_entry.key(), SourceUnitKey::MemoryDocument),
                    !rooted
                );
                let selected_entry = index
                    .declarations()
                    .iter()
                    .find(|decl| decl.name() == "entry")
                    .unwrap()
                    .id();
                let (program, entry) = lower_scalar_unit_with_entry(
                    &sources,
                    &inputs,
                    &names,
                    &environment,
                    &typed,
                    &owned,
                    selected_entry,
                )
                .unwrap_or_else(|error| {
                    panic!("rooted={rooted}, select={select}, early={early}: {error:?}")
                });
                crate::ssa::verify::verify_program(&program).unwrap();
                let wrapper = program.modules[entry.module().index()]
                    .function(entry)
                    .unwrap();
                let hold_id = actual_wrapper_call(wrapper, [select, early]);
                let hold = program.modules[hold_id.module().index()]
                    .function(hold_id)
                    .unwrap();
                assert!(
                    sources
                        .slice(hold.origin.span())
                        .unwrap()
                        .starts_with("fun hold(")
                );
                let llvm =
                    crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
                inspect(&program, hold, &llvm, select, early);
            }
        }
    }
}

fn actual_wrapper_call(wrapper: &Function, expected: [bool; 2]) -> super::model::FunctionId {
    let mut calls = wrapper.instructions.iter().filter_map(|ins| {
        let Operation::DirectCall {
            callee,
            arguments,
            receiver,
        } = &ins.operation
        else {
            return None;
        };
        assert!(receiver.is_none());
        Some((*callee, arguments))
    });
    let (callee, arguments) = calls.next().expect("wrapper actually invokes hold");
    assert!(calls.next().is_none());
    assert_eq!(arguments.len(), expected.len());
    let operation = |id| {
        let Definition::InstructionResult { instruction, .. } =
            wrapper.entity(id).unwrap().definition
        else {
            panic!("wrapper must form its own Boolean literal arguments");
        };
        &wrapper.instructions[instruction.index()].operation
    };
    for (argument, selected) in arguments.iter().zip(expected) {
        let Operation::BorrowBegin {
            place,
            kind: LoanKind::Shared,
        } = operation(*argument)
        else {
            panic!("hold receives the actual shared argument");
        };
        let Operation::RootPlace { owner } = operation(EntityId::Place(*place)) else {
            panic!("the exact Boolean owner backs the argument place");
        };
        assert_eq!(
            operation(EntityId::Value(*owner)),
            &Operation::Constant(ScalarConstant::Boolean(selected))
        );
    }
    callee
}

fn assert_exact_holder_flow(program: &Program, hold: &Function) {
    let root = hold.block(hold.entry_block().unwrap()).unwrap();
    let [EntityId::Loan(select), EntityId::Loan(early)] = root.parameters.as_slice() else {
        panic!("the original shared Boolean parameters");
    };
    let (yes, no) = conditional(hold, root, *select);

    let mut joins = Vec::new();
    for (chosen, bytes) in [(yes, b"a"), (no, b"b")] {
        let arm = hold.block(chosen.target).unwrap();
        let [literal] = operations(hold, arm)
            .try_into()
            .expect("one literal per first arm");
        let Operation::StringLiteral { bytes: actual, .. } = &literal.operation else {
            panic!("the first arm forms its own String: {literal:?}");
        };
        assert_eq!(actual.as_slice(), bytes);
        let TerminatorKind::Branch(to_join) = &arm.terminator.as_ref().unwrap().kind else {
            panic!("both actual String paths reach the second operand");
        };
        let join = hold.block(to_join.target).unwrap();
        let selected_string = forwarded(hold, to_join, EntityId::Value(result(literal)));
        let continued_early = forwarded(
            hold,
            to_join,
            forwarded(hold, chosen, EntityId::Loan(*early)),
        );
        joins.push((result(literal), join.id, selected_string, continued_early));
    }
    let [yes, no] = joins.as_slice() else {
        panic!("two first-operand definitions");
    };
    assert_ne!(yes.0, no.0);
    assert_eq!((yes.1, yes.2, yes.3), (no.1, no.2, no.3));
    let EntityId::Loan(joined_early) = yes.3 else {
        panic!("original second Boolean loan");
    };
    let (returning, completing) = conditional(hold, hold.block(yes.1).unwrap(), joined_early);
    assert_ne!(returning.target, completing.target);

    let early_drop = forwarded(hold, returning, yes.2);
    let returning = hold.block(returning.target).unwrap();
    let [drop] = operations(hold, returning)
        .try_into()
        .expect("only the completed temporary");
    assert!(
        matches!(drop.operation, Operation::Drop { owner } if EntityId::Value(owner) == early_drop)
    );
    assert_void_return(returning);

    let normal_string = forwarded(hold, completing, yes.2);
    let completing = hold.block(completing.target).unwrap();
    let [integer, aggregate, allocation, drop] = operations(hold, completing)
        .try_into()
        .expect("complete, allocate and release just this Holder");
    assert_eq!(
        integer.operation,
        Operation::Constant(ScalarConstant::Integer(1))
    );
    let Operation::AggregateConstruct {
        aggregate: payload_type,
        fields,
    } = &aggregate.operation
    else {
        panic!("construct the payload from the two completed arguments");
    };
    assert_eq!(fields.len(), 2);
    assert_eq!(EntityId::Value(fields[0]), normal_string);
    assert_eq!(fields[1], result(integer));
    let Operation::HeapAllocate { owner, payload } = allocation.operation else {
        panic!("allocate the actual constructed payload");
    };
    assert_eq!(payload, result(aggregate));
    let kind = &program.modules[owner.module().index()].types[owner.index()];
    assert!(
        matches!(kind, super::model::SsaTypeKind::HeapOwner { payload: Some(actual), .. } if actual == payload_type)
    );
    assert_eq!(
        drop.operation,
        Operation::Drop {
            owner: result(allocation)
        }
    );
    assert_void_return(completing);
}

fn operations<'a>(f: &'a Function, block: &Block) -> Vec<&'a Instruction> {
    block
        .instructions
        .iter()
        .map(|id| &f.instructions[id.index()])
        .collect()
}

fn conditional<'a>(
    f: &'a Function,
    block: &'a Block,
    input: super::model::LoanId,
) -> (&'a Edge, &'a Edge) {
    let [read] = operations(f, block)
        .try_into()
        .expect("one actual condition read");
    assert_eq!(
        read.operation,
        Operation::Read {
            source: PlaceAccess::Loan(input)
        }
    );
    let TerminatorKind::Conditional {
        condition,
        when_true,
        when_false,
    } = &block.terminator.as_ref().unwrap().kind
    else {
        panic!("actual two-way branch");
    };
    assert_eq!(*condition, result(read));
    assert_ne!(when_true.target, when_false.target);
    (when_true, when_false)
}

fn forwarded(f: &Function, edge: &Edge, source: EntityId) -> EntityId {
    let target = f.block(edge.target).unwrap();
    let matches = edge
        .arguments
        .iter()
        .zip(&target.parameters)
        .filter_map(|(argument, parameter)| (*argument == source).then_some(*parameter))
        .collect::<Vec<_>>();
    let [parameter] = matches.as_slice() else {
        panic!("the actual entity must travel exactly once along this edge: {source:?}");
    };
    *parameter
}

fn result(instruction: &Instruction) -> ValueId {
    let [EntityId::Value(value)] = instruction.results.as_slice() else {
        panic!("one actual SSA result: {instruction:?}");
    };
    *value
}

fn assert_void_return(block: &Block) {
    assert!(matches!(
        block.terminator.as_ref().unwrap().kind,
        TerminatorKind::Return { ref values } if values.is_empty()
    ));
}

const COUNTER: &str = r#"
#include <assert.h>
#include <stdlib.h>
static void *live[EXPECTED_COUNT];
static int allocated, released;
void *recorded_malloc(size_t bytes) {
    assert(bytes && allocated < EXPECTED_COUNT);
    void *pointer = malloc(bytes);
    assert(pointer);
    for (int i = 0; i < allocated; ++i) assert(live[i] != pointer);
    live[allocated++] = pointer;
    return pointer;
}
void recorded_free(void *pointer) {
    assert(pointer && released < EXPECTED_COUNT);
    // The selected String's allocation is first and is released before its parent.
    assert(pointer == live[released]);
    if (released == 0) assert(*(const char *)pointer == EXPECTED_BYTE);
    live[released++] = NULL;
    free(pointer);
}
__attribute__((destructor)) static void verify(void) {
    assert(allocated == EXPECTED_COUNT && released == EXPECTED_COUNT);
}
"#;
