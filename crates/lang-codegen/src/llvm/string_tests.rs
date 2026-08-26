use lang_frontend::source::SourceMap;

use crate::ssa::model::{
    EntityId, EntityType, LoanKind, Operation, Origin, Program, SsaTypeId, TerminatorKind, ValueId,
};

use super::render_verified_program;

fn origin() -> Origin {
    let mut sources = SourceMap::default();
    let source = sources
        .add_source("string-runtime.ko", "fun main(): Bool")
        .expect("source");
    Origin::Source(sources.span(source, 0, 3).expect("span"))
}

fn value(entity: EntityId) -> ValueId {
    let EntityId::Value(value) = entity else {
        panic!("expected value")
    };
    value
}

fn literal(
    function: &mut crate::ssa::model::Function,
    block: crate::ssa::model::BlockId,
    string: SsaTypeId,
    bytes: &[u8],
    origin: &Origin,
) -> ValueId {
    value(
        function
            .append_instruction(
                block,
                Operation::StringLiteral {
                    string,
                    bytes: bytes.to_vec(),
                },
                vec![EntityType::Value(string)],
                origin.clone(),
            )
            .expect("literal")
            .1[0],
    )
}

#[test]
fn string_runtime_ir_has_target_layout_checked_concat_equality_print_and_drop() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("string_runtime");
    let module = program.module_mut(module_id).expect("module");
    let string = module.add_string_owner_type();
    let boolean = module.intern_type(crate::ssa::model::SsaTypeKind::Boolean);
    let function_id = module
        .add_function("exercise", vec![boolean], origin.clone())
        .expect("function");
    let function = module.function_mut(function_id).expect("function");
    let entry = function
        .add_block(Vec::new(), origin.clone())
        .expect("entry");
    let left = literal(function, entry, string, b"A\0", &origin);
    let right = literal(function, entry, string, "界".as_bytes(), &origin);
    let joined = value(
        function
            .append_instruction(
                entry,
                Operation::StringConcat {
                    left: EntityId::Value(left),
                    right: EntityId::Value(right),
                },
                vec![EntityType::Value(string)],
                origin.clone(),
            )
            .expect("concat")
            .1[0],
    );
    let equal = value(
        function
            .append_instruction(
                entry,
                Operation::StringEqual {
                    left: EntityId::Value(joined),
                    right: EntityId::Value(left),
                },
                vec![EntityType::Value(boolean)],
                origin.clone(),
            )
            .expect("equal")
            .1[0],
    );
    let place = function
        .append_instruction(
            entry,
            Operation::RootPlace { owner: joined },
            vec![EntityType::Place(string)],
            origin.clone(),
        )
        .expect("root place")
        .1[0];
    let EntityId::Place(place) = place else {
        panic!("expected place")
    };
    let loan = function
        .append_instruction(
            entry,
            Operation::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            },
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target: string,
            }],
            origin.clone(),
        )
        .expect("loan")
        .1[0];
    let EntityId::Loan(loan) = loan else {
        panic!("expected loan")
    };
    function
        .append_instruction(
            entry,
            Operation::PrintString { value: loan },
            Vec::new(),
            origin.clone(),
        )
        .expect("print");
    function
        .append_instruction(
            entry,
            Operation::BorrowEnd { loan },
            Vec::new(),
            origin.clone(),
        )
        .expect("borrow end");
    for owner in [joined, right, left] {
        function
            .append_instruction(entry, Operation::Drop { owner }, Vec::new(), origin.clone())
            .expect("drop");
    }
    function
        .set_terminator(
            entry,
            TerminatorKind::Return {
                values: vec![equal],
            },
            origin,
        )
        .expect("return");

    let ir = render_verified_program(&program).expect("String program must lower and verify");
    assert!(ir.contains("type { ptr, i64, i64 }"));
    assert!(ir.contains("private constant [2 x i8] c\"A\\00\""));
    assert!(ir.contains("call ptr @malloc(i64"));
    assert!(ir.contains("call void @llvm.memcpy"));
    assert!(ir.contains("icmp ult i64"));
    assert!(ir.contains("load i8, ptr"));
    assert_eq!(ir.matches("call i64 @write(i32 1, ptr").count(), 2);
    assert!(ir.contains("extractvalue %koven.string"));
    assert!(ir.contains("call void @free(ptr"));
}

#[test]
fn empty_static_string_uses_null_cap_zero_without_allocation() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("empty_string");
    let module = program.module_mut(module_id).expect("module");
    let string = module.add_string_owner_type();
    let function_id = module
        .add_function("empty", Vec::new(), origin.clone())
        .expect("function");
    let function = module.function_mut(function_id).expect("function");
    let entry = function
        .add_block(Vec::new(), origin.clone())
        .expect("entry");
    let empty = literal(function, entry, string, b"", &origin);
    function
        .append_instruction(
            entry,
            Operation::Drop { owner: empty },
            Vec::new(),
            origin.clone(),
        )
        .expect("drop");
    function
        .set_terminator(entry, TerminatorKind::Return { values: Vec::new() }, origin)
        .expect("return");

    let ir = render_verified_program(&program).expect("empty String must lower and verify");
    assert!(ir.contains("%koven.string.t0 zeroinitializer"));
    assert!(!ir.contains("declare ptr @malloc"));
    assert!(!ir.contains("llvm.memcpy"));
}
