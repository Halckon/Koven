use std::collections::BTreeMap;

use crate::{
    ast::ExpressionId,
    parser::{Expression, ParsedFile},
    type_checking::{
        BuiltinType, ConstructionDescriptor, ConstructionTarget, Copyability, ExpressionCategory,
        NominalKind, ParameterMode, TypeKind, TypedFile,
    },
};

use super::{Checker, ExpressionUse, Flows, OwnershipCheckingError, State};
use crate::ownership_checking::{
    ConstructionDeliveryEffect, ConstructionDeliveryKind, ConstructionOwnershipPlan,
    ConstructionRootDropObligation, ConstructionRootKind,
};

pub(super) struct Analysis {
    pub(super) descriptors: BTreeMap<usize, ConstructionDescriptor>,
    plans: Vec<ConstructionOwnershipPlan>,
}

impl Analysis {
    pub(super) fn new(
        parsed: &ParsedFile,
        typed: &TypedFile,
    ) -> Result<Self, OwnershipCheckingError> {
        let mut descriptors = BTreeMap::new();
        for descriptor in typed.constructions() {
            validate_descriptor(parsed, typed, descriptor)?;
            if descriptors
                .insert(descriptor.expression().index(), descriptor.clone())
                .is_some()
            {
                return Err(invalid(descriptor.expression()));
            }
        }
        Ok(Self {
            descriptors,
            plans: Vec::new(),
        })
    }

    pub(super) fn descriptor(&self, expression: ExpressionId) -> Option<ConstructionDescriptor> {
        self.descriptors.get(&expression.index()).cloned()
    }

    fn push(&mut self, plan: ConstructionOwnershipPlan) {
        self.plans.push(plan);
    }

    pub(super) fn finish(mut self, valid: bool) -> Vec<ConstructionOwnershipPlan> {
        if !valid {
            self.plans.clear();
        }
        self.plans
    }
}

fn validate_descriptor(
    parsed: &ParsedFile,
    typed: &TypedFile,
    descriptor: &ConstructionDescriptor,
) -> Result<(), OwnershipCheckingError> {
    let expression = descriptor.expression();
    if typed.expression_type(expression) != Some(descriptor.result_type())
        || typed.types().get(descriptor.result_type()).is_none()
        || descriptor
            .instance()
            .type_arguments()
            .iter()
            .any(|&ty| typed.types().get(ty).is_none())
    {
        return Err(invalid(expression));
    }

    let node = parsed.ast().expressions().get(expression)?;
    let source_arguments = match node.payload() {
        Expression::Call { arguments, .. } => arguments
            .iter()
            .map(|argument| argument.value)
            .collect::<Vec<_>>(),
        Expression::Name | Expression::Member { .. } if descriptor.arguments().is_empty() => {
            Vec::new()
        }
        _ => return Err(invalid(expression)),
    };
    if source_arguments.len() != descriptor.arguments().len() {
        return Err(invalid(expression));
    }

    let expected_symbols = match descriptor.target() {
        ConstructionTarget::Nominal(target) => typed
            .nominals()
            .iter()
            .find(|nominal| nominal.id() == target)
            .filter(|nominal| {
                matches!(nominal.kind(), NominalKind::Class | NominalKind::ValueClass)
            })
            .map(|nominal| {
                nominal
                    .fields()
                    .iter()
                    .copied()
                    .map(Some)
                    .collect::<Vec<_>>()
            }),
        ConstructionTarget::EnumCase(target) => typed
            .enum_cases()
            .iter()
            .find(|case| case.id() == target)
            .map(|case| {
                case.payloads()
                    .iter()
                    .map(|(symbol, _)| Some(*symbol))
                    .collect::<Vec<_>>()
            }),
        ConstructionTarget::IntrinsicBox => Some(vec![None]),
    }
    .ok_or_else(|| invalid(expression))?;
    if expected_symbols.len() != descriptor.arguments().len() {
        return Err(invalid(expression));
    }

    let mut seen_evaluations = vec![false; source_arguments.len()];
    for (parameter_index, argument) in descriptor.arguments().iter().enumerate() {
        let evaluation_index = argument.evaluation_index();
        if argument.parameter_index() != parameter_index
            || argument.parameter_symbol() != expected_symbols[parameter_index]
            || argument.mode() != ParameterMode::Value
            || evaluation_index >= source_arguments.len()
            || seen_evaluations[evaluation_index]
            || source_arguments[evaluation_index] != argument.argument()
            || typed.expression_category(argument.argument()) != Some(argument.category())
            || typed.types().get(argument.parameter_type()).is_none()
        {
            return Err(invalid(expression));
        }
        seen_evaluations[evaluation_index] = true;
    }
    if descriptor.target() == ConstructionTarget::IntrinsicBox
        && descriptor.arguments().first().is_none_or(|argument| {
            argument.parameter_name() != "element" || argument.parameter_symbol().is_some()
        })
    {
        return Err(invalid(expression));
    }
    Ok(())
}

fn invalid(expression: ExpressionId) -> OwnershipCheckingError {
    OwnershipCheckingError::InvalidConstructionDescriptor {
        expression: expression.index(),
    }
}

impl Checker<'_> {
    pub(super) fn check_construction(
        &mut self,
        descriptor: ConstructionDescriptor,
        state: State,
        _usage: ExpressionUse,
    ) -> Result<Flows, OwnershipCheckingError> {
        let diagnostic_count = self.diagnostics.len();
        let mut flows = Flows::next(state);
        let mut arguments = descriptor.arguments().to_vec();
        arguments.sort_by_key(|argument| argument.evaluation_index());
        let mut deliveries = Vec::with_capacity(arguments.len());

        for argument in arguments {
            if let Some(next) = flows.next.as_ref() {
                self.reject_borrowed_closure_escape(argument.argument(), next)?;
            }
            let argument_diagnostics = self.diagnostics.len();
            flows = self.chain_expression(flows, argument.argument(), ExpressionUse::Consume)?;
            self.release_last_closure_use(argument.argument(), &mut flows)?;

            if self.is_nothing_expression(argument.argument()) {
                flows.next = None;
                if self.diagnostics.len() == diagnostic_count {
                    self.construction.push(ConstructionOwnershipPlan::new(
                        descriptor.expression(),
                        descriptor.target(),
                        deliveries,
                        None,
                        Some(argument.argument()),
                    ));
                }
                return Ok(flows);
            }
            if self.diagnostics.len() != argument_diagnostics || flows.next.is_none() {
                continue;
            }
            deliveries.push(ConstructionDeliveryEffect::new(
                descriptor.expression(),
                argument.argument(),
                argument.parameter_index(),
                argument.parameter_symbol(),
                argument.evaluation_index(),
                self.delivery_kind(argument.argument(), argument.category())?,
            ));
        }

        if flows.next.is_some() && self.diagnostics.len() == diagnostic_count {
            let root_obligation = match self.typed.copyability(descriptor.result_type()) {
                Some(Copyability::Copyable) => None,
                Some(Copyability::MoveOnly) => Some(ConstructionRootDropObligation::new(
                    descriptor.expression(),
                    descriptor.result_type(),
                    self.root_kind(descriptor.expression(), descriptor.target())?,
                )),
                Some(Copyability::Unknown | Copyability::Error) | None => {
                    return Err(invalid(descriptor.expression()));
                }
            };
            self.construction.push(ConstructionOwnershipPlan::new(
                descriptor.expression(),
                descriptor.target(),
                deliveries,
                root_obligation,
                None,
            ));
        }
        Ok(flows)
    }

    fn delivery_kind(
        &self,
        expression: ExpressionId,
        category: ExpressionCategory,
    ) -> Result<ConstructionDeliveryKind, OwnershipCheckingError> {
        if category == ExpressionCategory::Temporary {
            return Ok(ConstructionDeliveryKind::DeliverTemporary);
        }
        let copyability = self
            .typed
            .expression_type(expression)
            .and_then(|ty| self.typed.copyability(ty));
        match copyability {
            Some(Copyability::Copyable) => Ok(ConstructionDeliveryKind::Copy),
            Some(Copyability::MoveOnly) => Ok(ConstructionDeliveryKind::Move),
            Some(Copyability::Unknown | Copyability::Error) | None => Err(invalid(expression)),
        }
    }

    fn root_kind(
        &self,
        expression: ExpressionId,
        target: ConstructionTarget,
    ) -> Result<ConstructionRootKind, OwnershipCheckingError> {
        match target {
            ConstructionTarget::IntrinsicBox => Ok(ConstructionRootKind::HeapOwner),
            ConstructionTarget::EnumCase(_) => Ok(ConstructionRootKind::Inline),
            ConstructionTarget::Nominal(target) => self
                .typed
                .nominals()
                .iter()
                .find(|nominal| nominal.id() == target)
                .and_then(|nominal| match nominal.kind() {
                    NominalKind::Class => Some(ConstructionRootKind::HeapOwner),
                    NominalKind::ValueClass => Some(ConstructionRootKind::Inline),
                    NominalKind::Interface | NominalKind::EnumClass | NominalKind::Object => None,
                })
                .ok_or_else(|| invalid(expression)),
        }
    }

    pub(super) fn is_nothing_expression(&self, expression: ExpressionId) -> bool {
        self.typed
            .expression_type(expression)
            .and_then(|ty| self.typed.types().get(ty))
            == Some(&TypeKind::Builtin(BuiltinType::Nothing))
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        lexer::lex,
        name_resolution::{NameEnvironment, resolve_names},
        parser::parse_file,
        source::SourceMap,
        type_checking::{BuiltinType, ConstructionDescriptor, TypeEnvironment, check_types},
    };

    use super::{OwnershipCheckingError, validate_descriptor};

    #[test]
    fn invalid_argument_identity_is_an_internal_error() {
        let mut sources = SourceMap::new();
        let source = sources
            .add_source(
                "invalid-construction-descriptor.ko",
                "class Holder(val item: Int)\nfun build(): Holder = Holder(1)",
            )
            .expect("source");
        let lexed = lex(&sources, source).expect("lex");
        let parsed = parse_file(&sources, &lexed).expect("parse");
        assert!(parsed.diagnostics().is_empty());

        let mut names = NameEnvironment::new();
        let builtins = BuiltinType::ALL.map(|builtin| {
            (
                names.declare_type(builtin.name()).expect("builtin"),
                builtin,
            )
        });
        let mut types = TypeEnvironment::new(&names);
        for (symbol, builtin) in builtins {
            types.bind_builtin(symbol, builtin).expect("binding");
        }
        let resolution = resolve_names(&sources, &parsed, &names).expect("names");
        let typed = check_types(&sources, &parsed, &resolution, &types).expect("types");
        assert!(typed.diagnostics().is_empty());
        let descriptor = typed.constructions().first().expect("construction");
        let malformed = ConstructionDescriptor::new(
            descriptor.expression(),
            descriptor.target(),
            descriptor.instance().type_arguments().to_vec(),
            descriptor.result_type(),
            Vec::new(),
        );

        assert!(matches!(
            validate_descriptor(&parsed, &typed, &malformed),
            Err(OwnershipCheckingError::InvalidConstructionDescriptor { .. })
        ));
    }
}
