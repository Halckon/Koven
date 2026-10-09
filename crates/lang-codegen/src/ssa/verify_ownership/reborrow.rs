//! Exact parent-child lifetime dependencies, including provider element loans.
use super::*;
impl ReborrowDependencies {
    pub(super) fn compute(module: &Module, function: &Function) -> Self {
        let mut parent_by_child = BTreeMap::new();
        let mut must_end_children = BTreeSet::new();
        for instruction in &function.instructions {
            let (source, must_end) = match instruction.operation {
                Operation::RangeConstruct { source, .. } | Operation::RangeCall { source, .. } => {
                    let Some(EntityId::Loan(child)) = instruction.results.last() else {
                        continue;
                    };
                    must_end_children.insert(*child);
                    if range::source::is_view(module, function, source) {
                        // A new descriptor owns a fresh root capability, independent of metadata.
                        // A borrowed View parameter is the function's implicit root capability.
                        let Some(root) = range::source::root(function, source) else {
                            continue;
                        };
                        if function.blocks[0]
                            .parameters
                            .contains(&EntityId::Loan(root))
                        {
                            parent_by_child.insert(*child, root);
                        }
                        continue;
                    }
                    (source, true)
                }
                Operation::SharedReborrow { source }
                | Operation::SharedHeapFieldLoan { base: source, .. }
                | Operation::BorrowCall { source, .. }
                | Operation::MapRequireValue { source, .. } => (source, true),
                // Capture field views inherit a borrow parameter's function extent, but they still
                // block an explicit parent end while active.
                Operation::SharedFieldLoan { base: source, .. }
                | Operation::SharedReferenceFollow { source } => (source, false),
                Operation::BorrowBegin { place, .. } => {
                    let Some(entity) = function.entity(EntityId::Place(place)) else {
                        continue;
                    };
                    let Definition::InstructionResult { instruction, .. } = entity.definition
                    else {
                        continue;
                    };
                    let Some(instruction) = function.instruction(instruction) else {
                        continue;
                    };
                    let source = match instruction.operation {
                        Operation::ContainerElementPlace {
                            owner: EntityId::Loan(source),
                            ..
                        }
                        | Operation::RangeElementPlace { view: source, .. } => source,
                        _ => continue,
                    };
                    (source, true)
                }
                _ => continue,
            };
            let Some(EntityId::Loan(child)) = instruction
                .results
                .iter()
                .find(|entity| matches!(entity, EntityId::Loan(_)))
            else {
                continue;
            };
            parent_by_child.insert(*child, source);
            if must_end {
                must_end_children.insert(*child);
            }
        }
        Self {
            parent_by_child,
            must_end_children,
            flows: LoanFlowAliases::compute(function),
        }
    }

    pub(super) fn is_derived(&self, loan: LoanId, _aliases: &AliasRoots) -> bool {
        self.must_end_children
            .iter()
            .any(|child| self.flows.equivalent(loan, *child))
    }

    pub(super) fn active_descendant(
        &self,
        parent: LoanId,
        _aliases: &AliasRoots,
        state: &BlockState,
    ) -> Option<LoanId> {
        for child in self.parent_by_child.keys() {
            let mut cursor = *child;
            let mut visited = BTreeSet::new();
            while visited.insert(cursor) {
                let Some(next) = self.parent_by_child.get(&cursor).copied() else {
                    break;
                };
                if self.flows.equivalent(parent, next) {
                    if let Some(active) = state
                        .loans
                        .iter()
                        .copied()
                        .find(|active| self.flows.equivalent(*active, *child))
                    {
                        return Some(active);
                    }
                    break;
                }
                cursor = next;
            }
        }
        None
    }
}
