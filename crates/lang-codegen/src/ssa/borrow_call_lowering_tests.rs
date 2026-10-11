//! 普通源码覆盖透明分组及字段来源调用，不改写前端事实或 SSA。
use super::{
    borrow_result_contract_tests::single, borrow_storage_contract_tests::unit, model::*,
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
