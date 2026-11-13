//! Conditional symbolic induction for the existing all-root child Replace body.
//! This is a source-level rule, not evidence that SSA implements the recurrence.
use super::*;
use crate::ownership_checking::compilation_unit::UnitSymbolId;
use crate::ownership_checking::{UnitDropFact, UnitOwnerId};

/// One universally quantified root recurrence, not a runtime generation counter.
/// For any finite activation count k: current(0)=entry(root), and
/// current(k+1)=fresh(producer, invocation, k). The step consumes current(k)
/// at `retirement`. Invocation is universally quantified too: restarting this child
/// from a later parent activation must not reuse a prior child instance.
/// `producer` identifies a definition site; freshness is per activation, not ID inequality.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitOwnerRootGenerationRuleV1 {
    root: UnitSymbolId,
    producer: UnitOwnerId,
    retirement: UnitDropFact,
}
impl UnitOwnerRootGenerationRuleV1 {
    /// Both the symbolic input root and the corresponding output slot.
    pub fn root(&self) -> UnitSymbolId {
        self.root
    }
    /// String producer instantiated afresh by each successful body activation.
    pub fn producer(&self) -> UnitOwnerId {
        self.producer.clone()
    }
    /// Original binding Drop consuming the arbitrary current input, not a fixed entry ID.
    pub fn retirement(&self) -> UnitDropFact {
        self.retirement
    }
}

/// Conditional child-loop induction schema, sealed from completed body/exit events.
/// Precondition: distinct roots each own one current value and carry no active loans.
/// Base: zero activations preserves those inputs. Step: the linear normal rule consumes
/// every current input once and installs distinct fresh per-activation definitions.
/// Closure: induction preserves that precondition for any number of normal activations.
/// False: all roots survive unchanged, so parent receives current(k), including k=0.
/// Checked abort is not a backedge. This rule does not prove termination, actual entry
/// ownership, backend realization, or a parent-loop induction.
#[derive(Clone, Debug)]
pub struct UnitOwnerChildGenerationRuleV1 {
    control: UnitStatementId,
    condition: UnitExpressionId,
    normal: UnitOwnerMultiRootNormalTransportV1,
    recurrences: Vec<UnitOwnerRootGenerationRuleV1>,
}
impl UnitOwnerChildGenerationRuleV1 {
    pub(super) fn from_nested(nested: &UnitOwnerNestedNormalBodyV1) -> Option<Self> {
        if !nested.is_consistent() {
            return None;
        }
        let (guard, counter, sites) = nested.generation_counter_tail()?;
        let normal = nested.normal();
        if guard == counter
            || normal.header_roots().contains(&guard)
            || normal.header_roots().contains(&counter)
            || guard.source_unit() != nested.control().source_unit()
            || counter.source_unit() != nested.control().source_unit()
            || sites.iter().enumerate().any(|(i, site)| {
                site.source_unit() != nested.control().source_unit() || sites[..i].contains(site)
            })
        {
            return None;
        }
        let roots = normal.header_roots();
        let (live, drops) = nested.false_exit_partition()?;
        // A retired child root cannot serve as the next parent/child activation input.
        if !drops.is_empty() || live != roots || nested.parent_resume_roots()? != roots {
            return None;
        }
        let recurrences = Self::derive(normal)?;
        Some(Self {
            control: nested.control(),
            condition: nested.condition(),
            normal: normal.clone(),
            recurrences,
        })
    }
    fn derive(
        normal: &UnitOwnerMultiRootNormalTransportV1,
    ) -> Option<Vec<UnitOwnerRootGenerationRuleV1>> {
        if !normal.is_consistent() {
            return None;
        }
        normal
            .header_roots()
            .iter()
            .map(|root| {
                let step = normal
                    .steps()
                    .iter()
                    .find(|step| step.header_root() == *root)?;
                // Do not infer an output from type/count: require the exact normal-rule root/site pair.
                if normal
                    .normal_outputs()
                    .iter()
                    .filter(|(r, p)| *r == *root && *p == step.producer())
                    .count()
                    != 1
                {
                    return None;
                }
                Some(UnitOwnerRootGenerationRuleV1 {
                    root: *root,
                    producer: step.producer(),
                    retirement: step.returned_value_drop(),
                })
            })
            .collect()
    }
    /// Exact child control whose normal step is generalized over activations.
    pub fn control(&self) -> UnitStatementId {
        self.control
    }
    /// Exact child condition; neither true nor termination is assumed.
    pub fn condition(&self) -> UnitExpressionId {
        self.condition
    }
    /// Recurrences in complete header-root order, not execution/Drop order.
    pub fn recurrences(&self) -> &[UnitOwnerRootGenerationRuleV1] {
        &self.recurrences
    }
    /// Recheck the sealed linear recurrence. Does not inspect backend SSA.
    pub fn is_consistent(&self) -> bool {
        self.control == self.normal.statement()
            && self.condition.source_unit() == self.control.source_unit()
            && Self::derive(&self.normal).as_ref() == Some(&self.recurrences)
    }
}
