//! 形成时解析 source owner；之后随 closure 流动的输入不得重新查询 binding。
use super::{CleanupConditionId, ClosureCaptureSource, DropPlanner, ExpressionId, ValueState};
use crate::ownership_checking::{CleanupCaptureInput, CleanupCaptureValue, CleanupConditions};

impl DropPlanner<'_, '_> {
    pub(super) fn capture_inputs(
        &mut self,
        lambda: ExpressionId,
        state: &ValueState,
    ) -> Vec<CleanupCaptureInput> {
        let mut inputs = Vec::new();
        for capture in self.checker.captures_of(lambda) {
            let input = CleanupCaptureInput {
                source: capture.source(),
                value: CleanupCaptureValue::Place(capture.source()),
                mode: capture.mode(),
                effect: capture.effect(),
                condition: state.path,
                origin: capture.reference_span(),
            };
            if let ClosureCaptureSource::Symbol(symbol) = capture.source()
                && let Some(index) = state.position(symbol)
            {
                for version in &state.values[index].versions {
                    let condition = self.conditions.and(state.path, version.condition);
                    if condition != CleanupConditionId::NEVER {
                        inputs.push(CleanupCaptureInput {
                            value: CleanupCaptureValue::Owner(version.owner),
                            condition,
                            ..input
                        });
                    }
                }
            } else if let Some((owner, enclosing)) = self.current_environment
                && self
                    .checker
                    .captures_of(enclosing)
                    .any(|outer| outer.source() == capture.source())
            {
                let slot = self
                    .conditions
                    .capture_slot(owner, capture.source())
                    .expect("checked enclosing capture has a registered slot");
                inputs.push(CleanupCaptureInput {
                    value: CleanupCaptureValue::Environment {
                        owner,
                        source: capture.source(),
                        slot,
                    },
                    ..input
                });
            } else {
                inputs.push(input);
            }
        }
        inputs
    }

    pub(super) fn restrict_capture_inputs(
        &mut self,
        inputs: &mut Vec<CleanupCaptureInput>,
        condition: CleanupConditionId,
    ) {
        for input in inputs.iter_mut() {
            input.condition = self.conditions.and(input.condition, condition);
        }
        inputs.retain(|input| input.condition != CleanupConditionId::NEVER);
    }

    pub(super) fn merge_capture_inputs(
        &mut self,
        lambda: ExpressionId,
        into: &mut Vec<CleanupCaptureInput>,
        inputs: Vec<CleanupCaptureInput>,
    ) {
        let slots = self
            .checker
            .captures_of(lambda)
            .map(|capture| capture.source())
            .collect::<Vec<_>>();
        merge_inputs(&mut self.conditions, into, inputs, &slots);
    }
}

fn merge_inputs(
    conditions: &mut CleanupConditions,
    into: &mut Vec<CleanupCaptureInput>,
    inputs: Vec<CleanupCaptureInput>,
    slots: &[ClosureCaptureSource],
) {
    for input in inputs {
        if let Some(prior) = into.iter_mut().find(|prior| {
            prior.source == input.source
                && prior.value == input.value
                && prior.mode == input.mode
                && prior.effect == input.effect
        }) {
            prior.condition = conditions.or(prior.condition, input.condition);
        } else {
            into.push(input);
        }
    }
    let order = slots
        .iter()
        .enumerate()
        .map(|(index, &source)| (source, index))
        .collect::<std::collections::BTreeMap<_, _>>();
    // 分支可以裁掉同槽的不同版本；按已检查槽序分组后才可整体逆序清理。
    into.sort_by_key(|input| (order[&input.source], input.value));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        name_resolution::SymbolId,
        ownership_checking::{CleanupOwnerValue, ClosureCaptureEffect, ClosureCaptureMode},
        source::SourceMap,
    };

    #[test]
    fn merging_versions_preserves_reverse_slot_cleanup_on_each_path() {
        let mut sources = SourceMap::new();
        let source = sources
            .add_source("capture-order.ko", "fun f() { if (flag) {} else {} }")
            .unwrap();
        let parsed =
            crate::parser::parse_file(&sources, &crate::lexer::lex(&sources, source).unwrap())
                .unwrap();
        let (control, node) = parsed
            .ast()
            .expressions()
            .iter()
            .find(|(_, node)| matches!(node.payload(), crate::parser::Expression::If { .. }))
            .unwrap();
        let mut table = CleanupConditions::default();
        let left = table.branch(control, node.span(), 2, 0).unwrap();
        let right = table.branch(control, node.span(), 2, 1).unwrap();
        let a = ClosureCaptureSource::Symbol(SymbolId(0));
        let b = ClosureCaptureSource::Symbol(SymbolId(1));
        let mut input = |source, condition| {
            let owner = table.create_owner(CleanupOwnerValue::Expression {
                expression: control,
                origin: node.span(),
            });
            CleanupCaptureInput {
                source,
                value: CleanupCaptureValue::Owner(owner),
                condition,
                mode: ClosureCaptureMode::Owned,
                effect: ClosureCaptureEffect::Move,
                origin: node.span(),
            }
        };
        let first = input(a, left);
        let second = input(a, right);
        let last = input(b, CleanupConditionId::ALWAYS);
        let mut merged = vec![first, last];
        merge_inputs(&mut table, &mut merged, vec![second, last], &[a, b]);
        for path in [left, right] {
            let dropped = merged
                .iter()
                .rev()
                .filter(|input| {
                    input.condition == path || input.condition == CleanupConditionId::ALWAYS
                })
                .map(|input| input.source)
                .collect::<Vec<_>>();
            assert_eq!(
                dropped,
                [b, a],
                "each selected environment releases the later slot first"
            );
        }
    }
}
