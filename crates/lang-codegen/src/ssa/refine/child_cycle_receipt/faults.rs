//! CI-only corruption of real emitted cycle state, checked before generic verification.
use super::super::build::owner_consumer_test_support::{OwnerFault, record_action, take_fault};
use super::*;
impl Emitted {
    pub(in crate::ssa::refine) fn inject(
        &mut self,
        body: &RefinedBody,
        function: &mut Function,
        retirement: &super::super::nested_body_receipt::Emitted,
    ) {
        if self.inject_scalar_entry(function) {
            return;
        }
        let faults = [
            OwnerFault::InitialConcatDuplicateInput,
            OwnerFault::InitialConcatMissingDrop,
            OwnerFault::InitialConcatEarlyDrop,
            OwnerFault::ChildCycleInitialSwap,
            OwnerFault::ChildCycleInitialConsumer,
            OwnerFault::ChildCycleParentTrueSwap,
            OwnerFault::ChildCycleParentEntryPredecessor,
            OwnerFault::ChildCycleParentEntryConsumer,
            OwnerFault::ChildCycleMissingReceipt,
            OwnerFault::ChildCycleMissingObservation,
            OwnerFault::ChildCycleEntrySwap,
            OwnerFault::ChildCycleTrueSwap,
            OwnerFault::ChildCycleBackedgeSwap,
            OwnerFault::ChildCycleHeaderGuard,
            OwnerFault::ChildCycleStaleSuccess,
            OwnerFault::ChildCycleMissingSuccessRoot,
            OwnerFault::ChildCycleFalseSwap,
            OwnerFault::ChildCycleCheckedPolarity,
            OwnerFault::ChildCycleAbortBackedge,
            OwnerFault::ChildCycleExtraPredecessor,
            OwnerFault::ChildCycleNewConsumer,
            OwnerFault::ChildCycleDuplicateAbortRoot,
        ];
        let Some(fault) = faults.into_iter().find(|fault| take_fault(*fault)) else {
            return;
        };
        assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(), Ok("true"));
        let p = self.plan.clone().expect("selected child cycle");
        let span = p.checked_span;
        retirement
            .finish(body, function, span)
            .expect("baseline retirement");
        let generations = retirement
            .generations(body, span)
            .expect("observed generations");
        self.finish(body, function, &generations, span)
            .expect("baseline independent cycle");
        let header = self.blocks[&p.header];
        let body_id = self.blocks[&p.body];
        let entry = self.blocks[&p.entry];
        let abort = self.blocks[&p.abort];
        let exit = self.blocks[&p.exit];
        let root_index =
            |role: BlockRef, root: Slot| self.slots[&role].iter().position(|s| *s == root).unwrap();
        let (root, input, new) = generations[0];
        let first = root_index(p.header, p.roots[0]);
        let second = root_index(p.header, p.roots[1]);
        match fault {
            OwnerFault::InitialConcatDuplicateInput
            | OwnerFault::InitialConcatMissingDrop
            | OwnerFault::InitialConcatEarlyDrop => {
                let source_role = p
                    .scalar_entry
                    .as_ref()
                    .and_then(|s| s.split.map(|roles| roles[0]))
                    .or(p.initial_base)
                    .expect("actual source entry");
                let block_id = self.blocks[&source_role];
                let (producer, left) = function
                    .instructions
                    .iter()
                    .find_map(|i| {
                        if i.block != block_id {
                            return None;
                        }
                        if let Operation::StringConcat { left, .. } = i.operation {
                            Some((i.id, left))
                        } else {
                            None
                        }
                    })
                    .expect("actual initial concat");
                if fault == OwnerFault::InitialConcatDuplicateInput {
                    let i = function
                        .instructions
                        .iter_mut()
                        .find(|i| i.id == producer)
                        .unwrap();
                    i.operation = Operation::StringConcat { left, right: left };
                } else {
                    let drop = function.instructions.iter().find(|i|
                        matches!(i.operation, Operation::Drop { owner } if EntityId::Value(owner) == left)
                    ).unwrap().id;
                    if fault == OwnerFault::InitialConcatMissingDrop {
                        function
                            .instructions
                            .iter_mut()
                            .find(|i| i.id == drop)
                            .unwrap()
                            .operation = Operation::PrintLiteral { bytes: vec![b'\n'] };
                    } else {
                        let block = function
                            .blocks
                            .iter_mut()
                            .find(|b| b.id == block_id)
                            .unwrap();
                        let at = block
                            .instructions
                            .iter()
                            .position(|id| *id == drop)
                            .unwrap();
                        block.instructions.remove(at);
                        let before = block
                            .instructions
                            .iter()
                            .position(|id| *id == producer)
                            .unwrap();
                        block.instructions.insert(before, drop);
                    }
                }
            }
            OwnerFault::ChildCycleInitialSwap => {
                let initial = p.initial_base.expect("selected base");
                let a = root_index(p.parent_header, p.roots[0]);
                let b = root_index(p.parent_header, p.roots[1]);
                let block = function
                    .blocks
                    .iter_mut()
                    .find(|b| b.id == self.blocks[&initial])
                    .unwrap();
                let TerminatorKind::Branch(edge) = &mut block.terminator.as_mut().unwrap().kind
                else {
                    panic!("initial");
                };
                assert_ne!(edge.arguments[a], edge.arguments[b]);
                edge.arguments.swap(a, b);
            }
            OwnerFault::ChildCycleInitialConsumer => {
                let initial = p.initial_base.expect("selected base");
                let owner = self.seen[&initial][&p.roots[1]];
                let EntityId::Value(owner) = owner else {
                    panic!("initial owner");
                };
                let instruction = function
                    .instructions
                    .iter_mut()
                    .find(|i| {
                        i.block == self.blocks[&initial]
                            && matches!(i.operation, Operation::Compare { .. })
                    })
                    .expect("scalar parent guard after initial literal");
                assert!(
                    !instruction
                        .operation
                        .entities()
                        .contains(&EntityId::Value(owner))
                );
                instruction.operation = Operation::Drop { owner };
            }

            OwnerFault::ChildCycleParentTrueSwap => {
                let a = root_index(p.entry, p.roots[0]);
                let b = root_index(p.entry, p.roots[1]);
                let block = function
                    .blocks
                    .iter_mut()
                    .find(|b| b.id == self.blocks[&p.parent_header])
                    .unwrap();
                let TerminatorKind::Conditional { when_true, .. } =
                    &mut block.terminator.as_mut().unwrap().kind
                else {
                    panic!("parent header");
                };
                assert_ne!(when_true.arguments[a], when_true.arguments[b]);
                when_true.arguments.swap(a, b);
            }
            OwnerFault::ChildCycleParentEntryPredecessor => {
                let block = function.blocks.iter_mut().find(|b| b.id == exit).unwrap();
                assert!(
                    !block
                        .terminator
                        .as_ref()
                        .unwrap()
                        .kind
                        .targets()
                        .contains(&entry)
                );
                block.terminator.as_mut().unwrap().kind = TerminatorKind::Branch(Edge {
                    target: entry,
                    arguments: self.slots[&p.entry]
                        .iter()
                        .map(|s| self.seen[&p.parent_header][s])
                        .collect(),
                });
            }
            OwnerFault::ChildCycleParentEntryConsumer => {
                let value = self.parameter(p.entry, root, span).unwrap();
                let EntityId::Value(owner) = value else {
                    panic!("entry root");
                };
                let id = *function
                    .block(entry)
                    .unwrap()
                    .instructions
                    .first()
                    .expect("scalar child initialization");
                let instruction = function
                    .instructions
                    .iter_mut()
                    .find(|i| i.id == id)
                    .unwrap();
                assert!(!instruction.operation.entities().contains(&value));
                instruction.operation = Operation::Drop { owner };
            }

            OwnerFault::ChildCycleMissingReceipt => {
                self.plan.take();
            }
            OwnerFault::ChildCycleMissingObservation => {
                self.seen.remove(&p.backedge);
            }
            OwnerFault::ChildCycleEntrySwap | OwnerFault::ChildCycleBackedgeSwap => {
                let from = if fault == OwnerFault::ChildCycleEntrySwap {
                    entry
                } else {
                    self.blocks[&p.backedge]
                };
                let b = function.blocks.iter_mut().find(|b| b.id == from).unwrap();
                let TerminatorKind::Branch(edge) = &mut b.terminator.as_mut().unwrap().kind else {
                    panic!("entry");
                };
                assert_ne!(edge.arguments[first], edge.arguments[second]);
                edge.arguments.swap(first, second);
            }
            OwnerFault::ChildCycleHeaderGuard => {
                let EntityId::Value(flag) = self.checked.as_ref().unwrap().results[1] else {
                    panic!("flag");
                };
                let b = function.blocks.iter_mut().find(|b| b.id == header).unwrap();
                let TerminatorKind::Conditional { condition, .. } =
                    &mut b.terminator.as_mut().unwrap().kind
                else {
                    panic!("header");
                };
                assert_ne!(*condition, flag);
                *condition = flag;
            }
            OwnerFault::ChildCycleStaleSuccess
            | OwnerFault::ChildCycleMissingSuccessRoot
            | OwnerFault::ChildCycleCheckedPolarity
            | OwnerFault::ChildCycleDuplicateAbortRoot => {
                let position = root_index(p.backedge, root);
                let abort_position = root_index(p.abort, root);
                let b = function
                    .blocks
                    .iter_mut()
                    .find(|b| b.id == body_id)
                    .unwrap();
                let TerminatorKind::Conditional {
                    when_true,
                    when_false,
                    ..
                } = &mut b.terminator.as_mut().unwrap().kind
                else {
                    panic!("checked");
                };
                if fault == OwnerFault::ChildCycleStaleSuccess {
                    assert_eq!(when_false.arguments[position], new);
                    assert_ne!(input, new);
                    when_false.arguments[position] = input;
                } else if fault == OwnerFault::ChildCycleMissingSuccessRoot {
                    assert_eq!(when_false.arguments[position], new);
                    when_false.arguments.remove(position);
                } else if fault == OwnerFault::ChildCycleCheckedPolarity {
                    std::mem::swap(when_true, when_false);
                } else {
                    assert_eq!(when_true.arguments[abort_position], new);
                    when_true.arguments.push(new);
                }
            }
            OwnerFault::ChildCycleTrueSwap => {
                let a = root_index(p.body, p.roots[0]);
                let b = root_index(p.body, p.roots[1]);
                let block = function.blocks.iter_mut().find(|b| b.id == header).unwrap();
                let TerminatorKind::Conditional { when_true, .. } =
                    &mut block.terminator.as_mut().unwrap().kind
                else {
                    panic!("header");
                };
                assert_ne!(when_true.arguments[a], when_true.arguments[b]);
                when_true.arguments.swap(a, b);
            }
            OwnerFault::ChildCycleFalseSwap => {
                let a = root_index(p.exit, p.roots[0]);
                let b = root_index(p.exit, p.roots[1]);
                let block = function.blocks.iter_mut().find(|b| b.id == header).unwrap();
                let TerminatorKind::Conditional { when_false, .. } =
                    &mut block.terminator.as_mut().unwrap().kind
                else {
                    panic!("header");
                };
                assert_ne!(when_false.arguments[a], when_false.arguments[b]);
                when_false.arguments.swap(a, b);
            }
            OwnerFault::ChildCycleAbortBackedge | OwnerFault::ChildCycleExtraPredecessor => {
                let from = if fault == OwnerFault::ChildCycleAbortBackedge {
                    abort
                } else {
                    exit
                };
                let args = self.slots[&p.header]
                    .iter()
                    .map(|slot| self.seen[&p.entry][slot])
                    .collect();
                let block = function.blocks.iter_mut().find(|b| b.id == from).unwrap();
                assert!(
                    !block
                        .terminator
                        .as_ref()
                        .unwrap()
                        .kind
                        .targets()
                        .contains(&header)
                );
                block.terminator.as_mut().unwrap().kind = TerminatorKind::Branch(Edge {
                    target: header,
                    arguments: args,
                });
            }
            OwnerFault::ChildCycleNewConsumer => {
                let EntityId::Value(new) = new else {
                    panic!("new owner");
                };
                let drop = function
                    .instructions
                    .iter_mut()
                    .find(|i| i.block == body_id && matches!(i.operation, Operation::Drop { .. }))
                    .unwrap();
                assert!(!drop.operation.entities().contains(&EntityId::Value(new)));
                drop.operation = Operation::Drop { owner: new };
            }
            _ => unreachable!("selected cycle fault"),
        }
        record_action("child-cycle-fault-hit", 1);
        assert!(
            self.finish(body, function, &generations, span).is_err(),
            "independent child-cycle must reject before generic verifier"
        );
        record_action("child-cycle-independent-rejection", 1);
    }
}
