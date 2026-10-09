//! Exact descriptor provenance follows paired root loans, never metadata storage aliases.
use super::*;

pub(in crate::ssa::verify_ownership) fn is_view(
    module: &Module,
    function: &Function,
    loan: LoanId,
) -> bool {
    function.entity(EntityId::Loan(loan)).is_some_and(|data| {
        matches!(
            module.type_kind(data.ty.semantic_type()),
            Some(super::super::super::model::SsaTypeKind::RangeView { .. })
        )
    })
}

pub(in crate::ssa::verify_ownership) fn root(function: &Function, loan: LoanId) -> Option<LoanId> {
    let pairs = pairs::compute(function);
    let flows = LoanFlowAliases::compute(function);
    resolve(function, loan, &pairs, &flows, &mut BTreeSet::new(), true)
}

/// A child's capability remains live even after the original caller loan and parent metadata end.
pub(in crate::ssa::verify_ownership) fn capability(
    function: &Function,
    loan: LoanId,
) -> Option<LoanId> {
    let pairs = pairs::compute(function);
    let flows = LoanFlowAliases::compute(function);
    resolve(function, loan, &pairs, &flows, &mut BTreeSet::new(), false)
}

fn resolve(
    function: &Function,
    loan: LoanId,
    pairs: &BTreeSet<(ValueId, LoanId)>,
    flows: &LoanFlowAliases,
    visiting: &mut BTreeSet<LoanId>,
    flatten: bool,
) -> Option<LoanId> {
    if !visiting.insert(loan) {
        return None;
    }
    let result = (|| match function.entity(EntityId::Loan(loan))?.definition {
        Definition::BlockParameter { block, .. } if block.index() == 0 => Some(loan),
        Definition::BlockParameter { .. } => {
            let mut roots = BTreeSet::new();
            for origin in flows.origins.get(&loan)? {
                roots.insert(resolve(function, *origin, pairs, flows, visiting, flatten)?);
            }
            (roots.len() == 1).then(|| *roots.first().expect("one root"))
        }
        Definition::InstructionResult { instruction, .. } => {
            match function.instruction(instruction)?.operation {
                Operation::SharedReborrow { source } | Operation::BorrowCall { source, .. } => {
                    resolve(function, source, pairs, flows, visiting, flatten)
                }
                Operation::RangeConstruct { source, .. } | Operation::RangeCall { source, .. } => {
                    if flatten {
                        resolve(function, source, pairs, flows, visiting, flatten)
                    } else {
                        Some(loan)
                    }
                }
                Operation::BorrowBegin { place, .. }
                    if pairs.iter().any(|(view, _)| {
                        function
                            .entity(EntityId::Value(*view))
                            .map(|data| data.ty.semantic_type())
                            == function
                                .entity(EntityId::Loan(loan))
                                .map(|data| data.ty.semantic_type())
                    }) =>
                {
                    let Definition::InstructionResult { instruction, .. } =
                        function.entity(EntityId::Place(place))?.definition
                    else {
                        return None;
                    };
                    let Operation::RootPlace { owner } =
                        function.instruction(instruction)?.operation
                    else {
                        return None;
                    };
                    let mut roots = BTreeSet::new();
                    for (_, source) in pairs.iter().filter(|(view, _)| *view == owner) {
                        roots.insert(if flatten {
                            resolve(function, *source, pairs, flows, visiting, flatten)?
                        } else {
                            *source
                        });
                    }
                    (roots.len() == 1).then(|| *roots.first().expect("one root"))
                }
                Operation::BorrowBegin { .. } => Some(loan),
                _ => None,
            }
        }
    })();
    visiting.remove(&loan);
    result
}
