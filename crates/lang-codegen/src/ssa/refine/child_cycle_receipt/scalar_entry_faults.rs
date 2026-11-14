//! CI-only mutations, checked directly by the scalar-entry receipt.
use super::super::build::owner_consumer_test_support::{OwnerFault, record_action, take_fault};
use super::*;
use crate::ssa::model::Definition;
impl Emitted {
    pub(super) fn inject_scalar_entry(&self, function: &mut Function) -> bool {
        use OwnerFault::*;
        let Some(fault) = [
            ScalarEntryArmConsume,
            ScalarEntryJoinEscape,
            ScalarEntryArmDefinition,
            ScalarEntryArmSwap,
            ScalarEntryJoinReentry,
            ScalarEntryWrongCall,
        ]
        .into_iter()
        .find(|f| take_fault(*f)) else {
            return false;
        };
        assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(), Ok("true"));
        let p = self.plan.as_ref().unwrap();
        let prefix = p.scalar_entry.as_ref().unwrap();
        let [head, arm, _] = prefix.split.unwrap();
        let join = prefix.join.unwrap();
        let span = p.checked_span;
        self.scalar_initial_base(function, span)
            .expect("uncorrupted scalar entry");
        let arm_owner = self.parameter(arm, p.roots[0], span).unwrap();
        let join_owner = self.parameter(join, p.roots[0], span).unwrap();
        match fault {
            ScalarEntryArmConsume => {
                let EntityId::Value(owner) = arm_owner else {
                    panic!("value");
                };
                let instruction = function
                    .instructions
                    .iter_mut()
                    .find(|i| {
                        i.block == self.blocks[&p.body]
                            && matches!(i.operation, Operation::Constant(_))
                    })
                    .expect("later scalar constant");
                instruction.operation = Operation::Drop { owner };
            }
            ScalarEntryJoinEscape => {
                let EntityId::Value(owner) = join_owner else {
                    panic!("value");
                };
                let block = function
                    .blocks
                    .iter_mut()
                    .find(|b| b.id == self.blocks[&p.abort])
                    .unwrap();
                block.terminator.as_mut().unwrap().kind = TerminatorKind::Return {
                    values: vec![owner],
                };
            }
            ScalarEntryArmDefinition => {
                let EntityId::Value(owner) = arm_owner else {
                    panic!("value");
                };
                function.values[owner.index()].definition = Definition::BlockParameter {
                    block: self.blocks[&head],
                    index: 0,
                };
            }
            ScalarEntryArmSwap => {
                let a = self.slots[&join]
                    .iter()
                    .position(|s| *s == p.roots[0])
                    .unwrap();
                let b = self.slots[&join]
                    .iter()
                    .position(|s| *s == p.roots[1])
                    .unwrap();
                let block = function
                    .blocks
                    .iter_mut()
                    .find(|b| b.id == self.blocks[&arm])
                    .unwrap();
                let TerminatorKind::Branch(edge) = &mut block.terminator.as_mut().unwrap().kind
                else {
                    panic!("arm edge");
                };
                edge.arguments.swap(a, b);
            }
            ScalarEntryJoinReentry => {
                let target = self.blocks[&join];
                let block = function
                    .blocks
                    .iter_mut()
                    .find(|b| b.id == self.blocks[&p.backedge])
                    .unwrap();
                let TerminatorKind::Branch(edge) = &mut block.terminator.as_mut().unwrap().kind
                else {
                    panic!("backedge");
                };
                edge.target = target;
            }
            ScalarEntryWrongCall => {
                let instruction = function
                    .instructions
                    .iter_mut()
                    .find(|i| {
                        i.block == self.blocks[&arm]
                            && matches!(i.operation, Operation::DirectCall { .. })
                    })
                    .unwrap();
                let Operation::DirectCall { callee, .. } = &mut instruction.operation else {
                    unreachable!();
                };
                assert_ne!(*callee, function.id);
                *callee = function.id;
            }
            _ => unreachable!(),
        }
        record_action("scalar-entry-fault-hit", 1);
        assert!(
            self.scalar_initial_base(function, span).is_err(),
            "independent scalar entry rejection"
        );
        record_action("scalar-entry-independent-rejection", 1);
        true
    }
}
