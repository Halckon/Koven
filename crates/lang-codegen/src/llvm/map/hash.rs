//! Map hash LLVM implementation.
use super::*;

/// 计算键的哈希值（返回 size_t IntValue）。
#[allow(clippy::too_many_arguments)]
pub(super) fn build_key_hash<'ctx>(
    context: &'ctx Context,
    builder: &Builder<'ctx>,
    function: FunctionValue<'ctx>,
    runtime: &RuntimeAbi<'ctx>,
    module: &Module,
    key_ty: SsaTypeId,
    key: BasicValueEnum<'ctx>,
    name: &str,
) -> Result<IntValue<'ctx>, LlvmAdapterError> {
    match module.type_kind(key_ty) {
        Some(SsaTypeKind::Integer { bits: _, .. }) => {
            let int_val = key.into_int_value();
            let size_int = builder.build_int_cast(
                int_val,
                runtime.size_type(),
                &format!("{name}.cast_int"),
            )?;
            let multiplier = runtime.size_type().const_int(2654435761, false);
            let mul = builder.build_int_mul(size_int, multiplier, &format!("{name}.mul"))?;
            let shift = runtime.size_type().const_int(16, false);
            let shr = builder.build_right_shift(mul, shift, false, &format!("{name}.shr"))?;
            Ok(builder.build_xor(mul, shr, &format!("{name}.hash"))?)
        }
        Some(SsaTypeKind::Boolean) => {
            let bool_val = key.into_int_value();
            Ok(builder.build_int_z_extend(
                bool_val,
                runtime.size_type(),
                &format!("{name}.bool_hash"),
            )?)
        }
        Some(SsaTypeKind::Char) => {
            let char_val = key.into_int_value();
            let size_int = builder.build_int_cast(
                char_val,
                runtime.size_type(),
                &format!("{name}.char_cast"),
            )?;
            let multiplier = runtime.size_type().const_int(2654435761, false);
            Ok(builder.build_int_mul(size_int, multiplier, &format!("{name}.char_hash"))?)
        }
        Some(SsaTypeKind::StringOwner) => {
            // String: FNV-1a 哈希
            let str_struct = key.into_struct_value();
            let bytes_ptr = builder
                .build_extract_value(str_struct, 0, "str_ptr")?
                .into_pointer_value();
            let length = builder
                .build_extract_value(str_struct, 1, "str_len")?
                .into_int_value();

            let preheader = builder.get_insert_block().unwrap();
            let loop_check = context.append_basic_block(function, &format!("{name}.fnv_check"));
            let loop_body = context.append_basic_block(function, &format!("{name}.fnv_body"));
            let done = context.append_basic_block(function, &format!("{name}.fnv_done"));

            // FNV offset basis: 2166136261 (32位) / 14695981039346656037 (64位)
            let fnv_basis = if runtime.size_type().get_bit_width() >= 64 {
                14695981039346656037_u64
            } else {
                2166136261_u64
            };
            let fnv_prime = if runtime.size_type().get_bit_width() >= 64 {
                1099511628211_u64
            } else {
                16777619_u64
            };

            let basis_val = runtime.size_type().const_int(fnv_basis, false);
            let prime = runtime.size_type().const_int(fnv_prime, false);
            builder.build_unconditional_branch(loop_check)?;

            builder.position_at_end(loop_check);
            let idx_phi = builder.build_phi(runtime.size_type(), "fnv.idx")?;
            idx_phi.add_incoming(&[(&runtime.size_type().const_zero(), preheader)]);
            let hash_phi = builder.build_phi(runtime.size_type(), "fnv.hash")?;
            hash_phi.add_incoming(&[(&basis_val, preheader)]);

            let current_idx = idx_phi.as_basic_value().into_int_value();
            let current_hash = hash_phi.as_basic_value().into_int_value();

            let finished = builder.build_int_compare(
                IntPredicate::UGE,
                current_idx,
                length,
                "fnv.finished",
            )?;
            builder.build_conditional_branch(finished, done, loop_body)?;

            builder.position_at_end(loop_body);
            // SAFETY: the UTF-8 byte index is checked against length and uses an i8 layout.
            let byte_ptr = unsafe {
                builder.build_gep(context.i8_type(), bytes_ptr, &[current_idx], "byte_ptr")?
            };
            let byte_val = builder
                .build_load(context.i8_type(), byte_ptr, "byte")?
                .into_int_value();
            let byte_extended =
                builder.build_int_z_extend(byte_val, runtime.size_type(), "byte_ext")?;
            let xored = builder.build_xor(current_hash, byte_extended, "xored")?;
            let next_hash = builder.build_int_mul(xored, prime, "next_hash")?;
            let next_idx = builder.build_int_add(
                current_idx,
                runtime.size_type().const_int(1, false),
                "next_idx",
            )?;
            let loop_body_bb = builder.get_insert_block().unwrap();
            builder.build_unconditional_branch(loop_check)?;
            idx_phi.add_incoming(&[(&next_idx, loop_body_bb)]);
            hash_phi.add_incoming(&[(&next_hash, loop_body_bb)]);

            builder.position_at_end(done);
            Ok(current_hash)
        }
        _ => Err(LlvmAdapterError::Unsupported(format!(
            "不支持的 Map 键哈希类型: {key_ty:?}"
        ))),
    }
}

/// 比较两个键是否相等（返回 i1 Boolean IntValue）。
#[allow(clippy::too_many_arguments)]
pub(super) fn build_key_equal<'ctx>(
    context: &'ctx Context,
    builder: &Builder<'ctx>,
    function: FunctionValue<'ctx>,
    runtime: &RuntimeAbi<'ctx>,
    module: &Module,
    key_ty: SsaTypeId,
    left: BasicValueEnum<'ctx>,
    right: BasicValueEnum<'ctx>,
    name: &str,
) -> Result<IntValue<'ctx>, LlvmAdapterError> {
    match module.type_kind(key_ty) {
        Some(SsaTypeKind::Integer { .. } | SsaTypeKind::Boolean | SsaTypeKind::Char) => {
            let left_int = left.into_int_value();
            let right_int = right.into_int_value();
            Ok(builder.build_int_compare(
                IntPredicate::EQ,
                left_int,
                right_int,
                &format!("{name}.eq"),
            )?)
        }
        Some(SsaTypeKind::StringOwner) => {
            let left_struct = left.into_struct_value();
            let right_struct = right.into_struct_value();
            let left_view = super::super::string::view(builder, left_struct, &format!("{name}.l"))?;
            let right_view =
                super::super::string::view(builder, right_struct, &format!("{name}.r"))?;
            super::super::string::equal(
                context,
                builder,
                function,
                runtime,
                left_view,
                right_view,
                &format!("{name}.streq"),
            )
        }
        _ => Err(LlvmAdapterError::Unsupported(format!(
            "不支持的 Map 键类型比较: {key_ty:?}"
        ))),
    }
}
