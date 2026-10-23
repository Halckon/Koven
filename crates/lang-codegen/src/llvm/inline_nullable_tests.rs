//! ADR-0033 inline nullable: tagged `{present, payload}` and the String/container pointer niche.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

use lang_frontend::source::SourceMap;

use crate::ssa::model::{
    BlockId, ComparisonOperator, Edge, EntityId, EntityType, Function, FunctionId, LoanId,
    LoanKind, ModelError, Operation, Origin, Program, ScalarConstant, SequentialContainerKind,
    SsaTypeId, SsaTypeKind, TerminatorKind, ValueId,
};
use crate::ssa::verify::verify_program;

use super::render_verified_program_with_entry;

fn origin() -> Origin {
    let mut sources = SourceMap::default();
    let source = sources
        .add_source("inline-nullable.ko", "val a: Int? = 5")
        .expect("source");
    Origin::Source(sources.span(source, 0, 3).expect("span"))
}

fn emit(
    function: &mut Function,
    block: BlockId,
    operation: Operation,
    results: Vec<EntityType>,
) -> Vec<EntityId> {
    function
        .append_instruction(block, operation, results, origin())
        .expect("instruction")
        .1
}

fn value(entity: EntityId) -> ValueId {
    let EntityId::Value(value) = entity else {
        panic!("expected value")
    };
    value
}

fn emit_value(
    function: &mut Function,
    block: BlockId,
    operation: Operation,
    ty: SsaTypeId,
) -> ValueId {
    value(emit(function, block, operation, vec![EntityType::Value(ty)])[0])
}

fn print(function: &mut Function, block: BlockId, text: &str) {
    emit(
        function,
        block,
        Operation::PrintLiteral {
            bytes: text.as_bytes().to_vec(),
        },
        Vec::new(),
    );
}

fn finish(function: &mut Function, block: BlockId, kind: TerminatorKind) {
    function
        .set_terminator(block, kind, origin())
        .expect("terminator");
}

fn goto(target: BlockId) -> TerminatorKind {
    TerminatorKind::Branch(Edge {
        target,
        arguments: Vec::new(),
    })
}

/// Branches on `owner`; returns `(null, non_null, non_null_owner, view)`.
fn branch(
    function: &mut Function,
    block: BlockId,
    owner: ValueId,
    nullable: SsaTypeId,
    inner: SsaTypeId,
) -> (BlockId, BlockId, ValueId, LoanId) {
    let null = function
        .add_block(vec![EntityType::Value(nullable)], origin())
        .expect("null block");
    let non_null = function
        .add_block(
            vec![
                EntityType::Value(nullable),
                EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: inner,
                },
            ],
            origin(),
        )
        .expect("non-null block");
    let parameters = function.block(non_null).expect("block").parameters.clone();
    let EntityId::Loan(view) = parameters[1] else {
        panic!("expected view")
    };
    finish(
        function,
        block,
        TerminatorKind::NullableBranch {
            owner,
            when_null: Edge {
                target: null,
                arguments: vec![EntityId::Value(owner)],
            },
            when_non_null: Edge {
                target: non_null,
                arguments: vec![EntityId::Value(owner)],
            },
            view,
        },
    );
    (null, non_null, value(parameters[0]), view)
}

fn null_parameter(function: &Function, block: BlockId) -> ValueId {
    value(function.block(block).expect("block").parameters[0])
}

/// `Int?`: `5` takes the non-null edge and compares equal; `null` takes the null edge.
fn int_program() -> (Program, FunctionId, SsaTypeId) {
    let mut program = Program::default();
    let module_id = program.add_module("inline_int");
    let module = program.module_mut(module_id).expect("module");
    let int = module.intern_type(SsaTypeKind::Integer {
        bits: 32,
        signed: true,
    });
    let boolean = module.intern_type(SsaTypeKind::Boolean);
    let nullable = module.add_inline_nullable_type(int).expect("Int?");
    let main = module
        .add_function("main", Vec::new(), origin())
        .expect("main");
    let function = module.function_mut(main).expect("function");
    let entry = function.add_block(Vec::new(), origin()).expect("entry");
    let five = emit_value(
        function,
        entry,
        Operation::Constant(ScalarConstant::Integer(5)),
        int,
    );
    let wrapped = emit_value(
        function,
        entry,
        Operation::NullableWrap {
            nullable,
            owner: five,
        },
        nullable,
    );
    let (null, non_null, owner, view) = branch(function, entry, wrapped, nullable, int);
    let second = function.add_block(Vec::new(), origin()).expect("second");
    print(function, null, "bad-null\n");
    finish(function, null, goto(second));
    let taken = emit_value(
        function,
        non_null,
        Operation::NullableTake { owner, proof: view },
        int,
    );
    let equal = emit_value(
        function,
        non_null,
        Operation::Compare {
            operator: ComparisonOperator::Equal,
            left: taken,
            right: five,
        },
        boolean,
    );
    let yes = function.add_block(Vec::new(), origin()).expect("yes");
    let no = function.add_block(Vec::new(), origin()).expect("no");
    finish(
        function,
        non_null,
        TerminatorKind::Conditional {
            condition: equal,
            when_true: Edge {
                target: yes,
                arguments: Vec::new(),
            },
            when_false: Edge {
                target: no,
                arguments: Vec::new(),
            },
        },
    );
    print(function, yes, "five\n");
    finish(function, yes, goto(second));
    print(function, no, "bad-value\n");
    finish(function, no, goto(second));

    let empty = emit_value(
        function,
        second,
        Operation::NullableNull { nullable },
        nullable,
    );
    let (null, non_null, owner, view) = branch(function, second, empty, nullable, int);
    print(function, null, "none\n");
    finish(
        function,
        null,
        TerminatorKind::Return { values: Vec::new() },
    );
    emit_value(
        function,
        non_null,
        Operation::NullableTake { owner, proof: view },
        int,
    );
    print(function, non_null, "bad-some\n");
    finish(
        function,
        non_null,
        TerminatorKind::Return { values: Vec::new() },
    );
    (program, main, nullable)
}

/// `String?` (pointer niche): a concatenated owner is printed through the non-null view
/// and dropped as a nullable; a separate `null` is dropped without freeing.
fn string_program() -> (Program, FunctionId) {
    let mut program = Program::default();
    let module_id = program.add_module("inline_string");
    let module = program.module_mut(module_id).expect("module");
    let string = module.add_string_owner_type();
    let nullable = module.add_inline_nullable_type(string).expect("String?");
    let main = module
        .add_function("main", Vec::new(), origin())
        .expect("main");
    let function = module.function_mut(main).expect("function");
    let entry = function.add_block(Vec::new(), origin()).expect("entry");
    let mut literal = |bytes: &[u8]| {
        emit_value(
            function,
            entry,
            Operation::StringLiteral {
                string,
                bytes: bytes.to_vec(),
            },
            string,
        )
    };
    let left = literal(b"ab");
    let right = literal(b"cd");
    let joined = emit_value(
        function,
        entry,
        Operation::StringConcat {
            left: EntityId::Value(left),
            right: EntityId::Value(right),
        },
        string,
    );
    for owner in [right, left] {
        emit(function, entry, Operation::Drop { owner }, Vec::new());
    }
    let wrapped = emit_value(
        function,
        entry,
        Operation::NullableWrap {
            nullable,
            owner: joined,
        },
        nullable,
    );
    let (null, non_null, owner, view) = branch(function, entry, wrapped, nullable, string);
    let tail = function.add_block(Vec::new(), origin()).expect("tail");
    let null_owner = null_parameter(function, null);
    print(function, null, "bad-null\n");
    emit(
        function,
        null,
        Operation::Drop { owner: null_owner },
        Vec::new(),
    );
    finish(function, null, goto(tail));
    emit(
        function,
        non_null,
        Operation::PrintString { value: view },
        Vec::new(),
    );
    emit(
        function,
        non_null,
        Operation::BorrowEnd { loan: view },
        Vec::new(),
    );
    emit(function, non_null, Operation::Drop { owner }, Vec::new());
    finish(function, non_null, goto(tail));
    let empty = emit_value(
        function,
        tail,
        Operation::NullableNull { nullable },
        nullable,
    );
    emit(function, tail, Operation::Drop { owner: empty }, Vec::new());
    print(function, tail, "done\n");
    finish(
        function,
        tail,
        TerminatorKind::Return { values: Vec::new() },
    );
    (program, main)
}

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Directory(PathBuf);
impl Drop for Directory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).expect("remove inline nullable fixture directory");
    }
}

/// Links `llvm` with a malloc/free counter and checks stdout and balanced allocations.
fn run_counted(llvm: &str, allocations: usize, stdout: &[u8]) {
    let directory = Directory(std::env::temp_dir().join(format!(
        "koven-inline-nullable-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )));
    fs::create_dir(&directory.0).expect("fixture directory");
    let ir = directory.0.join("program.ll");
    let counter = directory.0.join("counter.c");
    let executable = directory.0.join("program");
    let llvm = llvm
        .replace("@malloc(", "@counted_malloc(")
        .replace("@free(", "@counted_free(");
    fs::write(&ir, llvm).expect("write IR");
    fs::write(
        &counter,
        format!(
            r#"
#include <stdlib.h>
#include <assert.h>
static int allocated, released;
void *counted_malloc(size_t n) {{ ++allocated; void *p = malloc(n); assert(p); return p; }}
void counted_free(void *p) {{ assert(p); ++released; free(p); }}
__attribute__((destructor)) static void verify(void) {{
    assert(allocated == {allocations} && released == {allocations});
}}
"#
        ),
    )
    .expect("write counter");
    let link = Command::new(crate::test_support::ir_clang())
        .arg(&ir)
        .arg(&counter)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("clang launches");
    assert!(link.status.success(), "{link:?}");
    let run = Command::new(&executable)
        .output()
        .expect("program launches");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, stdout, "{run:?}");
    assert!(run.stderr.is_empty(), "{run:?}");
}

#[test]
fn inline_int_nullable_uses_present_tag_and_runs_both_edges() {
    let (program, main, nullable) = int_program();
    verify_program(&program).expect("Int? program verifies");
    let ir = render_verified_program_with_entry(&program, main).expect("Int? lowers");
    let name = format!("%koven.nullable.t{} = type {{ i1, i32 }}", nullable.index());
    assert!(ir.contains(&name), "{ir}");
    assert!(
        !ir.contains("@koven.drop"),
        "Copyable Int? needs no drop glue: {ir}"
    );
    run_counted(&ir, 0, b"five\nnone\n");
}

#[test]
fn inline_string_nullable_uses_pointer_niche_and_frees_once() {
    let (program, main) = string_program();
    verify_program(&program).expect("String? program verifies");
    let ir = render_verified_program_with_entry(&program, main).expect("String? lowers");
    assert!(
        !ir.contains("koven.nullable"),
        "niche keeps the String layout: {ir}"
    );
    assert!(
        ir.contains("icmp ne ptr"),
        "niche drop tests the data pointer: {ir}"
    );
    run_counted(&ir, 1, b"abcd\ndone\n");
}

#[test]
fn inline_nullable_rejects_pointer_like_nested_and_map_presence_inners() {
    let mut program = Program::default();
    let module_id = program.add_module("inline_reject");
    let module = program.module_mut(module_id).expect("module");
    let payload = module
        .add_aggregate_type("Node.payload", Vec::new())
        .expect("payload");
    let node = module.declare_heap_owner("Node").expect("owner");
    module.define_heap_owner(node, payload).expect("definition");
    let int = module.intern_type(SsaTypeKind::Integer {
        bits: 32,
        signed: true,
    });
    let nullable_int = module.add_inline_nullable_type(int).expect("Int?");
    let map_result = module.add_map_result_type(int).expect("map result");
    for inner in [node, nullable_int, map_result] {
        assert!(matches!(
            module.add_inline_nullable_type(inner),
            Err(ModelError::ExpectedInlineNullableInner { ty }) if ty == inner
        ));
    }
    let list = module
        .add_sequential_container_type(SequentialContainerKind::List, int)
        .expect("List<Int>");
    let nullable_list = module.add_inline_nullable_type(list).expect("List<Int>?");
    assert_eq!(module.inline_nullable(nullable_list), Some((list, true)));
    // A hand-built identity whose niche flag disagrees with its inner type is rejected.
    module.intern_type(SsaTypeKind::InlineNullable {
        inner: int,
        niche: true,
    });
    assert!(verify_program(&program).is_err());
}
