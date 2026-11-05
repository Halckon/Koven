//! Source-qualified named place identity and logical path overlap.

use crate::name_resolution::UnitSymbolId;

use super::super::ElementIndexIdentity;

/// compilation-unit 中由根 binding、字段路径及完整逻辑索引路径组成的稳定 place。
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct UnitOwnershipPlace {
    root: UnitSymbolId,
    fields: Vec<UnitSymbolId>,
    elements: Vec<ElementIndexIdentity>,
}

impl UnitOwnershipPlace {
    pub(in crate::ownership_checking) fn new(
        root: UnitSymbolId,
        fields: Vec<UnitSymbolId>,
    ) -> Self {
        Self {
            root,
            fields,
            elements: Vec::new(),
        }
    }

    pub(in crate::ownership_checking) fn push_field(&mut self, field: UnitSymbolId) -> bool {
        if !self.elements.is_empty() {
            return false;
        }
        self.fields.push(field);
        true
    }

    pub(in crate::ownership_checking) fn push_element(&mut self, element: ElementIndexIdentity) {
        self.elements.push(element);
    }

    /// 返回拥有该 place 的 source-qualified 根 binding。
    #[must_use]
    pub const fn root(&self) -> UnitSymbolId {
        self.root
    }

    /// 返回从根到目标的字段路径。
    #[must_use]
    pub fn fields(&self) -> &[UnitSymbolId] {
        &self.fields
    }

    /// 返回 terminal 顺序容器逻辑索引；非 element place 返回 `None`。
    #[must_use]
    pub const fn element(&self) -> Option<ElementIndexIdentity> {
        match self.elements.as_slice().last() {
            Some(element) => Some(*element),
            None => None,
        }
    }

    /// 返回字段路径之后从外到内的完整逻辑索引路径。
    #[must_use]
    pub fn elements(&self) -> &[ElementIndexIdentity] {
        &self.elements
    }

    /// 返回该 place 是否精确表示根 binding 自身。
    #[must_use]
    pub const fn is_root(&self) -> bool {
        self.fields.is_empty() && self.elements.is_empty()
    }

    pub(in crate::ownership_checking) fn overlaps(&self, other: &Self) -> bool {
        if self.root != other.root
            || !self
                .fields
                .iter()
                .zip(&other.fields)
                .all(|(left, right)| left == right)
        {
            return false;
        }
        if self.fields.len() != other.fields.len() {
            return true;
        }
        self.elements
            .iter()
            .zip(&other.elements)
            .all(|(&left, &right)| left.may_alias(right))
    }
}
