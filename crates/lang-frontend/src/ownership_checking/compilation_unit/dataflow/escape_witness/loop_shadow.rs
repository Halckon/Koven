//! Private diagnostic seeds for the proven non-repeating, null-clearing loop family.
//! No loop-body consumer reads these seeds; unsupported writes poison publication.
use super::super::{Checker, Flows, State};
use super::{Coverage, EscapeWitnesses};
use crate::{
    ast::ExpressionId,
    name_resolution::UnitSymbolId,
    parser::{Expression, LiteralKind},
    type_checking::{UnitExpressionId, UnitTypeKind},
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in super::super) struct LoopEscapeShadow(Option<Box<LoopEscapeShadowState>>);

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct LoopEscapeShadowState {
    candidates: BTreeSet<UnitSymbolId>,
    bindings: BTreeMap<UnitSymbolId, BTreeSet<UnitExpressionId>>,
    incomplete: BTreeSet<UnitSymbolId>,
    invalid: BTreeSet<UnitSymbolId>,
    published: BTreeSet<UnitSymbolId>,
    depth: usize,
}
impl LoopEscapeShadowState {
    pub(in super::super) fn seed(
        &mut self,
        symbol: UnitSymbolId,
        value: UnitExpressionId,
        facts: &EscapeWitnesses,
    ) {
        self.candidates.insert(symbol);
        self.observe_outside(symbol, value, facts);
    }
    fn observe_outside(
        &mut self,
        symbol: UnitSymbolId,
        value: UnitExpressionId,
        facts: &EscapeWitnesses,
    ) {
        let witnesses = facts.results.get(&value).cloned().unwrap_or_default();
        if facts.coverage.get(&value) == Some(&Coverage::Known) {
            self.incomplete.remove(&symbol);
            self.invalid.remove(&symbol);
            self.bindings.insert(symbol, witnesses);
        } else {
            self.incomplete.insert(symbol);
            if witnesses.is_empty() {
                self.bindings.remove(&symbol);
            } else {
                self.bindings.insert(symbol, witnesses);
            }
        }
    }
    pub(in super::super) fn merge(&mut self, other: Self) {
        debug_assert_eq!(self.depth, other.depth);
        self.candidates
            .retain(|symbol| other.candidates.contains(symbol));
        self.incomplete.extend(other.incomplete);
        self.invalid.extend(other.invalid);
        self.published.extend(other.published);
        for (symbol, witnesses) in other.bindings {
            self.bindings.entry(symbol).or_default().extend(witnesses);
        }
        let candidates = &self.candidates;
        self.bindings
            .retain(|symbol, _| candidates.contains(symbol));
    }
    pub(in super::super) fn retain(&mut self, mut keep: impl FnMut(&UnitSymbolId) -> bool) {
        self.candidates.retain(&mut keep);
        self.bindings.retain(|symbol, _| keep(symbol));
        self.incomplete.retain(&mut keep);
        self.invalid.retain(&mut keep);
        self.published.retain(&mut keep);
    }
}
impl LoopEscapeShadow {
    pub(in super::super) fn seed(
        &mut self,
        symbol: UnitSymbolId,
        value: UnitExpressionId,
        facts: &EscapeWitnesses,
    ) {
        self.0
            .get_or_insert_with(Default::default)
            .seed(symbol, value, facts);
    }
    pub(in super::super) fn merge(&mut self, other: Self) {
        match (&mut self.0, other.0) {
            (Some(left), Some(right)) => left.merge(*right),
            (Some(left), None) => {
                left.candidates.clear();
                left.bindings.clear();
            }
            (left @ None, Some(mut right)) => {
                right.candidates.clear();
                right.bindings.clear();
                *left = Some(right);
            }
            (None, None) => {}
        }
    }
    pub(in super::super) fn outer_candidates(&self) -> Option<&BTreeSet<UnitSymbolId>> {
        self.0
            .as_deref()
            .filter(|shadow| shadow.depth == 0)
            .map(|shadow| &shadow.candidates)
    }
    pub(in super::super) fn is_outer_candidate(&self, symbol: UnitSymbolId) -> bool {
        self.outer_candidates()
            .is_some_and(|candidates| candidates.contains(&symbol))
    }
    pub(in super::super) fn retain(&mut self, keep: impl FnMut(&UnitSymbolId) -> bool) {
        if let Some(shadow) = &mut self.0 {
            shadow.retain(keep);
            if **shadow == LoopEscapeShadowState::default() {
                self.0 = None;
            }
        }
    }
}
impl Checker<'_> {
    pub(in super::super) fn record_loop_escape_write(
        &self,
        symbol: UnitSymbolId,
        value: ExpressionId,
        state: &mut State,
    ) {
        let Some(shadow) = &mut state.loop_escape_shadow.0 else {
            return;
        };
        if !shadow.candidates.contains(&symbol) {
            return;
        }
        if shadow.depth == 0 {
            shadow.observe_outside(symbol, self.unit_expression(value), &state.escape_witnesses);
            return;
        }
        // A contextual null has the target's nullable type, not necessarily Nothing?.
        // Use the checked literal's value identity, crossing only transparent Group.
        // The caller already proved successful root assignment and normal completion.
        let nullable = matches!(
            self.typed
                .expression_type(self.unit_expression(value))
                .and_then(|ty| self.typed.types().get(ty)),
            Some(UnitTypeKind::Nullable(_))
        );
        let mut literal = value;
        let null_only = loop {
            match self
                .parsed
                .ast()
                .expressions()
                .get(literal)
                .map(|node| node.payload())
            {
                Ok(Expression::Group { expression }) => literal = *expression,
                Ok(Expression::Literal(LiteralKind::Null)) => break nullable,
                _ => break false,
            }
        };
        if null_only {
            shadow.bindings.insert(symbol, BTreeSet::new());
            // An earlier unsupported write on this path remains sticky.
        } else {
            shadow.invalid.insert(symbol);
        }
    }
}
impl State {
    pub(in super::super) fn begin_loop_escape_shadow(&mut self) {
        // Quarantine every loop-written candidate (active pre-loop or published by
        // an earlier bounded loop) before any loop condition/body can read it.
        let Some(shadow) = &mut self.loop_escape_shadow.0 else {
            return;
        };
        for symbol in shadow
            .candidates
            .iter()
            .copied()
            .chain(std::mem::take(&mut shadow.published))
        {
            self.escape_witnesses.bindings.remove(&symbol);
            self.escape_witnesses.known_bindings.remove(&symbol);
            self.escape_witnesses.mutable_locals.remove(&symbol);
        }
        shadow.depth += 1;
    }
    fn finish_loop_escape_shadow(&mut self, supported: bool) {
        let Some(shadow) = &mut self.loop_escape_shadow.0 else {
            return;
        };
        debug_assert!(shadow.depth > 0);
        shadow.depth -= 1;
        if !supported {
            shadow.invalid.extend(shadow.candidates.iter().copied());
        }

        if shadow.depth != 0 {
            return;
        }
        shadow
            .published
            .extend(shadow.candidates.difference(&shadow.invalid).copied());
        self.publish_loop_escape_shadow();
    }
    pub(in super::super) fn publish_loop_escape_shadow(&mut self) {
        let Some(shadow) = &self.loop_escape_shadow.0 else {
            return;
        };
        if shadow.depth != 0 {
            return;
        }
        for &symbol in shadow.published.intersection(&shadow.candidates) {
            if shadow.invalid.contains(&symbol) {
                continue;
            }
            let witnesses = shadow.bindings.get(&symbol);
            if shadow.incomplete.contains(&symbol) {
                self.escape_witnesses.known_bindings.remove(&symbol);
                if let Some(witnesses) = witnesses.filter(|w| !w.is_empty()) {
                    self.escape_witnesses
                        .bindings
                        .insert(symbol, witnesses.clone());
                    self.escape_witnesses.mutable_locals.insert(symbol);
                } else {
                    self.escape_witnesses.bindings.remove(&symbol);
                }
                continue;
            }
            let Some(witnesses) = witnesses else {
                continue;
            };
            if witnesses.is_empty() {
                self.escape_witnesses.bindings.remove(&symbol);
            } else {
                self.escape_witnesses
                    .bindings
                    .insert(symbol, witnesses.clone());
            }
            self.escape_witnesses.known_bindings.insert(symbol);
            self.escape_witnesses.mutable_locals.insert(symbol);
        }
    }
}
impl Flows {
    pub(in super::super) fn finish_loop_escape_shadow(&mut self, supported: bool) {
        for state in [&mut self.next, &mut self.breaks, &mut self.continues]
            .into_iter()
            .flatten()
        {
            state.finish_loop_escape_shadow(supported);
            #[cfg(test)]
            tests::observe(state, supported);
        }
    }
}

#[cfg(test)]
mod tests {
    #[derive(Clone)]
    struct Observation {
        shadow: super::LoopEscapeShadowState,
        active: super::EscapeWitnesses,
        supported: bool,
    }
    thread_local! {
        static OBSERVATIONS: std::cell::RefCell<Option<Vec<Observation>>> = const { std::cell::RefCell::new(None) };
    }
    pub(super) fn observe(state: &super::State, supported: bool) {
        OBSERVATIONS.with_borrow_mut(|observations| {
            if let Some(observations) = observations {
                observations.push(Observation {
                    shadow: state
                        .loop_escape_shadow
                        .0
                        .as_deref()
                        .cloned()
                        .unwrap_or_default(),
                    active: (*state.escape_witnesses).clone(),
                    supported,
                });
            }
        });
    }

    use super::*;
    use crate::ownership_checking::compilation_unit::formed_tests::Fixture;

    fn seeded() -> (State, UnitSymbolId, UnitExpressionId) {
        let fixture = Fixture::new(&[(
            "shadow.ko",
            "fun run(flag:Boolean) { val f = {}; var optional = if(flag) f else null }",
        )]);
        let symbol = fixture.symbol(0, "optional");
        let source = UnitExpressionId::new(fixture.source(0), fixture.expression(0, "{}"));
        let mut facts = EscapeWitnesses::default();
        facts.coverage.insert(source, Coverage::Known);
        facts.results.insert(source, [source].into());
        let mut state = State::default();
        state.loop_escape_shadow.seed(symbol, source, &facts);
        (state, symbol, source)
    }
    #[test]
    fn ordinary_states_do_not_allocate_shadow_for_nested_loops() {
        let mut state = State::default();
        state.begin_loop_escape_shadow();
        state.begin_loop_escape_shadow();
        state.finish_loop_escape_shadow(false);
        state.finish_loop_escape_shadow(true);
        super::super::super::merge_state(&mut state, State::default());
        assert!(state.loop_escape_shadow.0.is_none());
        assert_eq!(
            std::mem::size_of::<LoopEscapeShadow>(),
            std::mem::size_of::<usize>()
        );
    }
    #[test]
    fn absent_arm_and_cloned_shadow_preserve_independent_authority() {
        let (mut state, symbol, source) = seeded();
        state.begin_loop_escape_shadow();
        state.finish_loop_escape_shadow(true);
        let mut changed = state.clone();
        changed
            .loop_escape_shadow
            .0
            .as_mut()
            .unwrap()
            .bindings
            .insert(symbol, BTreeSet::new());
        assert_eq!(
            state
                .loop_escape_shadow
                .0
                .as_ref()
                .unwrap()
                .bindings
                .get(&symbol),
            Some(&[source].into())
        );
        for (mut left, right) in [(state.clone(), State::default()), (State::default(), state)] {
            super::super::super::merge_state(&mut left, right);
            assert!(
                left.loop_escape_shadow
                    .0
                    .as_ref()
                    .unwrap()
                    .candidates
                    .is_empty()
            );
            assert!(
                left.loop_escape_shadow
                    .0
                    .as_ref()
                    .unwrap()
                    .published
                    .contains(&symbol)
            );
            assert!(!left.escape_witnesses.known_bindings.contains(&symbol));
            left.begin_loop_escape_shadow();
            assert!(!left.escape_witnesses.bindings.contains_key(&symbol));
            left.finish_loop_escape_shadow(true);
        }
    }
    #[test]
    fn only_outer_proven_completion_publishes_nonempty_seed() {
        let (mut state, symbol, source) = seeded();
        state.begin_loop_escape_shadow();
        state.begin_loop_escape_shadow();
        state.finish_loop_escape_shadow(true);
        assert_eq!(state.loop_escape_shadow.0.as_mut().unwrap().depth, 1);
        assert!(state.escape_witnesses.bindings.is_empty());
        state.finish_loop_escape_shadow(true);
        assert_eq!(
            state.escape_witnesses.bindings.get(&symbol),
            Some(&[source].into())
        );
        state.begin_loop_escape_shadow();
        assert!(!state.escape_witnesses.bindings.contains_key(&symbol));
        assert!(!state.escape_witnesses.known_bindings.contains(&symbol));
        assert!(!state.escape_witnesses.mutable_locals.contains(&symbol));
        state.finish_loop_escape_shadow(false);
        assert!(
            state
                .loop_escape_shadow
                .0
                .as_mut()
                .unwrap()
                .invalid
                .contains(&symbol)
        );
        assert!(!state.escape_witnesses.bindings.contains_key(&symbol));
    }
    #[test]
    fn unsupported_nested_frame_never_becomes_known_empty() {
        let (mut state, symbol, _) = seeded();
        state.begin_loop_escape_shadow();
        state.begin_loop_escape_shadow();
        state.finish_loop_escape_shadow(false);
        state
            .loop_escape_shadow
            .0
            .as_mut()
            .unwrap()
            .bindings
            .insert(symbol, BTreeSet::new());
        state.finish_loop_escape_shadow(true);
        assert!(
            state
                .loop_escape_shadow
                .0
                .as_mut()
                .unwrap()
                .invalid
                .contains(&symbol)
        );
        assert!(!state.escape_witnesses.known_bindings.contains(&symbol));
        assert!(!state.escape_witnesses.mutable_locals.contains(&symbol));
    }
    #[test]
    fn unsupported_branch_is_sticky_in_either_merge_order() {
        let (mut base, symbol, _) = seeded();
        base.begin_loop_escape_shadow();
        let mut unknown = base.clone();
        unknown
            .loop_escape_shadow
            .0
            .as_mut()
            .unwrap()
            .invalid
            .insert(symbol);
        unknown
            .loop_escape_shadow
            .0
            .as_mut()
            .unwrap()
            .bindings
            .insert(symbol, BTreeSet::new());
        for (mut left, right) in [(base.clone(), unknown.clone()), (unknown, base)] {
            super::super::super::merge_state(&mut left, right);
            left.finish_loop_escape_shadow(true);
            assert!(!left.escape_witnesses.known_bindings.contains(&symbol));
            assert!(
                left.loop_escape_shadow
                    .0
                    .as_mut()
                    .unwrap()
                    .invalid
                    .contains(&symbol)
            );
        }
    }
    #[test]
    fn scope_pruning_removes_seed_and_published_authority() {
        let (mut state, symbol, _) = seeded();
        state.begin_loop_escape_shadow();
        state.finish_loop_escape_shadow(true);
        state
            .loop_escape_shadow
            .retain(|candidate| *candidate != symbol);
        assert_eq!(state.loop_escape_shadow, LoopEscapeShadow::default());
    }
    #[test]
    fn unknown_outer_arm_blocks_republication_in_both_orders() {
        let (base, symbol, source) = seeded();
        let mut published = base.clone();
        published.begin_loop_escape_shadow();
        published.finish_loop_escape_shadow(true);
        let mut unknown = base;
        unknown
            .loop_escape_shadow
            .0
            .as_mut()
            .unwrap()
            .observe_outside(symbol, source, &EscapeWitnesses::default());
        for (mut left, right) in [(published.clone(), unknown.clone()), (unknown, published)] {
            super::super::super::merge_state(&mut left, right);
            assert!(
                left.loop_escape_shadow
                    .0
                    .as_mut()
                    .unwrap()
                    .incomplete
                    .contains(&symbol)
            );
            assert!(!left.escape_witnesses.known_bindings.contains(&symbol));
            assert_eq!(
                left.escape_witnesses.bindings.get(&symbol),
                Some(&[source].into())
            );
        }
    }
    #[test]
    fn real_unmodeled_backedges_and_writes_do_not_publish_an_empty_proof() {
        for (body, expects_unsupported_frame) in [
            ("while (iterate) { optional = null; continue }", true),
            ("while (iterate) { optional = null }", true),
            (
                "loop { while (if(choose) { continue } else false) { optional=null; break }; break }",
                true,
            ),
            (
                "loop { while (iterate) { optional = null; break }; if (choose) { continue }; break }",
                true,
            ),
            (
                "while (iterate) { optional = null; break }; for (item in xs) { optional = null }",
                true,
            ),
            (
                "loop { val tmp = optional; optional = tmp; optional = null; break }",
                false,
            ),
            (
                "loop { if (choose) { optional = ({}) } else { optional = null }; break }",
                false,
            ),
        ] {
            let text = format!(
                "fun read(xs: List<Int>) {{}}\nfun run(own xs: List<Int>, choose:Boolean, iterate:Boolean): () -> Unit {{ val f: () -> Unit = {{ read(xs) }}; var optional=if(choose) f else null; {body}; return optional!! }}"
            );
            let fixture = Fixture::new(&[("unmodeled.ko", &text)]);
            let symbol = fixture.symbol(0, "optional");
            OBSERVATIONS.with_borrow_mut(|slot| *slot = Some(Vec::new()));
            let owned = fixture.shared();
            let observations = OBSERVATIONS.with_borrow_mut(|slot| slot.take().unwrap());
            eprintln!("UNCLAIMED {body}: {:?}", owned.diagnostics());
            // Deliberately do not assert that absence of a diagnostic is safe.
            // These checks concern the private proof boundary, not general loop acceptance.
            let last = observations.last().expect("actual loop exit observation");
            assert_eq!(last.shadow.depth, 0, "{body}");
            assert!(last.shadow.invalid.contains(&symbol), "{body}");
            assert!(!last.shadow.published.contains(&symbol), "{body}");
            assert!(!last.active.known_bindings.contains(&symbol), "{body}");
            assert!(!last.active.mutable_locals.contains(&symbol), "{body}");
            for inner in observations.iter().filter(|o| o.shadow.depth > 0) {
                assert!(!inner.active.known_bindings.contains(&symbol), "{body}");
                assert!(!inner.shadow.published.contains(&symbol), "{body}");
            }
            if expects_unsupported_frame {
                assert!(observations.iter().any(|o| !o.supported), "{body}");
            }
        }
    }
}
