//! CI-only real SSA corruption, checked directly by the composition receipt.
use super::super::build::owner_consumer_test_support::{OwnerFault, record_action, take_fault};
use super::*;
use crate::ssa::model::{Definition, LoanKind};
impl Emitted {
    pub(in super::super) fn inject_composition(&self, body: &RefinedBody, function: &mut Function) {
        let faults = [
            OwnerFault::ParentCompositionCarrySwap,
            OwnerFault::ParentCompositionUnchangedConsume,
            OwnerFault::ParentCompositionSharedLoan,
            OwnerFault::ParentCompositionNewConsume,
            OwnerFault::ParentCompositionDuplicateProducer,
        ];
        let Some(fault) = faults.into_iter().find(|f| take_fault(*f)) else {
            return;
        };
        assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(), Ok("true"));
        let child = body.child_cycle_plans.0.as_ref().unwrap();
        let p = body.nested_resume_plans.0.as_ref().unwrap();
        let span = p.spans[0];
        self.finish(body, function, span).expect("baseline resume");
        self.compose(body, function, span)
            .expect("baseline composition");
        let (replace, _, _, new, _) = self.replace.unwrap();
        match fault {
            OwnerFault::ParentCompositionDuplicateProducer => {
                let (_, _, rhs, _, _) = self.replace.unwrap();
                let Definition::InstructionResult {
                    instruction,
                    index: 0,
                } = function.entity(rhs).unwrap().definition
                else {
                    panic!("literal producer");
                };
                let block = function
                    .blocks
                    .iter_mut()
                    .find(|b| b.id == self.blocks[&p.resume])
                    .unwrap();
                assert_eq!(
                    block
                        .instructions
                        .iter()
                        .filter(|id| **id == instruction)
                        .count(),
                    1
                );
                let at = block
                    .instructions
                    .iter()
                    .position(|id| *id == instruction)
                    .unwrap();
                block.instructions.insert(at, instruction);
            }
            OwnerFault::ParentCompositionCarrySwap => {
                let c = p.carry.as_ref().unwrap();
                let (from, to) = c.edges[0];
                let a = self.slots[&to]
                    .iter()
                    .position(|r| *r == child.roots[0])
                    .unwrap();
                let b = self.slots[&to]
                    .iter()
                    .position(|r| *r == child.roots[1])
                    .unwrap();
                let block = function
                    .blocks
                    .iter_mut()
                    .find(|b| b.id == self.blocks[&from])
                    .unwrap();
                let edge = match &mut block.terminator.as_mut().unwrap().kind {
                    TerminatorKind::Branch(e) => e,
                    TerminatorKind::Conditional { when_false, .. } => when_false,
                    _ => panic!("carry"),
                };
                assert_ne!(edge.arguments[a], edge.arguments[b]);
                edge.arguments.swap(a, b);
            }
            OwnerFault::ParentCompositionUnchangedConsume
            | OwnerFault::ParentCompositionNewConsume => {
                let root = *child.roots.iter().find(|r| **r != p.root).unwrap();
                let pos = self.slots[&p.resume]
                    .iter()
                    .position(|s| *s == root)
                    .unwrap();
                let (block, value) = if fault == OwnerFault::ParentCompositionUnchangedConsume {
                    (p.resume, self.parameters[&p.resume][pos])
                } else {
                    (child.initial_base.unwrap(), new)
                };
                let EntityId::Value(owner) = value else {
                    panic!("value");
                };
                let i = function
                    .instructions
                    .iter_mut()
                    .find(|i| {
                        i.block == self.blocks[&block]
                            && matches!(i.operation, Operation::Constant(_))
                    })
                    .expect("scalar constant");
                assert!(!i.operation.entities().contains(&value));
                i.operation = Operation::Drop { owner };
            }
            OwnerFault::ParentCompositionSharedLoan => {
                let Operation::RootReplace { loan, .. } =
                    function.instruction(replace).unwrap().operation
                else {
                    panic!("replace");
                };
                let Definition::InstructionResult {
                    instruction,
                    index: 0,
                } = function.entity(EntityId::Loan(loan)).unwrap().definition
                else {
                    panic!("loan");
                };
                let i = function
                    .instructions
                    .iter_mut()
                    .find(|i| i.id == instruction)
                    .unwrap();
                let Operation::BorrowBegin { kind, .. } = &mut i.operation else {
                    panic!("borrow");
                };
                assert_eq!(*kind, LoanKind::Exclusive);
                *kind = LoanKind::Shared;
            }
            _ => unreachable!(),
        }
        record_action("child-cycle-fault-hit", 1);
        assert!(
            self.compose(body, function, span).is_err(),
            "composition must independently reject"
        );
        record_action("child-cycle-independent-rejection", 1);
    }
}
