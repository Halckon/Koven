//! Original String I/O sources through strict MemoryDocument Unit admission.
//! Original single-root interpolation rejections retain independent Unit boundary tests.
//! Every original root is selected separately; this is not whole-file/native equivalence.
use lang_frontend::name_resolution::{SourceUnitKey, SymbolKind};

use super::{
    LoweringErrorKind,
    model::{
        AbortReason, EntityId, Function, FunctionId, Instruction, LoanKind, Operation, Origin,
        Program, ScalarConstant, TerminatorKind,
    },
    render::render_program,
    verify::verify_program,
};
use crate::{
    llvm::render_verified_program, native_tests::boxed_enum_tests::unit_source::UnitSource,
};

fn analyze(text: &str) -> UnitSource {
    let unit = UnitSource::analyze("lowering.ko", text).expect("strict original Unit admission");
    unit.assert_frontend_clean();
    assert!(unit.ownership().deferred().is_empty());
    let [document] = unit.names().names().index().source_units() else {
        panic!("one unchanged MemoryDocument");
    };
    assert_eq!(document.key(), &SourceUnitKey::MemoryDocument);
    assert_eq!(document.source_id(), unit.parsed().source_id());
    assert_eq!(
        unit.sources().source_name(document.source_id()),
        Ok("lowering.ko")
    );
    assert_eq!(unit.sources().source_text(document.source_id()), Ok(text));
    unit
}

fn lower(unit: &UnitSource, name: &str, source: &str) -> (Program, FunctionId) {
    let declaration = unit.declaration(name, SymbolKind::Function);
    let span = unit.declaration_span(declaration);
    assert_eq!(unit.sources().slice(span), Ok(source));
    let (program, root) = unit
        .lower_entry(declaration)
        .expect("original selected Unit root");
    assert_eq!(root_function(&program, root).origin, Origin::Source(span));
    verify_program(&program).expect("original Unit SSA verifies");
    render_verified_program(&program).expect("original Unit LLVM verifies, not native execution");
    (program, root)
}

fn root_function(program: &Program, root: FunctionId) -> &Function {
    program
        .module(root.module())
        .unwrap()
        .function(root)
        .unwrap()
}

#[test]
fn dynamic_string_error_unit_comparison() {
    let unit = analyze(SOURCE_ERROR);
    let (normal, root) = lower(&unit, "normal", "fun normal(): Unit {}");
    assert!(
        root_function(&normal, root)
            .blocks
            .iter()
            .all(|block| matches!(
                block.terminator.as_ref().unwrap().kind,
                TerminatorKind::Return { .. }
            ))
    );
    assert!(!render_program(&normal).contains("abort @source("));
    let (program, root) = lower(
        &unit,
        "abortNow",
        &SOURCE_ERROR[SOURCE_ERROR.find("fun abortNow").unwrap()..],
    );
    let function = root_function(&program, root);
    let [block] = function.blocks.as_slice() else {
        panic!("straight-line abort")
    };
    let [entry @ EntityId::Loan(_)] = block.parameters.as_slice() else {
        panic!("original message loan")
    };
    let operations = block
        .instructions
        .iter()
        .map(|id| function.instruction(*id).unwrap())
        .collect::<Vec<_>>();
    let literals = operations
        .iter()
        .enumerate()
        .filter_map(|(at, instruction)| {
            if let Operation::StringLiteral { bytes, .. } = &instruction.operation {
                assert_eq!(bytes, b"!");
                Some((at, instruction.results[0]))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let [(literal_at, suffix)] = literals.as_slice() else {
        panic!("one original suffix")
    };
    let (concat_at, concat) = operations
        .iter()
        .enumerate()
        .find(|(_, instruction)| {
            instruction.operation
                == Operation::StringConcat {
                    left: *entry,
                    right: *suffix,
                }
        })
        .expect("concat binds actual message and suffix");
    let EntityId::Value(owner) = concat.results[0] else {
        panic!("concat owner")
    };
    let terminator = block.terminator.as_ref().unwrap();
    assert_eq!(terminator.kind, TerminatorKind::Abort);
    let Some(AbortReason::Error { message }) = terminator.abort else {
        panic!("abort uses evaluated message")
    };
    let borrow_at = assert_owner_loan(&operations, owner, message);
    assert!(*literal_at < concat_at && concat_at < borrow_at);
    assert!(
        !operations
            .iter()
            .any(|instruction| matches!(instruction.operation,
        Operation::BorrowEnd { loan } if loan == message))
    );
    assert!(
        !operations
            .iter()
            .any(|instruction| matches!(instruction.operation,
        Operation::Drop { owner: dropped } if dropped == owner))
    );
    let ssa = render_program(&program);
    assert_eq!(ssa.matches("abort @source(").count(), 1, "{ssa}");
    assert!(!ssa.contains("call @error"), "{ssa}");
    for oracle in ["string.literal", "string.concat", "borrow.shared"] {
        assert!(ssa.contains(oracle), "{oracle}: {ssa}");
    }
}

fn assert_owner_loan(
    operations: &[&Instruction],
    owner: super::model::ValueId,
    loan: super::model::LoanId,
) -> usize {
    let (borrow_at, place) = operations
        .iter()
        .enumerate()
        .find_map(|(at, instruction)| match instruction.operation {
            Operation::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            } if instruction.results == [EntityId::Loan(loan)] => Some((at, place)),
            _ => None,
        })
        .expect("exact I/O loan is a shared borrow");
    let root_at = operations
        .iter()
        .position(|instruction| {
            instruction.operation == Operation::RootPlace { owner }
                && instruction.results == [EntityId::Place(place)]
        })
        .expect("I/O borrows the actual String owner");
    assert!(root_at < borrow_at);
    borrow_at
}

#[test]
fn dynamic_string_println_unit_comparison() {
    let unit = analyze(SOURCE_PRINT);
    let (borrowed, root) = lower(
        &unit,
        "borrowed",
        "fun borrowed(input: String): Unit { println(input) }",
    );
    assert_entry_print(root_function(&borrowed, root));
    let borrowed_ssa = render_program(&borrowed);
    assert_eq!(borrowed_ssa.matches("print.string").count(), 1);
    assert_eq!(borrowed_ssa.matches("end_borrow").count(), 0);
    assert!(!borrowed_ssa.contains("print.literal"));
    let (program, root) = lower(
        &unit,
        "output",
        &SOURCE_PRINT[SOURCE_PRINT.find("fun output").unwrap()..],
    );
    let function = root_function(&program, root);
    let bytes = function
        .instructions
        .iter()
        .filter_map(|instruction| match &instruction.operation {
            Operation::StringLiteral { bytes, .. } => Some(bytes.as_slice()),
            _ => None,
        })
        .collect::<Vec<_>>();
    let expected = [
        b"".as_slice(),
        b"Hello".as_slice(),
        "你好".as_bytes(),
        b"\\\'\"\n\r\t\0$".as_slice(),
    ];
    assert_eq!(bytes, expected);
    // Follow the actual constant-true CFG edges, not instruction arena order.
    let mut block = &function.blocks[0];
    let mut visited = Vec::new();
    let mut printed = Vec::new();
    loop {
        assert!(!visited.contains(&block.id), "original source has no loops");
        visited.push(block.id);
        let operations = block
            .instructions
            .iter()
            .map(|id| function.instruction(*id).unwrap())
            .collect::<Vec<_>>();
        for (print_at, instruction) in operations.iter().enumerate() {
            let Operation::PrintString { value: loan } = instruction.operation else {
                continue;
            };
            let (literal_at, literal) = operations
                .iter()
                .enumerate()
                .find(|(_, instruction)| {
                    matches!(instruction.operation, Operation::StringLiteral { .. })
                })
                .expect("one literal in each source if");
            let Operation::StringLiteral { bytes, .. } = &literal.operation else {
                unreachable!()
            };
            let [EntityId::Value(owner)] = literal.results.as_slice() else {
                panic!("literal owner")
            };
            let borrow_at = assert_owner_loan(&operations, *owner, loan);
            let end_at = operations
                .iter()
                .position(|instruction| instruction.operation == Operation::BorrowEnd { loan })
                .expect("exact print loan ends");
            assert!(literal_at < borrow_at && borrow_at < print_at);
            assert_eq!(end_at, print_at + 1);
            printed.push(bytes.as_slice());
        }
        let target = match &block.terminator.as_ref().unwrap().kind {
            TerminatorKind::Conditional {
                condition,
                when_true,
                ..
            } => {
                assert!(
                    function
                        .instructions
                        .iter()
                        .any(|instruction| instruction.operation
                            == Operation::Constant(ScalarConstant::Boolean(true))
                            && instruction.results == [EntityId::Value(*condition)])
                );
                when_true.target
            }
            TerminatorKind::Branch(edge) => edge.target,
            TerminatorKind::Return { values } => {
                assert!(values.is_empty());
                break;
            }
            other => panic!("unexpected source CFG: {other:?}"),
        };
        block = function
            .blocks
            .iter()
            .find(|block| block.id == target)
            .unwrap();
    }
    assert_eq!(
        printed, expected,
        "decoded literals print in source execution order"
    );
    let ssa = render_program(&program);
    assert_eq!(ssa.matches("print.string").count(), 4);
    assert_eq!(ssa.matches("end_borrow").count(), 4);
    assert!(!ssa.contains("print.literal"));
    assert_eq!(
        borrowed_ssa.matches("print.string").count() + ssa.matches("print.string").count(),
        5
    );

    let text = "fun output(input: String): Unit { println(input) }";
    let nonliteral = analyze(text);
    let (program, root) = lower(&nonliteral, "output", text);
    let function = root_function(&program, root);
    assert_entry_print(function);
    let [EntityId::Loan(loan)] = function.blocks[0].parameters.as_slice() else {
        panic!("original nonliteral entry loan")
    };
    let ssa = render_program(&program);
    assert!(
        ssa.contains(&format!("print.string %l{}", loan.index())),
        "{ssa}"
    );
    assert!(!ssa.contains("borrow.shared"), "{ssa}");
}

fn assert_entry_print(function: &Function) {
    let [EntityId::Loan(loan)] = function.blocks[0].parameters.as_slice() else {
        panic!("original entry loan")
    };
    let prints = function
        .instructions
        .iter()
        .filter(|instruction| instruction.operation == Operation::PrintString { value: *loan })
        .count();
    assert_eq!(prints, 1, "println reuses the actual entry loan");
    assert!(!function.instructions.iter().any(|instruction| matches!(
        instruction.operation,
        Operation::BorrowBegin { .. } | Operation::BorrowEnd { .. }
    )));
}

#[test]
fn string_io_nested_interpolation_rejections_unit_comparison() {
    for (name, text) in [
        (
            "abortInterpolated",
            "fun abortInterpolated(): Unit { error(\"${1}\") }",
        ),
        ("output", "fun output(): Unit { println(\"${1}\") }"),
    ] {
        let unit = analyze(text);
        let declaration = unit.declaration(name, SymbolKind::Function);
        assert_eq!(
            unit.sources().slice(unit.declaration_span(declaration)),
            Ok(text)
        );
        let error = unit
            .lower_entry(declaration)
            .err()
            .expect("original nested interpolation remains unsupported");
        assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
        let span = error
            .span
            .expect("nested interpolation retains source span");
        assert_eq!(span.source_id(), unit.parsed().source_id());
        assert_eq!(unit.sources().slice(span), Ok("\"${1}\""));
        assert_eq!(span.start(), text.find('"').unwrap());
        assert_eq!(span.end(), text.rfind('"').unwrap() + 1);
    }
}
const SOURCE_ERROR: &str = "fun normal(): Unit {}\n\
         fun abortNow(message: String): Unit { error(message + \"!\") }";

const SOURCE_PRINT: &str = r#"fun borrowed(input: String): Unit { println(input) }
        fun output(): Unit {
            if (true) { println("") }
            if (true) { println("Hello") }
            if (true) { println("你好") }
            if (true) { println("\\\'\"\n\r\t\0\$") }
        }"#;
