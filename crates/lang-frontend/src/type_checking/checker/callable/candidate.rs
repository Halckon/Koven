//! 已选择调用候选的参数/结果合同。
use super::*;

#[derive(Clone)]
pub(super) struct CallCandidate {
    pub(super) target: CallableTarget,
    pub(super) declaration_span: Option<Span>,
    pub(super) type_parameters: Vec<SymbolId>,
    pub(super) instance_arguments: Vec<TypeId>,
    pub(super) receiver: Option<CallReceiverDescriptor>,
    pub(super) parameters: Vec<MappedParameter<TypeId>>,
    pub(super) return_type: TypeId,
    pub(super) borrow_return: Option<crate::type_checking::BorrowReturnContract>,
    pub(super) cross_thread_parameters: BTreeSet<usize>,
    pub(super) aborts: bool,
    pub(super) prints_line: bool,
}
