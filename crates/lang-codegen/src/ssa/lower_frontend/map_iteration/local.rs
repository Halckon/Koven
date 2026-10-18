//! The bounded, ordinary named-local Map proof consumes only published FE facts.
//! Formation history is an anchor; entry Expression owners and every incoming
//! establish current liveness. Runtime still checks birth, transport and absence.
mod conditions;
mod consumer;
mod identity;
pub(super) use identity::producer;
mod topology;

use super::*;
use lang_frontend::ownership_checking::{
    CleanupConditionId, CleanupOwnerValueId, CleanupSelectorId, DropFact, DropTarget,
};

#[derive(Clone)]
pub(super) struct Proof {
    pub(super) root: SymbolId,
    pub(super) initializer: ExpressionId,
    pub(super) consumer: ExpressionId,
    pub(super) birth: CleanupOwnerValueId,
    pub(super) header: CleanupOwnerValueId,
    pub(super) exit: CleanupOwnerValueId,
    /// The actual final consumption, not necessarily this provider's exhaustion.
    pub(super) drop: DropFact,
    pub(super) late_size: Option<ExpressionId>,
    pub(super) dead: Vec<DeadColumn>,
    pub(super) path: Option<Path>,
}

#[derive(Clone, Copy)]
pub(super) struct DeadColumn {
    pub(super) root: SymbolId,
    pub(super) initializer: ExpressionId,
    pub(super) previous: StatementId,
    pub(super) consumed: DropFact,
    pub(super) drop: DropFact,
    pub(super) header: CleanupOwnerValueId,
    pub(super) exit: CleanupOwnerValueId,
}

/// A precise FE control identity, never permission to assume a branch executed.
/// The lowering carrier must prove this successful arm dominates the preheader.
#[derive(Clone, Copy)]
pub(super) struct Path {
    pub(super) control: ExpressionId,
    pub(super) condition: ExpressionId,
    pub(super) selector: CleanupSelectorId,
    pub(super) branch: usize,
    pub(super) success: StatementId,
    pub(super) failure: StatementId,
    pub(super) size: ExpressionId,
    pub(super) previous: StatementId,
}

pub(super) fn qualify<'a>(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &'a OwnershipCheckedFile,
    plan: &'a MapIterationOwnershipPlan,
    body: StatementId,
    span: Span,
) -> Result<qualify::Qualified<'a>, LoweringError> {
    qualify_scope(parsed, names, typed, owned, plan, body, span).map_err(|failure| {
        if failure.kind == LoweringErrorKind::UnsupportedNode {
            error(LoweringErrorKind::UnsupportedNode, span)
        } else {
            failure
        }
    })
}

fn qualify_scope<'a>(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &'a OwnershipCheckedFile,
    plan: &'a MapIterationOwnershipPlan,
    body: StatementId,
    span: Span,
) -> Result<qualify::Qualified<'a>, LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let sites = identity::collect(parsed, names, typed, owned, plan, span)?;
    let mut proofs = Vec::new();
    for (index, site) in sites.iter().enumerate() {
        let previous = index
            .checked_sub(1)
            .map(|index| (&sites[index], &proofs[index]));
        let proof = topology::prove(parsed, owned, site, previous)?;
        proofs.push(proof);
    }
    // Match the complete root-or-associated-owner Drop set, including later dead
    // columns. No unrelated target can hide behind a known owner identity.
    let mut expected = Vec::new();
    let mut owners = Vec::new();
    for proof in &proofs {
        expected.push(proof.drop);
        owners.extend([proof.birth, proof.header, proof.exit]);
        for dead in &proof.dead {
            expected.push(dead.drop);
            owners.extend([dead.header, dead.exit]);
        }
    }
    if expected
        .iter()
        .enumerate()
        .any(|(index, fact)| expected[..index].contains(fact))
    {
        return Err(missing());
    }
    let related = owned
        .drops()
        .iter()
        .copied()
        .filter(|fact| {
            sites
                .iter()
                .any(|site| fact.target() == DropTarget::Named(site.root))
                || fact.owner().is_some_and(|owner| owners.contains(&owner))
        })
        .collect::<Vec<_>>();
    if related.len() != expected.len()
        || expected
            .iter()
            .any(|fact| related.iter().filter(|actual| *actual == fact).count() != 1)
    {
        return Err(missing());
    }
    let index = sites
        .iter()
        .position(|site| site.plan.descriptor().statement() == plan.descriptor().statement())
        .ok_or_else(missing)?;
    let site = &sites[index];
    if site.body != body {
        return Err(missing());
    }
    let proof = proofs.remove(index);
    Ok(qualify::Qualified {
        plan,
        source: SourceIdentity::Local {
            symbol: proof.root,
            initializer: proof.initializer,
        },
        body,
        origin: site.origin,
        owner: Some(source_owner::Proof {
            flow: None,
            drop: proof.drop,
            transfer: None,
            conditional_return: None,
        }),
        body_exit: IterationExitKind::Fallthrough,
        conditional: None,
        nested: None,
        local: Some(proof),
    })
}

/// Called only after the fallible whole-descriptor qualification preflight.
/// A false result is not used to translate a qualification corruption into U.
pub(super) fn supported_drop(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
    fact: DropFact,
) -> bool {
    typed.map_iterations().iter().any(|descriptor| {
        let Ok(qualified) = qualify::qualify(parsed, names, typed, owned, *descriptor) else {
            return false;
        };
        qualified.local.is_some_and(|proof| {
            proof.drop == fact || proof.dead.iter().any(|dead| dead.drop == fact)
        })
    })
}
