//! Independent original-source ownership topology and complete reachable SSA transport queries.
mod single;
mod ssa;
mod unit;

pub(super) use single::query as single;
pub(super) use ssa::query as ssa;
pub(super) use unit::query as unit;
