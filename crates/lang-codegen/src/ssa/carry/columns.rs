//! Layout-local columns preserve aliases only when every incoming entity agrees.
use crate::ssa::{
    lowering_support::LoweringError,
    model::{EntityId, EntityType},
};

#[derive(Clone, Copy)]
pub(in crate::ssa) struct ColumnId(usize);

impl ColumnId {
    pub(super) fn from_index(index: usize) -> Self {
        Self(index)
    }

    pub(in crate::ssa) fn index(self) -> usize {
        self.0
    }
}

#[derive(Clone, Copy)]
pub(in crate::ssa) enum ColumnMatch {
    Added(ColumnId),
    Reused(ColumnId),
}

impl ColumnMatch {
    pub(in crate::ssa) fn column(self) -> ColumnId {
        match self {
            Self::Added(column) | Self::Reused(column) => column,
        }
    }
}

/// Read one already-selected role from one exit; no column or type searches.
pub(in crate::ssa) trait IncomingRole<E> {
    fn current(&self, exit: &E) -> Result<EntityId, LoweringError>;
}

pub(in crate::ssa) struct IncomingColumns<'e, E> {
    exits: &'e [E],
    columns: Vec<Vec<EntityId>>,
    role_columns: Vec<ColumnId>,
}

impl<'e, E> IncomingColumns<'e, E> {
    pub(in crate::ssa) fn new(exits: &'e [E]) -> Self {
        Self {
            exits,
            columns: Vec::new(),
            role_columns: Vec::new(),
        }
    }

    fn collect(&self, role: &impl IncomingRole<E>) -> Result<Vec<EntityId>, LoweringError> {
        self.exits.iter().map(|exit| role.current(exit)).collect()
    }

    fn append(&mut self, column: Vec<EntityId>) -> ColumnId {
        let id = ColumnId(self.columns.len());
        self.columns.push(column);
        id
    }

    /// Seeds remain separate even when all incoming entities are identical.
    pub(in crate::ssa) fn seed(
        &mut self,
        role: &impl IncomingRole<E>,
    ) -> Result<ColumnId, LoweringError> {
        let column = self.collect(role)?;
        let id = self.append(column);
        self.role_columns.push(id);
        Ok(id)
    }

    pub(in crate::ssa) fn associate(
        &mut self,
        role: &impl IncomingRole<E>,
    ) -> Result<ColumnMatch, LoweringError> {
        let column = self.collect(role)?;
        let choice = if let Some(index) = self.columns.iter().position(|other| *other == column) {
            ColumnMatch::Reused(ColumnId(index))
        } else {
            ColumnMatch::Added(self.append(column))
        };
        self.role_columns.push(choice.column());
        Ok(choice)
    }

    pub(in crate::ssa) fn columns(&self) -> &[Vec<EntityId>] {
        &self.columns
    }

    pub(in crate::ssa) fn role_columns(&self) -> &[ColumnId] {
        &self.role_columns
    }

    /// Exit count is independent of column count, including an empty-parameter merge.
    pub(in crate::ssa) fn arguments(&self) -> impl Iterator<Item = Vec<EntityId>> + '_ {
        (0..self.exits.len())
            .map(move |index| self.columns.iter().map(|column| column[index]).collect())
    }
}

/// Called immediately after each association at the existing single-entry type gate.
pub(in crate::ssa) fn record_type(
    types: &mut Vec<EntityType>,
    choice: ColumnMatch,
    ty: EntityType,
    mismatch: LoweringError,
) -> Result<(), LoweringError> {
    match choice {
        ColumnMatch::Added(_) => types.push(ty),
        ColumnMatch::Reused(column) if types[column.index()] != ty => return Err(mismatch),
        ColumnMatch::Reused(_) => {}
    }
    Ok(())
}
