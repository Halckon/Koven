//! Registered numeric Range identity, independent of borrowed RangeView carriers.
use super::model::{ModelError, Module, Ownership, SsaTypeId, SsaTypeKind};

impl Module {
    pub(crate) fn add_integer_range_type(
        &mut self,
        endpoint: SsaTypeId,
    ) -> Result<SsaTypeId, ModelError> {
        self.check_type_id(endpoint)?;
        if !matches!(
            self.type_kind(endpoint),
            Some(SsaTypeKind::Integer {
                bits: 32 | 64,
                signed: true
            })
        ) {
            return Err(ModelError::ExpectedAggregate { ty: endpoint });
        }
        if let Some((&range, _)) = self.integer_ranges.iter().find(|(_, ty)| **ty == endpoint) {
            return Ok(range);
        }
        let empty = self.intern_type(SsaTypeKind::Boolean);
        let range = self.add_aggregate_type(
            format!("$integer.range.t{}", endpoint.index()),
            vec![endpoint, endpoint, empty],
        )?;
        self.integer_ranges.insert(range, endpoint);
        Ok(range)
    }

    pub(crate) fn integer_range_endpoint(&self, range: SsaTypeId) -> Option<SsaTypeId> {
        self.integer_ranges.get(&range).copied()
    }

    pub(super) fn valid_integer_range_type(&self, range: SsaTypeId, endpoint: SsaTypeId) -> bool {
        let Some([first, last, empty]) = self.aggregate_fields(range) else {
            return false;
        };
        matches!(
            self.type_kind(endpoint),
            Some(SsaTypeKind::Integer {
                bits: 32 | 64,
                signed: true
            })
        ) && *first == endpoint
            && *last == endpoint
            && matches!(self.type_kind(*empty), Some(SsaTypeKind::Boolean))
            && self.type_ownership(range) == Some(Ownership::Copyable)
    }
}
