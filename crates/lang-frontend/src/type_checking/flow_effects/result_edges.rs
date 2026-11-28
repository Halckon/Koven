//! Preserve incoming facts on Boolean edges proven to bypass every write to a binding.
use super::*;

pub(in crate::type_checking) fn splits_boolean_writes(
    ast: &SyntaxAst,
    mut expression: ExpressionId,
) -> Result<bool, AstError> {
    loop {
        match ast.expressions().get(expression)?.payload() {
            Expression::Group { expression: inner }
            | Expression::Prefix {
                operator: PrefixOperator::Not,
                operand: inner,
                ..
            } => expression = *inner,
            Expression::Binary {
                operator: BinaryOperator::LogicalAnd | BinaryOperator::LogicalOr,
                ..
            } => return Ok(true),
            _ => return Ok(false),
        }
    }
}

/// A key can be recovered only if it appears in the conservative source scan
/// and every such target is on the opposite result edge.
pub(in crate::type_checking) fn bypassed_result_writes<K: Ord + Copy>(
    outcomes: &SyntacticBooleanOutcomes<'_>,
    expression: ExpressionId,
    flow_key: impl Fn(ExpressionId) -> Option<K>,
) -> Result<[BTreeSet<K>; 2], AstError> {
    let (yes, no) = outcomes.possible_results(expression);
    if !yes || !no {
        return Ok(Default::default());
    }
    let writes = BooleanResultWrites::collect(outcomes, expression, flow_key)?;
    Ok([writes.bypassed(true), writes.bypassed(false)])
}

/// Conservative writes across all paths and on each structural Boolean result.
pub(in crate::type_checking) struct BooleanResultWrites<K> {
    all: BTreeSet<K>,
    yes: EdgeWrites<K>,
    no: EdgeWrites<K>,
}

/// Represent an edge by its writes or by the keys it excludes from `all`.
/// Selecting all prefix writes thereby needs only an empty exclusion set.
enum EdgeWrites<K> {
    Included(BTreeSet<K>),
    Bypassed(BTreeSet<K>),
}

impl<K: Ord + Copy> EdgeWrites<K> {
    fn from_writes(writes: BTreeSet<K>, all: &BTreeSet<K>) -> Self {
        if writes.len() <= all.len() - writes.len() {
            Self::Included(writes)
        } else {
            Self::Bypassed(all.difference(&writes).copied().collect())
        }
    }

    fn includes(&self, key: K) -> bool {
        match self {
            Self::Included(keys) => keys.contains(&key),
            Self::Bypassed(keys) => !keys.contains(&key),
        }
    }

    fn extend_rhs(
        &mut self,
        prefix_all: &BTreeSet<K>,
        rhs_all: &BTreeSet<K>,
        rhs_edge: Option<&Self>,
    ) {
        #[cfg(test)]
        super::super::short_circuit_cost_tests::record_prefix_summary_keys(rhs_all.len());
        for &key in rhs_all {
            let writes = rhs_edge.is_some_and(|edge| edge.includes(key));
            match self {
                Self::Included(keys) if writes => {
                    keys.insert(key);
                }
                Self::Bypassed(keys) if writes => {
                    keys.remove(&key);
                }
                Self::Bypassed(keys) if !prefix_all.contains(&key) => {
                    keys.insert(key);
                }
                Self::Included(_) | Self::Bypassed(_) => {}
            }
        }
    }
}

impl<K: Ord + Copy> BooleanResultWrites<K> {
    pub(in crate::type_checking) fn collect(
        outcomes: &SyntacticBooleanOutcomes<'_>,
        expression: ExpressionId,
        flow_key: impl Fn(ExpressionId) -> Option<K>,
    ) -> Result<Self, AstError> {
        let targets = collect_mutation_targets(
            outcomes,
            Pending::Expression(expression),
            |_| true,
            |_| Ok(true),
            |_| Ok(true),
            |_| false,
            false,
        )?;
        let on_edge = |matched| -> Result<BTreeSet<K>, AstError> {
            Ok(
                writes_on_boolean_edge(outcomes, expression, matched, &targets, |operand| {
                    Ok(outcomes.possible_results(operand))
                })?
                .into_iter()
                .filter_map(&flow_key)
                .collect(),
            )
        };
        let all = targets.iter().copied().filter_map(&flow_key).collect();
        Ok(Self {
            yes: EdgeWrites::from_writes(on_edge(true)?, &all),
            no: EdgeWrites::from_writes(on_edge(false)?, &all),
            all,
        })
    }

    pub(in crate::type_checking) fn all_keys(&self) -> &BTreeSet<K> {
        &self.all
    }

    pub(in crate::type_checking) fn bypasses_key(&self, matched: bool, key: K) -> bool {
        self.all.contains(&key) && !if matched { &self.yes } else { &self.no }.includes(key)
    }

    pub(in crate::type_checking) fn has_bypassed_keys(&self, matched: bool) -> bool {
        match if matched { &self.yes } else { &self.no } {
            EdgeWrites::Included(keys) => keys.len() < self.all.len(),
            EdgeWrites::Bypassed(keys) => !keys.is_empty(),
        }
    }

    pub(in crate::type_checking) fn bypassed(&self, matched: bool) -> BTreeSet<K> {
        let result: BTreeSet<_> = self.bypassed_keys(matched).copied().collect();
        #[cfg(test)]
        super::super::short_circuit_cost_tests::record_bypassed_keys_materialized(result.len());
        result
    }

    pub(in crate::type_checking) fn bypassed_keys(
        &self,
        matched: bool,
    ) -> impl Iterator<Item = &K> {
        let (difference, keys) = match if matched { &self.yes } else { &self.no } {
            EdgeWrites::Included(keys) => (Some(self.all.difference(keys)), None),
            EdgeWrites::Bypassed(keys) => (None, Some(keys.iter())),
        };
        difference
            .into_iter()
            .flatten()
            .chain(keys.into_iter().flatten())
    }

    /// Extend a completed left prefix using the same operand reachability as
    /// `writes_on_boolean_edge`. The caller supplies the RHS facts collected before
    /// checking that operand and omits the summary when execution bypasses it.
    pub(in crate::type_checking) fn extend_prefix(
        self,
        operator: BinaryOperator,
        (right_yes, right_no): (bool, bool),
        rhs: Option<Self>,
    ) -> Self {
        let and = operator == BinaryOperator::LogicalAnd;
        let Self { mut all, yes, no } = self;
        let (mut yes, mut no) = if and {
            (
                if right_yes {
                    yes
                } else {
                    EdgeWrites::Included(BTreeSet::new())
                },
                if right_no {
                    EdgeWrites::Bypassed(BTreeSet::new())
                } else {
                    no
                },
            )
        } else {
            (
                if right_yes {
                    EdgeWrites::Bypassed(BTreeSet::new())
                } else {
                    yes
                },
                if right_no {
                    no
                } else {
                    EdgeWrites::Included(BTreeSet::new())
                },
            )
        };
        if let Some(rhs) = rhs {
            yes.extend_rhs(&all, &rhs.all, right_yes.then_some(&rhs.yes));
            no.extend_rhs(&all, &rhs.all, right_no.then_some(&rhs.no));
            all.extend(rhs.all);
        }
        Self { all, yes, no }
    }
}

pub(in crate::type_checking) fn restore_bypassed_writes<'a, K: Copy + Ord + 'a, T: Copy>(
    facts: &mut BTreeMap<K, T>,
    versions: &mut BTreeMap<K, u64>,
    before_facts: &BTreeMap<K, T>,
    before_versions: &BTreeMap<K, u64>,
    bypassed: impl IntoIterator<Item = &'a K>,
) {
    restore_bypassed_versions(versions, before_versions, bypassed, |key| {
        if let Some(&fact) = before_facts.get(&key) {
            facts.entry(key).or_insert(fact);
        }
    });
}

pub(in crate::type_checking) fn restore_bypassed_versions<'a, K: Copy + Ord + 'a>(
    versions: &mut BTreeMap<K, u64>,
    before_versions: &BTreeMap<K, u64>,
    bypassed: impl IntoIterator<Item = &'a K>,
    mut restore_fact: impl FnMut(K),
) {
    for &key in bypassed {
        #[cfg(test)]
        super::super::short_circuit_cost_tests::record_bypassed_restore_key();
        let previous = before_versions.get(&key).copied();
        if !can_restore_bypassed_version(previous, versions.get(&key).copied()) {
            continue;
        }
        restore_fact(key);
        if let Some(version) = previous {
            versions.insert(key, version);
        } else {
            versions.remove(&key);
        }
    }
}

pub(in crate::type_checking) fn can_restore_bypassed_version(
    previous: Option<u64>,
    current: Option<u64>,
) -> bool {
    let current = current.unwrap_or_default();
    current != u64::MAX && previous.unwrap_or_default() < current
}
