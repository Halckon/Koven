use std::collections::BTreeMap;

use super::BuiltinType;

pub(crate) trait CanonicalTypeId: Copy {
    fn from_index(index: usize) -> Self;
    fn index(self) -> usize;
}

pub(crate) trait CanonicalTypeKind: Clone + Ord {
    type Id: CanonicalTypeId;

    fn initial_kinds() -> Vec<Self>;
    fn builtin(builtin: BuiltinType) -> Self;
}

/// 公共 local/unit 类型表共同使用的插入有序结构去重核心。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CanonicalTypeTable<K>
where
    K: CanonicalTypeKind,
{
    kinds: Vec<K>,
    ids: BTreeMap<K, K::Id>,
}

impl<K> CanonicalTypeTable<K>
where
    K: CanonicalTypeKind,
{
    pub(crate) fn new() -> Self {
        let mut table = Self {
            kinds: Vec::new(),
            ids: BTreeMap::new(),
        };
        for kind in K::initial_kinds() {
            table.intern(kind);
        }
        table
    }

    pub(crate) fn intern(&mut self, kind: K) -> K::Id {
        if let Some(id) = self.ids.get(&kind).copied() {
            return id;
        }
        let id = K::Id::from_index(self.kinds.len());
        self.kinds.push(kind.clone());
        self.ids.insert(kind, id);
        id
    }

    pub(crate) fn get(&self, id: K::Id) -> Option<&K> {
        self.kinds.get(id.index())
    }

    pub(crate) fn builtin(&self, builtin: BuiltinType) -> Option<K::Id> {
        self.ids.get(&K::builtin(builtin)).copied()
    }

    pub(crate) fn len(&self) -> usize {
        self.kinds.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.kinds.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::super::{
        BuiltinType, IntegerConstraint, TypeId, TypeKind, TypeTable,
        compilation_unit::{UnitTypeId, UnitTypeKind, UnitTypeTable},
    };

    #[test]
    fn local_and_unit_seed_kinds_have_the_same_exact_tail_order() {
        let local = TypeTable::new();
        let unit = UnitTypeTable::new();
        let signed = BuiltinType::ALL.len();
        let unsigned = signed + 1;
        let error = unsigned + 1;

        assert_eq!(
            local.get(TypeId::new(signed)),
            Some(&TypeKind::IntegerLiteral(IntegerConstraint::Signed))
        );
        assert_eq!(
            unit.get(UnitTypeId::new(signed)),
            Some(&UnitTypeKind::IntegerLiteral(IntegerConstraint::Signed))
        );
        assert_eq!(
            local.get(TypeId::new(unsigned)),
            Some(&TypeKind::IntegerLiteral(IntegerConstraint::Unsigned))
        );
        assert_eq!(
            unit.get(UnitTypeId::new(unsigned)),
            Some(&UnitTypeKind::IntegerLiteral(IntegerConstraint::Unsigned))
        );
        assert_eq!(local.get(TypeId::new(error)), Some(&TypeKind::Error));
        assert_eq!(unit.get(UnitTypeId::new(error)), Some(&UnitTypeKind::Error));
        assert_eq!(local.len(), error + 1);
        assert_eq!(unit.len(), error + 1);
    }
}
