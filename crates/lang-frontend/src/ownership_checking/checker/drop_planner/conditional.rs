//! 条件 owner 与 closure 来源共同流动；分支结果在 scope cleanup 前取得持有权。
mod instance_sources;
use super::{
    CleanupConditionId, ClosureCaptureEffect, ClosureCaptureMode, ClosureCaptureSource, DropFact,
    DropPlanner, DropPoint, DropTarget, Expression, ExpressionId, ExpressionUse,
    IterationCleanupAction, OwnerVersion, OwnershipCheckingError, SymbolId, ValueState,
};
use crate::ownership_checking::CleanupOwnerValueId;

#[derive(Clone, Debug)]
pub(super) struct ClosureOrigin {
    pub(super) inputs: Vec<crate::ownership_checking::CleanupCaptureInput>,
    /// 递归 phi 的有限 shared 持有摘要；不是根环境的真实捕获槽。
    pub(super) held_sources: Vec<OwnerVersion>,
    pub(super) captured: Vec<ClosureOrigin>,
    pub(super) captured_from: Option<ClosureCaptureSource>,
    pub(super) owner: crate::ownership_checking::CleanupOwnerValueId,
    pub(super) layout_owner: crate::ownership_checking::CleanupOwnerValueId,
    pub(super) closure: ExpressionId,
    pub(super) condition: CleanupConditionId,
}

struct SharedSourceRelease {
    owner: CleanupOwnerValueId,
    selected: CleanupConditionId,
    last_loan: Option<CleanupConditionId>,
    source_location: Option<(
        crate::ownership_checking::CleanupInstanceAddressId,
        crate::ownership_checking::CleanupCaptureSlotId,
    )>,
}

struct ReleaseTraversal<'a> {
    root: Option<CleanupOwnerValueId>,
    capture_path: &'a [usize],
    shared_sources: &'a mut Vec<SharedSourceRelease>,
}

impl ClosureOrigin {
    pub(super) fn holds_recursive_source(&self, owner: CleanupOwnerValueId) -> bool {
        self.held_sources
            .iter()
            .any(|held| held.owner == owner && held.condition != CleanupConditionId::NEVER)
            || self
                .captured
                .iter()
                .any(|captured| captured.holds_recursive_source(owner))
    }

    pub(super) fn conditions(&self) -> Vec<CleanupConditionId> {
        std::iter::once(self.condition)
            .chain(self.inputs.iter().map(|input| input.condition))
            .chain(self.held_sources.iter().map(|source| source.condition))
            .chain(self.captured.iter().flat_map(|origin| origin.conditions()))
            .collect()
    }

    pub(super) fn conditions_mut(&mut self) -> Vec<&mut CleanupConditionId> {
        let mut conditions = vec![&mut self.condition];
        conditions.extend(self.inputs.iter_mut().map(|input| &mut input.condition));
        conditions.extend(
            self.held_sources
                .iter_mut()
                .map(|source| &mut source.condition),
        );
        for origin in &mut self.captured {
            conditions.extend(origin.conditions_mut());
        }
        conditions
    }
}

impl DropPlanner<'_, '_> {
    pub(super) fn expression(
        &mut self,
        id: ExpressionId,
        usage: ExpressionUse,
        state: &mut ValueState,
    ) -> Result<bool, OwnershipCheckingError> {
        state.result_closures.clear();
        state.result_owners.clear();
        let payload = self.checker.parsed.ast().expressions().get(id)?.payload();
        let direct = match payload {
            Expression::Lambda { .. } => {
                let inputs = self.capture_inputs(id, state);
                let capture_sources = self
                    .checker
                    .captures_of(id)
                    .map(|capture| capture.source())
                    .collect::<Vec<_>>();
                let captured: Vec<ClosureOrigin> = self
                    .checker
                    .captures_of(id)
                    .filter(|capture| {
                        capture.mode() == ClosureCaptureMode::Owned
                            && capture.effect() == ClosureCaptureEffect::Move
                    })
                    .flat_map(|capture| {
                        let source = capture.source();
                        let origins = match source {
                            ClosureCaptureSource::Symbol(symbol) => state.closures.get(&symbol),
                            ClosureCaptureSource::This => None,
                        };
                        origins
                            .into_iter()
                            .flatten()
                            .cloned()
                            .map(move |mut origin| {
                                origin.captured_from = Some(source);
                                origin
                            })
                    })
                    .collect();
                let owner = self.conditions.create_closure_owner(
                    id,
                    self.checker.parsed.ast().expressions().get(id)?.span(),
                    inputs.clone(),
                    &capture_sources,
                );
                let release_layouts = inputs
                    .iter()
                    .filter(|input| {
                        input.mode() == ClosureCaptureMode::Owned
                            && input.effect() == ClosureCaptureEffect::Move
                    })
                    .filter_map(|input| self.recursive_capture_layout(input.value()))
                    .collect::<Vec<_>>();
                if let Some(&first) = release_layouts.first() {
                    let layout = match first {
                        crate::ownership_checking::ClosureReleaseLayout::Iteration(statement)
                            if release_layouts.iter().all(|candidate| *candidate == first)
                                && self
                                    .loop_capture_graphs
                                    .get(&statement.index())
                                    .is_some_and(|graph| {
                                        graph.nodes().iter().any(|node| node.closure() == id)
                                    }) =>
                        {
                            first
                        }
                        _ => crate::ownership_checking::ClosureReleaseLayout::File,
                    };
                    self.recursive_release_closure_owners.insert(owner, layout);
                } else if Self::needs_enclosing_instance_release(&inputs, &captured) {
                    self.recursive_release_closure_owners
                        .insert(owner, crate::ownership_checking::ClosureReleaseLayout::File);
                }
                vec![ClosureOrigin {
                    held_sources: Vec::new(),
                    inputs,
                    captured,
                    captured_from: None,
                    owner,
                    layout_owner: owner,
                    closure: id,
                    condition: state.path,
                }]
            }
            Expression::Name => self
                .checker
                .reference_symbol(self.checker.parsed.ast().expressions().get(id)?.span())
                .and_then(|symbol| state.closures.get(&symbol).cloned())
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        let forwards = matches!(
            payload,
            Expression::Group { .. }
                | Expression::If { .. }
                | Expression::When { .. }
                | Expression::NonNullAssert { .. }
                | Expression::Binary {
                    operator: crate::parser::BinaryOperator::Elvis,
                    ..
                }
        );
        let versions = if matches!(payload, Expression::Name) {
            self.checker
                .reference_symbol(self.checker.parsed.ast().expressions().get(id)?.span())
                .and_then(|symbol| state.position(symbol))
                .map(|index| state.values[index].versions.clone())
                .unwrap_or_default()
        } else if matches!(payload, Expression::Lambda { .. }) {
            vec![OwnerVersion {
                owner: direct[0].owner,
                condition: state.path,
                origin: self.checker.parsed.ast().expressions().get(id)?.span(),
            }]
        } else {
            Vec::new()
        };
        let creates_environment = matches!(payload, Expression::Lambda { .. });
        if creates_environment {
            self.plan_lambda_expression(id, &direct[0], &state.retained_sources)?;
        }
        let continues = self.expression_inner(id, usage, state)?;
        if continues && creates_environment {
            let owner = direct[0].owner;
            self.cleanup.push((
                DropPoint::AfterExpression(id),
                IterationCleanupAction::CreateClosureOwner { owner, closure: id },
            ));
            for edge in self
                .conditions
                .closure_capture_edges(owner)
                .expect("created closure has registered capture slots")
            {
                self.cleanup.push((
                    DropPoint::AfterExpression(id),
                    IterationCleanupAction::SaveClosureCapture {
                        owner,
                        target: edge.target(),
                        input: edge.input(),
                    },
                ));
            }
        }
        if !continues || usage == ExpressionUse::Read {
            state.result_closures.clear();
            state.result_owners.clear();
        } else if !forwards {
            state.result_closures = direct;
            state.result_owners = versions;
            if state.result_owners.is_empty() && self.is_move_only_temporary(id) {
                state.result_owners.push(self.expression_owner(
                    id,
                    self.checker.parsed.ast().expressions().get(id)?.span(),
                    state.path,
                ));
            }
        }
        Ok(continues)
    }

    /// Branch selectors are established only after condition evaluation completes.
    pub(super) fn enter_branch(
        &mut self,
        control: ExpressionId,
        count: usize,
        branch: usize,
        state: &mut ValueState,
    ) -> Result<(), OwnershipCheckingError> {
        let origin = self.checker.parsed.ast().expressions().get(control)?.span();
        let selector = self
            .conditions
            .branch(control, origin, count, branch)
            .expect("drop planner uses one checked branch layout per control expression");
        self.restrict_path(selector, state);
        Ok(())
    }

    pub(super) fn enter_when_alternative(
        &mut self,
        control: ExpressionId,
        entry: usize,
        alternative: usize,
        matched: bool,
        state: &mut ValueState,
    ) -> Result<(), OwnershipCheckingError> {
        let origin = self.checker.parsed.ast().expressions().get(control)?.span();
        let selector = self
            .conditions
            .when_alternative(control, origin, entry, alternative, matched)
            .expect("checked when alternative layout");
        self.restrict_path(selector, state);
        Ok(())
    }

    fn restrict_path(&mut self, selector: CleanupConditionId, state: &mut ValueState) {
        state.path = self.conditions.and(state.path, selector);
        for value in &mut state.values {
            value.condition = self.conditions.and(value.condition, state.path);
            self.restrict_versions(&mut value.versions, state.path);
        }
        self.restrict_versions(&mut state.result_owners, state.path);
        for source in &mut state.retained_sources {
            source.condition = self.conditions.and(source.condition, state.path);
        }
        state
            .retained_sources
            .retain(|source| source.condition != CleanupConditionId::NEVER);
        for temporary in &mut state.nullable_temporaries {
            self.restrict_versions(&mut temporary.versions, state.path);
        }
        state
            .values
            .retain(|value| value.condition != CleanupConditionId::NEVER);
        for origins in state
            .closures
            .values_mut()
            .chain(std::iter::once(&mut state.result_closures))
            .chain(
                state
                    .nullable_temporaries
                    .iter_mut()
                    .map(|temporary| &mut temporary.closures),
            )
        {
            for origin in origins.iter_mut() {
                self.restrict_closure_origin(origin, state.path);
            }
            origins.retain(|origin| origin.condition != CleanupConditionId::NEVER);
        }
    }

    fn restrict_closure_origin(
        &mut self,
        origin: &mut ClosureOrigin,
        condition: CleanupConditionId,
    ) {
        origin.condition = self.conditions.and(origin.condition, condition);
        self.restrict_capture_inputs(&mut origin.inputs, condition);
        for held in &mut origin.held_sources {
            held.condition = self.conditions.and(held.condition, condition);
        }
        origin
            .held_sources
            .retain(|held| held.condition != CleanupConditionId::NEVER);
        for captured in &mut origin.captured {
            self.restrict_closure_origin(captured, condition);
        }
        origin
            .captured
            .retain(|captured| captured.condition != CleanupConditionId::NEVER);
    }

    fn merge_origins(&mut self, into: &mut Vec<ClosureOrigin>, origins: Vec<ClosureOrigin>) {
        for origin in origins {
            if let Some(prior) = into.iter_mut().find(|prior| {
                prior.owner == origin.owner
                    && prior.layout_owner == origin.layout_owner
                    && prior.closure == origin.closure
                    && prior.captured_from == origin.captured_from
            }) {
                prior.condition = self.conditions.or(prior.condition, origin.condition);
                self.merge_capture_inputs(origin.closure, &mut prior.inputs, origin.inputs);
                for held in origin.held_sources {
                    if let Some(existing) = prior
                        .held_sources
                        .iter_mut()
                        .find(|source| source.owner == held.owner)
                    {
                        existing.condition = self.conditions.or(existing.condition, held.condition);
                    } else {
                        prior.held_sources.push(held);
                    }
                }
                prior
                    .held_sources
                    .sort_by_key(|source| source.owner.index());
                self.merge_origins(&mut prior.captured, origin.captured);
            } else {
                into.push(origin);
            }
        }
        into.sort_by_key(|origin| {
            (
                origin.closure.index(),
                origin.owner.index(),
                origin.layout_owner.index(),
                origin.captured_from,
            )
        });
    }

    pub(super) fn merge_value_states(&mut self, states: Vec<ValueState>) -> ValueState {
        let mut states = states.into_iter();
        let Some(mut merged) = states.next() else {
            return ValueState::default();
        };
        for state in states {
            merged.path = self.conditions.or(merged.path, state.path);
            for value in state.values {
                if let Some(index) = merged.position(value.symbol) {
                    let prior = &mut merged.values[index];
                    prior.condition = self.conditions.or(prior.condition, value.condition);
                    self.merge_versions(&mut prior.versions, value.versions);
                    if value.origin.start() < prior.origin.start() {
                        prior.origin = value.origin;
                    }
                } else {
                    merged.insert(value);
                }
            }
            for (symbol, origins) in state.closures {
                self.merge_origins(merged.closures.entry(symbol).or_default(), origins);
            }
            self.merge_origins(&mut merged.result_closures, state.result_closures);
            self.merge_versions(&mut merged.result_owners, state.result_owners);
            for source in state.retained_sources {
                if let Some(prior) = merged
                    .retained_sources
                    .iter_mut()
                    .find(|prior| prior.owner == source.owner)
                {
                    prior.condition = self.conditions.or(prior.condition, source.condition);
                } else {
                    merged.retained_sources.push(source);
                }
            }
            for temporary in state.nullable_temporaries {
                if let Some(prior) = merged.nullable_temporaries.iter_mut().find(|prior| {
                    prior.control == temporary.control && prior.subject == temporary.subject
                }) {
                    self.merge_origins(&mut prior.closures, temporary.closures);
                    self.merge_versions(&mut prior.versions, temporary.versions);
                }
            }
        }
        // Owners missing from one incoming edge still have a conditional cleanup obligation.
        merged.values.sort_by_key(|value| value.declaration.start());
        merged.retained_sources.sort_by_key(|source| source.owner);
        merged
    }

    fn guard(
        &self,
        condition: CleanupConditionId,
        path: CleanupConditionId,
    ) -> Option<CleanupConditionId> {
        (condition != path && condition != CleanupConditionId::ALWAYS).then_some(condition)
    }

    fn push_guarded(
        &mut self,
        fact: DropFact,
        condition: CleanupConditionId,
        path: CleanupConditionId,
    ) {
        if condition == CleanupConditionId::NEVER {
            return;
        }
        self.push_fact(match self.guard(condition, path) {
            Some(condition) => fact.with_condition(condition),
            None => fact,
        });
    }

    pub(super) fn drop_named(
        &mut self,
        point: DropPoint,
        symbol: SymbolId,
        state: &mut ValueState,
    ) {
        if self.owner_protected_by_context(symbol, state) {
            return;
        }
        let Some(index) = state.position(symbol) else {
            return;
        };
        let value = state.values[index].clone();
        if state.replacements.contains(&symbol) {
            // A transfer can abandon the RHS, but an inner loop jump must retain outer owners.
            let leaving = match point {
                DropPoint::ControlTransfer(expression) => self
                    .checker
                    .parsed
                    .ast()
                    .expressions()
                    .get(expression)
                    .is_ok_and(|node| {
                        matches!(node.payload(), Expression::Return { .. })
                            || (matches!(
                                node.payload(),
                                Expression::Break { .. } | Expression::Continue { .. }
                            ) && self
                                .loop_boundaries
                                .last()
                                .is_some_and(|depth| value.scope_depth > *depth))
                    }),
                _ => false,
            };
            if !leaving {
                return;
            }
        }
        let mut released_versions = Vec::new();
        let mut retained_versions = Vec::new();
        let mut released = CleanupConditionId::NEVER;
        let mut retained = CleanupConditionId::NEVER;
        for version in &value.versions {
            let mut held = CleanupConditionId::NEVER;
            for origin in state
                .closures
                .values()
                .flatten()
                .chain(&state.result_closures)
                .chain(
                    state
                        .nullable_temporaries
                        .iter()
                        .flat_map(|temporary| &temporary.closures),
                )
            {
                let selected =
                    self.shared_capture_condition(origin, version.owner, origin.condition);
                held = self.conditions.or(held, selected);
            }
            let not_held = self.conditions.not(held);
            let available = self.conditions.and(version.condition, not_held);
            if available != CleanupConditionId::NEVER {
                released = self.conditions.or(released, available);
                released_versions.push(OwnerVersion {
                    condition: available,
                    ..*version
                });
            }
            let held = self.conditions.and(version.condition, held);
            if held != CleanupConditionId::NEVER {
                retained = self.conditions.or(retained, held);
                retained_versions.push(OwnerVersion {
                    condition: held,
                    ..*version
                });
            }
        }
        if released == CleanupConditionId::NEVER {
            return;
        }
        if retained == CleanupConditionId::NEVER {
            state.remove_value(symbol);
        } else {
            state.values[index].condition = retained;
            state.values[index].versions = retained_versions;
        }
        let origins = state.closures.remove(&symbol).unwrap_or_default();
        let mut released_origins = Vec::new();
        let mut retained_origins = Vec::new();
        for origin in origins {
            let condition = self.conditions.and(origin.condition, released);
            if condition != CleanupConditionId::NEVER {
                let mut released_origin = ClosureOrigin {
                    condition,
                    ..origin.clone()
                };
                self.restrict_closure_origin(&mut released_origin, condition);
                released_origins.push(released_origin);
            }
            let condition = self.conditions.and(origin.condition, retained);
            if condition != CleanupConditionId::NEVER {
                let mut retained_origin = ClosureOrigin {
                    condition,
                    ..origin
                };
                self.restrict_closure_origin(&mut retained_origin, condition);
                retained_origins.push(retained_origin);
            }
        }
        if !retained_origins.is_empty() {
            state.closures.insert(symbol, retained_origins);
        }
        self.drop_closure_owner(
            DropFact::new(point, DropTarget::Named(symbol), value.origin),
            released,
            &released_versions,
            released_origins,
            state,
        );
    }

    /// A moved closure owns its captured environments, including their shared source loans.
    fn shared_capture_condition(
        &mut self,
        origin: &ClosureOrigin,
        owner: crate::ownership_checking::CleanupOwnerValueId,
        path: CleanupConditionId,
    ) -> CleanupConditionId {
        let mut held = CleanupConditionId::NEVER;
        for held_source in &origin.held_sources {
            if held_source.owner == owner {
                let selected = self.conditions.and(path, held_source.condition);
                held = self.conditions.or(held, selected);
            }
        }
        for input in &origin.inputs {
            if input.mode == ClosureCaptureMode::Shared
                && input.value == crate::ownership_checking::CleanupCaptureValue::Owner(owner)
            {
                let selected = self.conditions.and(path, input.condition);
                held = self.conditions.or(held, selected);
            }
        }
        for captured in &origin.captured {
            let path = self.conditions.and(path, captured.condition);
            let selected = self.shared_capture_condition(captured, owner, path);
            held = self.conditions.or(held, selected);
        }
        held
    }

    fn drop_closure_owner_inner(
        &mut self,
        fact: DropFact,
        condition: CleanupConditionId,
        versions: &[OwnerVersion],
        origins: Vec<ClosureOrigin>,
        state: &mut ValueState,
        mut traversal: ReleaseTraversal<'_>,
    ) {
        let point = fact.point();
        let mut pending = versions.iter().rev().copied().collect::<Vec<_>>();
        let mut versions = Vec::new();
        while let Some(version) = pending.pop() {
            if let Some(sources) = self.recursive_snapshot_sources.get(&version.owner).cloned() {
                for source in sources.into_iter().rev() {
                    let condition = self.conditions.and(version.condition, source.condition);
                    pending.push(OwnerVersion {
                        owner: source.owner,
                        condition,
                        origin: source.origin,
                    });
                }
            } else {
                versions.push(version);
            }
        }
        // 根实例动作沿已形成的 owned 边释放；每层均抑制该 owner 的树形后代。
        let instance_release_owners = versions
            .iter()
            .filter(|version| self.recursive_release_layout(version.owner).is_some())
            .map(|version| version.owner)
            .collect::<Vec<_>>();
        self.collect_instance_shared_sources(
            condition,
            &origins,
            &instance_release_owners,
            &mut traversal,
        );
        let origins = origins
            .into_iter()
            .filter(|origin| {
                !instance_release_owners.contains(&origin.owner)
                    && !instance_release_owners.contains(&origin.layout_owner)
            })
            .collect::<Vec<_>>();
        for origin in &origins {
            let root = traversal.root.unwrap_or(origin.owner);
            for input in origin.inputs.iter().rev() {
                let selected = self.conditions.and(origin.condition, input.condition);
                if input.mode == ClosureCaptureMode::Owned
                    && input.effect == ClosureCaptureEffect::Move
                {
                    let mut fact = DropFact::new(
                        point,
                        DropTarget::Captured {
                            owner: origin.owner,
                            closure: origin.closure,
                            source: input.source,
                            value: input.value,
                        },
                        input.origin,
                    );
                    let Some(slot) = self
                        .conditions
                        .capture_slot(origin.layout_owner, input.source)
                        .or_else(|| {
                            self.conditions.phi_capture_slot(
                                origin.layout_owner,
                                origin.closure,
                                input.source,
                            )
                        })
                    else {
                        // 没有已检查槽就不能给 captured drop 指派运行时实例地址。
                        self.enclosing_capture_phi.get_or_insert(origin.closure);
                        continue;
                    };
                    let Some(layout) = self.conditions.capture_slot_value(slot) else {
                        self.enclosing_capture_phi.get_or_insert(origin.closure);
                        continue;
                    };
                    let address = self
                        .conditions
                        .register_instance_address(root, traversal.capture_path);
                    fact = fact.with_capture_slot(slot).with_instance_address(address);
                    if matches!(
                        input.value,
                        crate::ownership_checking::CleanupCaptureValue::Owner(_)
                            | crate::ownership_checking::CleanupCaptureValue::Environment { .. }
                    ) {
                        let owner = match input.value {
                            crate::ownership_checking::CleanupCaptureValue::Owner(owner) => {
                                Some(owner)
                            }
                            _ => None,
                        };
                        if let Some(owner) = owner {
                            fact = fact.with_owner(owner);
                        }
                        let mut captured = origin
                            .captured
                            .iter()
                            .filter(|captured| {
                                captured.captured_from == Some(input.source)
                                    && owner.is_none_or(|owner| captured.owner == owner)
                            })
                            .cloned()
                            .collect::<Vec<_>>();
                        if owner.is_none()
                            && (captured.len() != 1
                                || !captured[0].inputs.iter().any(|input| {
                                    input.mode == ClosureCaptureMode::Owned
                                        && input.effect == ClosureCaptureEffect::Move
                                })
                                || captured[0]
                                    .inputs
                                    .iter()
                                    .any(|input| input.mode == ClosureCaptureMode::Shared))
                        {
                            // Only a unique owned child can be expanded without choosing a
                            // runtime candidate or reproducing shared loan release.
                            captured.clear();
                        }
                        for captured in &mut captured {
                            self.restrict_closure_origin(captured, selected);
                        }
                        captured.retain(|captured| captured.condition != CleanupConditionId::NEVER);
                        if !captured.is_empty() {
                            if owner.is_none() {
                                // An enclosing slot can also hold an opaque closure. Keep its
                                // parent drop on paths not covered by the known child.
                                let covered = captured.iter().fold(
                                    CleanupConditionId::NEVER,
                                    |covered, captured| {
                                        self.conditions.or(covered, captured.condition)
                                    },
                                );
                                let uncovered = self.conditions.not(covered);
                                let uncovered = self.conditions.and(selected, uncovered);
                                self.push_guarded(fact, uncovered, state.path);
                            }
                            let versions = if let Some(owner) = owner {
                                vec![OwnerVersion {
                                    owner,
                                    condition: selected,
                                    origin: input.origin,
                                }]
                            } else {
                                captured
                                    .iter()
                                    .map(|captured| OwnerVersion {
                                        owner: captured.owner,
                                        condition: captured.condition,
                                        origin: input.origin,
                                    })
                                    .collect::<Vec<_>>()
                            };
                            let mut child_path = traversal.capture_path.to_vec();
                            child_path.push(layout.position());
                            self.drop_closure_owner_inner(
                                fact,
                                selected,
                                &versions,
                                captured,
                                state,
                                ReleaseTraversal {
                                    root: Some(root),
                                    capture_path: &child_path,
                                    shared_sources: &mut *traversal.shared_sources,
                                },
                            );
                            continue;
                        }
                    }
                    self.push_guarded(fact, selected, state.path);
                }
            }
        }
        if versions.is_empty() {
            self.push_guarded(fact, condition, state.path);
        } else {
            for version in &versions {
                let selected = self.conditions.and(condition, version.condition);
                let origin = if matches!(fact.target(), DropTarget::Named(_)) {
                    version.origin
                } else {
                    fact.value_origin()
                };
                let mut released =
                    DropFact::new(point, fact.target(), origin).with_owner(version.owner);
                if let Some(slot) = fact.capture_slot() {
                    released = released.with_capture_slot(slot);
                }
                if let Some(address) = fact.instance_address() {
                    released = released.with_instance_address(address);
                }
                self.push_guarded(released, selected, state.path);
            }
        }
        for origin in origins {
            for input in origin
                .inputs
                .iter()
                .rev()
                .filter(|input| input.mode == ClosureCaptureMode::Shared)
            {
                let selected = self.conditions.and(origin.condition, input.condition);
                if selected == CleanupConditionId::NEVER {
                    continue;
                }
                let address = self.conditions.register_instance_address(
                    traversal.root.unwrap_or(origin.owner),
                    traversal.capture_path,
                );
                let slot = self
                    .conditions
                    .capture_slot(origin.layout_owner, input.source)
                    .or_else(|| {
                        self.conditions.phi_capture_slot(
                            origin.layout_owner,
                            origin.closure,
                            input.source,
                        )
                    });
                self.cleanup.push((
                    point,
                    IterationCleanupAction::EndCaptureLoan {
                        owner: origin.owner,
                        instance_address: address,
                        capture_slot: slot,
                        condition: self.guard(selected, state.path),
                        closure: origin.closure,
                        source: input.source,
                        value: input.value,
                    },
                ));
                if let crate::ownership_checking::CleanupCaptureValue::Owner(owner) = input.value {
                    let named = state
                        .values
                        .iter()
                        .any(|value| value.versions.iter().any(|version| version.owner == owner));
                    let retained = if named {
                        None
                    } else {
                        state
                            .retained_sources
                            .iter()
                            .find(|source| source.owner == owner)
                            .copied()
                    };
                    let source_location = retained.and_then(|_| {
                        if slot.is_none() {
                            self.enclosing_capture_phi.get_or_insert(origin.closure);
                        }
                        slot.map(|slot| (address, slot))
                    });
                    let last_loan =
                        retained
                            .zip(source_location)
                            .map(|(retained, (address, slot))| {
                                let released = self.conditions.and(retained.condition, selected);
                                let (selector, last_loan) =
                                    self.conditions.last_capture_loan(owner, retained.origin);
                                self.cleanup.push((
                                    point,
                                    IterationCleanupAction::TestLastCaptureLoan {
                                        owner,
                                        instance_address: address,
                                        capture_slot: slot,
                                        selector,
                                        condition: self.guard(released, state.path),
                                    },
                                ));
                                last_loan
                            });
                    traversal.shared_sources.push(SharedSourceRelease {
                        owner,
                        selected,
                        last_loan,
                        source_location,
                    });
                }
            }
        }
    }

    fn release_shared_sources(
        &mut self,
        point: DropPoint,
        shared_sources: Vec<SharedSourceRelease>,
        state: &mut ValueState,
    ) {
        let mut named_sources = Vec::new();
        let mut retained_releases = Vec::new();
        for release in shared_sources {
            let owner = release.owner;
            let source = state
                .values
                .iter()
                .find(|value| value.versions.iter().any(|version| version.owner == owner))
                .map(|value| value.symbol);
            if let Some(source) = source {
                if !named_sources.contains(&source) {
                    named_sources.push(source);
                }
            } else if let Some(retained) = state
                .retained_sources
                .iter()
                .find(|source| source.owner == owner)
                .copied()
            {
                let released = self.conditions.and(retained.condition, release.selected);
                if let Some((_, condition)) = retained_releases
                    .iter_mut()
                    .find(|(candidate, _)| *candidate == owner)
                {
                    *condition = self.conditions.or(*condition, released);
                } else {
                    retained_releases.push((owner, released));
                }
                let last_loan = release.last_loan.or_else(|| {
                    let (address, slot) = release.source_location?;
                    let (selector, last_loan) =
                        self.conditions.last_capture_loan(owner, retained.origin);
                    self.cleanup.push((
                        point,
                        IterationCleanupAction::TestLastCaptureLoan {
                            owner,
                            instance_address: address,
                            capture_slot: slot,
                            selector,
                            condition: self.guard(released, state.path),
                        },
                    ));
                    Some(last_loan)
                });
                if let Some(last_loan) = last_loan {
                    let Some((address, slot)) = release.source_location else {
                        continue;
                    };
                    let final_release = self.conditions.and(released, last_loan);
                    self.push_guarded(
                        DropFact::new(
                            point,
                            DropTarget::RetainedSource(ClosureCaptureSource::Symbol(
                                retained.symbol,
                            )),
                            retained.origin,
                        )
                        .with_owner(owner)
                        .with_instance_address(address)
                        .with_capture_slot(slot),
                        final_release,
                        state.path,
                    );
                }
            }
        }
        for (owner, released) in retained_releases {
            if let Some(index) = state
                .retained_sources
                .iter()
                .position(|source| source.owner == owner)
            {
                let not_released = self.conditions.not(released);
                let remaining = self
                    .conditions
                    .and(state.retained_sources[index].condition, not_released);
                if remaining == CleanupConditionId::NEVER {
                    state.retained_sources.remove(index);
                } else {
                    state.retained_sources[index].condition = remaining;
                }
            }
        }
        for source in named_sources {
            // Match-edge liveness does not include future reads in the selected entry body.
            if !matches!(point, DropPoint::WhenAlternativeMatch { .. })
                && !self.live_after(point).contains(&source)
            {
                self.drop_named(point, source, state);
            }
        }
    }
}
