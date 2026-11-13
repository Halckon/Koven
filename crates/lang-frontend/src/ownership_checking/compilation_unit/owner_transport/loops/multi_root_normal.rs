//! Parametric normal-body transfer, not a replay of fixed first-iteration tokens.
use super::*;
use crate::name_resolution::UnitSymbolId;
use crate::ownership_checking::{
    UnitDropFact, UnitDropTarget, UnitOwnerId, UnitOwnerOrigin, UnitOwnerProducerCatalogV1,
};

/// A checked Replace consumes the current header slot, not a fixed entry token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitOwnerRootReplaceStepV1 {
    pub(in crate::ownership_checking::compilation_unit) call: UnitExpressionId,
    pub(in crate::ownership_checking::compilation_unit) root: UnitSymbolId,
    pub(in crate::ownership_checking::compilation_unit) producer: UnitOwnerId,
    pub(in crate::ownership_checking::compilation_unit) retired_result: UnitDropFact,
}
impl UnitOwnerRootReplaceStepV1 {
    /// Actual checked primitive call, in declaration execution order.
    pub fn call(&self) -> UnitExpressionId {
        self.call
    }
    /// Formal current-header input location; no runtime instance is created here.
    pub fn header_root(&self) -> UnitSymbolId {
        self.root
    }
    /// Real String definition site. Each successful execution creates a fresh instance.
    pub fn producer(&self) -> UnitOwnerId {
        self.producer.clone()
    }
    /// Actual binding cleanup which consumes the returned current-header obligation.
    pub fn returned_value_drop(&self) -> UnitDropFact {
        self.retired_result
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Obligation {
    Header(UnitSymbolId),
    Produced(UnitOwnerId),
}
/// Normal-body rule for the exact linear, once-per-root direct-String Replace shape.
/// Proves formal header obligations are consumed once and fresh definitions occupy outputs.
/// It does not prove loop exits, condition outcomes, lowering, or a concrete runtime trace.
#[derive(Clone, Debug)]
pub struct UnitOwnerMultiRootNormalTransportV1 {
    control: UnitStatementId,
    roots: Vec<UnitSymbolId>,
    catalog: UnitOwnerProducerCatalogV1,
    steps: Vec<UnitOwnerRootReplaceStepV1>,
    manifest: NormalManifest,
    outputs: Vec<(UnitSymbolId, UnitOwnerId)>,
}
#[derive(Clone, Debug)]
struct NormalManifest {
    control: UnitStatementId,
    roots: Vec<UnitSymbolId>,
    steps: Vec<UnitOwnerRootReplaceStepV1>,
    definitions: Vec<crate::ownership_checking::UnitOwnerDefinition>,
}
impl UnitOwnerMultiRootNormalTransportV1 {
    // Only completed actual body-event collectors supply these typed steps.
    pub(in crate::ownership_checking::compilation_unit) fn seal(
        control: UnitStatementId,
        roots: Vec<UnitSymbolId>,
        catalog: UnitOwnerProducerCatalogV1,
        steps: Vec<UnitOwnerRootReplaceStepV1>,
    ) -> Option<Self> {
        let outputs = Self::transfer(control, &roots, &catalog, &steps)?;
        let manifest = NormalManifest {
            control,
            roots: roots.clone(),
            steps: steps.clone(),
            definitions: catalog.definitions().to_vec(),
        };
        Some(Self {
            control,
            roots,
            catalog,
            manifest,
            steps,
            outputs,
        })
    }
    /// Exact loop whose normal body establishes this rule.
    pub fn statement(&self) -> UnitStatementId {
        self.control
    }
    /// Header slots in entry order; actual Replace order is given by steps().
    pub fn header_roots(&self) -> &[UnitSymbolId] {
        &self.roots
    }
    /// Actual primitive/drop order; all inputs are formal current-header obligations.
    pub fn steps(&self) -> &[UnitOwnerRootReplaceStepV1] {
        &self.steps
    }
    /// Static output definitions in entry-root order, not persistent runtime instances.
    pub fn normal_outputs(&self) -> &[(UnitSymbolId, UnitOwnerId)] {
        &self.outputs
    }
    /// Actual body String definitions, separate from all header inputs.
    pub fn body_catalog(&self) -> &UnitOwnerProducerCatalogV1 {
        &self.catalog
    }
    /// Recheck symbolic input consumption and output conservation, independently of old P IDs.
    pub fn is_consistent(&self) -> bool {
        self.control == self.manifest.control
            && self.roots == self.manifest.roots
            && self.steps == self.manifest.steps
            && self.catalog.definitions() == self.manifest.definitions
            && Self::transfer(self.control, &self.roots, &self.catalog, &self.steps).as_ref()
                == Some(&self.outputs)
    }
    fn transfer(
        control: UnitStatementId,
        roots: &[UnitSymbolId],
        catalog: &UnitOwnerProducerCatalogV1,
        steps: &[UnitOwnerRootReplaceStepV1],
    ) -> Option<Vec<(UnitSymbolId, UnitOwnerId)>> {
        if roots.len() < 2
            || roots.len() != steps.len()
            || catalog.definitions().len() != steps.len()
            || !catalog.is_consistent()
            || roots.iter().enumerate().any(|(index, root)| {
                root.source_unit() != control.source_unit() || roots[..index].contains(root)
            })
        {
            return None;
        }
        let mut state: Vec<_> = roots
            .iter()
            .map(|root| (*root, Obligation::Header(*root)))
            .collect();
        let mut retired = Vec::new();
        for (index, step) in steps.iter().enumerate() {
            let definition = catalog.definition(step.producer.clone())?;
            let UnitOwnerOrigin::String(origin) = definition.origin() else {
                return None;
            };
            let UnitDropTarget::Named(binding) = step.retired_result.target() else {
                return None;
            };
            let UnitDropPoint::AfterStatement(declaration) = step.retired_result.point() else {
                return None;
            };
            if step.call.source_unit() != control.source_unit()
                || origin.source_unit() != control.source_unit()
                || binding.source_unit() != control.source_unit()
                || roots.contains(&binding)
                || declaration.source_unit() != control.source_unit()
                || steps[..index].iter().any(|prior| {
                    prior.call == step.call
                        || prior.producer == step.producer
                        || prior.retired_result.target() == step.retired_result.target()
                })
            {
                return None;
            }
            let position = roots.iter().position(|root| *root == step.root)?;
            // Repeated replacement of a new output is outside this rule, not silently retired.
            if state[position].1 != Obligation::Header(step.root) {
                return None;
            }
            retired.push(step.root);
            state[position].1 = Obligation::Produced(step.producer.clone());
        }
        if retired.len() != roots.len() {
            return None;
        }
        state
            .into_iter()
            .map(|(root, value)| match value {
                Obligation::Produced(owner) => Some((root, owner)),
                Obligation::Header(_) => None,
            })
            .collect()
    }
}
impl UnitOwnerObservation {
    /// A restricted normal-body transfer rule, not a complete loop/exit lowering capability.
    pub fn multi_root_normal_transport_v1(&self) -> Option<&UnitOwnerMultiRootNormalTransportV1> {
        self.multi_root_boundary_v1()?.normal_transport()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ownership_checking::compilation_unit::formed_tests::Fixture;
    #[test]
    fn ci_only_normal_rule_rejects_non_linear_or_aliased_transfer() {
        let fixture = Fixture::new(&[(
            "normal-rule-fault.ko",
            "fun run(flag: Boolean) { var a = \"a\"; var b = \"b\"; while(flag) { val x = replace(&a, \"x\"); val y = replace(&b, \"y\") }; val outA = a; val outB = b }",
        )]);
        let owned = fixture.shared();
        let rule = owned.owner_observations()[0]
            .multi_root_normal_transport_v1()
            .unwrap();
        let accepts = |steps| {
            UnitOwnerMultiRootNormalTransportV1::seal(
                rule.control,
                rule.roots.clone(),
                rule.catalog.clone(),
                steps,
            )
            .is_some()
        };
        assert!(accepts(rule.steps.clone()));
        let mut repeated = rule.steps.clone();
        repeated[1].root = repeated[0].root;
        assert!(!accepts(repeated));
        let mut aliased = rule.steps.clone();
        aliased[1].producer = aliased[0].producer.clone();
        assert!(!accepts(aliased));
        let mut reused_result = rule.steps.clone();
        reused_result[1].retired_result = reused_result[0].retired_result;
        assert!(!accepts(reused_result));
        let mut incomplete = rule.steps.clone();
        incomplete.pop();
        assert!(!accepts(incomplete));
        let mut reordered = rule.clone();
        reordered.steps.reverse();
        assert!(!reordered.is_consistent());
    }
}
