//! Freeze only checked callbacks inside the current qualified declaration scope.

use super::*;

#[derive(Default)]
pub(super) struct RuntimeCallbackScope {
    constructors: BTreeMap<SourceUnitId, ScopeRows>,
    map_actions: BTreeMap<SourceUnitId, ScopeRows>,
}

impl RuntimeCallbackScope {
    pub(super) fn build(
        parsed: &[&ParsedFile],
        typed: &CompilationUnitTypes,
        owned: &dyn UnitLoweringFacts,
    ) -> Self {
        let loans = RuntimeCallbackLoans::collect(typed, owned);
        Self {
            constructors: index(
                parsed,
                typed
                    .container_constructions()
                    .iter()
                    .enumerate()
                    .filter_map(|(i, d)| {
                        loans
                            .constructors
                            .contains(&d.expression())
                            .then_some((i, d.expression()))
                    }),
            ),
            map_actions: index(
                parsed,
                typed
                    .map_with_values()
                    .iter()
                    .enumerate()
                    .filter_map(|(i, d)| {
                        let call = d.expression();
                        loans
                            .map_actions
                            .contains(&(call, d.action()))
                            .then_some((i, call))
                    }),
            ),
        }
    }

    pub(super) fn constructors(&self, source: SourceUnitId, scope: Span) -> Vec<usize> {
        self.constructors
            .get(&source)
            .map_or_else(Vec::new, |rows| rows.in_scope(scope))
    }

    pub(super) fn map_actions(&self, source: SourceUnitId, scope: Span) -> Vec<usize> {
        self.map_actions
            .get(&source)
            .map_or_else(Vec::new, |rows| rows.in_scope(scope))
    }
}

#[derive(Default)]
struct ScopeRows {
    located: Vec<(usize, usize)>,
    unlocated: Vec<usize>,
}

impl ScopeRows {
    fn in_scope(&self, scope: Span) -> Vec<usize> {
        let start = self
            .located
            .partition_point(|&(offset, _)| offset < scope.start());
        let end = self
            .located
            .partition_point(|&(offset, _)| offset <= scope.end());
        let mut candidates = self.unlocated.clone();
        candidates.extend(self.located[start..end].iter().map(|&(_, ordinal)| ordinal));
        // Preserve frontend order, and let freezing check exact spans and report its
        // original MissingFact if a reachable source has an unlocated candidate.
        candidates.sort_unstable();
        candidates
    }
}

fn index(
    parsed: &[&ParsedFile],
    candidates: impl Iterator<Item = (usize, UnitExpressionId)>,
) -> BTreeMap<SourceUnitId, ScopeRows> {
    let mut indexed = BTreeMap::<_, ScopeRows>::new();
    for (ordinal, id) in candidates {
        let source = id.source_unit();
        let rows = indexed.entry(source).or_default();
        if let Some(node) = parsed
            .get(source.index())
            .and_then(|parsed| parsed.ast().expressions().get(id.expression()).ok())
        {
            rows.located.push((node.span().start(), ordinal));
        } else {
            rows.unlocated.push(ordinal);
        }
    }
    for rows in indexed.values_mut() {
        rows.located.sort_unstable();
    }
    indexed
}

/// Only matching checked loans admit synchronous runtime callback operands.
#[derive(Default)]
struct RuntimeCallbackLoans {
    constructors: BTreeSet<UnitExpressionId>,
    map_actions: BTreeSet<(UnitExpressionId, UnitExpressionId)>,
}

impl RuntimeCallbackLoans {
    fn collect(typed: &CompilationUnitTypes, owned: &dyn UnitLoweringFacts) -> Self {
        let mut index = Self::default();
        let constructors = typed
            .container_constructions()
            .iter()
            .filter(|descriptor| {
                descriptor.kind()
                    == lang_frontend::type_checking::ContainerConstructionKind::RuntimeLength
            })
            .map(|descriptor| descriptor.expression())
            .collect::<BTreeSet<_>>();
        let map_actions = typed
            .map_with_values()
            .iter()
            .map(|descriptor| (descriptor.expression(), descriptor.action()))
            .collect::<BTreeSet<_>>();
        if constructors.is_empty() && map_actions.is_empty() {
            return index;
        }
        for loan in owned.loans() {
            let call = loan.call();
            if constructors.contains(&call) {
                index.constructors.insert(call);
            }
            let action = (call, loan.argument());
            if map_actions.contains(&action) {
                index.map_actions.insert(action);
            }
        }
        index
    }
}

#[test]
fn callback_scope_selects_only_local_candidates_and_keeps_frontend_order() {
    use lang_frontend::source::SourceMap;
    for scale in [8, 16] {
        let mut sources = SourceMap::new();
        let file = sources
            .add_source("scope.ko", " ".repeat(8 * scale))
            .unwrap();
        let rows = ScopeRows {
            located: (0..scale)
                .flat_map(|i| [(8 * i + 2, 2 * i + 1), (8 * i + 4, 2 * i)])
                .collect(),
            unlocated: Vec::new(),
        };
        for i in 0..scale {
            let span = sources.span(file, 8 * i + 1, 8 * i + 6).unwrap();
            assert_eq!(rows.in_scope(span), [2 * i, 2 * i + 1]);
        }
        let missing = ScopeRows {
            located: rows.located,
            unlocated: vec![2 * scale],
        };
        assert_eq!(
            missing.in_scope(sources.span(file, 0, 6).unwrap()),
            [0, 1, 2 * scale],
            "reachable-source missing syntax must still be examined in original order"
        );
    }
}
