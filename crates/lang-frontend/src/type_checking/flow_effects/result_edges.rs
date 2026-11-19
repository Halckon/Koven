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
    let targets = collect_mutation_targets(
        outcomes,
        Pending::Expression(expression),
        |_| true,
        |_| Ok(true),
        |_| Ok(true),
        |_| false,
        false,
    )?;
    let all_keys = targets
        .iter()
        .copied()
        .filter_map(&flow_key)
        .collect::<BTreeSet<_>>();
    let skipped = |matched| -> Result<BTreeSet<K>, AstError> {
        let edge_keys =
            writes_on_boolean_edge(outcomes, expression, matched, &targets, |operand| {
                Ok(outcomes.possible_results(operand))
            })?
            .into_iter()
            .filter_map(&flow_key)
            .collect::<BTreeSet<_>>();
        Ok(all_keys.difference(&edge_keys).copied().collect())
    };
    Ok([skipped(true)?, skipped(false)?])
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
