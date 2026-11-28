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
    yes: BTreeSet<K>,
    no: BTreeSet<K>,
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
        let writes = Self {
            all: targets.iter().copied().filter_map(&flow_key).collect(),
            yes: on_edge(true)?,
            no: on_edge(false)?,
        };
        Ok(writes)
    }

    pub(in crate::type_checking) fn bypassed(&self, matched: bool) -> BTreeSet<K> {
        let edge = if matched { &self.yes } else { &self.no };
        self.all.difference(edge).copied().collect()
    }

    /// Extend a completed left prefix using the same operand reachability as
    /// `writes_on_boolean_edge`. When either outcome is requested, use all
    /// syntactically reachable writes in that operand.
    pub(in crate::type_checking) fn extend_prefix(
        self,
        outcomes: &SyntacticBooleanOutcomes<'_>,
        left: ExpressionId,
        operator: BinaryOperator,
        right: ExpressionId,
        flow_key: impl Fn(ExpressionId) -> Option<K>,
    ) -> Result<Self, AstError> {
        let and = operator == BinaryOperator::LogicalAnd;
        let (left_yes, left_no) = outcomes.possible_results(left);
        let (right_yes, right_no) = outcomes.possible_results(right);
        let runs_rhs = if and { left_yes } else { left_no };
        let rhs = runs_rhs
            .then(|| Self::collect(outcomes, right, flow_key))
            .transpose()?;
        let on_edge = |matched: bool| {
            let (want_yes, want_no) = if and {
                (if matched { right_yes } else { right_no }, !matched)
            } else {
                (matched, if matched { right_yes } else { right_no })
            };
            let mut writes = match (want_yes, want_no) {
                (true, true) => self.all.clone(),
                (true, false) => self.yes.clone(),
                (false, true) => self.no.clone(),
                (false, false) => BTreeSet::new(),
            };
            if let Some(rhs) = &rhs {
                if matched && right_yes {
                    writes.extend(&rhs.yes);
                } else if !matched && right_no {
                    writes.extend(&rhs.no);
                }
            }
            writes
        };
        let yes = on_edge(true);
        let no = on_edge(false);
        let mut all = self.all;
        if let Some(rhs) = rhs {
            all.extend(rhs.all);
        }
        Ok(Self { all, yes, no })
    }
}

pub(in crate::type_checking) fn restore_bypassed_writes<K: Copy + Ord, T: Copy>(
    facts: &mut BTreeMap<K, T>,
    versions: &mut BTreeMap<K, u64>,
    before_facts: &BTreeMap<K, T>,
    before_versions: &BTreeMap<K, u64>,
    bypassed: &BTreeSet<K>,
) {
    for &key in bypassed {
        let from = before_versions.get(&key).copied().unwrap_or_default();
        let to = versions.get(&key).copied().unwrap_or_default();
        if from >= to || to == u64::MAX {
            continue;
        }
        if let Some(&fact) = before_facts.get(&key) {
            facts.entry(key).or_insert(fact);
        }
        if let Some(&version) = before_versions.get(&key) {
            versions.insert(key, version);
        } else {
            versions.remove(&key);
        }
    }
}
