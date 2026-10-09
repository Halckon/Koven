//! SSA entity storage alias roots 与 CFG 不动点。
use super::*;

impl AliasRoots {
    pub(in crate::ssa) fn compute(function: &Function) -> Self {
        let mut roots = all_entities(function)
            .into_iter()
            .map(|entity| (entity, BTreeSet::new()))
            .collect::<BTreeMap<_, _>>();
        let mut incoming = BTreeMap::<EntityId, usize>::new();
        for block in &function.blocks {
            let terminator = block.terminator.as_ref().expect("terminator must exist");
            for edge in edges(&terminator.kind) {
                let target = function.block(edge.target).expect("target must exist");
                for parameter in &target.parameters {
                    *incoming.entry(*parameter).or_default() += 1;
                }
            }
        }
        for block in &function.blocks {
            for parameter in &block.parameters {
                if block.id.index() == 0 || incoming.get(parameter).copied().unwrap_or(0) == 0 {
                    roots
                        .get_mut(parameter)
                        .expect("parameter root exists")
                        .insert(*parameter);
                }
            }
        }
        for instruction in &function.instructions {
            for result in &instruction.results {
                if matches!(result, EntityId::Value(_)) {
                    roots
                        .get_mut(result)
                        .expect("result root exists")
                        .insert(*result);
                }
            }
        }

        loop {
            let mut changed = false;
            for block in &function.blocks {
                let terminator = block.terminator.as_ref().expect("terminator must exist");
                for edge in edges(&terminator.kind) {
                    let target = function.block(edge.target).expect("target must exist");
                    for (argument, parameter) in edge.arguments.iter().zip(&target.parameters) {
                        changed |= union_from(&mut roots, *parameter, *argument);
                    }
                }
            }
            for instruction in &function.instructions {
                match &instruction.operation {
                    Operation::RootPlace { owner } => {
                        changed |=
                            union_from(&mut roots, instruction.results[0], EntityId::Value(*owner));
                    }
                    Operation::BorrowBegin { place, .. } => {
                        changed |=
                            union_from(&mut roots, instruction.results[0], EntityId::Place(*place));
                    }
                    Operation::HeapPayloadPlace { owner } => {
                        changed |=
                            union_from(&mut roots, instruction.results[0], EntityId::Value(*owner));
                    }
                    Operation::SharedPayloadPlace { owner } => {
                        changed |= union_from(&mut roots, instruction.results[0], *owner);
                    }
                    Operation::RangeElementPlace { view, .. } => {
                        changed |=
                            union_from(&mut roots, instruction.results[0], EntityId::Loan(*view));
                    }
                    Operation::ContainerElementPlace { owner, .. } => {
                        changed |= union_from(&mut roots, instruction.results[0], *owner);
                    }
                    Operation::FieldPlace { base, .. } => {
                        changed |=
                            union_from(&mut roots, instruction.results[0], EntityId::Place(*base));
                    }
                    Operation::SharedFieldLoan { base, .. } => {
                        changed |=
                            union_from(&mut roots, instruction.results[0], EntityId::Loan(*base));
                    }
                    Operation::SharedHeapFieldLoan { base, .. } => {
                        changed |=
                            union_from(&mut roots, instruction.results[0], EntityId::Loan(*base));
                    }
                    Operation::RangeConstruct { source, .. }
                    | Operation::RangeCall { source, .. } => {
                        if let Some(loan) = instruction
                            .results
                            .iter()
                            .find(|entity| matches!(entity, EntityId::Loan(_)))
                            && let Some(root) = range::source::root(function, *source)
                        {
                            changed |= union_from(&mut roots, *loan, EntityId::Loan(root));
                        }
                    }
                    Operation::SharedReborrow { source }
                    | Operation::SharedReferenceFollow { source }
                    | Operation::BorrowCall { source, .. }
                    | Operation::MapRequireValue { source, .. } => {
                        changed |=
                            union_from(&mut roots, instruction.results[0], EntityId::Loan(*source));
                    }
                    _ => {}
                }
            }
            for block in &function.blocks {
                let Some(terminator) = &block.terminator else {
                    continue;
                };
                if let TerminatorKind::NullableBranch { owner, view, .. } = terminator.kind {
                    changed |= union_from(&mut roots, EntityId::Loan(view), EntityId::Value(owner));
                }
            }
            if !changed {
                break;
            }
        }

        Self { roots }
    }

    pub(in crate::ssa) fn overlap(&self, left: EntityId, right: EntityId) -> bool {
        let left = self.roots.get(&left).expect("left alias roots must exist");
        let right = self
            .roots
            .get(&right)
            .expect("right alias roots must exist");
        left.iter().any(|root| right.contains(root))
    }
}
