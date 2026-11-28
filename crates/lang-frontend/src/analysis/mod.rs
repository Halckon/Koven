//! 显式内存源码的纯分析编排与封闭只读产物；不承担宿主 IO 或推进策略。

mod display_source;
mod unit_basic_ownership;
mod unit_display_completion;
mod unit_names;
mod unit_recovery;
pub use display_source::{DISPLAY_PATH, append_builtin_display};
pub use unit_recovery::{UnitRecoveryAnalysisError, analyze_unit_recovery};

pub use unit_names::{
    UnitNameAnalysisError, UnitNameSnapshot, UnitSourceDescriptor, analyze_parsed_unit_names,
    analyze_unit_names,
};

pub use unit_basic_ownership::{BasicOwnershipOutcome, analyze_basic_unit_ownership};
pub use unit_display_completion::complete_unit_display_types;
