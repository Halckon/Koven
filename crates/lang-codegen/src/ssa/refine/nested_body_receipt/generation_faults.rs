//! CI-only late corruption checked directly by the child generation receipt.
use super::super::build::owner_consumer_test_support::{OwnerFault, record_action, take_fault};
use super::*;
use crate::ssa::model::{Definition, LoanKind};
impl Emitted {
    pub(in crate::ssa::refine) fn inject_generation(
        &self,
        body: &RefinedBody,
        function: &mut Function,
    ) {
        let faults = [
            OwnerFault::ChildGenerationProducer,
            OwnerFault::ChildGenerationSharedLoan,
            OwnerFault::ChildGenerationExtraLoanUse,
            OwnerFault::ChildGenerationDuplicateBorrowPlacement,
            OwnerFault::ChildGenerationDuplicateLiteralPlacement,
            OwnerFault::ChildGenerationDuplicateDropPlacement,
        ];
        let Some(fault) = faults.into_iter().find(|f| take_fault(*f)) else {
            return;
        };
        assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(), Ok("true"));
        let p = body.nested_body_plans.0.as_ref().unwrap();
        let span = p.steps[0].spans[0];
        self.finish(body, function, span)
            .expect("baseline child source and retirement");
        let id = self.observed[0].replace.as_ref().unwrap().0;
        let r = function.instruction(id).unwrap();
        let block = r.block;
        let Operation::RootReplace {
            loan, replacement, ..
        } = r.operation
        else {
            panic!("replace");
        };
        let Definition::InstructionResult {
            instruction: producer,
            ..
        } = function
            .entity(EntityId::Value(replacement))
            .unwrap()
            .definition
        else {
            panic!("producer");
        };
        let Definition::InstructionResult {
            instruction: borrow,
            ..
        } = function.entity(EntityId::Loan(loan)).unwrap().definition
        else {
            panic!("borrow");
        };
        match fault {
            OwnerFault::ChildGenerationDuplicateBorrowPlacement
            | OwnerFault::ChildGenerationDuplicateLiteralPlacement
            | OwnerFault::ChildGenerationDuplicateDropPlacement => {
                let duplicate = match fault {
                    OwnerFault::ChildGenerationDuplicateLiteralPlacement => producer,
                    OwnerFault::ChildGenerationDuplicateDropPlacement => {
                        self.observed[0].drop.unwrap()
                    }
                    _ => borrow,
                };
                // Keep the instruction arena, definition and original placement intact.
                // Only add a second placement in the real function entry.
                let entry = function.entry_block().unwrap();
                assert_ne!(entry, block);
                let other = function.blocks.iter_mut().find(|b| b.id == entry).unwrap();
                assert!(!other.instructions.contains(&duplicate));
                other.instructions.push(duplicate);
            }
            OwnerFault::ChildGenerationProducer => {
                let i = function
                    .instructions
                    .iter_mut()
                    .find(|i| i.id == producer)
                    .unwrap();
                assert!(matches!(i.operation, Operation::StringLiteral { .. }));
                i.operation = Operation::Consume { owner: replacement };
            }
            OwnerFault::ChildGenerationSharedLoan => {
                let i = function
                    .instructions
                    .iter_mut()
                    .find(|i| i.id == borrow)
                    .unwrap();
                let Operation::BorrowBegin { kind, .. } = &mut i.operation else {
                    panic!("borrow");
                };
                assert_eq!(*kind, LoanKind::Exclusive);
                *kind = LoanKind::Shared;
            }
            OwnerFault::ChildGenerationExtraLoanUse => {
                let i = function
                    .instructions
                    .iter_mut()
                    .find(|i| i.block == block && matches!(i.operation, Operation::Constant(_)))
                    .expect("counter literal");
                i.operation = Operation::BorrowEnd { loan };
            }
            _ => unreachable!(),
        }
        record_action("child-cycle-fault-hit", 1);
        assert!(
            self.finish(body, function, span).is_err(),
            "child generation receipt must reject before generic verifier"
        );
        record_action("child-cycle-independent-rejection", 1);
    }
}
