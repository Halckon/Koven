//! 普通源码覆盖透明分组及字段来源调用，不改写前端事实或 SSA。
use super::{
    borrow_memory_test_support::single, borrow_storage_contract_tests::unit, model::*,
    verify::verify_program,
};

const GROUPED_BINDING: &str = "fun view(source: String): borrow String from source = source\nfun consume(own source: String) { println(source) }\nfun entry() { val source = \"kept\".clone(); borrow val item = ((view(source))); println(item); consume(source) }";
const GROUPED_RETURN: &str = "fun view(source: String): borrow String from source = source\nfun wrap(source: String): borrow String from source = ((view(source)))\nfun entry() { val source = \"kept\".clone(); borrow val item = wrap(source); println(item) }";
const FIELD_RETURN: &str = "class Packet(val text: String)\nfun view(source: String): borrow String from source = source\nfun wrap(source: Packet): borrow String from source = view(source.text)\nfun consume(own source: Packet) {}\nfun entry() { val source = Packet(\"kept\".clone()); borrow val item = wrap(source); println(item); consume(source) }";

fn assert_borrow_call(program: &Program) {
    verify_program(program).unwrap();
    let entry = program.modules[0]
        .functions
        .iter()
        .find(|function| function.name.contains("entry"))
        .unwrap();
    let call = entry
        .instructions
        .iter()
        .find(|instruction| matches!(instruction.operation, Operation::BorrowCall { .. }))
        .unwrap();
    let Operation::BorrowCall { source, .. } = call.operation else {
        unreachable!()
    };
    let [EntityId::Loan(result)] = call.results.as_slice() else {
        panic!("borrow call must produce a loan");
    };
    let print = entry.instructions.iter().position(|instruction| {
        matches!(instruction.operation, Operation::PrintString { value } if value == *result)
    }).unwrap();
    let end = |loan| {
        entry.instructions.iter().position(|instruction| {
        matches!(instruction.operation, Operation::BorrowEnd { loan: ended } if ended == loan)
    }).unwrap()
    };
    assert!(print < end(*result) && end(*result) < end(source));
    crate::llvm::render_verified_program(program).unwrap();
}

#[test]
fn abandoned_call_prefix_demands_only_completed_nullable_parameter_storage() {
    let text = "fun unreachableCallee(number: Int?, own stop: Int, own suffix: Boolean?): Unit {}\nfun entry() { unreachableCallee(7, return, false) }";
    let program = unit(text).unwrap();
    verify_program(&program).unwrap();
    let module = &program.modules[0];
    assert!(
        !module
            .functions
            .iter()
            .any(|function| { function.name.contains("unreachableCallee") })
    );
    let wrappers: Vec<_> = module
        .types
        .iter()
        .filter_map(|ty| {
            if let SsaTypeKind::InlineNullable { inner, .. } = ty {
                Some(&module.types[inner.index()])
            } else {
                None
            }
        })
        .collect();
    assert_eq!(wrappers.len(), 1);
    assert_ne!(wrappers[0], &SsaTypeKind::Boolean);
    let entry = module
        .functions
        .iter()
        .find(|function| function.name.contains("entry"))
        .unwrap();
    assert!(
        entry
            .instructions
            .iter()
            .any(|instruction| { matches!(instruction.operation, Operation::NullableWrap { .. }) })
    );
}

#[test]
fn temporary_field_borrow_uses_the_field_loan_or_copies_before_wrapping() {
    for parameter in ["Int", "Int?"] {
        let program = unit(&format!(
            "value class Leaf(val number: Int)\nclass Holder(val leaf: Leaf)\nfun read(number: {parameter}): Unit {{}}\nfun entry() {{ read((Holder(Leaf(7))).leaf.number) }}"
        ))
        .unwrap();
        verify_program(&program).unwrap();
        let entry = program.modules[0]
            .functions
            .iter()
            .find(|function| function.name.contains("entry"))
            .unwrap();
        let instructions = &entry.instructions;
        let field = instructions
            .iter()
            .find(|instruction| matches!(instruction.operation, Operation::SharedFieldLoan { .. }))
            .unwrap();
        let Operation::SharedFieldLoan { base: heap, .. } = field.operation else {
            unreachable!()
        };
        let [EntityId::Loan(field)] = field.results.as_slice() else {
            panic!("field projection must produce a loan");
        };
        let producer = |entity| {
            instructions
                .iter()
                .find(|instruction| instruction.results.contains(&entity))
                .unwrap()
        };
        let Operation::SharedHeapFieldLoan { base: root, .. } =
            producer(EntityId::Loan(heap)).operation
        else {
            panic!("inline field must descend from the heap field");
        };
        let (call, delivered) = instructions
            .iter()
            .enumerate()
            .find_map(|(index, instruction)| {
                let Operation::DirectCall { arguments, .. } = &instruction.operation else {
                    return None;
                };
                let [EntityId::Loan(loan)] = arguments.as_slice() else {
                    return None;
                };
                Some((index, *loan))
            })
            .unwrap();
        let end = |loan| {
            instructions
                .iter()
                .position(|instruction| instruction.operation == Operation::BorrowEnd { loan })
                .unwrap()
        };
        assert!(end(*field) < end(heap) && end(heap) < end(root));
        if parameter == "Int" {
            assert_eq!(delivered, *field);
            assert!(call < end(*field));
        } else {
            let read = instructions.iter().position(|instruction| {
                matches!(instruction.operation, Operation::Read { source: PlaceAccess::Loan(source) } if source == *field)
            }).unwrap();
            let wrap = instructions
                .iter()
                .position(|instruction| {
                    matches!(instruction.operation, Operation::NullableWrap { .. })
                })
                .unwrap();
            let Operation::BorrowBegin { place, .. } =
                producer(EntityId::Loan(delivered)).operation
            else {
                panic!("the nullable argument must borrow its own slot");
            };
            let [EntityId::Value(wrapper)] = instructions[wrap].results.as_slice() else {
                panic!("expected the wrapped nullable value");
            };
            assert_eq!(
                producer(EntityId::Place(place)).operation,
                Operation::RootPlace { owner: *wrapper }
            );
            assert!(read < end(*field) && end(root) < wrap && wrap < call);
            assert!(call < end(delivered));
        }
    }
}

#[test]
fn grouped_binding_single_preserves_returned_loan() {
    assert_borrow_call(&single(GROUPED_BINDING).unwrap());
}

#[test]
fn grouped_binding_unit_preserves_returned_loan() {
    assert_borrow_call(&unit(GROUPED_BINDING).unwrap());
}

#[test]
fn grouped_return_single_preserves_forwarded_loan() {
    assert_borrow_call(&single(GROUPED_RETURN).unwrap());
}

#[test]
fn grouped_return_unit_preserves_forwarded_loan() {
    assert_borrow_call(&unit(GROUPED_RETURN).unwrap());
}

#[test]
fn field_return_unit_preserves_projected_source_loan() {
    let program = unit(FIELD_RETURN).unwrap();
    assert_borrow_call(&program);
    let entry = program.modules[0]
        .functions
        .iter()
        .find(|function| function.name.contains("entry"))
        .unwrap();
    let source = entry
        .instructions
        .iter()
        .find_map(|instruction| {
            if let Operation::BorrowCall { source, .. } = instruction.operation {
                Some(source)
            } else {
                None
            }
        })
        .unwrap();
    let source_end = entry.instructions.iter().position(|instruction| {
        matches!(instruction.operation, Operation::BorrowEnd { loan } if loan == source)
    }).unwrap();
    let consume = entry
        .instructions
        .iter()
        .position(|instruction| {
            matches!(&instruction.operation, Operation::DirectCall { arguments, .. }
            if matches!(arguments.as_slice(), [EntityId::Value(_)]))
        })
        .unwrap();
    assert!(
        source_end < consume,
        "last use restores the owner's move permission"
    );
    let wrapper = program.modules[0]
        .functions
        .iter()
        .find(|f| f.name.contains("wrap"))
        .unwrap();
    let field = wrapper
        .instructions
        .iter()
        .find(|instruction| matches!(instruction.operation, Operation::SharedHeapFieldLoan { .. }))
        .unwrap();
    let [EntityId::Loan(field_loan)] = field.results.as_slice() else {
        panic!("field projection must produce a loan");
    };
    assert!(wrapper.instructions.iter().any(|instruction| {
        matches!(instruction.operation, Operation::BorrowCall { source, .. } if source == *field_loan)
    }));
    assert!(
        !wrapper.instructions.iter().any(|instruction| {
            matches!(instruction.operation, Operation::BorrowEnd { loan } if loan == *field_loan)
        }),
        "forwarded source must remain live through BorrowReturn"
    );
}

#[test]
fn nested_nullable_copy_keeps_unrelated_drop_before_the_read() {
    let text = "class Boxed(val value: Int)\nclass Aux() { deinit() { println(\"aux\") } }\nfun view(source: Boxed, aux: Aux): borrow Int from source = source.value\nfun observe(value: Int?): Unit {}\nfun entry() { val boxed = Boxed(7); observe(view(boxed, Aux())) }";
    let program = unit(text).unwrap();
    verify_program(&program).unwrap();
    let entry = program.modules[0]
        .functions
        .iter()
        .find(|function| function.name.contains("entry"))
        .unwrap();
    let (call, result, source) = entry
        .instructions
        .iter()
        .enumerate()
        .find_map(|(index, instruction)| {
            match (&instruction.operation, instruction.results.as_slice()) {
                (Operation::BorrowCall { source, .. }, [EntityId::Loan(result)]) => {
                    Some((index, *result, *source))
                }
                _ => None,
            }
        })
        .unwrap();
    let position = |operation| {
        entry
            .instructions
            .iter()
            .position(|i| i.operation == operation)
            .unwrap()
    };
    let read = position(Operation::Read {
        source: PlaceAccess::Loan(result),
    });
    let end_result = position(Operation::BorrowEnd { loan: result });
    let end_source = position(Operation::BorrowEnd { loan: source });
    let drops = entry
        .instructions
        .iter()
        .enumerate()
        .filter_map(|(index, instruction)| {
            matches!(instruction.operation, Operation::Drop { .. }).then_some(index)
        })
        .collect::<Vec<_>>();
    let [drop_aux, drop_source] = drops.as_slice() else {
        panic!("expected one auxiliary and one source drop, got {drops:?}");
    };
    assert!(call < *drop_aux && *drop_aux < read);
    assert!(read < end_result && end_result < end_source && end_source < *drop_source);
}
