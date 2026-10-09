//! Phase 2 单文件类型检查与可供所有权阶段消费的 typed facts。

mod argument_mapping;
mod borrow_result;
mod call;
mod canonical;
mod checker;
mod compilation_unit;
mod constant;
mod constant_evaluation;
#[cfg(test)]
mod constant_evaluation_tests;
mod constant_graph;
mod constant_value;
mod construction;
mod container;
mod declaration_frontier;
mod error;
mod expression_use;
mod integer;
mod iteration;
mod literal_value;
mod map_descriptor;
mod map_with_descriptor;
mod model;
mod non_null_assertion;
mod nullable_when;
mod ownership_primitive;
mod parameter;
mod projection;
mod range_type_uses;
mod rc;
mod resource;
mod string;

use std::{sync::Arc, thread};

use crate::{
    name_resolution::{NameEnvironment, NameResolution},
    parser::ParsedFile,
    source::SourceMap,
};

pub(crate) use expression_use::{ExpressionUse, collect_expression_uses};

pub use borrow_result::{BorrowReturnContract, BorrowReturnOrigin};
mod result_source;
mod source_authority;
pub use call::*;
pub use compilation_unit::*;
pub use constant::*;
pub use constant_value::ConstValue;
pub use construction::*;
pub use container::*;
pub use error::TypeCheckingError;
pub use integer::*;
pub use iteration::*;
pub use literal_value::integer_literal_magnitude;
pub use map_descriptor::*;
pub use map_with_descriptor::MapWithValueDescriptor;
pub use model::*;
pub use non_null_assertion::*;
pub use nullable_when::*;
pub use ownership_primitive::*;
pub use parameter::*;
pub use projection::*;
pub use rc::*;
pub use resource::*;
pub use result_source::{CallableResultSource, CarrierReturnContract, CarrierSourceMarker};
mod range_construction;
pub use range_construction::{RangeConstructionDescriptor, RangeSizeDescriptor, RangeSourceKind};
mod range_extension;
pub use range_extension::RangeExtensionBinding;
pub use string::*;

/// 构造一组共享身份、包含全部编译器内建类型的标准分析环境。
///
/// 名称与类型环境必须成对传递给后续 pass；每次调用都会创建新的 analysis owner，避免不同
/// 编译任务通过全局状态串联。
#[must_use]
pub fn standard_environments() -> (NameEnvironment, TypeEnvironment) {
    let mut names = NameEnvironment::new();
    let builtins = BuiltinType::ALL.map(|builtin| {
        let symbol = names
            .declare_type(builtin.name())
            .expect("BuiltinType::ALL names must remain unique");
        (symbol, builtin)
    });
    let capabilities = [
        (
            names
                .declare_type("Copyable")
                .expect("the Copyable capability name must remain unique"),
            Capability::Copyable,
        ),
        (
            names
                .declare_type("Transferable")
                .expect("the Transferable capability name must remain unique"),
            Capability::Transferable,
        ),
        (
            names
                .declare_type("Hashable")
                .expect("the Hashable capability name must remain unique"),
            Capability::Hashable,
        ),
    ];
    let intrinsics = [
        ("Box", IntrinsicTypeConstructor::Box),
        ("Rc", IntrinsicTypeConstructor::Rc),
        ("Array", IntrinsicTypeConstructor::Array),
        ("List", IntrinsicTypeConstructor::List),
        ("MutableList", IntrinsicTypeConstructor::MutableList),
        ("Map", IntrinsicTypeConstructor::Map),
        ("MutableMap", IntrinsicTypeConstructor::MutableMap),
        ("View", IntrinsicTypeConstructor::View),
    ]
    .map(|(name, intrinsic)| {
        (
            names
                .declare_type(name)
                .expect("intrinsic type names must remain unique"),
            intrinsic,
        )
    });
    let error = names
        .declare_function("error")
        .expect("the standard error function name must remain unique");
    let println = names
        .declare_function("println")
        .expect("the standard println function name must remain unique");
    let intrinsic_callables = [
        ("arrayOf", IntrinsicCallable::ArrayOf),
        ("listOf", IntrinsicCallable::ListOf),
        ("mutableListOf", IntrinsicCallable::MutableListOf),
        ("mapOf", IntrinsicCallable::MapOf),
        ("mutableMapOf", IntrinsicCallable::MutableMapOf),
        ("replace", IntrinsicCallable::Replace),
        ("swap", IntrinsicCallable::Swap),
        ("rangeView", IntrinsicCallable::RangeView),
    ]
    .map(|(name, callable)| {
        (
            names
                .declare_function(name)
                .expect("intrinsic callable names must remain unique"),
            callable,
        )
    });
    let mut types = TypeEnvironment::new(&names);
    for (symbol, builtin) in builtins {
        types
            .bind_builtin(symbol, builtin)
            .expect("fresh builtin symbols must match their type environment");
    }
    for (symbol, capability) in capabilities {
        types
            .bind_capability(symbol, capability)
            .expect("fresh capability symbols must match their type environment");
    }
    for (symbol, intrinsic) in intrinsics {
        types
            .bind_intrinsic(symbol, intrinsic)
            .expect("fresh intrinsic type symbols must match their type environment");
    }
    types
        .bind_function(
            error,
            EnvironmentFunction {
                parameters: vec![EnvironmentParameter {
                    mode: ParameterMode::Borrow,
                    ty: EnvironmentType::Builtin(BuiltinType::String),
                }],
                return_type: EnvironmentType::Builtin(BuiltinType::Nothing),
                effects: vec![EnvironmentFunctionEffect::Abort],
            },
        )
        .expect("the standard error signature must satisfy its compiler-bound effect");
    types
        .bind_function(
            println,
            EnvironmentFunction {
                parameters: vec![EnvironmentParameter {
                    mode: ParameterMode::Borrow,
                    ty: EnvironmentType::Builtin(BuiltinType::String),
                }],
                return_type: EnvironmentType::Builtin(BuiltinType::Unit),
                effects: vec![EnvironmentFunctionEffect::PrintLine],
            },
        )
        .expect("the standard println signature must satisfy its compiler-bound effect");
    for (symbol, callable) in intrinsic_callables {
        types
            .bind_intrinsic_callable(symbol, callable)
            .expect("fresh intrinsic callable symbols must match their type environment");
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

    use crate::name_resolution::NameEnvironment;

    use super::{
        BuiltinType, Capability, EnvironmentFunction, EnvironmentFunctionEffect,
        EnvironmentParameter, EnvironmentType, ExternalTypeBinding, IntrinsicCallable,
        IntrinsicTypeConstructor, ParameterMode, TypeCheckingError, TypeEnvironment,
        standard_environments,
    };

    #[test]
    fn standard_environments_declare_all_compiler_bound_identities_once() {
        let (first_names, first_types) = standard_environments();
        let (second_names, _second_types) = standard_environments();
        let mut expected = BuiltinType::ALL
            .map(BuiltinType::name)
            .into_iter()
            .collect::<Vec<_>>();
        expected.extend([
            "Copyable",
            "Transferable",
            "Hashable",
            "Box",
            "Rc",
            "Array",
            "List",
            "MutableList",
            "Map",
            "MutableMap",
            "View",
            "error",
            "println",
            "arrayOf",
            "listOf",
            "mutableListOf",
            "mapOf",
            "mutableMapOf",
            "replace",
            "swap",
            "rangeView",
        ]);
        let actual = first_names
            .symbols()
            .iter()
            .map(|symbol| symbol.name())
            .collect::<Vec<_>>();

        assert_eq!(actual, expected);
        assert_eq!(
            actual.iter().copied().collect::<BTreeSet<_>>().len(),
            expected.len()
        );
        assert_eq!(
            second_names
                .symbols()
                .iter()
                .map(|symbol| symbol.name())
                .collect::<Vec<_>>(),
            actual
        );
        for (symbol, builtin) in first_names
            .symbols()
            .iter()
            .take(BuiltinType::ALL.len())
            .zip(BuiltinType::ALL)
        {
            assert!(matches!(
                first_types.binding(symbol.id()),
                Some(super::ExternalTypeBinding::Builtin(bound)) if *bound == builtin
            ));
        }
        let binding = |name: &str| {
            let symbol = first_names
                .symbols()
                .iter()
                .find(|symbol| symbol.name() == name)
                .expect("standard symbol");
            first_types.binding(symbol.id()).expect("standard binding")
        };
        assert_eq!(
            binding("Copyable"),
            &ExternalTypeBinding::Capability(Capability::Copyable)
        );
        assert_eq!(
            binding("Transferable"),
            &ExternalTypeBinding::Capability(Capability::Transferable)
        );
        assert_eq!(
            binding("Hashable"),
            &ExternalTypeBinding::Capability(Capability::Hashable)
        );
        for (name, intrinsic) in [
            ("Box", IntrinsicTypeConstructor::Box),
            ("Rc", IntrinsicTypeConstructor::Rc),
            ("Array", IntrinsicTypeConstructor::Array),
            ("List", IntrinsicTypeConstructor::List),
            ("MutableList", IntrinsicTypeConstructor::MutableList),
            ("Map", IntrinsicTypeConstructor::Map),
            ("MutableMap", IntrinsicTypeConstructor::MutableMap),
            ("View", IntrinsicTypeConstructor::View),
        ] {
            assert_eq!(binding(name), &ExternalTypeBinding::Intrinsic(intrinsic));
        }
        for (name, callable) in [
            ("arrayOf", IntrinsicCallable::ArrayOf),
            ("listOf", IntrinsicCallable::ListOf),
            ("mutableListOf", IntrinsicCallable::MutableListOf),
            ("mapOf", IntrinsicCallable::MapOf),
            ("mutableMapOf", IntrinsicCallable::MutableMapOf),
            ("replace", IntrinsicCallable::Replace),
            ("swap", IntrinsicCallable::Swap),
        ] {
            assert_eq!(
                binding(name),
                &ExternalTypeBinding::IntrinsicCallable(callable)
            );
        }
        let error = first_names
            .symbols()
            .iter()
            .find(|symbol| symbol.name() == "error")
            .expect("standard error symbol");
        assert!(matches!(
            first_types.binding(error.id()),
            Some(ExternalTypeBinding::Function(signature))
                if signature.parameters.len() == 1
                    && signature.parameters[0].mode == ParameterMode::Borrow
                    && signature.parameters[0].ty
                        == EnvironmentType::Builtin(BuiltinType::String)
                    && signature.return_type
                        == EnvironmentType::Builtin(BuiltinType::Nothing)
                    && signature.effects == [EnvironmentFunctionEffect::Abort]
        ));
        let println = first_names
            .symbols()
            .iter()
            .find(|symbol| symbol.name() == "println")
            .expect("standard println symbol");
        assert!(matches!(
            first_types.binding(println.id()),
            Some(ExternalTypeBinding::Function(signature))
                if signature.parameters.len() == 1
                    && signature.parameters[0].mode == ParameterMode::Borrow
                    && signature.parameters[0].ty
                        == EnvironmentType::Builtin(BuiltinType::String)
                    && signature.return_type
                        == EnvironmentType::Builtin(BuiltinType::Unit)
                    && signature.effects == [EnvironmentFunctionEffect::PrintLine]
        ));
    }

    #[test]
    fn abort_effect_accepts_only_the_standard_error_signature() {
        for (index, signature) in [
            EnvironmentFunction {
                parameters: Vec::new(),
                return_type: EnvironmentType::Builtin(BuiltinType::Nothing),
                effects: vec![EnvironmentFunctionEffect::Abort],
            },
            EnvironmentFunction {
                parameters: vec![EnvironmentParameter {
                    mode: ParameterMode::Value,
                    ty: EnvironmentType::Builtin(BuiltinType::String),
                }],
                return_type: EnvironmentType::Builtin(BuiltinType::Nothing),
                effects: vec![EnvironmentFunctionEffect::Abort],
            },
            EnvironmentFunction {
                parameters: vec![EnvironmentParameter {
                    mode: ParameterMode::Borrow,
                    ty: EnvironmentType::Builtin(BuiltinType::String),
                }],
                return_type: EnvironmentType::Builtin(BuiltinType::Unit),
                effects: vec![EnvironmentFunctionEffect::Abort],
            },
        ]
        .into_iter()
        .enumerate()
        {
            let mut names = NameEnvironment::new();
            let symbol = names
                .declare_function(format!("invalid{index}"))
                .expect("unique function");
            let mut types = TypeEnvironment::new(&names);
            assert!(matches!(
                types.bind_function(symbol, signature),
                Err(TypeCheckingError::InvalidExternalBinding)
            ));
        }
    }

    #[test]
    fn print_line_effect_accepts_only_the_standard_println_signature() {
        for (index, signature) in [
            EnvironmentFunction {
                parameters: Vec::new(),
                return_type: EnvironmentType::Builtin(BuiltinType::Unit),
                effects: vec![EnvironmentFunctionEffect::PrintLine],
            },
            EnvironmentFunction {
                parameters: vec![EnvironmentParameter {
                    mode: ParameterMode::Value,
                    ty: EnvironmentType::Builtin(BuiltinType::String),
                }],
                return_type: EnvironmentType::Builtin(BuiltinType::Unit),
                effects: vec![EnvironmentFunctionEffect::PrintLine],
            },
            EnvironmentFunction {
                parameters: vec![EnvironmentParameter {
                    mode: ParameterMode::Borrow,
                    ty: EnvironmentType::Builtin(BuiltinType::String),
                }],
                return_type: EnvironmentType::Builtin(BuiltinType::Nothing),
                effects: vec![EnvironmentFunctionEffect::PrintLine],
            },
        ]
        .into_iter()
        .enumerate()
        {
            let mut names = NameEnvironment::new();
            let symbol = names
                .declare_function(format!("invalid{index}"))
                .expect("unique function");
            let mut types = TypeEnvironment::new(&names);
            assert!(matches!(
                types.bind_function(symbol, signature),
                Err(TypeCheckingError::InvalidExternalBinding)
            ));
        }
    }
}
