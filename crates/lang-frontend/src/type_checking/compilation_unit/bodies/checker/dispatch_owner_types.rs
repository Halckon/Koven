//! Canonical owner types for committed static interface dispatch, before layouts are sealed.

use super::type_graph::{ConcreteTypes, substitute};
use super::*;
use crate::type_checking::{UnitCallDescriptor, UnitCallTarget, UnitCallableTarget, UnitTypeTable};

pub(super) struct StaticDispatchOwners {
    interface_members: BTreeSet<UnitSymbolId>,
    contextual: BTreeSet<UnitCallableTarget>,
    receivers: BTreeMap<UnitTypeId, BTreeSet<UnitCallableTarget>>,
}

impl StaticDispatchOwners {
    pub(super) fn new(signatures: &CompilationUnitSignatures) -> Self {
        let interface_members = signatures
            .declarations()
            .iter()
            .filter_map(|signature| signature.nominal())
            .filter(|nominal| nominal.kind() == NominalKind::Interface)
            .flat_map(|nominal| nominal.members())
            .filter_map(|member| match member.target() {
                UnitCallableTarget::Symbol(symbol) => Some(symbol),
                UnitCallableTarget::Declaration(_) => None,
            })
            .collect();
        Self {
            interface_members,
            contextual: BTreeSet::new(),
            receivers: BTreeMap::new(),
        }
    }

    pub(super) fn call(
        &self,
        call: &UnitCallDescriptor,
    ) -> Option<(UnitTypeId, UnitCallableTarget)> {
        let UnitCallTarget::Symbol(symbol) = call.target() else {
            return None;
        };
        if !self.interface_members.contains(&symbol) {
            return None;
        }
        Some((call.receiver()?.ty(), UnitCallableTarget::Symbol(symbol)))
    }

    pub(super) fn record(
        &mut self,
        types: &UnitTypeTable,
        receiver: UnitTypeId,
        target: UnitCallableTarget,
    ) {
        match types.get(receiver) {
            Some(UnitTypeKind::StaticSelf(_)) => {
                self.contextual.insert(target);
            }
            Some(UnitTypeKind::Nominal { .. }) => {
                self.receivers.entry(receiver).or_default().insert(target);
            }
            _ => {}
        }
    }

    pub(super) fn publish(
        self,
        signatures: &mut CompilationUnitSignatures,
        limit: usize,
    ) -> Result<(), CompilationUnitTypeError> {
        if self.receivers.is_empty() {
            return Ok(());
        }
        let routes = delegation_steps(signatures)?;
        let mut concrete = ConcreteTypes::for_closed_arguments();
        for (root, targets) in self.receivers {
            let mut pending = BTreeSet::from([root]);
            let mut processed = BTreeSet::new();
            let mut contextual_instances = BTreeMap::<_, usize>::new();
            while let Some(receiver) = pending.pop_first() {
                if !processed.insert(receiver) {
                    continue;
                }
                let Some(UnitTypeKind::Nominal {
                    declaration,
                    arguments,
                }) = signatures.types().get(receiver)
                else {
                    continue;
                };
                if !concrete.is_concrete(signatures.types(), receiver)? {
                    continue;
                }
                let declaration = *declaration;
                let nominal = signatures
                    .declaration(declaration)
                    .and_then(|sig| sig.nominal())
                    .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?;
                // A default can use its concrete Self for another committed interface call.
                // Contextual targets are conservatively shared across concrete receivers.
                let requested = |target| {
                    (receiver == root && targets.contains(&target))
                        || self.contextual.contains(&target)
                };
                let roots = nominal
                    .static_dispatch_overrides()
                    .iter()
                    .filter(|dispatch| requested(dispatch.requirement()))
                    .flat_map(|dispatch| {
                        [
                            dispatch.requirement_owner(),
                            dispatch.implementation_owner(),
                        ]
                    })
                    .collect::<BTreeSet<_>>();
                let substitutions = nominal
                    .type_parameters()
                    .iter()
                    .copied()
                    .zip(arguments.iter().copied())
                    .collect();
                let mut memo = BTreeMap::new();
                for owner in roots {
                    substitute(signatures.types_mut(), owner, &substitutions, &mut memo)?;
                }
                for &target in routes
                    .get(&declaration)
                    .into_iter()
                    .flat_map(|steps| steps.keys())
                {
                    if !requested(target) {
                        continue;
                    }
                    // Different default-body dispatches can grow type arguments across
                    // otherwise acyclic delegation chains. Keep the generic frontier.
                    let count = contextual_instances
                        .entry((declaration, target))
                        .or_default();
                    if *count > limit {
                        continue;
                    }
                    *count = count.saturating_add(1);
                    if let Some(delegate) =
                        publish_delegation_chain(signatures, &routes, receiver, target)?
                    {
                        pending.insert(delegate);
                    }
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
struct DelegationStep {
    field: UnitTypeId,
    requirement_owner: UnitTypeId,
    effective_owner: UnitTypeId,
    next_target: Option<UnitCallableTarget>,
}

type DelegationSteps = BTreeMap<DeclarationId, BTreeMap<UnitCallableTarget, DelegationStep>>;

fn delegation_steps(
    signatures: &CompilationUnitSignatures,
) -> Result<DelegationSteps, CompilationUnitTypeError> {
    let mut routes = DelegationSteps::new();
    for plan in signatures.delegations() {
        if plan.forwarders().is_empty() {
            continue;
        }
        let field = signatures
            .declaration(plan.owner())
            .and_then(|sig| sig.nominal())
            .and_then(|nominal| {
                nominal
                    .fields()
                    .iter()
                    .find(|field| field.symbol() == plan.target())
            })
            .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?;
        for forwarder in plan.forwarders() {
            let (effective_owner, next_target) = if let Some(hop) = forwarder.next_hop() {
                (hop.receiver_type(), Some(hop.requirement()))
            } else if let Some(implementation) = forwarder.implementation() {
                (implementation.receiver_type(), None)
            } else {
                continue;
            };
            routes.entry(plan.owner()).or_default().insert(
                forwarder.requirement(),
                DelegationStep {
                    field: field.ty(),
                    requirement_owner: forwarder.receiver_type(),
                    effective_owner,
                    next_target,
                },
            );
        }
    }
    Ok(routes)
}

/// Follow a single frozen forwarding chain. A repeated declaration and target is a
/// cycle even if the concrete field arguments change at each hop.
fn publish_delegation_chain(
    signatures: &mut CompilationUnitSignatures,
    routes: &DelegationSteps,
    mut receiver: UnitTypeId,
    mut target: UnitCallableTarget,
) -> Result<Option<UnitTypeId>, CompilationUnitTypeError> {
    let mut visited = BTreeSet::new();
    loop {
        let Some(UnitTypeKind::Nominal {
            declaration,
            arguments,
        }) = signatures.types().get(receiver)
        else {
            return Ok(None);
        };
        let Some(step) = routes
            .get(declaration)
            .and_then(|steps| steps.get(&target))
            .copied()
        else {
            return Ok(None);
        };
        if !visited.insert((*declaration, target)) {
            return Ok(None);
        }
        let nominal = signatures
            .declaration(*declaration)
            .and_then(|sig| sig.nominal())
            .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?;
        let substitutions = nominal
            .type_parameters()
            .iter()
            .copied()
            .zip(arguments.iter().copied())
            .collect();
        let mut memo = BTreeMap::new();
        for owner in [step.requirement_owner, step.effective_owner] {
            substitute(signatures.types_mut(), owner, &substitutions, &mut memo)?;
        }
        receiver = substitute(
            signatures.types_mut(),
            step.field,
            &substitutions,
            &mut memo,
        )?;
        let Some(next_target) = step.next_target else {
            return Ok(Some(receiver));
        };
        target = next_target;
    }
}
