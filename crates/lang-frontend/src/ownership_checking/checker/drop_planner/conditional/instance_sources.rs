//! 整棵 owner 收尾后的实例源批次与具名 source 摘要。
use super::{
    CleanupConditionId, CleanupOwnerValueId, ClosureCaptureMode, ClosureOrigin, DropFact,
    DropPlanner, IterationCleanupAction, OwnerVersion, ReleaseTraversal, SharedSourceRelease,
    ValueState,
};

impl DropPlanner<'_, '_> {
    /// 紧邻环境槽的条件/shared 子闭包不能用一个普通 Drop 掩盖其动态 loan。
    pub(super) fn needs_enclosing_instance_release(
        inputs: &[crate::ownership_checking::CleanupCaptureInput],
        captured: &[ClosureOrigin],
    ) -> bool {
        inputs.iter().any(|input| {
            if input.mode() != ClosureCaptureMode::Owned
                || input.effect() != crate::ownership_checking::ClosureCaptureEffect::Move
                || !matches!(
                    input.value(),
                    crate::ownership_checking::CleanupCaptureValue::Environment { .. }
                )
            {
                return false;
            }
            let children = captured
                .iter()
                .filter(|child| child.captured_from == Some(input.source()))
                .collect::<Vec<_>>();
            !children.is_empty()
                && (children.len() != 1
                    || children[0]
                        .inputs
                        .iter()
                        .any(|input| input.mode() == ClosureCaptureMode::Shared))
        })
    }

    /// 具名及 pending temporary 环境先结束完整 owner，再交付各来源的清理义务。
    pub(in super::super) fn drop_closure_owner(
        &mut self,
        fact: DropFact,
        condition: CleanupConditionId,
        versions: &[OwnerVersion],
        origins: Vec<ClosureOrigin>,
        state: &mut ValueState,
    ) {
        let point = fact.point();
        let first_step = self.cleanup.len();
        // Sibling captured environments may borrow the same source; release it after all loans.
        let mut shared_sources = Vec::new();
        self.drop_closure_owner_inner(
            fact,
            condition,
            versions,
            origins,
            state,
            ReleaseTraversal {
                root: None,
                capture_path: &[],
                shared_sources: &mut shared_sources,
            },
        );
        // 普通父环境可能包含递归子根；先完成整棵外层 owner，再清理根持有的源。
        let roots = self.cleanup[first_step..]
            .iter()
            .filter_map(|(_, action)| match action {
                IterationCleanupAction::ReleaseClosureInstances { root, .. } => Some(*root),
                _ => None,
            })
            .collect::<Vec<_>>();
        for root in roots {
            self.cleanup.push((
                point,
                IterationCleanupAction::ReleaseRetainedClosureSources { root },
            ));
        }
        self.release_shared_sources(point, shared_sources, state);
    }

    pub(super) fn collect_instance_shared_sources(
        &mut self,
        condition: CleanupConditionId,
        origins: &[ClosureOrigin],
        instance_release_owners: &[CleanupOwnerValueId],
        traversal: &mut ReleaseTraversal<'_>,
    ) {
        // 根动作的动态批次负责 retained source；这里只追踪具名源及静态状态，
        // 不再枚举可能被递归链运输到任意深度的 source 地址。
        let mut released_origins = origins
            .iter()
            .filter(|origin| {
                instance_release_owners.contains(&origin.owner)
                    || instance_release_owners.contains(&origin.layout_owner)
            })
            .collect::<Vec<_>>();
        while let Some(origin) = released_origins.pop() {
            for held in &origin.held_sources {
                let selected = self.conditions.and(condition, held.condition);
                traversal.shared_sources.push(SharedSourceRelease {
                    owner: held.owner,
                    selected,
                    last_loan: None,
                    source_location: None,
                });
            }
            for input in &origin.inputs {
                if input.mode == ClosureCaptureMode::Shared
                    && let crate::ownership_checking::CleanupCaptureValue::Owner(owner) =
                        input.value
                {
                    let selected = self.conditions.and(condition, input.condition);
                    traversal.shared_sources.push(SharedSourceRelease {
                        owner,
                        selected,
                        last_loan: None,
                        source_location: None,
                    });
                }
            }
            released_origins.extend(&origin.captured);
        }
    }
}
