//! 返回仅交接指定入口参数的 shared loan 链，其他局部 loan 仍须正常清理。
use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn verify_return(
    function: &Function,
    loan: LoanId,
    aliases: &AliasRoots,
    dependencies: &ReborrowDependencies,
    state: &mut BlockState,
    location: VerifyLocation,
    origin: &super::super::model::Origin,
    errors: &mut Vec<VerifyError>,
) {
    let Some((parameter, _)) = super::super::verify_borrow_result::parameter(function) else {
        return;
    };
    let expected = BTreeSet::from([EntityId::Loan(parameter)]);
    if aliases.roots.get(&EntityId::Loan(loan)) != Some(&expected) {
        errors.push(error(
            VerifyErrorKind::ReturnType { index: 0 },
            location.clone(),
            origin,
        ));
        return;
    }
    if !state.loans.contains(&loan) {
        errors.push(error(
            VerifyErrorKind::LoanInactive { loan },
            location.clone(),
            origin,
        ));
        return;
    }
    if let Some(dependent) = dependencies.active_descendant(loan, aliases, state) {
        errors.push(error(
            VerifyErrorKind::LoanDependencyActive {
                parent: loan,
                dependent,
            },
            location.clone(),
            origin,
        ));
        return;
    }
    let mut current = loan;
    let mut visited = BTreeSet::new();
    while visited.insert(current) {
        state.loans.remove(&current);
        if current == parameter {
            return;
        }
        let Some(parent) = dependencies.parent_by_child.get(&current).copied() else {
            break;
        };
        current = parent;
    }
    errors.push(error(
        VerifyErrorKind::ReturnType { index: 0 },
        location,
        origin,
    ));
}
