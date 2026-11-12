//! Fresh, declaration-only post-loop catalog. Never revives a legacy observation.
use super::*;
use crate::ownership_checking::UnitOwnerProducerCatalogV1;
mod body;
pub(super) mod bridge;

pub(super) struct Segment {
    predecessor: UnitStatementId,
    bridge: Option<bridge::Bridge>,
    normal: Option<body::Body>,
    normal_started: bool,
    statements: Vec<UnitStatementId>,
    condition: UnitExpressionId,
    body: UnitStatementId,
    bridge_condition: Option<UnitExpressionId>,
    bridge_body: Option<(UnitExpressionId, UnitStatementId)>,
    admitted: bool,
    ends_function: bool,
    next: usize,
    active: bool,
    invalid: bool,
    catalog: UnitOwnerProducerCatalogV1,
    pending: Option<(UnitExpressionId, UnitOwnerId)>,
    bindings: Vec<(UnitSymbolId, UnitOwnerId)>,
    captured: bool,
    events: Vec<UnitOwnerTransportEvent>,
}

// Reuse the exact counter discovery contract; do not infer tokens from this shape.
pub(super) fn discover(checker: &Checker<'_>, item: ItemId) -> Vec<Segment> {
    let candidates = root_incoming::counter_prefixes(checker, item);
    if candidates.len() < 2 {
        return Vec::new();
    }
    let ast = checker.parsed.ast();
    let Ok(node) = ast.items().get(item) else {
        return Vec::new();
    };
    let Item::Function { form, .. } = node.payload() else {
        return Vec::new();
    };
    let body = match form {
        FunctionForm::ImplicitUnitBlock(body)
        | FunctionForm::Explicit {
            body: FunctionBody::Block(body),
            ..
        } => *body,
        _ => return Vec::new(),
    };
    let Ok(node) = ast.statements().get(body) else {
        return Vec::new();
    };
    let Statement::Block { elements } = node.payload() else {
        return Vec::new();
    };
    let mut segments = Vec::new();
    for pair in candidates.windows(2) {
        let Some(first) = elements.iter().position(|id| *id == pair[0].0.statement()) else {
            return Vec::new();
        };
        let Some(second) = elements.iter().position(|id| *id == pair[1].0.statement()) else {
            return Vec::new();
        };
        if first + 1 >= second
            || !elements[first + 1..second].iter().all(|id| {
                ast.statements()
                    .get(*id)
                    .ok()
                    .is_some_and(|node| matches!(node.payload(), Statement::LocalVariable { .. }))
            })
        {
            return Vec::new();
        }
        let Ok(node) = ast.statements().get(elements[second]) else {
            return Vec::new();
        };
        let Statement::While {
            condition, body, ..
        } = node.payload()
        else {
            return Vec::new();
        };
        segments.push(Segment {
            predecessor: pair[0].0,
            bridge: None,
            normal: None,
            normal_started: false,
            condition: checker.unit_expression(*condition),
            body: UnitStatementId::new(checker.source_unit, *body),
            bridge_condition: None,
            bridge_body: None,
            admitted: false,
            ends_function: second + 1 == elements.len(),
            statements: elements[first + 1..=second]
                .iter()
                .map(|id| UnitStatementId::new(checker.source_unit, *id))
                .collect(),
            next: 0,
            active: false,
            invalid: false,
            captured: false,
            // Separate namespace: dense indexes cannot collide with cleared legacy IDs.
            catalog: UnitOwnerProducerCatalogV1 {
                identity: std::sync::Arc::new(()),
                callable: UnitOwnerCallable::Function(crate::type_checking::UnitItemId::new(
                    checker.source_unit,
                    item,
                )),
                definitions: Vec::new(),
            },
            events: Vec::new(),
            pending: None,
            bindings: Vec::new(),
        });
    }
    segments
}
impl Segment {
    fn matches_state(&self, state: &ValueState) -> bool {
        if let Some(bridge) = &self.bridge {
            return bridge.matches_state(state);
        }
        boundary_values(state).is_some_and(|values| {
            values.len() == self.bindings.len()
                && values
                    .iter()
                    .all(|v| self.bindings.iter().filter(|(s, _)| *s == v.symbol).count() == 1)
        })
    }
    fn statement(&mut self, statement: UnitStatementId, state: &ValueState) {
        if self.captured || self.invalid {
            return;
        }
        if !self.active {
            if self.statements.first() != Some(&statement) {
                return;
            }
            self.active = true;
            if self.bridge.is_none() && !empty_boundary(state) {
                self.invalid = true;
                return;
            }
        }
        if self.statements.get(self.next) != Some(&statement)
            || self.pending.is_some()
            || !self.matches_state(state)
        {
            self.invalid = true;
            return;
        }
        self.next += 1;
        if self.next == self.statements.len() {
            self.captured = if let Some(bridge) = &self.bridge {
                bridge.complete()
            } else {
                self.bindings.len() >= 2 && self.catalog.is_consistent()
            };
            self.invalid = !self.captured;
            self.active = false;
        }
    }
    fn literal(&mut self, expression: UnitExpressionId) {
        if self.bridge.is_some() {
            self.invalid = true;
            return;
        }
        if self.pending.is_some()
            || self
                .catalog
                .definitions
                .iter()
                .any(|d| d.origin == UnitOwnerOrigin::String(expression))
        {
            self.invalid = true;
            return;
        }
        let id = UnitOwnerId {
            identity: self.catalog.identity.clone(),
            callable: self.catalog.callable,
            index: self.catalog.definitions.len(),
        };
        self.catalog.definitions.push(UnitOwnerDefinition {
            id: id.clone(),
            origin: UnitOwnerOrigin::String(expression),
        });
        self.pending = Some((expression, id));
    }
}
impl ControlTransactions {
    pub(super) fn counter_batch_identities(&self, count: usize) -> Option<Vec<std::sync::Arc<()>>> {
        if self.counter_segments.len() + 1 != count || !self.counter_segments.last()?.ends_function
        {
            return None;
        }
        Some(
            self.counter_segments
                .iter()
                .map(|segment| segment.catalog.identity.clone())
                .collect(),
        )
    }

    pub(in super::super::super) fn segment_statement(
        &mut self,
        id: UnitStatementId,
        state: &ValueState,
    ) {
        let predecessor = self
            .counter_segments
            .iter()
            .find(|s| !s.active && !s.invalid && !s.captured && s.statements.first() == Some(&id))
            .map(|s| s.predecessor);
        let prior = (!self.root_branches_invalid)
            .then(|| predecessor.and_then(|control| self.private_counter_evidence(control)))
            .flatten();
        for segment in &mut self.counter_segments {
            if !segment.active && segment.statements.first() == Some(&id) && !empty_boundary(state)
            {
                segment.bridge = prior
                    .as_ref()
                    .and_then(|e| bridge::Bridge::new(e, segment.predecessor, state));
            }
            segment.statement(id, state);
        }
    }
    fn active_segment(&mut self) -> Option<&mut Segment> {
        self.counter_segments
            .iter_mut()
            .find(|s| s.active && !s.invalid && !s.captured)
    }
    pub(in super::super::super) fn segment_literal(&mut self, expression: UnitExpressionId) {
        if let Some(segment) = self.active_segment() {
            segment.literal(expression);
        }
    }
    pub(in super::super::super) fn segment_move(
        &mut self,
        expression: UnitExpressionId,
        value: Option<&OwnedValue>,
    ) {
        let Some(segment) = self.active_segment() else {
            return;
        };
        if let Some(bridge) = &mut segment.bridge {
            segment.invalid = !bridge.move_value(expression, value);
            return;
        }
        let index = value.and_then(|v| segment.bindings.iter().position(|(s, _)| *s == v.symbol));
        if segment.pending.is_some() || index.is_none() {
            segment.invalid = true;
            return;
        }
        let Some(index) = index else {
            return;
        };
        let (symbol, token) = segment.bindings.remove(index);
        segment.events.push(UnitOwnerTransportEvent::Move {
            expression,
            symbol,
            owner: token.clone(),
        });
        segment.pending = Some((expression, token));
    }
    pub(in super::super::super) fn segment_bind(
        &mut self,
        symbol: UnitSymbolId,
        expression: UnitExpressionId,
    ) {
        let Some(segment) = self.active_segment() else {
            return;
        };
        if let Some(bridge) = &mut segment.bridge {
            segment.invalid = !bridge.bind(symbol, expression);
            return;
        }
        let Some((source, token)) = segment.pending.take() else {
            segment.invalid = true;
            return;
        };
        if source != expression || segment.bindings.iter().any(|(s, _)| *s == symbol) {
            segment.invalid = true;
            return;
        }
        segment.events.push(UnitOwnerTransportEvent::Binding {
            symbol,
            initializer: expression,
            owner: token.clone(),
        });
        segment.bindings.push((symbol, token));
    }
    pub(in super::super::super) fn segment_drop(
        &mut self,
        point: crate::ownership_checking::UnitDropPoint,
        symbol: UnitSymbolId,
    ) {
        let Some(segment) = self.active_segment() else {
            return;
        };
        if segment.bridge.is_some() {
            segment.invalid = true;
            return;
        }
        let Some(index) = segment.bindings.iter().position(|(s, _)| *s == symbol) else {
            segment.invalid = true;
            return;
        };
        let (_, owner) = segment.bindings.remove(index);
        segment.events.push(UnitOwnerTransportEvent::Drop {
            point,
            symbol,
            owner,
        });
    }
    // Consume the fresh entry once. Never inject its tokens into legacy ValueState.
    pub(super) fn take_segment_entry(
        &mut self,
        control: UnitStatementId,
        state: &ValueState,
    ) -> Option<(UnitOwnerProducerCatalogV1, root_incoming::RootIncoming)> {
        let segment = self
            .counter_segments
            .iter_mut()
            .find(|s| s.statements.last() == Some(&control))?;
        // A static bridge is not a producer catalog or an entry permission.
        if segment.bridge.is_some() {
            return None;
        }
        if !segment.captured
            || segment.invalid
            || segment.admitted
            || !segment.matches_state(state)
            || state.values.iter().any(|value| value.owner.is_some())
        {
            segment.invalid = true;
            return None;
        }
        segment.admitted = true;
        Some((segment.catalog.clone(), segment.bindings.clone()))
    }
    // Only the discovered pure Name condition can preserve this separate entry namespace.
    pub(super) fn segment_condition(
        &self,
        control: UnitStatementId,
        condition: UnitExpressionId,
        state: &ValueState,
    ) -> bool {
        self.counter_segments.iter().any(|s| {
            s.statements.last() == Some(&control)
                && s.captured
                && s.admitted
                && !s.invalid
                && s.condition == condition
                && s.matches_state(state)
                && state.values.iter().all(|value| value.owner.is_none())
        })
    }
    // These are actual planner events, never candidate-discovery evidence.
    pub(super) fn record_bridge_condition(
        &mut self,
        control: UnitStatementId,
        condition: UnitExpressionId,
        state: &ValueState,
    ) {
        #[cfg(test)]
        if let Some(body) = self
            .counter_segments
            .iter()
            .find(|s| {
                s.statements.last() == Some(&control)
                    && s.bridge.is_some()
                    && s.captured
                    && !s.invalid
            })
            .map(|s| s.body)
        {
            if bridge_entry_tests::fault(1) {
                return;
            }
            if bridge_entry_tests::fault(3) {
                self.record_bridge_body_entry(control, condition, body, state);
            }
        }
        for segment in &mut self.counter_segments {
            if segment.statements.last() != Some(&control) || segment.bridge.is_none() {
                continue;
            }
            if segment.invalid {
                continue;
            }
            let valid = !self.root_branches_invalid
                && segment.captured
                && segment.condition == condition
                && segment.bridge_condition.is_none()
                && segment.bridge_body.is_none()
                && segment.matches_state(state);
            segment.invalid = !valid;
            segment.bridge_condition = valid.then_some(condition);
            segment.bridge_body = None;
        }
    }
    pub(super) fn record_bridge_body_entry(
        &mut self,
        control: UnitStatementId,
        condition: UnitExpressionId,
        body: UnitStatementId,
        state: &ValueState,
    ) {
        #[cfg(test)]
        if self.counter_segments.iter().any(|s| {
            s.statements.last() == Some(&control) && s.bridge.is_some() && s.captured && !s.invalid
        }) && bridge_entry_tests::fault(2)
        {
            return;
        }
        #[cfg(test)]
        let altered = (self.counter_segments.iter().any(|s| {
            s.statements.last() == Some(&control) && s.bridge.is_some() && s.captured && !s.invalid
        }) && bridge_entry_tests::fault(4))
        .then(|| {
            let mut altered = state.clone();
            altered.values.pop();
            altered
        });
        #[cfg(test)]
        let state = altered.as_ref().unwrap_or(state);
        for segment in &mut self.counter_segments {
            if segment.statements.last() != Some(&control) || segment.bridge.is_none() {
                continue;
            }
            if segment.invalid {
                continue;
            }
            let valid = !self.root_branches_invalid
                && segment.captured
                && segment.condition == condition
                && segment.body == body
                && segment.bridge_condition == Some(condition)
                && segment.bridge_body.is_none()
                && segment.matches_state(state);
            segment.invalid = !valid;
            segment.bridge_body = valid.then_some((condition, body));
            if !valid {
                segment.bridge_condition = None;
            }
        }
    }
    pub(super) fn suffixes_mut(
        &mut self,
    ) -> impl Iterator<Item = &mut root_incoming::root_suffix::RootSuffix> {
        let invalid = self.root_branches_invalid;
        self.root_entries
            .iter_mut()
            .filter_map(|(_, e)| e.suffix_mut())
            .chain(
                self.counter_segments
                    .iter_mut()
                    .filter(move |s| !invalid && !s.invalid && s.captured)
                    .filter_map(|s| s.normal.as_mut()?.suffix_mut()),
            )
    }
    pub(in super::super::super) fn take_control_bridge(
        &mut self,
    ) -> Option<crate::ownership_checking::UnitOwnerControlBridgeEvidenceV1> {
        if self.root_branches_invalid {
            return None;
        }
        let [segment] = self.counter_segments.as_mut_slice() else {
            return None;
        };
        if !segment.captured || segment.invalid {
            return None;
        }
        let next = *segment.statements.last()?;
        let (condition, body) = segment.bridge_body?;
        if segment.bridge_condition != Some(condition) {
            return None;
        }
        let normal = segment.normal.as_ref().and_then(|body| body.result());
        let tail = segment.normal.as_ref().and_then(|body| body.counter_tail());
        let exit = segment
            .normal
            .as_ref()
            .and_then(|body| body.exit_partition());
        let suffix = segment
            .normal
            .as_ref()
            .and_then(|body| body.suffix_result());
        segment
            .bridge
            .take()?
            .evidence(next, condition, body)?
            .with_next_normal(normal)?
            .with_next_counter_tail(tail)?
            .with_next_false_exit_partition(exit)?
            .with_next_false_exit_suffix(suffix)
    }
    #[cfg(test)]
    pub(super) fn segment_bridge(&self, control: UnitStatementId) -> Option<bridge::Bridge> {
        if self.root_branches_invalid {
            return None;
        }
        self.counter_segments
            .iter()
            .find(|s| s.statements.last() == Some(&control) && s.captured && !s.invalid)?
            .bridge
            .clone()
    }
    #[cfg(test)]
    pub(super) fn segment_catalog(&self, control: UnitStatementId) -> Option<Snapshot> {
        let segment = self.counter_segments.iter().find(|s| {
            s.statements.last() == Some(&control) && s.captured && !s.invalid && s.bridge.is_none()
        })?;
        Some(Snapshot {
            catalog: segment.catalog.clone(),
            bindings: segment.bindings.clone(),
            events: segment.events.clone(),
        })
    }
}

#[cfg(test)]
#[derive(Clone, Debug)]
pub(in super::super::super) struct Snapshot {
    pub(in super::super::super) catalog: UnitOwnerProducerCatalogV1,
    pub(in super::super::super) bindings: Vec<(UnitSymbolId, UnitOwnerId)>,
    pub(in super::super::super) events: Vec<UnitOwnerTransportEvent>,
}

#[cfg(test)]
mod tests {
    use super::super::super::tests::compare_and_inspect;
    use super::*;
    use crate::ownership_checking::compilation_unit::formed_tests::Fixture;

    #[test]
    fn bridge_preserves_both_predecessor_vectors_through_actual_aliases() {
        for alias in [false, true] {
            for fresh in [false, true] {
                let transfer = if fresh {
                    "var a1 = a0; var b1 = b0; val extra = \"unused\";"
                } else if alias {
                    "val saved_b = b0; val saved_a = a0; var b1 = saved_b; var a1 = saved_a;"
                } else {
                    "var a1 = a0; var b1 = b0;"
                };
                let source = format!(
                    r#"fun run(n: Int) {{
                    var n0 = n; var g0 = n0 > 0; var a0 = "a"; var b0 = "b";
                    while(g0) {{ val x = replace(&a0, "x"); val y = replace(&b0, "y"); n0 = n0 - 1; g0 = n0 > 0 }};
                    var n1 = n; var g1 = n1 > 0; {transfer}
                    while(g1) {{ val x = replace(&a1, "x1"); val y = replace(&b1, "y1"); n1 = n1 - 1; g1 = n1 > 0 }}
                }}"#
                );
                let fixture = Fixture::new(&[("bridge.ko", &source)]);
                let owned = fixture.shared();
                assert!(owned.diagnostics().is_empty());
                assert!(
                    owned
                        .owner_observations()
                        .iter()
                        .all(|o| o.counter_batch_evidence_v1().is_none())
                );
                compare_and_inspect(&fixture, &fixture.parts(&owned), |checker| {
                    let mut planner = DropPlanner::new(
                        checker,
                        crate::ownership_checking::compilation_unit::dataflow::liveness::build(
                            checker, true,
                        )
                        .unwrap(),
                    );
                    planner.owner_control_probe = Some(Vec::new());
                    let planner = planner.run().unwrap();
                    let visits = planner.owner_control_probe.as_ref().unwrap();
                    let bridges: Vec<_> = visits
                        .iter()
                        .filter(|v| v.entering)
                        .filter_map(|v| v.segment_bridge.as_ref().map(|bridge| (v.control, bridge)))
                        .collect();
                    assert_eq!(bridges.len(), usize::from(!fresh));
                    for (next, bridge) in bridges {
                        let (prior, sources) = bridge.sources();
                        assert_ne!(prior, next);
                        assert_eq!(sources.len(), 2);
                        assert_ne!(sources[0].0, sources[1].0);
                        for (_, entry, body) in &sources {
                            assert_ne!(entry, body);
                        }
                        let predecessor = visits
                            .iter()
                            .find(|v| !v.entering && v.control == prior)
                            .unwrap();
                        let evidence = predecessor.private_counter.as_ref().unwrap();
                        for (position, (_, entry, body)) in sources.iter().enumerate() {
                            let index = if alias { 1 - position } else { position };
                            let slot = &evidence.root_slots()[index];
                            assert_eq!(slot.entry(), *entry);
                            assert_eq!(slot.first_body_output(), *body);
                        }
                    }
                });
            }
        }
    }
    #[test]
    fn fresh_segment_tracks_real_aliases_and_dead_prefix_values() {
        for (carry, interrupted) in [(false, false), (true, false), (false, true)] {
            let middle = if interrupted {
                "while(false) { val unused = \"gap\" };"
            } else {
                ""
            };
            let suffix = if carry { "val kept = a0" } else { "" };
            let source = format!(
                r#"fun run(n: Int) {{
                var n0 = n; var g0 = n0 > 0; var a0 = "a"; var b0 = "b";
                while(g0) {{ val x = replace(&a0, "x"); val y = replace(&b0, "y"); n0 = n0 - 1; g0 = n0 > 0 }};
                {middle}
                var n1 = n; var g1 = n1 > 0;
                val discarded = "unused";
                val seed = "fresh"; val alias = seed; var a1 = alias; var b1 = "b";
                while(g1) {{ val x = replace(&a1, "x"); val y = replace(&b1, "y"); n1 = n1 - 1; g1 = n1 > 0 }};
                {suffix}
            }}"#
            );
            let fixture = Fixture::new(&[("segment-lifecycle.ko", &source)]);
            let owned = fixture.shared();
            assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
            assert!(
                owned
                    .owner_observations()
                    .iter()
                    .all(|o| o.counter_tail_evidence_v1().is_none())
            );
            compare_and_inspect(&fixture, &fixture.parts(&owned), |checker| {
                let mut planner = DropPlanner::new(
                    checker,
                    crate::ownership_checking::compilation_unit::dataflow::liveness::build(
                        checker, true,
                    )
                    .unwrap(),
                );
                planner.owner_control_probe = Some(Vec::new());
                let planner = planner.run().unwrap();
                let snapshots: Vec<_> = planner
                    .owner_control_probe
                    .as_ref()
                    .unwrap()
                    .iter()
                    .filter(|v| v.entering)
                    .filter_map(|v| v.segment_catalog.as_ref())
                    .collect();
                assert_eq!(snapshots.len(), usize::from(!carry && !interrupted));
                if let Some(snapshot) = snapshots.first() {
                    assert_eq!(snapshot.catalog.definitions().len(), 3);
                    assert_eq!(snapshot.bindings.len(), 2);
                    assert_eq!(
                        snapshot
                            .events
                            .iter()
                            .filter(|e| matches!(e, UnitOwnerTransportEvent::Move { .. }))
                            .count(),
                        2
                    );
                    let dropped: Vec<_> = snapshot
                        .events
                        .iter()
                        .filter_map(|e| {
                            if let UnitOwnerTransportEvent::Drop { owner, .. } = e {
                                Some(owner)
                            } else {
                                None
                            }
                        })
                        .collect();
                    assert_eq!(dropped.len(), 1);
                    assert!(
                        snapshot
                            .bindings
                            .iter()
                            .all(|(_, token)| token != dropped[0])
                    );
                    for token in dropped {
                        assert!(snapshot.catalog.definition(token.clone()).is_some());
                    }
                }
            });
        }
    }
    #[test]
    fn second_body_requires_each_actual_transfer_and_cleanup() {
        for (body, live, expected) in [
            (
                r#"val x = replace(&a1, "x"); val y = replace(&b1, "y")"#,
                false,
                true,
            ),
            (
                r#"val x = replace(&a1, "x"); val y = replace(&b1, "y")"#,
                true,
                true,
            ),
            (
                r#"val x = replace(&a1, "x"); val y = replace(&a1, "y")"#,
                true,
                false,
            ),
            (
                r#"val x = replace(&a1, "x"); val y = replace(&b1, "y"); val late = x"#,
                false,
                false,
            ),
            (
                r#"if(g1) { val x = replace(&a1, "x") }; val y = replace(&b1, "y")"#,
                true,
                false,
            ),
        ] {
            let suffix = if live { "val aa = a1; val bb = b1" } else { "" };
            let source = format!(
                r#"fun run(n: Int) {{
                var n0 = n; var g0 = n0 > 0; var a0 = "a"; var b0 = "b";
                while(g0) {{ val x = replace(&a0, "x"); val y = replace(&b0, "y"); n0 = n0 - 1; g0 = n0 > 0 }};
                var n1 = n; var g1 = n1 > 0; var a1 = "a"; var b1 = "b";
                while(g1) {{ {body}; n1 = n1 - 1; g1 = n1 > 0 }};
                {suffix}
            }}"#
            );
            let fixture = Fixture::new(&[("second-body-events.ko", &source)]);
            let owned = fixture.shared();
            assert!(
                owned.diagnostics().is_empty(),
                "{body}: {:?}",
                owned.diagnostics()
            );
            assert!(
                owned
                    .owner_observations()
                    .iter()
                    .all(|o| o.counter_tail_evidence_v1().is_none())
            );
            let batches: Vec<_> = owned
                .owner_observations()
                .iter()
                .filter_map(|o| o.counter_batch_evidence_v1())
                .collect();
            assert_eq!(batches.len(), usize::from(expected && !live), "{body}");
            compare_and_inspect(&fixture, &fixture.parts(&owned), |checker| {
                let mut planner = DropPlanner::new(
                    checker,
                    crate::ownership_checking::compilation_unit::dataflow::liveness::build(
                        checker, true,
                    )
                    .unwrap(),
                );
                planner.owner_control_probe = Some(Vec::new());
                let planner = planner.run().unwrap();
                let visits = planner.owner_control_probe.as_ref().unwrap();
                let entry = visits
                    .iter()
                    .find(|v| v.entering && v.segment_catalog.is_some())
                    .unwrap();
                let exit = visits
                    .iter()
                    .find(|v| !v.entering && v.control == entry.control)
                    .unwrap();
                assert_eq!(exit.private_counter.is_some(), expected, "{body}");
                if let Some(evidence) = &exit.private_counter {
                    assert_eq!(evidence.steps().len(), 2);
                    let (roots, drops) = evidence.false_exit_partition().unwrap();
                    assert_eq!(roots.len(), if live { 2 } else { 0 });
                    assert_eq!(drops.len(), if live { 0 } else { 2 });
                    for write in &exit.root_writes {
                        let (old, new) = write.token_link.as_ref().unwrap();
                        assert!(evidence.input_catalog().definition(old.clone()).is_some());
                        assert!(evidence.body_catalog().definition(new.clone()).is_some());
                        assert!(write.token_retirement.is_some());
                    }
                }
            });
        }
    }
    // Injected post-capture corruption: compile locally, execute only in remote CI.
    #[test]
    fn ci_only_independent_entry_is_single_use_and_condition_state_is_exact() {
        let segment = |i| {
            format!(
                r#"var n{i} = n; var g{i} = n{i} > 0; var a{i} = "a"; var b{i} = "b";
            while(g{i}) {{ val x = replace(&a{i}, "x"); val y = replace(&b{i}, "y"); n{i} = n{i} - 1; g{i} = n{i} > 0 }};"#
            )
        };
        let source = format!("fun run(n: Int) {{ {} {} }}", segment(0), segment(1));
        let fixture = Fixture::new(&[("segment-entry-fault.ko", &source)]);
        let owned = fixture.shared();
        compare_and_inspect(&fixture, &fixture.parts(&owned), |checker| {
            let mut planner = DropPlanner::new(
                checker,
                crate::ownership_checking::compilation_unit::dataflow::liveness::build(
                    checker, true,
                )
                .unwrap(),
            )
            .run()
            .unwrap();
            let segment = &planner.owner_loop.counter_segments[0];
            assert!(segment.admitted);
            let control = *segment.statements.last().unwrap();
            let condition = segment.condition;
            let snapshot = planner.owner_loop.segment_catalog(control).unwrap();
            let span = checker
                .parsed
                .ast()
                .expressions()
                .get(condition.expression())
                .unwrap()
                .span();
            // Structural fault model based on the actual captured symbols, not a fresh runtime trace.
            let mut state = ValueState {
                values: snapshot
                    .bindings
                    .iter()
                    .map(|(symbol, _)| OwnedValue {
                        symbol: *symbol,
                        owner: None,
                        origin: span,
                        declaration: span,
                        scope_depth: 1,
                    })
                    .collect(),
                ..ValueState::default()
            };
            assert!(
                planner
                    .owner_loop
                    .segment_condition(control, condition, &state)
            );
            state.values[0].owner = Some(snapshot.bindings[0].1.clone());
            assert!(
                !planner
                    .owner_loop
                    .segment_condition(control, condition, &state)
            );
            state.values[0].owner = None;
            let removed = state.values.pop().unwrap();
            assert!(
                !planner
                    .owner_loop
                    .segment_condition(control, condition, &state)
            );
            state.values.push(removed);
            assert!(
                planner
                    .owner_loop
                    .take_segment_entry(control, &state)
                    .is_none()
            );
            assert!(
                !planner
                    .owner_loop
                    .segment_condition(control, condition, &state)
            );
            assert!(planner.owner_loop.segment_catalog(control).is_none());
        });
    }
}

#[cfg(test)]
mod bridge_entry_tests {
    use crate::ownership_checking::compilation_unit::formed_tests::Fixture;
    thread_local! { static OMIT: std::cell::Cell<u8> = const { std::cell::Cell::new(0) }; }
    thread_local! { static HITS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }
    pub(super) fn fault(phase: u8) -> bool {
        OMIT.with(|value| value.get() == phase)
            && HITS.with(|hits| {
                if hits.get() != 0 {
                    return false;
                }
                hits.set(1);
                true
            })
    }
    #[test]
    fn ci_only_control_bridge_requires_actual_condition_and_body_entry() {
        let source = r#"fun run(n: Int) {
            var n0 = n; var g0 = n0 > 0; var a0 = "a"; var b0 = "b";
            while(g0) { val x = replace(&a0, "x"); val y = replace(&b0, "y"); n0 = n0 - 1; g0 = n0 > 0 };
            var n1 = n; var g1 = n1 > 0; var a1 = a0; var b1 = b0;
            while(g1) { val x = replace(&a1, "x1"); val y = replace(&b1, "y1"); n1 = n1 - 1; g1 = n1 > 0 }
        }"#;
        let fixture = Fixture::new(&[("bridge-entry-events.ko", source)]);
        assert!(
            fixture.shared().owner_observations()[0]
                .control_bridge_evidence_v1()
                .is_some()
        );
        for phase in [1, 2, 3, 4] {
            HITS.with(|hits| hits.set(0));
            OMIT.with(|value| value.set(phase));
            let result = fixture.shared();
            OMIT.with(|value| value.set(0));
            assert_eq!(HITS.with(|hits| hits.get()), 1);
            assert!(
                result.owner_observations()[0]
                    .control_bridge_evidence_v1()
                    .is_none()
            );
        }
    }
}
