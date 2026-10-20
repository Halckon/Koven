//! 显式内存源码的纯分析编排与封闭只读产物；不承担宿主 IO 或推进策略。

mod single_file;
mod unit_basic_ownership;
mod unit_names;
mod unit_recovery;
pub use unit_recovery::{UnitRecoveryAnalysisError, analyze_unit_recovery};

pub use unit_names::{
    UnitNameAnalysisError, UnitNameSnapshot, UnitSourceDescriptor, analyze_parsed_unit_names,
    analyze_unit_names,
};

pub use unit_basic_ownership::{BasicOwnershipOutcome, analyze_basic_unit_ownership};

pub use single_file::{
    SingleFileAnalysis, SingleFileAnalysisError, SingleFileStage, SingleFileTypedView,
    analyze_single_file,
};
