//! A may-contain query over actual owned storage, independent of callable signatures.

use std::collections::VecDeque;

use crate::ssa::model::{ClosureCaptureMode, Module, SsaTypeId, SsaTypeKind};

#[cfg(test)]
mod graph_tests;

pub(in crate::ssa::verify) struct TypeCaptures {
    may_contain: Vec<bool>,
    #[cfg(test)]
    stats: ClassificationStats,
}

#[cfg(test)]
#[derive(Default)]
struct ClassificationStats {
    node_visits: usize,
    storage_edge_visits: usize,
    propagation_edge_visits: usize,
}

impl TypeCaptures {
    /// The module's type and deinit checks must have succeeded before indexing storage edges.
    pub(in crate::ssa::verify) fn compute(module: &Module) -> Self {
        let mut parents = vec![Vec::new(); module.types.len()];
        let mut may_contain = vec![false; module.types.len()];
        let mut pending = VecDeque::new();
        #[cfg(test)]
        let mut stats = ClassificationStats::default();
        for (index, kind) in module.types.iter().enumerate() {
            #[cfg(test)]
            {
                stats.node_visits += 1;
            }
            let children = match kind {
                SsaTypeKind::Aggregate { fields, .. } => fields.clone(),
                SsaTypeKind::TaggedUnion { variants, .. } => variants.clone(),
                SsaTypeKind::HeapOwner { payload, .. }
                | SsaTypeKind::SharedOwner { payload, .. } => payload.iter().copied().collect(),
                SsaTypeKind::NullableHandle { inner } => vec![*inner],
                SsaTypeKind::SequentialContainer { element, .. } => vec![*element],
                SsaTypeKind::ConcreteClosure { captures, .. } => {
                    if captures
                        .iter()
                        .any(|capture| capture.mode == ClosureCaptureMode::Shared)
                    {
                        may_contain[index] = true;
                        pending.push_back(index);
                    }
                    captures
                        .iter()
                        .filter(|capture| capture.mode == ClosureCaptureMode::Owned)
                        .map(|capture| capture.ty)
                        .collect()
                }
                // Signatures and shared-reference targets do not describe owned contents.
                _ => Vec::new(),
            };
            for child in children {
                parents[child.index()].push(index);
                #[cfg(test)]
                {
                    stats.storage_edge_visits += 1;
                }
            }
        }
        while let Some(child) = pending.pop_front() {
            for parent in &parents[child] {
                #[cfg(test)]
                {
                    stats.propagation_edge_visits += 1;
                }
                if !may_contain[*parent] {
                    may_contain[*parent] = true;
                    pending.push_back(*parent);
                }
            }
        }
        Self {
            may_contain,
            #[cfg(test)]
            stats,
        }
    }

    pub(in crate::ssa::verify) fn contains(&self, ty: SsaTypeId) -> bool {
        self.may_contain[ty.index()]
    }
}
