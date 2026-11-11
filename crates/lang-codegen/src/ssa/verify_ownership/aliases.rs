//! SSA entity storage alias roots 与 CFG 不动点，以及 direct owned heap sibling fields 证明。
use super::*;

pub(in crate::ssa::verify_ownership) mod direct_fields {
    use super::*;

    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    enum Path {
        Unknown,
        Payload(EntityId, SsaTypeId),
        Field(EntityId, SsaTypeId, usize),
    }

    pub(in crate::ssa::verify_ownership) struct DirectFields(BTreeMap<EntityId, BTreeSet<Path>>);

    impl DirectFields {
        pub(in crate::ssa::verify_ownership) fn compute(
            function: &Function,
            roots: &BTreeMap<EntityId, BTreeSet<EntityId>>,
        ) -> Self {
            if !function
                .instructions
                .iter()
                .any(|instruction| matches!(instruction.operation, Operation::HeapPayloadPlace { .. }))
            {
                return Self(BTreeMap::new());
            }
            let incoming_blocks: BTreeSet<_> = function
                .blocks
                .iter()
                .flat_map(|block| {
                    edges(&block.terminator.as_ref().expect("verified terminator").kind)
                        .into_iter()
                        .map(|edge| edge.target)
                })
                .collect();
            let mut paths: BTreeMap<_, BTreeSet<_>> = all_entities(function)
                .into_iter()
                .map(|id| (id, BTreeSet::new()))
                .collect();
            for block in &function.blocks {
                if block.id.index() == 0 || !incoming_blocks.contains(&block.id) {
                    for parameter in &block.parameters {
                        paths.get_mut(parameter).unwrap().insert(Path::Unknown);
                    }
                }
            }
            loop {
                let mut changed = false;
                for instruction in &function.instructions {
                    for result in &instruction.results {
                        let incoming: BTreeSet<_> = match &instruction.operation {
                            Operation::HeapPayloadPlace { owner } => {
                                let owner = EntityId::Value(*owner);
                                match roots[&owner].iter().copied().collect::<Vec<_>>().as_slice() {
                                    [root] => [Path::Payload(
                                        *root,
                                        function.entity(owner).unwrap().ty.semantic_type(),
                                    )]
                                    .into(),
                                    _ => [Path::Unknown].into(),
                                }
                            }
                            Operation::FieldPlace { base, field } => paths[&EntityId::Place(*base)]
                                .iter()
                                .map(|path| match path {
                                    Path::Payload(root, ty) => Path::Field(*root, *ty, *field),
                                    _ => Path::Unknown,
                                })
                                .collect(),
                            Operation::BorrowBegin { place, .. } => paths[&EntityId::Place(*place)]
                                .iter()
                                .map(|path| match path {
                                    Path::Field(..) => *path,
                                    _ => Path::Unknown,
                                })
                                .collect(),
                            Operation::SharedReborrow { source } => paths[&EntityId::Loan(*source)]
                                .iter()
                                .map(|path| match path {
                                    Path::Field(..) => *path,
                                    _ => Path::Unknown,
                                })
                                .collect(),
                            _ => [Path::Unknown].into(),
                        };
                        let target = paths.get_mut(result).unwrap();
                        let before = target.len();
                        target.extend(incoming);
                        changed |= target.len() != before;
                    }
                }
                for block in &function.blocks {
                    for edge in edges(&block.terminator.as_ref().expect("verified terminator").kind) {
                        let target = function.block(edge.target).unwrap();
                        for (argument, parameter) in edge.arguments.iter().zip(&target.parameters) {
                            let incoming = paths[argument].clone();
                            let target = paths.get_mut(parameter).unwrap();
                            let before = target.len();
                            target.extend(incoming);
                            changed |= target.len() != before;
                        }
                    }
                }
                if !changed {
                    for path in paths.values_mut().filter(|path| path.is_empty()) {
                        path.insert(Path::Unknown);
                        changed = true;
                    }
                    if !changed {
                        break;
                    }
                }
            }
            Self(paths)
        }

        pub(in crate::ssa::verify_ownership) fn disjoint(
            &self,
            left: EntityId,
            right: EntityId,
        ) -> bool {
            let (Some(left), Some(right)) = (self.0.get(&left), self.0.get(&right)) else {
                return false;
            };
            if left.len() != 1 || right.len() != 1 {
                return false;
            }
            matches!((left.first(), right.first()),
                (Some(Path::Field(a, ta, fa)), Some(Path::Field(b, tb, fb)))
                    if a == b && ta == tb && fa != fb)
        }
    }
}

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
                    | Operation::CallableViewOf { source }
                    | Operation::SharedReferenceFollow { source }
                    | Operation::BorrowCall { source, .. }
                    | Operation::MapIterationKeyLoan { source, .. }
                    | Operation::MapIterationValueLoan { source, .. }
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

        let direct_fields = direct_fields::DirectFields::compute(function, &roots);
        Self {
            roots,
            direct_fields,
        }
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
