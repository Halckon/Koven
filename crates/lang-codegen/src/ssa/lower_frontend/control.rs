//! `if` 与短路 Boolean 表达式的 CFG 和 block-parameter lowering。

mod carried;
pub(in crate::ssa) mod map_frames;

use lang_frontend::name_resolution::SymbolId;

use crate::ssa::model::{EntityId, EntityType, ValueId};

#[derive(Clone)]
pub(in crate::ssa) struct LinearBindings {
    pub(in crate::ssa) slots: Vec<LinearBindingSlot>,
}

#[derive(Clone)]
pub(in crate::ssa) struct LinearBindingSlot {
    pub(in crate::ssa) symbol: Option<SymbolId>,
    pub(in crate::ssa) source: EntityId,
    pub(in crate::ssa) temporaries: Vec<usize>,
    pub(in crate::ssa) captures: Vec<(usize, usize)>,
    pub(in crate::ssa) result_sources: Vec<(SymbolId, usize)>,
}

impl LinearBindingSlot {
    pub(in crate::ssa) fn new(symbol: Option<SymbolId>, source: EntityId, _ty: EntityType) -> Self {
        Self {
            symbol,
            source,
            temporaries: Vec::new(),
            captures: Vec::new(),
            result_sources: Vec::new(),
        }
    }
}
