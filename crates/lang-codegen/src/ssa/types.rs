//! Target-independent named aggregate and heap-owner type construction.

use super::model::{
    ModelError, Module, Ownership, SequentialContainerKind, SsaTypeId, SsaTypeKind,
};

impl Module {
    /// 返回本 module 唯一的 UTF-8 String owner identity。
    pub(crate) fn add_string_owner_type(&mut self) -> SsaTypeId {
        self.intern_type(SsaTypeKind::StringOwner)
    }

    pub(crate) fn add_aggregate_type(
        &mut self,
        name: impl Into<String>,
        fields: Vec<SsaTypeId>,
    ) -> Result<SsaTypeId, ModelError> {
        let name = name.into();
        self.check_new_type_name(&name)?;
        let ownership = self.aggregate_ownership(&fields)?;
        Ok(self.push_named_type(
            name.clone(),
            SsaTypeKind::Aggregate {
                name,
                fields,
                ownership,
            },
        ))
    }

    pub(crate) fn add_tagged_union_type(
        &mut self,
        name: impl Into<String>,
        variants: Vec<SsaTypeId>,
    ) -> Result<SsaTypeId, ModelError> {
        let name = name.into();
        self.check_new_type_name(&name)?;
        if variants.iter().any(|variant| {
            !matches!(
                self.type_kind(*variant),
                Some(SsaTypeKind::Aggregate { .. })
            )
        }) {
            return Err(ModelError::ExpectedAggregate {
                ty: variants.first().copied().unwrap_or(SsaTypeId {
                    module: self.id,
                    index: self.types.len(),
                }),
            });
        }
        let ownership = self.aggregate_ownership(&variants)?;
        Ok(self.push_named_type(
            name.clone(),
            SsaTypeKind::TaggedUnion {
                name,
                variants,
                ownership,
            },
        ))
    }

    pub(crate) fn declare_heap_owner(
        &mut self,
        name: impl Into<String>,
    ) -> Result<SsaTypeId, ModelError> {
        let name = name.into();
        self.check_new_type_name(&name)?;
        Ok(self.push_named_type(
            name.clone(),
            SsaTypeKind::HeapOwner {
                name,
                payload: None,
            },
        ))
    }

    pub(crate) fn define_heap_owner(
        &mut self,
        id: SsaTypeId,
        payload: SsaTypeId,
    ) -> Result<(), ModelError> {
        self.check_type_id(id)?;
        self.check_type_id(payload)?;
        if !matches!(
            self.type_kind(payload),
            Some(SsaTypeKind::Aggregate { .. } | SsaTypeKind::TaggedUnion { .. })
        ) {
            return Err(ModelError::ExpectedAggregate { ty: payload });
        }
        let kind = self
            .types
            .get_mut(id.index())
            .ok_or(ModelError::UnknownType { ty: id })?;
        let SsaTypeKind::HeapOwner {
            payload: definition,
            ..
        } = kind
        else {
            return Err(ModelError::ExpectedHeapOwner { ty: id });
        };
        if definition.is_some() {
            return Err(ModelError::TypeAlreadyDefined { ty: id });
        }
        *definition = Some(payload);
        Ok(())
    }

    pub(crate) fn declare_shared_owner(
        &mut self,
        name: impl Into<String>,
    ) -> Result<SsaTypeId, ModelError> {
        let name = name.into();
        self.check_new_type_name(&name)?;
        Ok(self.push_named_type(
            name.clone(),
            SsaTypeKind::SharedOwner {
                name,
                payload: None,
            },
        ))
    }

    pub(crate) fn define_shared_owner(
        &mut self,
        id: SsaTypeId,
        payload: SsaTypeId,
    ) -> Result<(), ModelError> {
        self.check_type_id(id)?;
        self.check_type_id(payload)?;
        if !self.type_is_defined(payload) {
            return Err(ModelError::UnknownType { ty: payload });
        }
        let kind = self
            .types
            .get_mut(id.index())
            .ok_or(ModelError::UnknownType { ty: id })?;
        let SsaTypeKind::SharedOwner {
            payload: definition,
            ..
        } = kind
        else {
            return Err(ModelError::ExpectedSharedOwner { ty: id });
        };
        if definition.is_some() {
            return Err(ModelError::TypeAlreadyDefined { ty: id });
        }
        *definition = Some(payload);
        Ok(())
    }

    pub(crate) fn add_sequential_container_type(
        &mut self,
        kind: SequentialContainerKind,
        element: SsaTypeId,
    ) -> Result<SsaTypeId, ModelError> {
        self.check_type_id(element)?;
        Ok(self.intern_type(SsaTypeKind::SequentialContainer { kind, element }))
    }

    /// 建立只接受已定义 pointer-like owner 的 nullable handle identity。
    pub(crate) fn add_nullable_handle_type(
        &mut self,
        inner: SsaTypeId,
    ) -> Result<SsaTypeId, ModelError> {
        self.check_type_id(inner)?;
        if !self.type_is_defined(inner)
            || !matches!(
                self.type_kind(inner),
                Some(SsaTypeKind::HeapOwner { .. } | SsaTypeKind::SharedOwner { .. })
            )
        {
            return Err(ModelError::ExpectedPointerLikeOwner { ty: inner });
        }
        Ok(self.intern_type(SsaTypeKind::NullableHandle { inner }))
    }

    pub(crate) fn type_ownership(&self, id: SsaTypeId) -> Option<Ownership> {
        match self.type_kind(id)? {
            SsaTypeKind::Unit
            | SsaTypeKind::Boolean
            | SsaTypeKind::Char
            | SsaTypeKind::Integer { .. } => Some(Ownership::Copyable),
            SsaTypeKind::SharedReference { .. } => Some(Ownership::Copyable),
            SsaTypeKind::Opaque { ownership, .. }
            | SsaTypeKind::ZeroSized { ownership, .. }
            | SsaTypeKind::Aggregate { ownership, .. }
            | SsaTypeKind::TaggedUnion { ownership, .. } => Some(*ownership),
            SsaTypeKind::HeapOwner { .. }
            | SsaTypeKind::SharedOwner { .. }
            | SsaTypeKind::StringOwner
            | SsaTypeKind::NullableHandle { .. } => Some(Ownership::MoveOnly),
            SsaTypeKind::SequentialContainer { .. } => Some(Ownership::MoveOnly),
            SsaTypeKind::FunctionPointer { .. } | SsaTypeKind::ConcreteClosure { .. } => {
                Some(Ownership::MoveOnly)
            }
        }
    }

    pub(crate) fn type_is_defined(&self, id: SsaTypeId) -> bool {
        match self.type_kind(id) {
            Some(
                SsaTypeKind::HeapOwner { payload, .. } | SsaTypeKind::SharedOwner { payload, .. },
            ) => payload.is_some(),
            Some(_) => true,
            None => false,
        }
    }

    pub(crate) fn type_kind(&self, id: SsaTypeId) -> Option<&SsaTypeKind> {
        (id.module() == self.id)
            .then(|| self.types.get(id.index()))
            .flatten()
    }

    pub(crate) fn aggregate_fields(&self, id: SsaTypeId) -> Option<&[SsaTypeId]> {
        match self.type_kind(id)? {
            SsaTypeKind::Aggregate { fields, .. } => Some(fields),
            _ => None,
        }
    }

    pub(crate) fn heap_payload(&self, id: SsaTypeId) -> Option<SsaTypeId> {
        match self.type_kind(id)? {
            SsaTypeKind::HeapOwner { payload, .. } => *payload,
            _ => None,
        }
    }

    pub(crate) fn shared_payload(&self, id: SsaTypeId) -> Option<SsaTypeId> {
        match self.type_kind(id)? {
            SsaTypeKind::SharedOwner { payload, .. } => *payload,
            _ => None,
        }
    }

    pub(crate) fn nullable_inner(&self, id: SsaTypeId) -> Option<SsaTypeId> {
        match self.type_kind(id)? {
            SsaTypeKind::NullableHandle { inner } => Some(*inner),
            _ => None,
        }
    }

    pub(crate) fn tagged_variants(&self, id: SsaTypeId) -> Option<&[SsaTypeId]> {
        match self.type_kind(id)? {
            SsaTypeKind::TaggedUnion { variants, .. } => Some(variants),
            _ => None,
        }
    }

    pub(crate) fn sequential_container(
        &self,
        id: SsaTypeId,
    ) -> Option<(SequentialContainerKind, SsaTypeId)> {
        match self.type_kind(id)? {
            SsaTypeKind::SequentialContainer { kind, element } => Some((*kind, *element)),
            _ => None,
        }
    }

    fn aggregate_ownership(&self, fields: &[SsaTypeId]) -> Result<Ownership, ModelError> {
        let mut ownership = Ownership::Copyable;
        for field in fields {
            self.check_type_id(*field)?;
            if self.type_ownership(*field) == Some(Ownership::MoveOnly) {
                ownership = Ownership::MoveOnly;
            }
        }
        Ok(ownership)
    }

    pub(super) fn check_new_type_name(&self, name: &str) -> Result<(), ModelError> {
        if name.is_empty() {
            return Err(ModelError::EmptyTypeName);
        }
        if self.named_type_ids.contains_key(name) {
            return Err(ModelError::DuplicateTypeName {
                name: name.to_owned(),
            });
        }
        Ok(())
    }

    pub(super) fn push_named_type(&mut self, name: String, kind: SsaTypeKind) -> SsaTypeId {
        let id = SsaTypeId {
            module: self.id,
            index: self.types.len(),
        };
        self.types.push(kind);
        self.named_type_ids.insert(name, id);
        id
    }

    pub(super) fn check_type_id(&self, ty: SsaTypeId) -> Result<(), ModelError> {
        if ty.module() != self.id {
            return Err(ModelError::WrongTypeOwner {
                expected: self.id,
                actual: ty.module(),
            });
        }
        self.type_kind(ty)
            .map(|_| ())
            .ok_or(ModelError::UnknownType { ty })
    }
}
