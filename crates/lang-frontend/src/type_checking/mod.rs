//! Phase 2 单文件类型检查与可供所有权阶段消费的 typed facts。

mod call;
mod checker;
mod container;
mod error;
mod model;
mod parameter;
mod projection;

use std::{sync::Arc, thread};

use crate::{
    name_resolution::{NameEnvironment, NameResolution},
    parser::ParsedFile,
    source::SourceMap,
};

pub use call::*;
pub use container::*;
pub use error::TypeCheckingError;
pub use model::*;
pub use parameter::*;
pub use projection::*;

/// 构造一组共享身份、包含全部编译器内建类型的标准分析环境。
///
/// 名称与类型环境必须成对传递给后续 pass；每次调用都会创建新的 analysis owner，避免不同
/// 编译任务通过全局状态串联。
#[must_use]
pub fn standard_environments() -> (NameEnvironment, TypeEnvironment) {
    let mut names = NameEnvironment::new();
    let declarations = BuiltinType::ALL.map(|builtin| {
        let symbol = names
            .declare_type(builtin.name())
            .expect("BuiltinType::ALL names must remain unique");
        (symbol, builtin)
    });
    let mut types = TypeEnvironment::new(&names);
    for (symbol, builtin) in declarations {
        types
            .bind_builtin(symbol, builtin)
            .expect("fresh builtin symbols must match their type environment");
    }
    (names, types)
}

/// 对已完成名称解析的单文件执行当前 Phase 2 类型检查流水线。
pub fn check_types(
    sources: &SourceMap,
    parsed: &ParsedFile,
    names: &NameResolution,
    environment: &TypeEnvironment,
) -> Result<TypedFile, TypeCheckingError> {
    if parsed.source_id() != names.source_id() {
        return Err(TypeCheckingError::MismatchedNameSource);
    }
    if !Arc::ptr_eq(names.environment_owner(), environment.owner()) {
        return Err(TypeCheckingError::MismatchedNameEnvironment);
    }
    thread::scope(|scope| {
        thread::Builder::new()
            .name("koven-type-checker".to_owned())
            .stack_size(32 * 1024 * 1024)
            .spawn_scoped(scope, || {
                checker::check(sources, parsed, names, environment)
            })
            .map_err(|error| TypeCheckingError::CheckerThread(error.kind()))?
            .join()
            .map_err(|_| TypeCheckingError::CheckerThreadPanicked)?
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::{BuiltinType, standard_environments};

    #[test]
    fn standard_environments_declare_every_builtin_once_in_canonical_order() {
        let (first_names, first_types) = standard_environments();
        let (second_names, _second_types) = standard_environments();
        let expected = BuiltinType::ALL.map(BuiltinType::name);
        let actual = first_names
            .symbols()
            .iter()
            .map(|symbol| symbol.name())
            .collect::<Vec<_>>();

        assert_eq!(actual, expected);
        assert_eq!(actual.iter().copied().collect::<BTreeSet<_>>().len(), 16);
        assert_eq!(
            second_names
                .symbols()
                .iter()
                .map(|symbol| symbol.name())
                .collect::<Vec<_>>(),
            expected
        );
        for (symbol, builtin) in first_names.symbols().iter().zip(BuiltinType::ALL) {
            assert!(matches!(
                first_types.binding(symbol.id()),
                Some(super::ExternalTypeBinding::Builtin(bound)) if *bound == builtin
            ));
        }
    }
}
