//! Finite audit model seeded by real sealed planner evidence, never a runtime trace.
//! Model instances are NOT UnitOwnerId values and cannot enter public products.
use super::*;
use crate::name_resolution::UnitSymbolId;
use crate::ownership_checking::{UnitDropFact, UnitOwnerId};
use std::sync::Arc;

#[derive(Clone, Debug)]
struct ModeledInstance {
    model: Arc<()>,
    definition: UnitOwnerId,
    born_at: usize,
}
impl PartialEq for ModeledInstance {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.model, &other.model)
            && self.definition == other.definition
            && self.born_at == other.born_at
    }
}
impl Eq for ModeledInstance {}
struct Slot {
    root: UnitSymbolId,
    entry: UnitOwnerId,
    body: UnitOwnerId,
    retirement: UnitDropFact,
}
struct ActivationModel<'a> {
    identity: Arc<()>,
    slots: &'a [Slot],
    current: Vec<(UnitSymbolId, ModeledInstance)>,
    retired: Vec<(ModeledInstance, UnitDropFact)>,
    round: usize,
}
impl<'a> ActivationModel<'a> {
    fn new(slots: &'a [Slot]) -> Self {
        let identity = Arc::new(());
        Self {
            identity: identity.clone(),
            slots,
            current: slots
                .iter()
                .map(|slot| {
                    (
                        slot.root,
                        ModeledInstance {
                            model: identity.clone(),
                            definition: slot.entry.clone(),
                            born_at: 0,
                        },
                    )
                })
                .collect(),
            retired: Vec::new(),
            round: 0,
        }
    }
    // A modeled normal-body transfer, not replay of planner events or allocation.
    fn normal_prefix_step(&mut self) {
        assert_eq!(self.current.len(), self.slots.len());
        let next_round = self.round + 1;
        let mut next = Vec::new();
        let mut consumed = Vec::new();
        for (slot, (root, old)) in self.slots.iter().zip(&self.current) {
            assert_eq!(slot.root, *root);
            assert!(!self.retired.iter().any(|(prior, _)| prior == old));
            let new = ModeledInstance {
                model: self.identity.clone(),
                definition: slot.body.clone(),
                born_at: next_round,
            };
            assert_ne!(&new, old);
            next.push((*root, new));
            consumed.push((old.clone(), slot.retirement));
        }
        // All old sources are read before any modeled target is replaced.
        assert_eq!(next.len(), self.slots.len());
        self.current = next;
        self.retired.extend(consumed);
        self.round = next_round;
    }
}

#[test]
fn real_multi_root_evidence_seeds_finite_activation_prefix_model() {
    for reversed in [false, true] {
        let mut text = SOURCE.replace("        break\n", "");
        if reversed {
            text = text
                .replace("&left", "&temporary")
                .replace("&right", "&left")
                .replace("&temporary", "&right");
        }
        let fixture = Fixture::new(&[("finite-root-activation-model.ko", &text)]);
        let owned = fixture.shared();
        assert!(owned.diagnostics().is_empty());
        assert!(owned.clone().validate().is_ok());
        compare_and_inspect(&fixture, &fixture.parts(&owned), |checker| {
            let mut planner = super::super::super::super::DropPlanner::new(
                checker,
                super::super::super::super::liveness::build(checker, true).unwrap(),
            );
            planner.owner_control_probe = Some(Vec::new());
            let planner = planner.run().unwrap();
            let visits = planner.owner_control_probe.as_ref().unwrap();
            let equation = visits[1].normal_equation.as_ref().unwrap();
            let control = equation.control();
            let entry = equation
                .incoming(control, StaticRootPredecessor::ZeroBody)
                .unwrap();
            let normal = equation
                .incoming(control, StaticRootPredecessor::FirstBody)
                .unwrap();
            let transfers = equation.transfers(control).unwrap();
            assert!(!entry.is_empty());
            assert_eq!(entry.len(), normal.len());
            assert_eq!(entry.len(), transfers.len());
            let slots: Vec<_> = entry
                .iter()
                .zip(normal)
                .zip(transfers)
                .map(|(((root, old), (next_root, new)), transfer)| {
                    assert_eq!(root, next_root);
                    assert_eq!(*root, transfer.root);
                    Slot {
                        root: *root,
                        entry: old.clone(),
                        body: new.clone(),
                        retirement: transfer.retired_through,
                    }
                })
                .collect();
            for prefix_length in [0, 1, 2, 8] {
                let mut model = ActivationModel::new(&slots);
                let independent = ActivationModel::new(&slots);
                assert_ne!(model.current[0].1, independent.current[0].1);
                assert_eq!(model.current[0].1, model.current[0].1.clone());
                for _ in 0..prefix_length {
                    model.normal_prefix_step();
                }
                assert_eq!(model.retired.len(), prefix_length * slots.len());
                for (slot, (root, current)) in slots.iter().zip(&model.current) {
                    assert_eq!(slot.root, *root);
                    assert_eq!(current.born_at, prefix_length);
                    assert_eq!(
                        current.definition,
                        if prefix_length == 0 {
                            slot.entry.clone()
                        } else {
                            slot.body.clone()
                        }
                    );
                    assert!(!model.retired.iter().any(|(old, _)| old == current));
                }
                for (index, (old, fact)) in model.retired.iter().enumerate() {
                    let slot = &slots[index % slots.len()];
                    assert_eq!(old.born_at, index / slots.len());
                    assert_eq!(
                        old.definition,
                        if old.born_at == 0 {
                            slot.entry.clone()
                        } else {
                            slot.body.clone()
                        }
                    );
                    assert_eq!(*fact, slot.retirement);
                    assert!(model.retired[..index].iter().all(|(prior, _)| prior != old));
                }
            }
        });
    }
}
