//! Narrow, seeded provenance proof for direct owned heap sibling fields.
use super::*;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Path {
    Unknown,
    Payload(EntityId, SsaTypeId),
    Field(EntityId, SsaTypeId, usize),
}

pub(super) struct DirectFields(BTreeMap<EntityId, BTreeSet<Path>>);

impl DirectFields {
    pub(super) fn compute(
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
                // A predecessor cycle without an originating projection is unknown, not
                // an absent alternative that a known join input may silently override.
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

    pub(super) fn disjoint(&self, left: EntityId, right: EntityId) -> bool {
        // Empty cycles and any unknown/mixed incoming paths cannot establish a proof.
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
