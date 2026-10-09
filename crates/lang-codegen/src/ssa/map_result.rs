//! Map 查询/owned remove 的独立结果 identity；显式存在位，不扩展 NullableHandle。
use super::model::{ModelError, Module, Ownership, SsaTypeId, SsaTypeKind};

impl Module {
    pub(crate) fn add_map_result_type(
        &mut self,
        value: SsaTypeId,
    ) -> Result<SsaTypeId, ModelError> {
        self.check_type_id(value)?;
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

    pub(super) fn valid_map_result_type(&self, result: SsaTypeId, value: SsaTypeId) -> bool {
        let Some([present, payload]) = self.aggregate_fields(result) else {
            return false;
        };
        matches!(self.type_kind(*present), Some(SsaTypeKind::Boolean))
            && *payload == value
            && self.type_ownership(value).is_some()
            && self.type_ownership(result) == self.type_ownership(value)
    }
}
