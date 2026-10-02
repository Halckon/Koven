//! 显式内存源码的纯分析编排与封闭只读产物；不承担宿主 IO 或推进策略。

mod unit_names;

pub use unit_names::{
    UnitNameAnalysisError, UnitNameSnapshot, UnitSourceDescriptor, analyze_unit_names,
};
