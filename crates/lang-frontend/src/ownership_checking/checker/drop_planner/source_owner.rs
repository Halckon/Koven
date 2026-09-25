//! Binding 只定位当前值；移动保留定义身份，分支分别保留各版本的可用性。
use super::{CleanupConditionId, DropPlanner, ExpressionId, StatementId, SymbolId, ValueState};
use crate::{
    ownership_checking::{CleanupOwnerValue, CleanupOwnerValueId},
    source::Span,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct OwnerVersion {
    pub(super) owner: CleanupOwnerValueId,
    pub(super) condition: CleanupConditionId,
    pub(super) origin: Span,
}

impl DropPlanner<'_, '_> {
    /// 临时值已求出时复用其定义；合流或投影未留下该定义时指向完整 backing 结果。
    pub(super) fn temporary_owner_version(
        &mut self,
        expression: ExpressionId,
        origin: Span,
        state: &ValueState,
    ) -> OwnerVersion {
        let owner = if let Some(&owner) = self.temporary_backing_owners.get(&expression.index()) {
            owner
        } else {
            let owner = state
                .result_owners
                .iter()
                .find_map(|version| {
                    matches!(
                        self.conditions.owner_value(version.owner),
                        Some(CleanupOwnerValue::Expression { expression: current, .. })
                            if *current == expression
                    )
                    .then_some(version.owner)
                })
                .unwrap_or_else(|| self.expression_owner(expression, origin, state.path).owner);
            self.temporary_backing_owners
                .insert(expression.index(), owner);
            owner
        };
        OwnerVersion {
            owner,
            condition: state.path,
            origin,
        }
    }

    pub(super) fn parameter_owner(&mut self, symbol: SymbolId, origin: Span) -> OwnerVersion {
        OwnerVersion {
            owner: self
                .conditions
                .create_owner(CleanupOwnerValue::Parameter { symbol, origin }),
            condition: CleanupConditionId::ALWAYS,
            origin,
        }
    }

    pub(super) fn expression_owner(
        &mut self,
        expression: ExpressionId,
        origin: Span,
        condition: CleanupConditionId,
    ) -> OwnerVersion {
        OwnerVersion {
            owner: self
                .conditions
                .create_owner(CleanupOwnerValue::Expression { expression, origin }),
            condition,
            origin,
        }
    }

    pub(super) fn component_owner(
        &mut self,
        statement: StatementId,
        symbol: SymbolId,
        origin: Span,
        condition: CleanupConditionId,
    ) -> OwnerVersion {
        OwnerVersion {
            owner: self.conditions.create_owner(CleanupOwnerValue::Component {
                statement,
                symbol,
                origin,
            }),
            condition,
            origin,
        }
    }

    pub(super) fn bind_result_owners(
        &self,
        origin: Span,
        state: &mut ValueState,
    ) -> Vec<OwnerVersion> {
        let mut versions = std::mem::take(&mut state.result_owners);
        // Drop 的诊断位置仍指接收 binding；owner 定义保留原参数/求值身份。
        for version in &mut versions {
            version.origin = origin;
        }
        versions
    }

    pub(super) fn restrict_versions(
        &mut self,
        versions: &mut Vec<OwnerVersion>,
        condition: CleanupConditionId,
    ) {
        for version in versions.iter_mut() {
            version.condition = self.conditions.and(version.condition, condition);
        }
        versions.retain(|version| version.condition != CleanupConditionId::NEVER);
    }

    pub(super) fn merge_versions(
        &mut self,
        into: &mut Vec<OwnerVersion>,
        versions: Vec<OwnerVersion>,
    ) {
        for version in versions {
            if let Some(prior) = into.iter_mut().find(|prior| prior.owner == version.owner) {
                prior.condition = self.conditions.or(prior.condition, version.condition);
            } else {
                into.push(version);
            }
        }
        into.sort_by_key(|version| version.owner);
    }
}
