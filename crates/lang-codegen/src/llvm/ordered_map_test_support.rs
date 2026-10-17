//! Test-only physical metadata from the real type map; no Map operations or oracle.

use inkwell::{context::Context, targets::TargetData};

use crate::ssa::model::{Module, SsaTypeId, SsaTypeKind};

#[derive(Debug)]
pub(crate) struct OrderedMapLayout {
    pub(crate) header_name: String,
    pub(crate) header_offsets: [u64; 6],
    pub(crate) header_size: u64,
    pub(crate) header_alignment: u32,
    pub(crate) slot_type: String,
    pub(crate) slot_offsets: [u64; 5],
    pub(crate) slot_stride: u64,
    pub(crate) slot_alignment: u32,
    pub(crate) string_offsets: Option<[u64; 2]>,
    pub(crate) pointer_bits: u32,
}

pub(crate) fn describe(
    module: &Module,
    map_type: SsaTypeId,
    data_layout: &str,
) -> OrderedMapLayout {
    // A separate context avoids creating renamed .0 types in the parsed raw product.
    let context = Context::create();
    let target = TargetData::create(data_layout);
    let types = super::type_map::TypeMap::lower(&context, module, &target)
        .expect("real SSA types must have a valid native layout");
    let (_, key, value) = module.map_container(map_type).expect("bound Map type");
    let size_type = context.ptr_sized_int_type(&target, None);
    assert_eq!(size_type.get_bit_width(), 64, "supported native hosts only");
    let header = types.basic_type(map_type).unwrap().into_struct_type();
    assert_eq!(header.count_fields(), 6);
    assert!(header.get_field_type_at_index(0).unwrap().is_pointer_type());
    for index in 1..6 {
        assert_eq!(
            header.get_field_type_at_index(index),
            Some(size_type.into())
        );
    }
    let slot = super::map::get_slot_type(&context, &types, size_type, key, value)
        .expect("real Map slot layout");
    assert_eq!(slot.count_fields(), 5);
    assert_eq!(
        slot.get_field_type_at_index(0),
        Some(context.i32_type().into())
    );
    assert_eq!(
        slot.get_field_type_at_index(1),
        Some(types.basic_type(key).unwrap())
    );
    assert_eq!(
        slot.get_field_type_at_index(2),
        Some(types.basic_type(value).unwrap())
    );
    for index in [3, 4] {
        assert_eq!(slot.get_field_type_at_index(index), Some(size_type.into()));
    }
    let string_offsets = if module.type_kind(key) == Some(&SsaTypeKind::StringOwner) {
        let string = types.basic_type(key).unwrap().into_struct_type();
        assert_eq!(string.count_fields(), 3);
        assert!(string.get_field_type_at_index(0).unwrap().is_pointer_type());
        for index in [1, 2] {
            assert_eq!(
                string.get_field_type_at_index(index),
                Some(size_type.into())
            );
        }
        Some(std::array::from_fn(|index| {
            target.offset_of_element(&string, index as u32).unwrap()
        }))
    } else {
        None
    };
    OrderedMapLayout {
        header_name: header.get_name().unwrap().to_str().unwrap().to_owned(),
        header_offsets: std::array::from_fn(|index| {
            target.offset_of_element(&header, index as u32).unwrap()
        }),
        header_size: target.get_abi_size(&header),
        header_alignment: target.get_abi_alignment(&header),
        slot_type: slot.print_to_string().to_string(),
        slot_offsets: std::array::from_fn(|index| {
            target.offset_of_element(&slot, index as u32).unwrap()
        }),
        slot_stride: target.get_abi_size(&slot),
        slot_alignment: target.get_abi_alignment(&slot),
        string_offsets,
        pointer_bits: size_type.get_bit_width(),
    }
}
