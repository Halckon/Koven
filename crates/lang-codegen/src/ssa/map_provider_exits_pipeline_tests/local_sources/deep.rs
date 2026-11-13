//! Independent original-source ownership topology and complete reachable SSA transport queries.
pub(in crate::ssa) mod ssa;

pub(super) use ssa::query as ssa;
pub(super) use ssa::unit::query as unit;

pub(in crate::ssa) mod nullable_map_guard_support;
