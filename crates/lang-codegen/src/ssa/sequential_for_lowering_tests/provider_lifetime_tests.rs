use super::super::{
    model::{EntityId, Operation, Program},
    verify::verify_program,
};

fn fixture() -> Program {
    super::analyze(
        "fun scan(xs: Array<Int>): Int { var total = 0; for (item in xs) { total = total + item }; return total }",
    )
}

#[test]
fn provider_stale_place_is_rejected_after_source_end() {
    let mut program = fixture();
    let function = &mut program.modules[0].functions[0];
    let element = function
        .instructions
        .iter()
        .find(|instruction| {
            matches!(
                instruction.operation,
                Operation::ContainerElementPlace { .. }
            )
        })
        .unwrap()
        .clone();
    let Operation::ContainerElementPlace {
        owner: EntityId::Loan(source),
        ..
    } = element.operation
    else {
        panic!("source loan")
    };
    let terminator = function.blocks[element.block.index()].terminator.take();
    let (end, _) = function
        .append_instruction(
            element.block,
            Operation::BorrowEnd { loan: source },
            vec![],
            element.origin.clone(),
        )
        .unwrap();
    let block = &mut function.blocks[element.block.index()];
    block.instructions.retain(|id| *id != end);
    let position = block
        .instructions
        .iter()
        .position(|id| *id == element.id)
        .unwrap();
    block.instructions.insert(position + 1, end);
    block.terminator = terminator;
    let error = verify_program(&program).expect_err("place cannot outlive provider source");
    assert!(
        error.errors.iter().any(|error| matches!(
            error.kind,
            super::super::verify::VerifyErrorKind::PlaceUnavailable { .. }
        )),
        "{error:?}"
    );
}

#[test]
fn entry_reborrow_cannot_revive_a_finished_provider_source() {
    let mut program = fixture();
    let function = &mut program.modules[0].functions[0];
    let element = function
        .instructions
        .iter()
        .find(|instruction| {
            matches!(
                instruction.operation,
                Operation::ContainerElementPlace { .. }
            )
        })
        .unwrap()
        .clone();
    let Operation::ContainerElementPlace {
        owner: EntityId::Loan(source),
        ..
    } = element.operation
    else {
        panic!("source loan")
    };
    let target = function.entity(EntityId::Loan(source)).unwrap().ty;
    let terminator = function.blocks[element.block.index()].terminator.take();
    let (end, _) = function
        .append_instruction(
            element.block,
            Operation::BorrowEnd { loan: source },
            vec![],
            element.origin.clone(),
        )
        .unwrap();
    let (reborrow, _) = function
        .append_instruction(
            element.block,
            Operation::SharedReborrow { source },
            vec![target],
            element.origin.clone(),
        )
        .unwrap();
    let block = &mut function.blocks[element.block.index()];
    block
        .instructions
        .retain(|id| *id != end && *id != reborrow);
    let position = block
        .instructions
        .iter()
        .position(|id| *id == element.id)
        .unwrap();
    block
        .instructions
        .splice(position..position, [end, reborrow]);
    block.terminator = terminator;
    let error = verify_program(&program).expect_err("finished source cannot be reborrowed");
    assert!(
        error.errors.iter().any(|error| matches!(
            error.kind,
            super::super::verify::VerifyErrorKind::LoanInactive { .. }
        )),
        "{error:?}"
    );
}
