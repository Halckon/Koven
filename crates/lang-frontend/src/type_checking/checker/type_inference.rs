//! Callable 与 construction 共用的结构化不变类型实参匹配。

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    name_resolution::SymbolId,
    source::Span,
    type_checking::{TypeId, TypeKind},
};

use super::Checker;

impl Checker<'_> {
    pub(super) fn infer_type_arguments(
        &self,
        template: TypeId,
        actual: TypeId,
        parameters: &BTreeSet<SymbolId>,
        substitutions: &mut BTreeMap<SymbolId, TypeId>,
        origins: &mut BTreeMap<SymbolId, Span>,
        origin: Span,
    ) -> Result<(), SymbolId> {
        if let TypeKind::TypeParameter(parameter) = self.kind(template)
            && parameters.contains(parameter)
        {
            if self.is_deferred(actual) {
                return Ok(());
            }
            return match substitutions.get(parameter).copied() {
                Some(previous) if previous != actual => Err(*parameter),
                Some(_) => Ok(()),
                None => {
                    substitutions.insert(*parameter, actual);
                    origins.insert(*parameter, origin);
                    Ok(())
                }
            };
        }
        if !self.contains_type_parameter(template, parameters, &mut BTreeSet::new()) {
            return Ok(());
        }
        match (self.kind(template), self.kind(actual)) {
            (TypeKind::Nullable(template), TypeKind::Nullable(actual))
            | (TypeKind::StaticSelf(template), TypeKind::StaticSelf(actual)) => self
                .infer_type_arguments(
                    *template,
                    *actual,
                    parameters,
                    substitutions,
                    origins,
                    origin,
                ),
            (
                TypeKind::Function {
                    move_only: template_move,
                    parameters: template_parameters,
                    return_type: template_return,
                },
                TypeKind::Function {
                    move_only: actual_move,
                    parameters: actual_parameters,
                    return_type: actual_return,
                },
            ) if template_move == actual_move
                && template_parameters.len() == actual_parameters.len()
                && template_parameters
                    .iter()
                    .zip(actual_parameters)
                    .all(|(template, actual)| template.mode == actual.mode) =>
            {
                for (template, actual) in template_parameters.iter().zip(actual_parameters) {
                    self.infer_type_arguments(
                        template.ty,
                        actual.ty,
                        parameters,
                        substitutions,
                        origins,
                        origin,
                    )?;
                }
                self.infer_type_arguments(
                    *template_return,
                    *actual_return,
                    parameters,
                    substitutions,
                    origins,
                    origin,
                )
            }
            (
                TypeKind::Nominal {
                    nominal: template_nominal,
                    arguments: template_arguments,
                },
                TypeKind::Nominal {
                    nominal: actual_nominal,
                    arguments: actual_arguments,
                },
            ) if template_nominal == actual_nominal
                && template_arguments.len() == actual_arguments.len() =>
            {
                self.infer_argument_lists(
                    template_arguments,
                    actual_arguments,
                    parameters,
                    substitutions,
                    origins,
                    origin,
                )
            }
            (
                TypeKind::Intrinsic {
                    constructor: template_constructor,
                    arguments: template_arguments,
                },
                TypeKind::Intrinsic {
                    constructor: actual_constructor,
                    arguments: actual_arguments,
                },
            ) if template_constructor == actual_constructor
                && template_arguments.len() == actual_arguments.len() =>
            {
                self.infer_argument_lists(
                    template_arguments,
                    actual_arguments,
                    parameters,
                    substitutions,
                    origins,
                    origin,
                )
            }
            _ => Err(self
                .first_contained_parameter(template, parameters, &mut BTreeSet::new())
                .expect("a recursive inference path contains a requested type parameter")),
        }
    }

    fn infer_argument_lists(
        &self,
        templates: &[TypeId],
        actuals: &[TypeId],
        parameters: &BTreeSet<SymbolId>,
        substitutions: &mut BTreeMap<SymbolId, TypeId>,
        origins: &mut BTreeMap<SymbolId, Span>,
        origin: Span,
    ) -> Result<(), SymbolId> {
        for (&template, &actual) in templates.iter().zip(actuals) {
            self.infer_type_arguments(
                template,
                actual,
                parameters,
                substitutions,
                origins,
                origin,
            )?;
        }
        Ok(())
    }

    fn contains_type_parameter(
        &self,
        ty: TypeId,
        parameters: &BTreeSet<SymbolId>,
        active: &mut BTreeSet<TypeId>,
    ) -> bool {
        self.first_contained_parameter(ty, parameters, active)
            .is_some()
    }

    fn first_contained_parameter(
        &self,
        ty: TypeId,
        parameters: &BTreeSet<SymbolId>,
        active: &mut BTreeSet<TypeId>,
    ) -> Option<SymbolId> {
        if !active.insert(ty) {
            return None;
        }
        let result = match self.kind(ty) {
            TypeKind::TypeParameter(parameter) if parameters.contains(parameter) => {
                Some(*parameter)
            }
            TypeKind::Nullable(inner) | TypeKind::StaticSelf(inner) => {
                self.first_contained_parameter(*inner, parameters, active)
            }
            TypeKind::Function {
                parameters: function_parameters,
                return_type,
                ..
            } => function_parameters
                .iter()
                .find_map(|parameter| {
                    self.first_contained_parameter(parameter.ty, parameters, active)
                })
                .or_else(|| self.first_contained_parameter(*return_type, parameters, active)),
            TypeKind::Nominal { arguments, .. } | TypeKind::Intrinsic { arguments, .. } => {
                arguments.iter().find_map(|argument| {
                    self.first_contained_parameter(*argument, parameters, active)
                })
            }
            TypeKind::EnumCase { root, .. } => {
                self.first_contained_parameter(*root, parameters, active)
            }
            _ => None,
        };
        active.remove(&ty);
        result
    }
}
