//! Complete relevant guard state, including pending field-child identities.
use super::super::super::pending_call::PendingFieldChild;
use super::*;

type PendingCallSnapshot = ((usize, usize), Option<LoanId>, Option<PendingFieldChild>);

#[derive(Debug, PartialEq, Eq)]
pub(in crate::ssa::lower_frontend) struct Snapshot {
    function: Function,
    block: BlockId,
    source: Option<SourceToken>,
    function_ids: BTreeMap<FunctionInstanceKey, FunctionId>,
    type_ids: BTreeMap<TypeId, SsaTypeId>,
    nullable_maps: BTreeMap<SsaTypeId, SsaTypeId>,
    bindings: BTreeMap<SymbolId, LoweredValue>,
    proofs: BTreeMap<SymbolId, NonNullProof>,
    borrow: BTreeMap<SymbolId, LoanId>,
    result_loans: BTreeMap<SymbolId, Vec<LoanId>>,
    captures: BTreeMap<(usize, usize), LoanId>,
    temporaries: BTreeMap<usize, ValueId>,
    pending: Vec<PendingCallSnapshot>,
}

impl Snapshot {
    pub(in crate::ssa::lower_frontend) fn take(lowerer: &ExpressionLowerer<'_>) -> Self {
        // These fixture-proven states cannot be written by the selected guards.
        assert!(lowerer.loops.is_empty());
        assert!(!lowerer.map_loop_owners.has_loop_carriers());
        assert!(lowerer.readonly_receiver.is_none());
        assert!(lowerer.thunk_expression.is_none());
        Self {
            function: lowerer.function.clone(),
            block: lowerer.block,
            source: lowerer.source_token,
            function_ids: lowerer.function_ids.clone(),
            type_ids: lowerer.type_ids.clone(),
            nullable_maps: lowerer.nullable_maps.clone(),
            bindings: lowerer.bindings.clone(),
            proofs: lowerer.non_null_bindings.clone(),
            borrow: lowerer.borrow_bindings.clone(),
            result_loans: lowerer.result_source_loans.clone(),
            captures: lowerer.capture_loans.clone(),
            temporaries: lowerer.temporaries.clone(),
            pending: lowerer
                .pending_call_loans
                .iter()
                .map(|(&key, &loan)| (key, loan, lowerer.pending_call_loans.child(&key)))
                .collect(),
        }
    }
}
