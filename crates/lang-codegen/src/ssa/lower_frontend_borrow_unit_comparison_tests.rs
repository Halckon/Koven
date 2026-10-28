//! Stage A: unchanged original sources through strict MemoryDocument Unit admission.
//! Legacy tests remain intact. Selected-root programs are not whole-file equivalence;
//! LLVM verification below is not native execution or allocation/free evidence.
use lang_frontend::name_resolution::{SourceUnitKey, SymbolKind};

use super::{
    model::{EntityId, EntityType, LoanKind, Operation, Origin, TerminatorKind},
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
        panic!("one original MemoryDocument, no wrapper or source split");
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

#[test]
fn repeated_class_and_box_root_borrows_unit_comparison() {
    let text = SOURCE_CLASS;
    let unit = analyze(text);
    let declaration = unit.declaration("exercise", SymbolKind::Function);
    let span = unit.declaration_span(declaration);
    assert_eq!(
        unit.sources().slice(span),
        Ok(&text[text.find("fun exercise").unwrap()..])
    );
    let (program, root) = unit
        .lower_entry(declaration)
        .expect("original exercise Unit root");
    let module = program.module(root.module()).expect("root module");
    let function = module.function(root).expect("root function");
    assert_eq!(function.origin, Origin::Source(span));
    assert!(!function.blocks.is_empty());
    let mut targets = Vec::new();
    for (name, source) in [
        ("inspect", "fun inspect(resource: Resource): Unit {}"),
        (
            "inspectBox",
            "fun inspectBox(resource: Box<Token>): Unit {}",
        ),
    ] {
        let declaration = unit.declaration(name, SymbolKind::Function);
        let span = unit.declaration_span(declaration);
        assert_eq!(unit.sources().slice(span), Ok(source));
        let callees = module
            .functions
            .iter()
            .filter(|callee| callee.origin == Origin::Source(span))
            .collect::<Vec<_>>();
        let [callee] = callees.as_slice() else {
            panic!("one exact source callee");
        };
        let [parameter @ EntityId::Loan(_)] = callee.blocks[0].parameters.as_slice() else {
            panic!("one original Borrow parameter");
        };
        assert!(matches!(
            callee.entity(*parameter).expect("parameter").ty,
            EntityType::Loan {
                kind: LoanKind::Shared,
                ..
            }
        ));
        targets.push(callee.id);
    }
    assert_ne!(targets[0], targets[1]);
    let calls = function
        .instructions
        .iter()
        .filter_map(|instruction| {
            if let Operation::DirectCall {
                callee,
                receiver,
                arguments,
            } = &instruction.operation
            {
                assert!(receiver.is_none());
                assert!(matches!(arguments.as_slice(), [EntityId::Loan(_)]));
                Some(*callee)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(calls, [targets[0], targets[0], targets[1], targets[1]]);
    let [block] = function.blocks.as_slice() else {
        panic!("straight-line root borrows stay in one execution block");
    };
    let operations = block
        .instructions
        .iter()
        .map(|id| function.instruction(*id).expect("block instruction"))
        .collect::<Vec<_>>();
    let allocations = operations
        .iter()
        .enumerate()
        .filter_map(|(index, instruction)| match instruction.operation {
            Operation::HeapAllocate { .. } => Some((index, instruction.results[0])),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [
        (resource_at, EntityId::Value(resource)),
        (boxed_at, EntityId::Value(boxed)),
    ] = allocations.as_slice()
    else {
        panic!("Resource and Box each allocate one distinct root owner");
    };
    assert_ne!(resource, boxed);
    let mut call_index = 0;
    for (call_at, instruction) in operations.iter().enumerate() {
        let Operation::DirectCall {
            callee, arguments, ..
        } = &instruction.operation
        else {
            continue;
        };
        let [EntityId::Loan(loan)] = arguments.as_slice() else {
            panic!("root loan")
        };
        assert_eq!(
            *callee,
            targets[call_index / 2],
            "callee order in execution block"
        );
        let (allocation_at, owner) = if call_index < 2 {
            (*resource_at, *resource)
        } else {
            (*boxed_at, *boxed)
        };
        let (borrow_at, place) = operations
            .iter()
            .enumerate()
            .find_map(|(index, instruction)| match instruction.operation {
                Operation::BorrowBegin {
                    place,
                    kind: LoanKind::Shared,
                } if instruction.results == [EntityId::Loan(*loan)] => Some((index, place)),
                _ => None,
            })
            .expect("exact call loan is defined by a shared root borrow");
        let root_at = operations
            .iter()
            .position(|instruction| {
                instruction.operation == Operation::RootPlace { owner }
                    && instruction.results == [EntityId::Place(place)]
            })
            .expect("both calls borrow their actual Resource or Box owner");
        let end_at = operations
            .iter()
            .position(|instruction| instruction.operation == Operation::BorrowEnd { loan: *loan })
            .expect("exact loan ends in the same block");
        assert!(allocation_at < root_at && root_at < borrow_at && borrow_at < call_at);
        assert_eq!(
            end_at,
            call_at + 1,
            "call ends its own loan before later work"
        );
        call_index += 1;
    }
    assert_eq!(call_index, 4);
    verify_program(&program).expect("complete original Unit SSA verifies");
    let ssa = render_program(&program);
    for operation in ["root_place", "borrow.shared", "end_borrow"] {
        assert_eq!(ssa.matches(operation).count(), 4, "{operation}: {ssa}");
    }
    // Preserve the two per-callee call-count oracles without legacy @f0/@f1 assumptions.
    for target in targets {
        assert_eq!(
            ssa.matches(&format!("call @f{}(", target.index())).count(),
            2,
            "{ssa}"
        );
    }
    assert!(!ssa.contains("shared.payload_place"), "{ssa}");
    render_verified_program(&program).expect("original Unit LLVM verifies");
}

#[test]
fn string_binary_views_value_calls_and_returns_unit_comparison() {
    let text = SOURCE_STRING;
    let unit = analyze(text);
    let declaration = unit.declaration("combine", SymbolKind::Function);
    let span = unit.declaration_span(declaration);
    assert_eq!(
        unit.sources().slice(span),
        Ok(&text[text.find("fun combine").unwrap()..])
    );
    let (program, root) = unit
        .lower_entry(declaration)
        .expect("original combine Unit root");
    let module = program.module(root.module()).expect("root module");
    let combine = module.function(root).expect("exact combine function");
    assert_eq!(combine.origin, Origin::Source(span));
    let [left @ EntityId::Loan(left_loan), right @ EntityId::Value(_)] =
        combine.blocks[0].parameters.as_slice()
    else {
        panic!("original left Borrow and own right Value parameters");
    };
    let identity = unit.declaration("identity", SymbolKind::Function);
    let identity_span = unit.declaration_span(identity);
    assert_eq!(
        unit.sources().slice(identity_span),
        Ok("fun identity(own input: String): String = input")
    );
    let callees = module
        .functions
        .iter()
        .filter(|callee| callee.origin == Origin::Source(identity_span))
        .collect::<Vec<_>>();
    let [callee] = callees.as_slice() else {
        panic!("one exact identity declaration");
    };
    assert!(matches!(
        callee.blocks[0].parameters.as_slice(),
        [EntityId::Value(_)]
    ));
    let calls = combine
        .instructions
        .iter()
        .filter_map(|instruction| match &instruction.operation {
            Operation::DirectCall {
                callee: target,
                receiver,
                arguments,
            } if *target == callee.id => Some((receiver, arguments)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(receiver, arguments)] = calls.as_slice() else {
        panic!("one actual identity call");
    };
    assert!(receiver.is_none());
    assert!(matches!(arguments.as_slice(), [EntityId::Value(_)]));
    assert_eq!(
        arguments.as_slice(),
        &[*right],
        "identity receives the original owned right"
    );
    let [block] = combine.blocks.as_slice() else {
        panic!("straight-line String fixture stays in one execution block");
    };
    let operations = block
        .instructions
        .iter()
        .map(|id| combine.instruction(*id).expect("block instruction"))
        .collect::<Vec<_>>();
    let (literal_at, literal) = operations.iter().enumerate().find(|(_, instruction)| {
        matches!(&instruction.operation, Operation::StringLiteral { bytes, .. } if bytes == b"!")
    }).expect("original suffix literal");
    let [EntityId::Value(suffix)] = literal.results.as_slice() else {
        panic!("owned literal")
    };
    let (concat_at, concat) = operations
        .iter()
        .enumerate()
        .find(|(_, instruction)| {
            instruction.operation
                == Operation::StringConcat {
                    left: *left,
                    right: EntityId::Value(*suffix),
                }
        })
        .expect("concat reads the actual entry loan and literal result");
    let drop_at = operations
        .iter()
        .position(|instruction| instruction.operation == Operation::Drop { owner: *suffix })
        .expect("exact suffix owner is dropped");
    assert!(literal_at < concat_at && concat_at < drop_at);
    let first_drop = operations
        .iter()
        .position(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
        .unwrap();
    assert!(concat_at < first_drop);
    let (equal_at, equal) = operations
        .iter()
        .enumerate()
        .find(|(_, instruction)| {
            instruction.operation
                == Operation::StringEqual {
                    left: concat.results[0],
                    right: *right,
                }
        })
        .expect("comparison reads the concat result and original right");
    let EntityId::Value(equal_result) = equal.results[0] else {
        panic!("Boolean result")
    };
    let not_at = operations
        .iter()
        .position(|instruction| {
            instruction.operation
                == Operation::BooleanNot {
                    operand: equal_result,
                }
        })
        .expect("inequality negates the exact comparison result");
    let (call_at, call) = operations
        .iter()
        .enumerate()
        .find(|(_, instruction)| {
            matches!(&instruction.operation, Operation::DirectCall { callee: target, arguments, .. }
            if *target == callee.id && arguments == &[*right])
        })
        .expect("identity receives the same owned right after comparison");
    assert!(drop_at < equal_at && equal_at < not_at && not_at < call_at);
    let [EntityId::Value(result)] = call.results.as_slice() else {
        panic!("identity result")
    };
    assert!(matches!(&block.terminator.as_ref().expect("return").kind,
        TerminatorKind::Return { values } if values == &[*result]));
    verify_program(&program).expect("complete original Unit SSA verifies");
    let ssa = render_program(&program);
    assert!(
        ssa.contains(&format!(
            "string.concat %l{}, %v{}",
            left_loan.index(),
            suffix.index()
        )),
        "{ssa}"
    );
    // Bind the rendered concat to the actual entry loan, never a legacy %l0 assumption.
    for operation in ["string.concat", "string.equal", "not ", "call @"] {
        assert!(ssa.contains(operation), "{operation}: {ssa}");
    }
    render_verified_program(&program).expect("original Unit LLVM verifies");
}

const SOURCE_CLASS: &str = "class Resource {}\n\
         value class Token(val item: Int)\n\
         fun inspect(resource: Resource): Unit {}\n\
         fun inspectBox(resource: Box<Token>): Unit {}\n\
         fun exercise(): Unit {\n\
             val resource = Resource()\n\
             val first = inspect(resource)\n\
             val second = inspect(resource)\n\
             val boxed = Box(Token(1))\n\
             val third = inspectBox(boxed)\n\
             val fourth = inspectBox(boxed)\n\
         }";

const SOURCE_STRING: &str = "fun identity(own input: String): String = input\n\
         fun combine(left: String, own right: String): String {\n\
             val joined = left + \"!\"\n\
             val differs = joined != right\n\
             return identity(right)\n\
         }";
