//! LLVM 21 / Inkwell 兼容边界。

mod adapter;
mod aggregate {
    //! First-class aggregate value operation lowering。

    use inkwell::{
        builder::Builder,
        values::{BasicValueEnum, StructValue},
    };

    use crate::ssa::model::SsaTypeId;

    use super::{LlvmAdapterError, type_map::TypeMap};

    pub(super) fn construct<'ctx>(
        builder: &Builder<'ctx>,
        types: &TypeMap<'ctx>,
        aggregate: SsaTypeId,
        fields: &[BasicValueEnum<'ctx>],
        name: &str,
    ) -> Result<StructValue<'ctx>, LlvmAdapterError> {
        let mut value = types.aggregate_type(aggregate)?.const_zero();
        for (index, field) in fields.iter().enumerate() {
            value = builder
                .build_insert_value(value, *field, index as u32, &format!("{name}.field{index}"))?
                .into_struct_value();
        }
        value.set_name(name);
        Ok(value)
    }

    pub(super) fn project<'ctx>(
        builder: &Builder<'ctx>,
        aggregate: StructValue<'ctx>,
        field: usize,
        name: &str,
    ) -> Result<BasicValueEnum<'ctx>, LlvmAdapterError> {
        Ok(builder.build_extract_value(aggregate, field as u32, name)?)
    }

    pub(super) fn explode<'ctx>(
        builder: &Builder<'ctx>,
        aggregate: StructValue<'ctx>,
        names: &[String],
    ) -> Result<Vec<BasicValueEnum<'ctx>>, LlvmAdapterError> {
        names
            .iter()
            .enumerate()
            .map(|(index, name)| Ok(builder.build_extract_value(aggregate, index as u32, name)?))
            .collect()
    }
}
#[cfg(test)]
#[allow(unused_imports)]
pub(crate) use layout_tests::aggregate_tests;
#[cfg(test)]
#[allow(unused_imports)]
pub(crate) use object_tests::borrowed_generate_tests;
pub(crate) mod closure;
#[cfg(test)]
#[allow(unused_imports)]
pub(crate) use crate::native::unit_tests::constants::numeric_literal_tests::packed_closure_tests_6 as closure_tests;
mod container;
#[cfg(test)]
pub(crate) mod container_tests;
#[allow(unused_imports)]
pub(crate) use closure::debug;
#[cfg(test)]
pub(crate) mod debug_tests;
#[cfg(test)]
pub(crate) mod emission_failure {
    //! Thread-local object-path fault injection, compiled exclusively into tests.

    use std::{
        cell::RefCell,
        path::{Path, PathBuf},
    };

    thread_local! {
        static FAILURE: RefCell<Option<(PathBuf, usize)>> = const { RefCell::new(None) };
    }

    pub(crate) struct Guard(Option<(PathBuf, usize)>);

    impl Guard {
        pub(crate) fn new(path: PathBuf) -> Self {
            assert!(path.parent().expect("fault parent").is_file());
            Self::replace_path(path)
        }

        #[cfg(target_os = "linux")]
        pub(crate) fn full_device() -> Self {
            use std::os::unix::fs::FileTypeExt;

            let path = PathBuf::from("/dev/full");
            assert!(
                std::fs::metadata(&path)
                    .unwrap()
                    .file_type()
                    .is_char_device()
            );
            Self::replace_path(path)
        }

        fn replace_path(path: PathBuf) -> Self {
            Self(FAILURE.with(|state| state.replace(Some((path, 0)))))
        }

        pub(crate) fn calls(&self) -> usize {
            FAILURE.with(|state| state.borrow().as_ref().expect("live guard").1)
        }
    }

    impl Drop for Guard {
        fn drop(&mut self) {
            FAILURE.with(|state| state.replace(self.0.take()));
        }
    }

    pub(super) fn object_path(path: &Path) -> PathBuf {
        FAILURE.with(|state| {
            let mut state = state.borrow_mut();
            match state.as_mut() {
                Some((fault, count)) => {
                    *count += 1;
                    fault.clone()
                }
                None => path.to_path_buf(),
            }
        })
    }
}
mod entities {
    //! SSA entity 类型与 instruction result 的只读 LLVM 映射辅助。

    use crate::ssa::model::{
        EntityId, EntityType, Function, Instruction, LoanId, PlaceAccess, PlaceId, SsaTypeId,
        ValueId,
    };

    use super::LlvmAdapterError;

    pub(super) fn value_type(
        function: &Function,
        value: ValueId,
    ) -> Result<SsaTypeId, LlvmAdapterError> {
        match function.entity(EntityId::Value(value)).map(|data| data.ty) {
            Some(EntityType::Value(ty)) => Ok(ty),
            _ => Err(LlvmAdapterError::InvalidSsa(
                "ValueId 缺少 value entity type".to_owned(),
            )),
        }
    }

    pub(super) fn place_type(
        function: &Function,
        place: PlaceId,
    ) -> Result<SsaTypeId, LlvmAdapterError> {
        match function.entity(EntityId::Place(place)).map(|data| data.ty) {
            Some(EntityType::Place(ty)) => Ok(ty),
            _ => Err(LlvmAdapterError::InvalidSsa(
                "PlaceId 缺少 place entity type".to_owned(),
            )),
        }
    }

    pub(super) fn access_type(
        function: &Function,
        access: PlaceAccess,
    ) -> Result<SsaTypeId, LlvmAdapterError> {
        let entity = match access {
            PlaceAccess::Place(place) => EntityId::Place(place),
            PlaceAccess::Loan(loan) => EntityId::Loan(loan),
        };
        function
            .entity(entity)
            .map(|data| data.ty.semantic_type())
            .ok_or_else(|| LlvmAdapterError::InvalidSsa("place access 缺少 entity type".to_owned()))
    }

    pub(super) fn value_results(
        instruction: &Instruction,
    ) -> Result<Vec<ValueId>, LlvmAdapterError> {
        instruction
            .results
            .iter()
            .map(|entity| match entity {
                EntityId::Value(value) => Ok(*value),
                EntityId::Place(_) | EntityId::Loan(_) => Err(LlvmAdapterError::Unsupported(
                    "LLVM instruction result 必须是 value".to_owned(),
                )),
            })
            .collect()
    }

    pub(super) fn place_result(instruction: &Instruction) -> Result<PlaceId, LlvmAdapterError> {
        match instruction.results.as_slice() {
            [EntityId::Place(place)] => Ok(*place),
            _ => Err(LlvmAdapterError::InvalidSsa(
                "place operation 必须产生一个 place".to_owned(),
            )),
        }
    }

    pub(super) fn loan_result(instruction: &Instruction) -> Result<LoanId, LlvmAdapterError> {
        match instruction.results.as_slice() {
            [EntityId::Loan(loan)] => Ok(*loan),
            _ => Err(LlvmAdapterError::InvalidSsa(
                "borrow begin 必须产生一个 loan".to_owned(),
            )),
        }
    }

    pub(super) fn value_name(value: ValueId) -> String {
        format!("v{}", value.index())
    }
}
#[cfg(test)]
#[allow(unused_imports)]
pub(crate) use container_tests::entry_tests;
#[allow(unused_imports)]
pub(crate) use string::entry;
#[cfg(test)]
mod inline_nullable_tests;
pub(crate) mod layout;
#[cfg(test)]
pub(crate) mod layout_tests;
mod map;
#[cfg(test)]
#[allow(unused_imports)]
pub(crate) use object_tests::nullable_tests;
#[cfg(test)]
pub(crate) mod object_tests;
#[cfg(test)]
pub(crate) mod ordered_map_test_support {
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
}
mod runtime;
#[cfg(test)]
mod runtime_tests;
mod scalar {
    //! LLVM scalar signedness decisions derived from verified SSA types.

    use inkwell::IntPredicate;

    use crate::ssa::model::{ComparisonOperator, Function, Module, SsaTypeKind, ValueId};

    use super::{LlvmAdapterError, entities::value_type};

    pub(super) fn ordering_predicate(
        module: &Module,
        function: &Function,
        operand: ValueId,
        signed: IntPredicate,
        unsigned: IntPredicate,
    ) -> Result<IntPredicate, LlvmAdapterError> {
        Ok(if integer_signed(module, function, operand)? {
            signed
        } else {
            unsigned
        })
    }

    pub(super) fn comparison_predicate(
        module: &Module,
        function: &Function,
        operand: ValueId,
        operator: ComparisonOperator,
    ) -> Result<IntPredicate, LlvmAdapterError> {
        Ok(match operator {
            ComparisonOperator::Equal => IntPredicate::EQ,
            ComparisonOperator::NotEqual => IntPredicate::NE,
            ComparisonOperator::LessThan => ordering_predicate(
                module,
                function,
                operand,
                IntPredicate::SLT,
                IntPredicate::ULT,
            )?,
            ComparisonOperator::LessThanOrEqual => ordering_predicate(
                module,
                function,
                operand,
                IntPredicate::SLE,
                IntPredicate::ULE,
            )?,
            ComparisonOperator::GreaterThan => ordering_predicate(
                module,
                function,
                operand,
                IntPredicate::SGT,
                IntPredicate::UGT,
            )?,
            ComparisonOperator::GreaterThanOrEqual => ordering_predicate(
                module,
                function,
                operand,
                IntPredicate::SGE,
                IntPredicate::UGE,
            )?,
        })
    }

    pub(super) fn integer_signed(
        module: &Module,
        function: &Function,
        value: ValueId,
    ) -> Result<bool, LlvmAdapterError> {
        match module.type_kind(value_type(function, value)?) {
            Some(SsaTypeKind::Integer { signed, .. }) => Ok(*signed),
            _ => Err(LlvmAdapterError::InvalidSsa(
                "integer operation 的 operand 不是整数".to_owned(),
            )),
        }
    }
}
pub(crate) mod string;
#[cfg(test)]
#[allow(unused_imports)]
pub(crate) use debug_tests::string_tests;
#[cfg(test)]
#[allow(unused_imports)]
pub(crate) use object_tests::synthetic_zst_tests;
mod tagged {
    //! Tagged-union construction and statically refined payload access.

    use inkwell::{
        builder::Builder,
        values::{IntValue, PointerValue, StructValue},
    };

    use crate::ssa::model::SsaTypeId;

    use super::{LlvmAdapterError, type_map::TypeMap};

    pub(super) fn construct<'ctx>(
        builder: &Builder<'ctx>,
        types: &TypeMap<'ctx>,
        tagged: SsaTypeId,
        variant: usize,
        payload: StructValue<'ctx>,
        name: &str,
    ) -> Result<StructValue<'ctx>, LlvmAdapterError> {
        let layout = types.tagged_layout(tagged)?;
        if layout.payloads.get(variant).copied() != Some(payload.get_type()) {
            return Err(LlvmAdapterError::InvalidSsa(
                "tagged construction payload type does not match variant".to_owned(),
            ));
        }
        let storage = builder.build_alloca(layout.value, &format!("{name}.slot"))?;
        let tag = builder.build_struct_gep(layout.value, storage, 0, &format!("{name}.tag"))?;
        builder.build_store(
            tag,
            payload.get_type().get_context().i32_type().const_int(
                u64::try_from(variant).map_err(|_| {
                    LlvmAdapterError::InvalidSsa("tagged variant index exceeds i32 tag".to_owned())
                })?,
                false,
            ),
        )?;
        let payload_slot =
            builder.build_struct_gep(layout.value, storage, 1, &format!("{name}.payload"))?;
        builder.build_store(payload_slot, payload)?;
        Ok(builder
            .build_load(layout.value, storage, name)?
            .into_struct_value())
    }

    pub(super) fn discriminant<'ctx>(
        builder: &Builder<'ctx>,
        types: &TypeMap<'ctx>,
        tagged: SsaTypeId,
        owner: StructValue<'ctx>,
        name: &str,
    ) -> Result<IntValue<'ctx>, LlvmAdapterError> {
        let layout = types.tagged_layout(tagged)?;
        if owner.get_type() != layout.value {
            return Err(LlvmAdapterError::InvalidSsa(
                "tagged discriminant has invalid owner".to_owned(),
            ));
        }
        Ok(builder
            .build_extract_value(owner, 0, name)?
            .into_int_value())
    }

    pub(super) fn payload_place<'ctx>(
        builder: &Builder<'ctx>,
        types: &TypeMap<'ctx>,
        tagged: SsaTypeId,
        variant: usize,
        owner: StructValue<'ctx>,
        name: &str,
    ) -> Result<PointerValue<'ctx>, LlvmAdapterError> {
        let layout = types.tagged_layout(tagged)?;
        if layout.payloads.get(variant).is_none() || owner.get_type() != layout.value {
            return Err(LlvmAdapterError::InvalidSsa(
                "tagged payload place has invalid owner or variant".to_owned(),
            ));
        }
        let storage = builder.build_alloca(layout.value, &format!("{name}.owner"))?;
        builder.build_store(storage, owner)?;
        Ok(builder.build_struct_gep(layout.value, storage, 1, name)?)
    }
}
#[allow(unused_imports)]
pub(crate) use layout::type_map;
#[cfg(test)]
mod unit_storage_tests;
#[allow(unused_imports)]
pub(crate) use closure::utf8;
#[cfg(test)]
mod utf8_tests {
    //! The shared scanner must report the start of the first invalid sequence, not the bad continuation.
    use inkwell::{context::Context, module::Linkage};
    use std::{fs, process::Command};

    #[test]
    fn utf8_error_offset_native_matches_strict_sequence_boundaries() {
        let cases: &[&[u8]] = &[
            b"",
            b"ASCII\0bytes",
            "中文🙂".as_bytes(),
            &[0xc2, 0x80],
            &[0xdf, 0xbf],
            &[0xe0, 0xa0, 0x80],
            &[0xed, 0x9f, 0xbf],
            &[0xee, 0x80, 0x80],
            &[0xef, 0xbf, 0xbf],
            &[0xf0, 0x90, 0x80, 0x80],
            &[0xf4, 0x8f, 0xbf, 0xbf],
            &[0x80],
            &[0xbf],
            &[0xc0, 0x80],
            &[0xc1, 0xbf],
            &[0xe0, 0x9f, 0xbf],
            &[0xed, 0xa0, 0x80],
            &[0xf0, 0x8f, 0xbf, 0xbf],
            &[0xf4, 0x90, 0x80, 0x80],
            &[0xf5, 0x80, 0x80, 0x80],
            &[0xff],
            &[0x41, 0xe2, 0x28, 0xa1],
            &[0x41, 0xe2, 0x82],
            &[0x41, 0xc2],
            &[0x41, 0xf0, 0x90, 0x80],
            &[0xc2, 0xa2, 0xe2, 0x82],
            &[0xc2, 0xa2, 0x80],
            &[0xc2, 0xa2, 0xe2, 0x28, 0xa1, 0xff],
            &[0xe2, 0x82, 0xac, 0xf0, 0x90, 0x80, 0x41],
            &[0xe2, 0x82, 0xac, 0xc2, 0x41],
        ];
        let context = Context::create();
        let module = context.create_module("utf8-offset-probe");
        let scanner =
            super::utf8::define_error_offset(&context, &module, context.i64_type()).unwrap();
        scanner.as_global_value().set_name("test_utf8_error_offset");
        scanner.set_linkage(Linkage::External);
        module.verify().unwrap();
        let mut harness = String::from(
            "#include <stdint.h>\n#include <stdio.h>\nextern uint64_t test_utf8_error_offset(const unsigned char *, uint64_t);\nint main(void) {\nif(test_utf8_error_offset((const unsigned char*)0,0)!=UINT64_MAX)return 90;\n",
        );
        for (index, bytes) in cases.iter().enumerate() {
            let expected = std::str::from_utf8(bytes)
                .err()
                .map_or(u64::MAX, |error| error.valid_up_to() as u64);
            let data = if bytes.is_empty() {
                "0".to_owned()
            } else {
                bytes
                    .iter()
                    .map(u8::to_string)
                    .collect::<Vec<_>>()
                    .join(",")
            };
            harness.push_str(&format!("{{const unsigned char data[]={{ {data} }}; uint64_t actual=test_utf8_error_offset(data,{}); if(actual!=UINT64_C({expected})){{fprintf(stderr,\"case {index}: %llu\\n\",(unsigned long long)actual);return 1;}}}}\n", bytes.len()));
        }
        harness.push_str("for(unsigned value=0;value<256;value++){unsigned char byte=(unsigned char)value;uint64_t expected=value<128?UINT64_MAX:0;if(test_utf8_error_offset(&byte,1)!=expected)return 2;}\nreturn 0;}\n");
        let directory = crate::native_tests::TestDirectory::create();
        let ir = directory.join("scanner.ll");
        let c = directory.join("probe.c");
        let binary = directory.join("probe");
        fs::write(&ir, module.print_to_string().to_bytes()).unwrap();
        fs::write(&c, harness).unwrap();
        let linked = Command::new(crate::test_support::ir_clang())
            .arg("-Wno-override-module")
            .arg(&ir)
            .arg(&c)
            .arg("-o")
            .arg(&binary)
            .output()
            .unwrap();
        assert!(linked.status.success(), "{linked:?}");
        let run = Command::new(binary).output().unwrap();
        assert!(run.status.success(), "{run:?}");
        assert!(run.stdout.is_empty() && run.stderr.is_empty(), "{run:?}");
    }
}

use inkwell::OptimizationLevel;
use inkwell::builder::BuilderError;
#[cfg(test)]
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::targets::{
    CodeModel, InitializationConfig, RelocMode, Target, TargetMachine, TargetTriple,
};
use lang_frontend::source::SourceMap;

use crate::ssa::model::Program;

const MACOS_TARGET: &str = "aarch64-apple-darwin";
const LINUX_TARGET: &str = "x86_64-unknown-linux-gnu";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum LlvmAdapterError {
    InvalidSsa(String),
    InvalidEntry(String),
    Debug(String),
    InvalidLayout(layout::TargetLayoutError),
    Object(String),
    Target(String),
    Build(String),
    Unsupported(String),
    Verify(String),
}

impl From<BuilderError> for LlvmAdapterError {
    fn from(error: BuilderError) -> Self {
        Self::Build(error.to_string())
    }
}

/// 把已验证的 target-independent SSA 映射为当前受支持宿主的 LLVM IR 文本。
#[cfg(test)]
pub(crate) fn render_verified_program(program: &Program) -> Result<String, LlvmAdapterError> {
    adapter::render_verified_program(program, None)
}

#[cfg(test)]
pub(crate) fn render_verified_program_with_entry(
    program: &Program,
    entry: crate::ssa::model::FunctionId,
) -> Result<String, LlvmAdapterError> {
    adapter::render_verified_program(
        program,
        Some(entry::NativeEntryPlan::NoArguments { function: entry }),
    )
}

#[cfg(test)]
pub(crate) fn render_verified_program_with_entry_plan(
    program: &Program,
    entry: entry::NativeEntryPlan,
) -> Result<String, LlvmAdapterError> {
    adapter::render_verified_program(program, Some(entry))
}

pub(crate) fn emit_verified_object(
    program: &Program,
    sources: &SourceMap,
    entry: impl Into<entry::NativeEntryPlan>,
    path: &std::path::Path,
) -> Result<(), LlvmAdapterError> {
    adapter::emit_verified_object(program, sources, entry.into(), path)
}

#[cfg(test)]
pub(crate) fn render_verified_program_with_debug(
    program: &Program,
    sources: &SourceMap,
    entry: crate::ssa::model::FunctionId,
) -> Result<String, LlvmAdapterError> {
    adapter::render_verified_program_with_debug(
        program,
        sources,
        entry::NativeEntryPlan::NoArguments { function: entry },
    )
}

/// 构造最小标量模块，以验证固定 LLVM 工具链、target backend 和 verifier 边界。
#[cfg(test)]
pub(crate) fn render_scalar_smoke_module() -> Result<String, LlvmAdapterError> {
    let (triple, target_machine) = native_target_machine()?;

    let context = Context::create();
    let module = context.create_module("koven.scalar-smoke");
    configure_module(&module, &triple, &target_machine);

    let int_type = context.i32_type();
    let function_type = int_type.fn_type(&[int_type.into(), int_type.into()], false);
    let function = module.add_function("add", function_type, None);
    let entry = context.append_basic_block(function, "entry");
    let builder = context.create_builder();
    builder.position_at_end(entry);

    let left = function
        .get_nth_param(0)
        .expect("函数签名固定包含第一个参数")
        .into_int_value();
    let right = function
        .get_nth_param(1)
        .expect("函数签名固定包含第二个参数")
        .into_int_value();
    let sum = builder
        .build_int_add(left, right, "sum")
        .map_err(|error| LlvmAdapterError::Build(error.to_string()))?;
    builder
        .build_return(Some(&sum))
        .map_err(|error| LlvmAdapterError::Build(error.to_string()))?;

    module
        .verify()
        .map_err(|error| LlvmAdapterError::Verify(error.to_string()))?;
    Ok(module.print_to_string().to_string())
}

pub(crate) fn native_target_triple() -> Result<&'static str, LlvmAdapterError> {
    target_for_host(
        std::env::consts::ARCH,
        std::env::consts::OS,
        cfg!(target_env = "gnu"),
        cfg!(target_pointer_width = "64"),
    )
}

fn target_for_host(
    arch: &str,
    os: &str,
    gnu: bool,
    pointer64: bool,
) -> Result<&'static str, LlvmAdapterError> {
    match (arch, os, gnu, pointer64) {
        ("aarch64", "macos", _, true) => Ok(MACOS_TARGET),
        ("x86_64", "linux", true, true) => Ok(LINUX_TARGET),
        _ => Err(LlvmAdapterError::Target(format!(
            "unsupported native host: {arch}-{os}; supported targets: {MACOS_TARGET}, {LINUX_TARGET}"
        ))),
    }
}

fn native_target_machine() -> Result<(TargetTriple, TargetMachine), LlvmAdapterError> {
    target_machine_for_triple(native_target_triple()?)
}

fn target_machine_for_triple(
    target_triple: &str,
) -> Result<(TargetTriple, TargetMachine), LlvmAdapterError> {
    match target_triple {
        MACOS_TARGET => Target::initialize_aarch64(&InitializationConfig::default()),
        LINUX_TARGET => Target::initialize_x86(&InitializationConfig::default()),
        _ => {
            return Err(LlvmAdapterError::Target(format!(
                "unsupported native target: {target_triple}"
            )));
        }
    }
    let triple = TargetTriple::create(target_triple);
    let target = Target::from_triple(&triple)
        .map_err(|error| LlvmAdapterError::Target(error.to_string()))?;
    let target_machine = target
        .create_target_machine(
            &triple,
            "generic",
            "",
            OptimizationLevel::None,
            RelocMode::PIC,
            CodeModel::Default,
        )
        .ok_or_else(|| {
            LlvmAdapterError::Target(format!("无法创建 {target_triple} target machine"))
        })?;
    Ok((triple, target_machine))
}

fn configure_module(module: &Module<'_>, triple: &TargetTriple, machine: &TargetMachine) {
    module.set_triple(triple);
    module.set_data_layout(&machine.get_target_data().get_data_layout());
}

#[cfg(test)]
mod tests {
    use inkwell::context::Context;
    use lang_frontend::source::SourceMap;

    use crate::ssa::model::{
        BinaryOperator, EntityId, EntityType, Operation, Origin, Program, SsaTypeKind,
        TerminatorKind, ValueId,
    };

    use super::{
        LINUX_TARGET, LlvmAdapterError, MACOS_TARGET, native_target_triple,
        render_scalar_smoke_module, render_verified_program, target_for_host,
        target_machine_for_triple,
    };

    #[test]
    #[cfg(all(target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
    fn linux_host_renders_x86_64_gnu_target() {
        let llvm = render_scalar_smoke_module().expect("Linux target must be available");
        assert!(llvm.contains("target triple = \"x86_64-unknown-linux-gnu\""));
    }

    #[test]
    fn renders_deterministic_verified_native_scalar_module() {
        let first = render_scalar_smoke_module().expect("固定 LLVM 21 矩阵应生成合法模块");
        let second = render_scalar_smoke_module().expect("重复生成应保持确定");

        crate::test_support::diagnostics::assert_text_eq!(first, second);
        let target = native_target_triple().expect("supported native host");
        assert!(first.contains(&format!("target triple = \"{target}\"")));
        assert!(first.contains("define i32 @add(i32 %0, i32 %1)"));
        assert!(first.contains("%sum = add i32 %0, %1"));
    }

    #[test]
    fn native_host_selection_rejects_unsupported_platforms() {
        assert_eq!(
            target_for_host("aarch64", "macos", false, true),
            Ok(MACOS_TARGET)
        );
        assert_eq!(
            target_for_host("x86_64", "linux", true, true),
            Ok(LINUX_TARGET)
        );
        for (arch, os, gnu, pointer64) in [
            ("x86_64", "linux", false, true),
            ("aarch64", "linux", true, true),
            ("x86_64", "macos", false, true),
            ("x86_64", "windows", true, true),
            ("x86_64", "linux", true, false),
        ] {
            assert!(matches!(
                target_for_host(arch, os, gnu, pointer64),
                Err(LlvmAdapterError::Target(_))
            ));
        }
    }

    #[test]
    fn both_supported_backends_keep_a_64_bit_pointer_layout() {
        for target in [MACOS_TARGET, LINUX_TARGET] {
            let (triple, machine) = target_machine_for_triple(target).expect("enabled backend");
            assert_eq!(triple.as_str().to_str().unwrap(), target);
            assert_eq!(machine.get_target_data().get_pointer_byte_size(None), 8);
        }
    }

    #[test]
    fn llvm_verifier_rejects_missing_terminator() {
        let context = Context::create();
        let module = context.create_module("invalid");
        let function = module.add_function("broken", context.void_type().fn_type(&[], false), None);
        context.append_basic_block(function, "entry");

        let error = module
            .verify()
            .expect_err("缺少 terminator 的 basic block 必须被 LLVM verifier 拒绝");
        assert!(error.to_string().contains("does not have terminator"));
    }

    #[test]
    fn adapter_rejects_invalid_ssa_before_constructing_llvm() {
        let mut sources = SourceMap::new();
        let source = sources.add_source("invalid.ko", "fun broken() {}").unwrap();
        let span = sources.span(source, 0, 15).unwrap();
        let mut program = Program::default();
        let module = program.add_module("invalid");
        let module = program.module_mut(module).unwrap();
        let function = module
            .add_function("broken", Vec::new(), Origin::Source(span))
            .unwrap();
        module
            .function_mut(function)
            .unwrap()
            .add_block(Vec::new(), Origin::Source(span))
            .unwrap();

        let error = render_verified_program(&program)
            .expect_err("invalid SSA must not reach LLVM construction");
        assert!(matches!(error, LlvmAdapterError::InvalidSsa(_)));
    }

    #[test]
    fn adapter_maps_verified_unchecked_scalar_primitives() {
        let mut sources = SourceMap::new();
        let source = sources.add_source("primitive.ko", "fun primitive").unwrap();
        let origin = Origin::Source(sources.span(source, 0, 13).unwrap());
        let mut program = Program::default();
        let module = program.add_module("primitive");
        let module = program.module_mut(module).unwrap();
        let integer = module.intern_type(SsaTypeKind::Integer {
            bits: 32,
            signed: false,
        });
        let boolean = module.intern_type(SsaTypeKind::Boolean);
        let function = module
            .add_function("primitive", vec![boolean], origin.clone())
            .unwrap();
        let function = module.function_mut(function).unwrap();
        let entry = function
            .add_block(
                vec![EntityType::Value(integer), EntityType::Value(integer)],
                origin.clone(),
            )
            .unwrap();
        let parameters = function.block(entry).unwrap().parameters.clone();
        let left = value(parameters[0]);
        let right = value(parameters[1]);
        let (_, sum) = function
            .append_instruction(
                entry,
                Operation::Binary {
                    operator: BinaryOperator::Add,
                    left,
                    right,
                },
                vec![EntityType::Value(integer)],
                origin.clone(),
            )
            .unwrap();
        let (_, copied) = function
            .append_instruction(
                entry,
                Operation::Copy {
                    source: value(sum[0]),
                },
                vec![EntityType::Value(integer)],
                origin.clone(),
            )
            .unwrap();
        let (_, comparison) = function
            .append_instruction(
                entry,
                Operation::Binary {
                    operator: BinaryOperator::LessThan,
                    left: value(copied[0]),
                    right,
                },
                vec![EntityType::Value(boolean)],
                origin.clone(),
            )
            .unwrap();
        function
            .set_terminator(
                entry,
                TerminatorKind::Return {
                    values: vec![value(comparison[0])],
                },
                origin,
            )
            .unwrap();

        let llvm = render_verified_program(&program).expect("verified primitives must lower");
        assert!(llvm.contains("add i32 %v0, %v1"));
        assert!(llvm.contains("icmp ult i32"));
    }

    fn value(entity: EntityId) -> ValueId {
        let EntityId::Value(value) = entity else {
            panic!("expected ValueId")
        };
        value
    }
}
