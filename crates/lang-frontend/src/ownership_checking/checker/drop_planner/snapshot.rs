//! 将 RHS 的选择保存到独立 owner，再按同一顺序流提交给 binding。
use std::collections::BTreeSet;

use super::{
    ClosureOrigin, DropPlanner, DropPoint, ExpressionId, IterationCleanupAction,
    OwnershipCheckingError, SymbolId, ValueState,
};
use crate::ownership_checking::{
    CleanupCaptureInput, CleanupCaptureValue, CleanupCondition, CleanupConditionId,
    CleanupConditions, CleanupOwnerValueId, CleanupSelectorId, ClosureCaptureSource,
};

fn uses_selector(
    table: &CleanupConditions,
    condition: CleanupConditionId,
    selector: CleanupSelectorId,
) -> bool {
    let mut pending = vec![condition];
    let mut seen = BTreeSet::new();
    while let Some(next) = pending.pop() {
        if !seen.insert(next) {
            continue;
        }
        if let Some(CleanupCondition::Choice {
            selector: candidate,
            branches,
        }) = table.get(next)
        {
            if *candidate == selector {
                return true;
            }
            pending.extend(branches);
        }
    }
    false
}

fn unique_environment_input(
    conditions: &mut CleanupConditions,
    inputs: &[CleanupCaptureInput],
    source: ClosureCaptureSource,
    when: CleanupConditionId,
) -> Option<CleanupCaptureValue> {
    let mut values = BTreeSet::new();
    let mut covered = CleanupConditionId::NEVER;
    for input in inputs.iter().filter(|input| input.source() == source) {
        if conditions.and(when, input.condition()) == CleanupConditionId::NEVER {
            continue;
        }
        covered = conditions.or(covered, input.condition());
        values.insert(input.value());
    }
    let uncovered = conditions.not(covered);
    if conditions.and(when, uncovered) != CleanupConditionId::NEVER {
        return None;
    }
    let value = (values.len() == 1).then(|| *values.first().unwrap())?;
    matches!(value, CleanupCaptureValue::Environment { .. }).then_some(value)
}

fn located_snapshot_source(
    conditions: &mut CleanupConditions,
    roots: &[ClosureOrigin],
    source: CleanupSelectorId,
    when: CleanupConditionId,
) -> Option<CleanupCaptureValue> {
    let mut sources = BTreeSet::new();
    for root in roots {
        let root_when = conditions.and(when, root.condition);
        if root_when == CleanupConditionId::NEVER {
            continue;
        }
        if uses_selector(conditions, root.condition, source)
            || root
                .inputs
                .iter()
                .any(|input| uses_selector(conditions, input.condition(), source))
        {
            return None;
        }
        for captured in &root.captured {
            let captured_when = conditions.and(root_when, captured.condition);
            if captured_when == CleanupConditionId::NEVER
                || !captured
                    .conditions()
                    .into_iter()
                    .any(|condition| uses_selector(conditions, condition, source))
            {
                continue;
            }
            let value = if captured.captured.is_empty() && captured.inputs.is_empty() {
                conditions
                    .owner_snapshot(captured.owner)
                    .filter(|snapshot| {
                        snapshot
                            .copies()
                            .iter()
                            .any(|saved| saved.target() == source)
                    })
                    .and(captured.captured_from)
                    .and_then(|capture_source| {
                        unique_environment_input(
                            conditions,
                            &root.inputs,
                            capture_source,
                            captured_when,
                        )?;
                        // 形成动作已把 owned 来源移出父槽；快照从结果环境的接收槽读取。
                        let slot = conditions.capture_slot(root.layout_owner, capture_source)?;
                        Some(CleanupCaptureValue::Environment {
                            owner: root.owner,
                            source: capture_source,
                            slot,
                        })
                    })
            } else {
                None
            };
            // 无法定位的相关候选也必须阻止“唯一来源”发布。
            sources.insert(value);
        }
    }
    (sources.len() == 1)
        .then(|| sources.first().copied())
        .flatten()
        .flatten()
}

impl DropPlanner<'_, '_> {
    pub(super) fn save_result_snapshot(
        &mut self,
        value: ExpressionId,
        state: &mut ValueState,
    ) -> Result<Option<CleanupOwnerValueId>, OwnershipCheckingError> {
        let source_roots = state
            .result_owners
            .iter()
            .flat_map(|version| {
                self.phi_root_conditions(version.owner).into_iter().map(
                    |(closure, source, condition)| (closure, source, version.condition, condition),
                )
            })
            .collect::<Vec<_>>()
            .into_iter()
            .map(|(closure, source, available, condition)| {
                (closure, source, self.conditions.and(available, condition))
            })
            .collect::<Vec<_>>();
        if state.result_closures.is_empty() && source_roots.is_empty() {
            return Ok(None);
        }
        // 值可用性与 capture 持有必须使用同一选择；保留原路径作为分支内快照的可达保护。
        let roots = state
            .values
            .iter()
            .flat_map(|value| {
                std::iter::once(value.condition)
                    .chain(value.versions.iter().map(|version| version.condition))
            })
            .chain(state.result_owners.iter().map(|version| version.condition))
            .chain(state.retained_sources.iter().map(|source| source.condition))
            .chain(
                state
                    .closures
                    .values()
                    .flatten()
                    .flat_map(|origin| origin.conditions()),
            )
            .chain(
                state
                    .result_closures
                    .iter()
                    .flat_map(|origin| origin.conditions()),
            )
            .chain(state.nullable_temporaries.iter().flat_map(|temporary| {
                temporary
                    .closures
                    .iter()
                    .flat_map(|origin| origin.conditions())
                    .chain(temporary.versions.iter().map(|version| version.condition))
            }))
            .chain(source_roots.iter().map(|(_, _, condition)| *condition))
            .collect::<Vec<_>>();
        let owner = self
            .conditions
            .snapshot_conditions(
                value,
                self.checker.parsed.ast().expressions().get(value)?.span(),
                &roots,
                &state
                    .result_closures
                    .iter()
                    .map(|origin| (origin.owner, origin.condition))
                    .collect::<Vec<_>>(),
            )
            .expect("planner conditions belong to this table");
        let copies = self
            .conditions
            .owner_snapshot(owner)
            .expect("new snapshot")
            .copies()
            .to_vec();
        // 可达性查询不能向正式条件表添加仅供定位的节点。
        let mut probe = self.conditions.clone();
        let located_copies = copies
            .iter()
            .filter_map(|copy| {
                located_snapshot_source(
                    &mut probe,
                    &state.result_closures,
                    copy.source(),
                    copy.when(),
                )
                .map(|value| (copy.source(), value))
            })
            .collect::<Vec<_>>();
        for (source, value) in located_copies {
            assert!(
                self.conditions
                    .set_snapshot_copy_source_value(owner, source, value)
            );
        }
        if self
            .conditions
            .owner_snapshot(owner)
            .expect("new snapshot")
            .copies()
            .is_empty()
        {
            return Ok(None);
        }
        assert!(
            self.conditions.set_snapshot_value_inputs(
                owner,
                &state
                    .result_owners
                    .iter()
                    .map(|version| (version.owner, version.condition))
                    .collect::<Vec<_>>(),
            )
        );
        self.cleanup.push((
            DropPoint::AfterExpression(value),
            IterationCleanupAction::SaveOwnerSnapshot {
                condition: (state.path != super::CleanupConditionId::ALWAYS).then_some(state.path),
                owner,
                value,
            },
        ));

        let rebound = self
            .conditions
            .owner_snapshot(owner)
            .expect("known snapshot")
            .conditions()
            .to_vec();
        let mut rebound = rebound.into_iter();
        for condition in state
            .values
            .iter_mut()
            .flat_map(|value| {
                std::iter::once(&mut value.condition).chain(
                    value
                        .versions
                        .iter_mut()
                        .map(|version| &mut version.condition),
                )
            })
            .chain(
                state
                    .result_owners
                    .iter_mut()
                    .map(|version| &mut version.condition),
            )
            .chain(
                state
                    .retained_sources
                    .iter_mut()
                    .map(|source| &mut source.condition),
            )
            .chain(
                state
                    .closures
                    .values_mut()
                    .flatten()
                    .flat_map(|origin| origin.conditions_mut()),
            )
            .chain(
                state
                    .result_closures
                    .iter_mut()
                    .flat_map(|origin| origin.conditions_mut()),
            )
            .chain(state.nullable_temporaries.iter_mut().flat_map(|temporary| {
                temporary
                    .closures
                    .iter_mut()
                    .flat_map(|origin| origin.conditions_mut())
                    .chain(
                        temporary
                            .versions
                            .iter_mut()
                            .map(|version| &mut version.condition),
                    )
            }))
        {
            *condition = self.conditions.and(
                state.path,
                rebound.next().expect("one condition per state field"),
            );
        }
        let mapped_roots = source_roots
            .into_iter()
            .map(|(closure, source, _)| {
                (
                    closure,
                    source,
                    self.conditions.and(
                        state.path,
                        rebound.next().expect("one condition per phi root"),
                    ),
                )
            })
            .collect::<Vec<_>>();
        assert!(rebound.next().is_none());
        if !mapped_roots.is_empty() {
            self.snapshot_phi_roots.insert(owner, mapped_roots);
        }
        for origin in &mut state.result_closures {
            origin.owner = owner;
        }
        if state.result_owners.iter().any(|version| {
            self.recursive_release_layout(version.owner).is_some()
                || self.recursive_snapshot_sources.contains_key(&version.owner)
        }) {
            // 快照可合流普通 closure 与递归 phi；保留各来源的选择条件供根释放拆分。
            self.recursive_snapshot_sources
                .insert(owner, state.result_owners.clone());
        }
        state.result_owners = vec![super::OwnerVersion {
            owner,
            condition: state.path,
            origin: self.checker.parsed.ast().expressions().get(value)?.span(),
        }];
        Ok(Some(owner))
    }

    pub(super) fn commit_snapshot(
        &mut self,
        owner: Option<CleanupOwnerValueId>,
        value: ExpressionId,
        target: SymbolId,
    ) {
        if let Some(owner) = owner {
            self.cleanup.push((
                DropPoint::AfterExpression(value),
                IterationCleanupAction::CommitOwnerSnapshot { owner, target },
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        name_resolution::SymbolId,
        ownership_checking::{
            CleanupCaptureInput, ClosureCaptureEffect, ClosureCaptureMode, ClosureCaptureSource,
        },
        source::SourceMap,
    };

    #[test]
    fn snapshot_source_location_requires_one_enclosing_environment() {
        let mut sources = SourceMap::new();
        let source_id = sources
            .add_source("snapshot-source.ko", "fun f() { if (true) {} else {} }")
            .unwrap();
        let parsed =
            crate::parser::parse_file(&sources, &crate::lexer::lex(&sources, source_id).unwrap())
                .unwrap();
        let (expression, node) = parsed
            .ast()
            .expressions()
            .iter()
            .find(|(_, node)| matches!(node.payload(), crate::parser::Expression::If { .. }))
            .unwrap();
        let source = ClosureCaptureSource::Symbol(SymbolId(0));
        let mut table = CleanupConditions::default();
        let first = table.create_closure_owner(expression, node.span(), vec![], &[source]);
        let second = table.create_closure_owner(expression, node.span(), vec![], &[source]);
        let left = table.branch(expression, node.span(), 2, 0).unwrap();
        let right = table.branch(expression, node.span(), 2, 1).unwrap();
        let input = |owner, condition| CleanupCaptureInput {
            source,
            value: CleanupCaptureValue::Environment {
                owner,
                source,
                slot: table.capture_slot(owner, source).unwrap(),
            },
            mode: ClosureCaptureMode::Owned,
            effect: ClosureCaptureEffect::Move,
            condition,
            origin: node.span(),
        };
        let first_input = input(first, left);
        let second_input = input(second, right);
        let complete_input = input(first, CleanupConditionId::ALWAYS);
        let mut probe = table.clone();
        assert_eq!(
            unique_environment_input(
                &mut probe,
                &[first_input, second_input],
                source,
                CleanupConditionId::ALWAYS,
            ),
            None,
            "two possible parent instances cannot be represented by one source location"
        );
        assert_eq!(
            unique_environment_input(&mut probe, &[first_input, second_input], source, left),
            Some(first_input.value())
        );
        assert_eq!(
            unique_environment_input(&mut probe, &[first_input, second_input], source, right),
            Some(second_input.value())
        );
        assert_eq!(
            unique_environment_input(
                &mut probe,
                &[first_input],
                source,
                CleanupConditionId::ALWAYS,
            ),
            None,
            "an input on only one arm cannot cover the whole capture path"
        );
        assert_eq!(
            unique_environment_input(
                &mut probe,
                &[first_input, input(first, right)],
                source,
                CleanupConditionId::ALWAYS
            ),
            Some(first_input.value())
        );

        let leaf_owner = table
            .snapshot_conditions(expression, node.span(), &[left, right], &[])
            .unwrap();
        let leaf_snapshot = table.owner_snapshot(leaf_owner).unwrap();
        let saved_selector = leaf_snapshot.copies()[0].target();
        let selected_condition = leaf_snapshot.conditions()[0];
        let selected_leaf = ClosureOrigin {
            held_sources: Vec::new(),
            inputs: Vec::new(),
            captured: Vec::new(),
            captured_from: Some(source),
            owner: leaf_owner,
            layout_owner: leaf_owner,
            closure: expression,
            condition: selected_condition,
        };
        let root = |owner, inputs| ClosureOrigin {
            held_sources: Vec::new(),
            inputs,
            captured: vec![selected_leaf.clone()],
            captured_from: None,
            owner,
            layout_owner: owner,
            closure: expression,
            condition: CleanupConditionId::ALWAYS,
        };
        let good = root(second, vec![complete_input]);
        let ambiguous = root(second, vec![complete_input, second_input]);
        let saved_location = CleanupCaptureValue::Environment {
            owner: second,
            source,
            slot: table.capture_slot(second, source).unwrap(),
        };
        let mut probe = table.clone();
        assert_eq!(
            located_snapshot_source(
                &mut probe,
                std::slice::from_ref(&good),
                saved_selector,
                CleanupConditionId::ALWAYS,
            ),
            Some(saved_location)
        );
        assert_eq!(
            located_snapshot_source(
                &mut probe,
                &[good, ambiguous],
                saved_selector,
                CleanupConditionId::ALWAYS,
            ),
            None,
            "an unresolved reachable candidate must block source location"
        );
    }
}
