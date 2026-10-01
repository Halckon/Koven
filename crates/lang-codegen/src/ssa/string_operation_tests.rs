use lang_frontend::source::SourceMap;

use super::{
    model::{
        BlockId, EntityId, EntityType, Function, LoanId, LoanKind, Operation, Origin, Ownership,
        Program, SsaTypeId, SsaTypeKind, TerminatorKind, ValueId,
    },
    render::render_program,
    verify::{VerifyErrorKind, verify_program},
};

fn origin() -> Origin {
    let mut sources = SourceMap::default();
    let source = sources
        .add_source("string-owner.ko", "fun main(): Unit")
        .expect("source");
    Origin::Source(sources.span(source, 0, 3).expect("span"))
}

fn value(entity: EntityId) -> ValueId {
    let EntityId::Value(value) = entity else {
        panic!("expected value")
    };
    value
}

fn loan(entity: EntityId) -> LoanId {
    let EntityId::Loan(loan) = entity else {
        panic!("expected loan")
    };
    loan
}

fn append(
    function: &mut Function,
    block: BlockId,
    operation: Operation,
    results: Vec<EntityType>,
    origin: &Origin,
) -> Vec<EntityId> {
    function
        .append_instruction(block, operation, results, origin.clone())
        .expect("instruction")
        .1
}

fn literal(
    function: &mut Function,
    block: BlockId,
    string: SsaTypeId,
    bytes: &[u8],
    origin: &Origin,
) -> ValueId {
    value(
        append(
            function,
            block,
            Operation::StringLiteral {
                string,
                bytes: bytes.to_vec(),
            },
            vec![EntityType::Value(string)],
            origin,
        )[0],
    )
}

fn shared_loan(
    function: &mut Function,
    block: BlockId,
    owner: ValueId,
    string: SsaTypeId,
    origin: &Origin,
) -> LoanId {
    let place = append(
        function,
        block,
        Operation::RootPlace { owner },
        vec![EntityType::Place(string)],
        origin,
    )[0];
    let EntityId::Place(place) = place else {
        panic!("expected place")
    };
    loan(
        append(
            function,
            block,
            Operation::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            },
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target: string,
            }],
            origin,
        )[0],
    )
}

#[test]
fn string_owner_operations_verify_preserve_views_and_render_deterministically() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("string");
    let module = program.module_mut(module_id).expect("module");
    let string = module.add_string_owner_type();
    assert_eq!(module.add_string_owner_type(), string);
    assert_eq!(module.type_ownership(string), Some(Ownership::MoveOnly));
    let boolean = module.intern_type(SsaTypeKind::Boolean);
    let function_id = module
        .add_function("compare", vec![boolean], origin.clone())
        .expect("function");
    let function = module.function_mut(function_id).expect("function");
    let entry = function
        .add_block(Vec::new(), origin.clone())
        .expect("entry");

    let hello = literal(function, entry, string, b"hello\0", &origin);
    let hello_view = shared_loan(function, entry, hello, string, &origin);
    let world = literal(function, entry, string, "世界".as_bytes(), &origin);
    let joined = value(
        append(
            function,
            entry,
            Operation::StringConcat {
                left: EntityId::Loan(hello_view),
                right: EntityId::Value(world),
            },
            vec![EntityType::Value(string)],
            &origin,
        )[0],
    );
    let equal = value(
        append(
            function,
            entry,
            Operation::StringEqual {
                left: EntityId::Value(joined),
                right: EntityId::Loan(hello_view),
            },
            vec![EntityType::Value(boolean)],
            &origin,
        )[0],
    );
    let joined_view = shared_loan(function, entry, joined, string, &origin);
    append(
        function,
        entry,
        Operation::PrintString { value: joined_view },
        Vec::new(),
        &origin,
    );
    append(
        function,
        entry,
        Operation::BorrowEnd { loan: joined_view },
        Vec::new(),
        &origin,
    );
    append(
        function,
        entry,
        Operation::BorrowEnd { loan: hello_view },
        Vec::new(),
        &origin,
    );
    for owner in [joined, world, hello] {
        append(
            function,
            entry,
            Operation::Drop { owner },
            Vec::new(),
            &origin,
        );
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

    verify_program(&program).expect("well-typed String views and owner exits must verify");
    let rendered = render_program(&program);
    assert!(rendered.contains("!t0 = string_owner"));
    assert!(rendered.contains("string.literal !t0, [104, 101, 108, 108, 111, 0]"));
    assert!(rendered.contains("string.concat %l0, %v1"));
    assert!(rendered.contains("string.equal %v2, %l0"));
    assert!(rendered.contains("print.string %l1"));
    assert_eq!(rendered, render_program(&program));
}

#[test]
fn verifier_rejects_invalid_utf8_and_non_string_operation_shapes() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("invalid-string");
    let module = program.module_mut(module_id).expect("module");
    let string = module.add_string_owner_type();
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 64,
        signed: true,
    });
    let function_id = module
        .add_function("invalid", Vec::new(), origin.clone())
        .expect("function");
    let function = module.function_mut(function_id).expect("function");
    let entry = function
        .add_block(Vec::new(), origin.clone())
        .expect("entry");
    let malformed = value(
        append(
            function,
            entry,
            Operation::StringLiteral {
                string,
                bytes: vec![0xff],
            },
            vec![EntityType::Value(string)],
            &origin,
        )[0],
    );
    append(
        function,
        entry,
        Operation::StringConcat {
            left: EntityId::Value(malformed),
            right: EntityId::Value(malformed),
        },
        vec![EntityType::Value(integer)],
        &origin,
    );
    append(
        function,
        entry,
        Operation::Drop { owner: malformed },
        Vec::new(),
        &origin,
    );
    function
        .set_terminator(entry, TerminatorKind::Return { values: Vec::new() }, origin)
        .expect("return");

    let errors = verify_program(&program).expect_err("invalid String contracts must be rejected");
    assert_eq!(
        errors
            .errors
            .iter()
            .filter(|error| matches!(error.kind, VerifyErrorKind::OperationContract { .. }))
            .count(),
        2
    );
}

#[test]
fn verifier_requires_shared_active_print_loan_and_blocks_early_drop() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("string-loans");
    let module = program.module_mut(module_id).expect("module");
    let string = module.add_string_owner_type();
    let function_id = module
        .add_function("loans", Vec::new(), origin.clone())
        .expect("function");
    let function = module.function_mut(function_id).expect("function");
    let entry = function
        .add_block(Vec::new(), origin.clone())
        .expect("entry");
    let owner = literal(function, entry, string, b"loan", &origin);
    let view = shared_loan(function, entry, owner, string, &origin);
    append(
        function,
        entry,
        Operation::Drop { owner },
        Vec::new(),
        &origin,
    );
    append(
        function,
        entry,
        Operation::BorrowEnd { loan: view },
        Vec::new(),
        &origin,
    );
    append(
        function,
        entry,
        Operation::PrintString { value: view },
        Vec::new(),
        &origin,
    );
    for _ in 0..2 {
        append(
            function,
            entry,
            Operation::Drop { owner },
            Vec::new(),
            &origin,
        );
    }
    function
        .set_terminator(entry, TerminatorKind::Return { values: Vec::new() }, origin)
        .expect("return");

    let errors = verify_program(&program).expect_err("loan lifetime violations must be rejected");
    assert!(errors.errors.iter().any(|error| {
        matches!(
            error.kind,
            VerifyErrorKind::OwnerLoanConflict { value } if value == owner
        )
    }));
    assert!(errors.errors.iter().any(|error| {
        matches!(error.kind, VerifyErrorKind::LoanInactive { loan } if loan == view)
    }));
    assert!(errors.errors.iter().any(|error| {
        matches!(error.kind, VerifyErrorKind::ValueUnavailable { value } if value == owner)
    }));
}

#[test]
fn verifier_rejects_exclusive_string_views() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("exclusive-string");
    let module = program.module_mut(module_id).expect("module");
    let string = module.add_string_owner_type();
    let boolean = module.intern_type(SsaTypeKind::Boolean);
    let function_id = module
        .add_function("exclusive", Vec::new(), origin.clone())
        .expect("function");
    let function = module.function_mut(function_id).expect("function");
    let entry = function
        .add_block(Vec::new(), origin.clone())
        .expect("entry");
    let owner = literal(function, entry, string, b"value", &origin);
    let place = append(
        function,
        entry,
        Operation::RootPlace { owner },
        vec![EntityType::Place(string)],
        &origin,
    )[0];
    let EntityId::Place(place) = place else {
        panic!("expected place")
    };
    let exclusive = loan(
        append(
            function,
            entry,
            Operation::BorrowBegin {
                place,
                kind: LoanKind::Exclusive,
            },
            vec![EntityType::Loan {
                kind: LoanKind::Exclusive,
                target: string,
            }],
            &origin,
        )[0],
    );
    append(
        function,
        entry,
        Operation::StringEqual {
            left: EntityId::Loan(exclusive),
            right: EntityId::Value(owner),
        },
        vec![EntityType::Value(boolean)],
        &origin,
    );
    append(
        function,
        entry,
        Operation::PrintString { value: exclusive },
        Vec::new(),
        &origin,
    );
    function
        .set_terminator(entry, TerminatorKind::Abort, origin)
        .expect("abort");

    let errors = verify_program(&program).expect_err("exclusive String views must be rejected");
    assert_eq!(
        errors
            .errors
            .iter()
            .filter(|error| matches!(error.kind, VerifyErrorKind::OperationContract { .. }))
            .count(),
        2
    );
}

#[test]
fn verifier_rejects_value_views_while_an_exclusive_loan_is_active() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("exclusive-owner-view");
    let module = program.module_mut(module_id).expect("module");
    let string = module.add_string_owner_type();
    let boolean = module.intern_type(SsaTypeKind::Boolean);
    let function_id = module
        .add_function("exclusive_owner", Vec::new(), origin.clone())
        .expect("function");
    let function = module.function_mut(function_id).expect("function");
    let entry = function
        .add_block(Vec::new(), origin.clone())
        .expect("entry");
    let owner = literal(function, entry, string, b"value", &origin);
    let place = append(
        function,
        entry,
        Operation::RootPlace { owner },
        vec![EntityType::Place(string)],
        &origin,
    )[0];
    let EntityId::Place(place) = place else {
        panic!("expected place")
    };
    let exclusive = loan(
        append(
            function,
            entry,
            Operation::BorrowBegin {
                place,
                kind: LoanKind::Exclusive,
            },
            vec![EntityType::Loan {
                kind: LoanKind::Exclusive,
                target: string,
            }],
            &origin,
        )[0],
    );
    append(
        function,
        entry,
        Operation::StringEqual {
            left: EntityId::Value(owner),
            right: EntityId::Value(owner),
        },
        vec![EntityType::Value(boolean)],
        &origin,
    );
    append(
        function,
        entry,
        Operation::BorrowEnd { loan: exclusive },
        Vec::new(),
        &origin,
    );
    append(
        function,
        entry,
        Operation::Drop { owner },
        Vec::new(),
        &origin,
    );
    function
        .set_terminator(entry, TerminatorKind::Return { values: Vec::new() }, origin)
        .expect("return");

    let errors = verify_program(&program).expect_err("exclusive loans must block owner reads");
    assert!(errors.errors.iter().any(|error| {
        matches!(
            error.kind,
            VerifyErrorKind::OwnerLoanConflict { value } if value == owner
        )
    }));
}

#[test]
fn string_clone_requires_active_shared_loan_and_creates_independent_owner() {
    for drop_source_first in [false, true] {
        let origin = origin();
        let mut program = Program::default();
        let module_id = program.add_module("clone");
        let module = program.module_mut(module_id).unwrap();
        let string = module.add_string_owner_type();
        let id = module
            .add_function("clone", Vec::new(), origin.clone())
            .unwrap();
        let function = module.function_mut(id).unwrap();
        let entry = function.add_block(Vec::new(), origin.clone()).unwrap();
        let source = literal(function, entry, string, "界\0é".as_bytes(), &origin);
        let view = shared_loan(function, entry, source, string, &origin);
        let copy = value(
            append(
                function,
                entry,
                Operation::StringClone { source: view },
                vec![EntityType::Value(string)],
                &origin,
            )[0],
        );
        append(
            function,
            entry,
            Operation::BorrowEnd { loan: view },
            Vec::new(),
            &origin,
        );
        let order = if drop_source_first {
            [source, copy]
        } else {
            [copy, source]
        };
        for owner in order {
            append(
                function,
                entry,
                Operation::Drop { owner },
                Vec::new(),
                &origin,
            );
        }
        function
            .set_terminator(entry, TerminatorKind::Return { values: Vec::new() }, origin)
            .unwrap();
        verify_program(&program).expect("clone and source have independent owner obligations");
        let rendered = render_program(&program);
        assert!(rendered.contains("string.clone"));
        assert_eq!(rendered, render_program(&program));
        let llvm = crate::llvm::render_verified_program(&program).unwrap();
        assert!(llvm.contains("clone.allocate"));
        assert!(llvm.contains("llvm.memcpy"));
        assert!(!llvm.contains("strong.next"));
    }
}

#[test]
fn string_clone_verifier_rejects_ended_exclusive_or_nonstring_loans_and_wrong_result() {
    for invalid in ["ended", "exclusive", "nonstring", "result"] {
        let origin = origin();
        let mut program = Program::default();
        let module_id = program.add_module("clone-invalid");
        let module = program.module_mut(module_id).unwrap();
        let string = module.add_string_owner_type();
        let boolean = module.intern_type(SsaTypeKind::Boolean);
        let id = module
            .add_function("clone", Vec::new(), origin.clone())
            .unwrap();
        let function = module.function_mut(id).unwrap();
        let entry = function.add_block(Vec::new(), origin.clone()).unwrap();
        let owner = literal(function, entry, string, b"value", &origin);
        let place = append(
            function,
            entry,
            Operation::RootPlace { owner },
            vec![EntityType::Place(string)],
            &origin,
        )[0];
        let EntityId::Place(place) = place else {
            panic!("place")
        };
        let kind = if invalid == "exclusive" {
            LoanKind::Exclusive
        } else {
            LoanKind::Shared
        };
        let target = if invalid == "nonstring" {
            boolean
        } else {
            string
        };
        let view = loan(
            append(
                function,
                entry,
                Operation::BorrowBegin { place, kind },
                vec![EntityType::Loan { kind, target }],
                &origin,
            )[0],
        );
        if invalid == "ended" {
            append(
                function,
                entry,
                Operation::BorrowEnd { loan: view },
                Vec::new(),
                &origin,
            );
        }
        let result_type = if invalid == "result" { boolean } else { string };
        let copy = value(
            append(
                function,
                entry,
                Operation::StringClone { source: view },
                vec![EntityType::Value(result_type)],
                &origin,
            )[0],
        );
        if invalid != "ended" {
            append(
                function,
                entry,
                Operation::BorrowEnd { loan: view },
                Vec::new(),
                &origin,
            );
        }
        if result_type == string {
            append(
                function,
                entry,
                Operation::Drop { owner: copy },
                Vec::new(),
                &origin,
            );
        }
        append(
            function,
            entry,
            Operation::Drop { owner },
            Vec::new(),
            &origin,
        );
        function
            .set_terminator(entry, TerminatorKind::Return { values: Vec::new() }, origin)
            .unwrap();
        let errors = verify_program(&program).expect_err(invalid);
        assert!(
            errors.errors.iter().any(|error| if invalid == "ended" {
                matches!(error.kind, VerifyErrorKind::LoanInactive { loan } if loan == view)
            } else {
                matches!(error.kind, VerifyErrorKind::OperationContract { .. })
            }),
            "{invalid}: {errors:?}"
        );
    }
}
