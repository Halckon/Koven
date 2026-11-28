//! Canonical owner types for committed static interface dispatch, before layouts are sealed.

use super::type_graph::{ConcreteTypes, substitute};
use super::*;
use crate::type_checking::{
    UnitCallDescriptor, UnitCallTarget, UnitCallableTarget, UnitNominalSignature, UnitTypeTable,
};

pub(super) struct BodyRoute {
    pub(super) target: UnitCallTarget,
    pub(super) arguments: Vec<UnitTypeId>,
    pub(super) receiver: UnitTypeId,
}

pub(super) struct StaticDispatchOwners {
    interface_members: BTreeMap<UnitSymbolId, bool>,
    routes: DelegationSteps,
    contextual: BTreeSet<UnitCallableTarget>,
    receivers: BTreeMap<UnitTypeId, BTreeSet<UnitCallableTarget>>,
}

impl StaticDispatchOwners {
    pub(super) fn new(
        signatures: &CompilationUnitSignatures,
    ) -> Result<Self, CompilationUnitTypeError> {
        let interface_members = signatures
            .declarations()
            .iter()
            .filter_map(|signature| signature.nominal())
            .filter(|nominal| nominal.kind() == NominalKind::Interface)
            .flat_map(|nominal| nominal.members())
            .filter_map(|member| match member.target() {
                UnitCallableTarget::Symbol(symbol) => Some((symbol, member.has_body())),
                UnitCallableTarget::Declaration(_) => None,
            })
            .collect();
        Ok(Self {
            interface_members,
            routes: delegation_steps(signatures)?,
            contextual: BTreeSet::new(),
            receivers: BTreeMap::new(),
        })
    }

    pub(super) fn call(
        &self,
        call: &UnitCallDescriptor,
    ) -> Option<(UnitTypeId, UnitCallableTarget)> {
        let UnitCallTarget::Symbol(symbol) = call.target() else {
            return None;
        };
        if !self.interface_members.contains_key(&symbol) {
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

    /// Follow the checked receiver route while retaining the callable's own type slots.
    pub(super) fn body_route(
        &self,
        signatures: &mut CompilationUnitSignatures,
        receiver: UnitTypeId,
        requirement: UnitCallableTarget,
        arguments: Vec<UnitTypeId>,
        contextual: bool,
    ) -> Result<Option<BodyRoute>, CompilationUnitTypeError> {
        let direct = |arguments| {
            Some(BodyRoute {
                target: call_target(requirement),
                arguments,
                receiver,
            })
        };
        let Some(UnitTypeKind::Nominal { declaration, .. }) = signatures.types().get(receiver)
        else {
            return Ok(direct(arguments));
        };
        if self
            .routes
            .get(declaration)
            .is_some_and(|steps| steps.contains_key(&requirement))
        {
            return Ok(follow_delegation_chain(
                signatures,
                &self.routes,
                receiver,
                requirement,
                Some(arguments),
            )?
            .and_then(|route| {
                route.arguments.map(|arguments| BodyRoute {
                    target: call_target(route.target),
                    arguments,
                    receiver: route.receiver,
                })
            }));
        }
        let UnitCallableTarget::Symbol(symbol) = requirement else {
            return Ok(direct(arguments));
        };
        if !contextual && self.interface_members.get(&symbol) == Some(&true) {
            return Ok(direct(arguments));
        }
        let (nominal, substitutions) = receiver_owner(signatures, receiver)?;
        let Some(route) = nominal
            .static_dispatch_overrides()
            .iter()
            .find(|route| route.requirement() == requirement)
            .copied()
        else {
            return Ok(direct(arguments));
        };
        let arguments = remap_owner_arguments(
            signatures,
            &substitutions,
            &mut BTreeMap::new(),
            route.requirement_owner(),
            route.implementation_owner(),
            Some(arguments),
        )?
        .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?;
        Ok(Some(BodyRoute {
            target: call_target(route.implementation()),
            arguments,
            receiver,
        }))
    }

    pub(super) fn publish(
        self,
        signatures: &mut CompilationUnitSignatures,
        limit: usize,
    ) -> Result<(), CompilationUnitTypeError> {
        if self.receivers.is_empty() {
            return Ok(());
        }
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
                for &target in self
                    .routes
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
                    if let Some(route) =
                        follow_delegation_chain(signatures, &self.routes, receiver, target, None)?
                    {
                        pending.insert(route.receiver);
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
    destination: Option<DelegationDestination>,
}

#[derive(Clone, Copy)]
struct DelegationDestination {
    owner: UnitTypeId,
    target: UnitCallableTarget,
    forward: bool,
}

struct DelegationRoute {
    receiver: UnitTypeId,
    target: UnitCallableTarget,
    arguments: Option<Vec<UnitTypeId>>,
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
            let destination = if let Some(hop) = forwarder.next_hop() {
                Some(DelegationDestination {
                    owner: hop.receiver_type(),
                    target: hop.requirement(),
                    forward: true,
                })
            } else {
                forwarder
                    .implementation()
                    .map(|implementation| DelegationDestination {
                        owner: implementation.receiver_type(),
                        target: implementation.target(),
                        forward: false,
                    })
            };
            routes.entry(plan.owner()).or_default().insert(
                forwarder.requirement(),
                DelegationStep {
                    field: field.ty(),
                    requirement_owner: forwarder.receiver_type(),
                    destination,
                },
            );
        }
    }
    Ok(routes)
}

/// Follow a single frozen forwarding chain. A repeated declaration and target is a
/// cycle even if the concrete field arguments change at each hop.
fn follow_delegation_chain(
    signatures: &mut CompilationUnitSignatures,
    routes: &DelegationSteps,
    mut receiver: UnitTypeId,
    mut target: UnitCallableTarget,
    mut arguments: Option<Vec<UnitTypeId>>,
) -> Result<Option<DelegationRoute>, CompilationUnitTypeError> {
    let mut visited = BTreeSet::new();
    loop {
        let Some(UnitTypeKind::Nominal { declaration, .. }) = signatures.types().get(receiver)
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
        let Some(destination) = step.destination else {
            return Ok(None);
        };
        if !visited.insert((*declaration, target)) {
            return Ok(None);
        }
        let (_, substitutions) = receiver_owner(signatures, receiver)?;
        let mut memo = BTreeMap::new();
        arguments = remap_owner_arguments(
            signatures,
            &substitutions,
            &mut memo,
            step.requirement_owner,
            destination.owner,
            arguments,
        )?;
        receiver = substitute(
            signatures.types_mut(),
            step.field,
            &substitutions,
            &mut memo,
        )?;
        target = destination.target;
        if !destination.forward {
            return Ok(Some(DelegationRoute {
                receiver,
                target,
                arguments,
            }));
        }
    }
}

fn call_target(target: UnitCallableTarget) -> UnitCallTarget {
    match target {
        UnitCallableTarget::Symbol(symbol) => UnitCallTarget::Symbol(symbol),
        UnitCallableTarget::Declaration(declaration) => UnitCallTarget::Declaration(declaration),
    }
}

fn receiver_owner(
    signatures: &CompilationUnitSignatures,
    receiver: UnitTypeId,
) -> Result<(&UnitNominalSignature, BTreeMap<UnitSymbolId, UnitTypeId>), CompilationUnitTypeError> {
    let Some(UnitTypeKind::Nominal {
        declaration,
        arguments,
    }) = signatures.types().get(receiver)
    else {
        return Err(CompilationUnitTypeError::MissingDeclarationSymbol);
    };
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
    Ok((nominal, substitutions))
}

fn remap_owner_arguments(
    signatures: &mut CompilationUnitSignatures,
    substitutions: &BTreeMap<UnitSymbolId, UnitTypeId>,
    memo: &mut BTreeMap<UnitTypeId, UnitTypeId>,
    requirement_owner: UnitTypeId,
    implementation_owner: UnitTypeId,
    arguments: Option<Vec<UnitTypeId>>,
) -> Result<Option<Vec<UnitTypeId>>, CompilationUnitTypeError> {
    let required = substitute(
        signatures.types_mut(),
        requirement_owner,
        substitutions,
        memo,
    )?;
    let implementation = substitute(
        signatures.types_mut(),
        implementation_owner,
        substitutions,
        memo,
    )?;
    let Some(arguments) = arguments else {
        return Ok(None);
    };
    let (
        Some(UnitTypeKind::Nominal {
            arguments: required,
            ..
        }),
        Some(UnitTypeKind::Nominal {
            arguments: implementation,
            ..
        }),
    ) = (
        signatures.types().get(required),
        signatures.types().get(implementation),
    )
    else {
        return Err(CompilationUnitTypeError::MissingDeclarationSymbol);
    };
    let callable = arguments
        .strip_prefix(required.as_slice())
        .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?;
    Ok(Some(
        implementation.iter().chain(callable).copied().collect(),
    ))
}
