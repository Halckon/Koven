//! Reachable compilation-unit body 使用的 scalar storage type 预规划。

use std::collections::BTreeMap;

use lang_frontend::{
    parser::ParsedFile,
    source::Span,
    type_checking::{UnitTypeId, ValidatedCompilationUnitTypes},
};

use super::{
    super::{LoweringError, LoweringErrorKind, model::SsaTypeId, unit_plan::resolve_concrete_type},
    builtin_type, intern_scalar_type, is_scalar_storage_builtin, lowering_error,
};
use crate::ssa::{model::Module, unit_plan::UnitPlannedInstance};

/// 只为当前 reachable instance body 中实际出现的已支持 storage type 建立 SSA identity。
pub(super) fn intern_body_scalar_types(
    module: &mut Module,
    parsed: &ParsedFile,
    instance: &UnitPlannedInstance,
    typed: &ValidatedCompilationUnitTypes,
    type_ids: &mut BTreeMap<UnitTypeId, SsaTypeId>,
) -> Result<(), LoweringError> {
    for (&expression, &ty) in typed.types().expression_types() {
        if expression.source_unit() != instance.source_unit() {
            continue;
        }
        let span = parsed
            .ast()
            .expressions()
            .get(expression.expression())
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, instance.span()))?
            .span();
        if !span_contains(instance.span(), span) {
            continue;
        }
        let concrete = resolve_concrete_type(typed, ty, instance.substitutions(), span)?;
        if builtin_type(typed, concrete).is_some_and(is_scalar_storage_builtin) {
            intern_scalar_type(module, typed, type_ids, concrete, span)?;
        }
    }
    Ok(())
}

fn span_contains(owner: Span, child: Span) -> bool {
    owner.source_id() == child.source_id()
        && owner.start() <= child.start()
        && child.end() <= owner.end()
}

#[cfg(test)]
mod tests {
    use lang_frontend::{
        name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
    };

    use super::*;
    use crate::ssa::{
        model::{Program, SsaTypeKind},
        unit_lower_test_support::{analyze, declaration, parsed},
        unit_plan::plan_unit_instances,
    };

    #[test]
    fn generic_body_type_plan_resolves_the_concrete_instance_type() {
        let mut sources = SourceMap::new();
        let (provider_source, provider) = parsed(
            &mut sources,
            "p/provider.ko",
            "package p\n\
             fun <T> keep(own input: T): Unit {\n\
                 val kept = input\n\
             }",
        );
        let (consumer_source, consumer) = parsed(
            &mut sources,
            "q/consumer.ko",
            "package q\nfun entry(): Unit { val done = p.keep(\"generic\") }",
        );
        let inputs = [
            SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
            SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
        ];
        let (name_environment, type_environment) = standard_environments();
        let (names, typed, owned) =
            analyze(&sources, &inputs, &name_environment, &type_environment);
        let instances = plan_unit_instances(
            &sources,
            &inputs,
            &names,
            &type_environment,
            &typed,
            &owned,
            declaration(&names, "q", "entry"),
        )
        .expect("generic unit instance plan succeeds");
        let keep = instances
            .iter()
            .find(|instance| !instance.key().type_arguments().is_empty())
            .expect("concrete generic keep instance exists");

        let mut program = Program::default();
        let module_id = program.add_module("type-plan-test");
        let module = program
            .module_mut(module_id)
            .expect("new test module exists");
        let mut type_ids = BTreeMap::new();
        intern_body_scalar_types(module, &provider, keep, &typed, &mut type_ids)
            .expect("direct T body fact resolves through the String substitution");

        assert_eq!(module.types, vec![SsaTypeKind::StringOwner]);
        assert_eq!(type_ids.len(), 1);
    }
}
