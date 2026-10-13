//! SPEC-0309: inspect only unchanged Programs lowered from ordinary source.
use super::{
    LoweringErrorKind, borrow_result_contract_tests::single, borrow_storage_contract_tests::unit,
    model::*, verify::verify_program,
};
use crate::native_borrow_value_field_cases::{CLONE, COPYABLE, STRING};

fn named<'a>(program: &'a Program, name: &str) -> &'a Function {
    program.modules[0]
        .functions
        .iter()
        .find(|f| f.name.contains(name))
        .unwrap()
}

fn check(program: Program, copyable: bool) {
    verify_program(&program).unwrap();
    crate::llvm::render_verified_program(&program).unwrap();
    let f = named(&program, "observe");
    let parent = f.block(f.entry_block().unwrap()).unwrap().parameters[0];
    assert!(matches!(
        f.entity(parent).unwrap().ty,
        EntityType::Loan {
            kind: LoanKind::Shared,
            ..
        }
    ));
    let children = f
        .instructions
        .iter()
        .enumerate()
        .filter_map(|(i, instruction)| {
            if let Operation::SharedFieldLoan { base, field } = instruction.operation {
                assert_eq!(EntityId::Loan(base), parent);
                assert_eq!(field, usize::from(!copyable));
                Some((i, instruction.results[0]))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(
        children.len(),
        2,
        "each repeated access derives its own child"
    );
    for (begin, child) in children {
        let ends = f.instructions.iter().enumerate().filter_map(|(i, instruction)| {
            matches!(instruction.operation, Operation::BorrowEnd { loan } if EntityId::Loan(loan) == child).then_some(i)
        }).collect::<Vec<_>>();
        let [end] = ends.as_slice() else {
            panic!("one end per child")
        };
        assert!(begin < *end);
        assert!(f.instructions[begin + 1..*end].iter().any(|instruction| {
            if copyable {
                matches!(instruction.operation, Operation::Read { source: PlaceAccess::Loan(loan) } if EntityId::Loan(loan) == child)
            } else {
                matches!(instruction.operation, Operation::DirectCall { .. }) && instruction.operation.entities().contains(&child)
            }
        }));
    }
    for instruction in &f.instructions {
        assert!(
            !matches!(instruction.operation, Operation::BorrowEnd { loan } if EntityId::Loan(loan) == parent)
        );
        assert!(
            !matches!(instruction.operation, Operation::Read { source: PlaceAccess::Loan(loan) } if EntityId::Loan(loan) == parent)
        );
        assert!(!matches!(
            instruction.operation,
            Operation::Drop { .. }
                | Operation::StringClone { .. }
                | Operation::AggregateProject { .. }
        ));
    }
    let entry = named(&program, "entry");
    let calls = entry
        .instructions
        .iter()
        .enumerate()
        .filter_map(|(index, instruction)| {
            if let Operation::DirectCall {
                callee, arguments, ..
            } = &instruction.operation
            {
                Some((
                    index,
                    program.modules[0].function(*callee).unwrap(),
                    arguments,
                ))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let (_, _, delivered) = calls
        .iter()
        .find(|(_, function, _)| function.name.contains("finish"))
        .unwrap();
    let [EntityId::Value(owner)] = delivered.as_slice() else {
        panic!("whole owner delivery")
    };
    let reads = calls
        .iter()
        .filter(|(_, function, _)| function.name.contains("observe"))
        .collect::<Vec<_>>();
    assert_eq!(reads.len(), 2);
    for (call, _, arguments) in reads {
        let [EntityId::Loan(loan)] = arguments.as_slice() else {
            panic!("caller borrows owner")
        };
        let Definition::InstructionResult { instruction, .. } =
            entry.entity(EntityId::Loan(*loan)).unwrap().definition
        else {
            panic!("caller loan")
        };
        let Operation::BorrowBegin { place, .. } =
            entry.instruction(instruction).unwrap().operation
        else {
            panic!("root loan")
        };
        let Definition::InstructionResult { instruction, .. } =
            entry.entity(EntityId::Place(place)).unwrap().definition
        else {
            panic!("root storage")
        };
        assert!(
            matches!(entry.instruction(instruction).unwrap().operation, Operation::RootPlace { owner: root } if root == *owner)
        );
        let ends = entry.instructions.iter().enumerate().filter_map(|(i, instruction)| matches!(instruction.operation, Operation::BorrowEnd { loan: ended } if ended == *loan).then_some(i)).collect::<Vec<_>>();
        let [end] = ends.as_slice() else {
            panic!("one caller loan end")
        };
        let (delivery, _, _) = calls
            .iter()
            .find(|(_, function, _)| function.name.contains("finish"))
            .unwrap();
        assert!(*call < *end && *end < *delivery);
    }
    assert!(
        !entry.instructions.iter().any(
            |i| matches!(i.operation, Operation::Drop { owner: dropped } if dropped == *owner)
        )
    );
    let finish = named(&program, "finish");
    let owner = finish
        .block(finish.entry_block().unwrap())
        .unwrap()
        .parameters[0];
    assert!(matches!(owner, EntityId::Value(_)));
    let drops = finish.instructions.iter().enumerate().filter_map(|(i, instruction)| {
        matches!(instruction.operation, Operation::Drop { owner: dropped } if EntityId::Value(dropped) == owner).then_some(i)
    }).collect::<Vec<_>>();
    let [drop] = drops.as_slice() else {
        panic!("final consumer drops the whole owner once")
    };
    assert!(
        finish.instructions[..*drop]
            .iter()
            .any(|i| matches!(i.operation, Operation::DirectCall { .. }))
    );
}

#[test]
fn borrow_value_field_copyable_single() {
    check(single(COPYABLE).unwrap(), true);
}
#[test]
fn borrow_value_field_copyable_unit() {
    check(unit(COPYABLE).unwrap(), true);
}
#[test]
fn borrow_value_field_string_single() {
    check(single(STRING).unwrap(), false);
}
#[test]
fn borrow_value_field_string_unit() {
    check(unit(STRING).unwrap(), false);
}
#[test]
fn borrow_value_field_clone_controls() {
    for program in [single(CLONE), unit(CLONE)] {
        let program = program.unwrap();
        verify_program(&program).unwrap();
        crate::llvm::render_verified_program(&program).unwrap();
        assert!(
            named(&program, "observe")
                .instructions
                .iter()
                .any(|i| matches!(i.operation, Operation::StringClone { .. }))
        );
    }
}
#[test]
fn borrow_value_field_owned_generic_nested_boundaries() {
    for source in [
        "value class Holder(val text: String)\nfun entry() { val holder = Holder(\"text\"); println(holder.text) }",
        "value class Holder<T>(val text: T)\nfun observe(holder: Holder<String>) { println(holder.text) }\nfun entry() { val holder = Holder(\"text\"); observe(holder) }",
        "value class Inner(val text: String)\nvalue class Outer(val inner: Inner)\nfun observe(outer: Outer) { println(outer.inner.text) }\nfun entry() { val outer = Outer(Inner(\"text\")); observe(outer) }",
    ] {
        assert_eq!(
            unit(source).err().expect("bounded source rejection").kind,
            LoweringErrorKind::UnsupportedNode,
            "{source}"
        );
    }
}
