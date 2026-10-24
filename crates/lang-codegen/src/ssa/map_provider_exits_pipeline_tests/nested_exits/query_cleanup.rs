//! Unit 原始退出表；旧 owner/condition/phi 行为由同轮 SSA 全列和完整库存证明。
#[path = "query_cleanup_unit.rs"]
mod unit_cleanup;
use super::{Failure, failure, need, one};
pub(super) use unit_cleanup::{caller as unit_caller, check as unit};
