//! One real planner traversal and its complete publication, including opt-in observations.
use super::*;

pub(super) fn plan_with_observations(
    checker: &Checker<'_>,
    enabled: bool,
    loan_actions_enabled: bool,
) -> Result<Analysis, OwnershipCheckingError> {
    let liveness = liveness::build(checker, true)?;
    let mut deferred: Vec<_> = liveness
        .deferred
        .iter()
        .copied()
        .map(|expression| {
            UnitOwnershipDeferredFact::new(expression, OwnershipDeferredReason::IndexPlace)
        })
        .collect();
    let mut planner = DropPlanner::new(checker, liveness);
    #[cfg(feature = "d10-catalog-experiment")]
    planner.enable_producer_catalog();
    #[cfg(feature = "d10-catalog-experiment")]
    planner.enable_loop_generation_probe();
    #[cfg(feature = "d10-catalog-experiment")]
    planner.enable_named_wrapper_probe();
    #[cfg(feature = "d10-catalog-experiment")]
    planner.enable_split_wrapper_probe();
    planner.owner_observation_enabled = enabled;
    planner.iteration_actions.enabled = loan_actions_enabled;
    let planner = planner.run()?;
    let iterations = planner.iteration_plans()?;
    let map_iterations = planner.map_iteration_plans()?;
    let mut iteration_owner_scopes: Vec<_> = planner
        .iteration_scope_depths
        .iter()
        .flat_map(|(statement, depth)| {
            planner
                .binding_depths
                .iter()
                .map(move |(symbol, binding_depth)| {
                    (
                        *statement,
                        crate::ownership_checking::UnitDropTarget::Named(*symbol),
                        binding_depth > depth,
                    )
                })
        })
        .collect();
    iteration_owner_scopes.extend(planner.iteration_temporary_scopes.iter().copied());
    let iteration_required_exits = planner
        .iteration_exits
        .iter()
        .map(|(owner, kind, point)| (*owner, *kind, point.into_unit(checker.source_unit)))
        .collect();
    let iteration_loan_ends = planner
        .iteration_actions
        .iter()
        .filter(|(_, action)| {
            matches!(
                action,
                crate::ownership_checking::UnitIterationCleanupAction::EndCallLoan(_)
                    | crate::ownership_checking::UnitIterationCleanupAction::EndReceiverLoan(_)
                    | crate::ownership_checking::UnitIterationCleanupAction::EndCaptureLoan { .. }
            )
        })
        .map(|(point, action)| (point.into_unit(checker.source_unit), action.clone()))
        .collect();
    deferred.extend(planner.resource_deferred.iter().copied());
    if !planner.resource_deferred.is_empty() {
        return Ok(Analysis {
            owner_observations: planner.owner_observations,
            deferred,
            ..Analysis::default()
        });
    }
    let direct_call_loan_ends = planner.direct_call_loan_ends();
    let borrow_call_prefix_ends = planner.borrow_call_prefix_ends();
    let exclusive_call_prefix_ends = planner.exclusive_call_prefix_ends();
    let receiver_call_prefix_ends = planner.receiver_call_prefix_ends();
    let receiver_call_return_ends = planner.iteration_actions.receiver_return_ends();
    let borrow_call_handoffs = planner.iteration_actions.borrow_handoffs();
    let borrow_ends = planner.iteration_actions.borrow_binding_ends();
    let borrow_call_transports = planner.iteration_actions.consumer_transports();
    let borrow_forward_transports = planner.iteration_actions.forward_transports();
    #[cfg(feature = "d10-catalog-experiment")]
    let loop_catalog = planner.loop_pending();
    #[cfg(feature = "d10-catalog-experiment")]
    let nested_catalog = planner.nested_pending();
    #[cfg(feature = "d10-catalog-experiment")]
    let dual_nested_catalog = planner.dual_nested_pending();
    #[cfg(feature = "d10-catalog-experiment")]
    let after_child_catalog = planner.after_child_pending();
    #[cfg(feature = "d10-catalog-experiment")]
    let wrapped_dual_catalog = planner.wrapped_dual_pending();
    #[cfg(feature = "d10-catalog-experiment")]
    let split_source_catalog = planner.split_source_pending();
    #[cfg(feature = "d10-catalog-experiment")]
    let wrapped_after_child_catalog = planner.wrapped_after_child_pending();
    let drops = planner
        .facts
        .into_iter()
        .map(|fact| fact.into_unit(checker.source_unit))
        .collect();
    let conditional_receiver_drops = planner
        .conditional_receiver_facts
        .into_iter()
        .map(|fact| fact.into_unit(checker.source_unit))
        .collect();
    Ok(Analysis {
        #[cfg(feature = "d10-catalog-experiment")]
        drop_traversals: planner.drop_traversals,
        #[cfg(feature = "d10-catalog-experiment")]
        split_source_catalog,
        #[cfg(feature = "d10-catalog-experiment")]
        loop_catalog,
        #[cfg(feature = "d10-catalog-experiment")]
        nested_catalog,
        #[cfg(feature = "d10-catalog-experiment")]
        dual_nested_catalog,
        #[cfg(feature = "d10-catalog-experiment")]
        after_child_catalog,
        #[cfg(feature = "d10-catalog-experiment")]
        wrapped_dual_catalog,
        #[cfg(feature = "d10-catalog-experiment")]
        wrapped_after_child_catalog,
        #[cfg(feature = "d10-catalog-experiment")]
        catalog: planner.producer_catalog.and_then(|c| c.into_pending()),
        owner_observations: planner.owner_observations,
        loan_action_observations: planner
            .iteration_actions
            .finish(checker.source_unit, checker.parsed.source_id()),
        direct_call_loan_ends,
        string_clone_loan_ends: planner.string_clone_loan_ends,
        borrow_call_prefix_ends,
        borrow_call_handoffs,
        borrow_call_transports,
        borrow_binding_transports: planner.borrow_binding_transports,
        borrow_forward_transports,
        exclusive_call_prefix_ends,
        receiver_call_prefix_ends,
        receiver_call_return_ends,
        no_carry: planner.no_carry_exit.finish(borrow_ends),
        iteration_conditional_scopes: planner.iteration_conditional_scopes,
        iteration_owner_scopes,
        iteration_loan_ends,
        iterations,
        map_iterations,
        iteration_required_exits,
        drops,
        conditional_receiver_drops,
        deferred,
    })
}
