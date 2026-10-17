//! Map 查询结果与 Map-only nullable storage；物理 presence 不授予查询操作资格。
use super::model::{ModelError, Module, Ownership, SsaTypeId, SsaTypeKind};

impl Module {
    pub(crate) fn add_map_result_type(
        &mut self,
        value: SsaTypeId,
    ) -> Result<SsaTypeId, ModelError> {
        self.check_type_id(value)?;
        if matches!(
            self.type_kind(value),
            Some(SsaTypeKind::MapContainer { .. })
        ) {
            return self.add_nullable_map_type(value);
        }
        if !matches!(
            self.type_ownership(value),
            Some(Ownership::Copyable | Ownership::MoveOnly)
        ) {
            return Err(ModelError::ExpectedAggregate { ty: value });
        }
        if let Some((&result, _)) = self.map_results.iter().find(|(_, inner)| **inner == value) {
            return Ok(result);
        }
        let present = self.intern_type(SsaTypeKind::Boolean);
        let result = self.add_aggregate_type(
            format!("$map.result.t{}", value.index()),
            vec![present, value],
        )?;
        self.map_results.insert(result, value);
        Ok(result)
    }

    pub(crate) fn map_result_value(&self, result: SsaTypeId) -> Option<SsaTypeId> {
        self.map_results.get(&result).copied()
    }

    pub(crate) fn add_nullable_map_type(
        &mut self,
        inner: SsaTypeId,
    ) -> Result<SsaTypeId, ModelError> {
        self.check_type_id(inner)?;
        if !self.type_is_defined(inner)
            || !matches!(
                self.type_kind(inner),
                Some(SsaTypeKind::MapContainer { .. })
            )
            || self.type_ownership(inner) != Some(Ownership::MoveOnly)
        {
            return Err(ModelError::ExpectedMapContainer { ty: inner });
        }
        if let Some((&nullable, _)) = self
            .nullable_maps
            .iter()
            .find(|(_, value)| **value == inner)
        {
            return Ok(nullable);
        }
        let present = self.intern_type(SsaTypeKind::Boolean);
        let nullable = self.add_aggregate_type(
            format!("$nullable.map.t{}", inner.index()),
            vec![present, inner],
        )?;
        self.nullable_maps.insert(nullable, inner);
        Ok(nullable)
    }

    pub(crate) fn nullable_map_inner(&self, nullable: SsaTypeId) -> Option<SsaTypeId> {
        self.nullable_maps.get(&nullable).copied()
    }

    /// Only physical nullable layout/drop/encapsulation may use this union.
    /// MapGet, MapResultUnwrap and scalar comparisons retain their R-only contracts.
    pub(crate) fn presence_payload_type(&self, wrapper: SsaTypeId) -> Option<SsaTypeId> {
        self.nullable_map_inner(wrapper)
            .or_else(|| self.map_result_value(wrapper))
    }

    pub(super) fn valid_nullable_map_type(&self, nullable: SsaTypeId, inner: SsaTypeId) -> bool {
        let Some([present, payload]) = self.aggregate_fields(nullable) else {
            return false;
        };
        self.type_is_defined(nullable)
            && self.type_is_defined(inner)
            && matches!(self.type_kind(*present), Some(SsaTypeKind::Boolean))
            && *payload == inner
            && matches!(
                self.type_kind(inner),
                Some(SsaTypeKind::MapContainer { .. })
            )
            && self.type_ownership(inner) == Some(Ownership::MoveOnly)
            && self.type_ownership(nullable) == Some(Ownership::MoveOnly)
    }

    pub(super) fn valid_map_result_type(&self, result: SsaTypeId, value: SsaTypeId) -> bool {
        let Some([present, payload]) = self.aggregate_fields(result) else {
            return false;
        };
        matches!(self.type_kind(*present), Some(SsaTypeKind::Boolean))
            && *payload == value
            && !self.nullable_maps.contains_key(&result)
            && !matches!(
                self.type_kind(value),
                Some(SsaTypeKind::MapContainer { .. })
            )
            && self.type_ownership(value).is_some()
            && self.type_ownership(result) == self.type_ownership(value)
    }
}
