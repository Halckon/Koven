//! Independent original-source ownership topology and complete reachable SSA transport queries.
mod ssa;
mod unit;

pub(super) use ssa::query as ssa;
pub(super) use unit::query as unit;
