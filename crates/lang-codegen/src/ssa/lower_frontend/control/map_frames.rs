//! CFG queries retained by the nested Map transport observer.

pub(in crate::ssa) mod carrier {
    use crate::ssa::model::{BlockId, Function, TerminatorKind};

    pub(in crate::ssa) fn terminator(
        function: &Function,
        block: BlockId,
    ) -> Option<&TerminatorKind> {
        function
            .block(block)?
            .terminator
            .as_ref()
            .map(|terminator| &terminator.kind)
    }
}
