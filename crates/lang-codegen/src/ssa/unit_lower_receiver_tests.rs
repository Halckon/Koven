// 所有 receiver 测试与共享 helper 仅通过 ssa/mod.rs 的 cfg(test) 入口编译。
use lang_frontend::{
    name_resolution::SourceUnitInput,
    source::SourceMap,
    type_checking::{BuiltinType, UnitTypeKind, standard_environments},
};

use super::{
    model::{
        EntityId, EntityType, Function, LoanKind, Operation, PlaceAccess, SsaTypeKind,
        TerminatorKind,
    },
    render::render_program,
    unit_lower::lower_scalar_unit_with_entry,
    unit_lower_test_support::{analyze, declaration, parsed},
};
use crate::llvm::render_verified_program;

mod borrowing;
mod delegation;
mod inout_fields;
mod inout_inline;
mod interface;
mod representation;
mod value_delivery;

fn function<'a>(
    mut functions: impl Iterator<Item = &'a Function>,
    name_fragment: &str,
) -> &'a Function {
    functions
        .find(|function| function.name.contains(name_fragment))
        .expect("planned function exists")
}
