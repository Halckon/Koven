//! Finite canonical publication from committed call sites, without rechecking bodies.

use super::dispatch_owner_types::StaticDispatchOwners;
use super::type_graph::{ConcreteTypes, substitute_for_receiver};
use super::*;
use crate::parser::NameMarker;
use crate::type_checking::CompletedUnitCalls;
use crate::type_checking::{
    IntrinsicTypeConstructor, UnitCallDescriptor, UnitCallTarget, UnitCallableTarget,
    UnitDisplayTarget, UnitInterpolationPart,
};

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct Instance {
    target: UnitCallTarget,
    arguments: Vec<UnitTypeId>,
    static_self: Option<UnitTypeId>,
}

impl Instance {
    fn is_specialized(&self) -> bool {
        !self.arguments.is_empty() || self.static_self.is_some()
    }

    fn is_closed(
        &self,
        concrete: &mut ConcreteTypes,
        signatures: &CompilationUnitSignatures,
    ) -> Result<bool, CompilationUnitTypeError> {
        for ty in self.arguments.iter().copied().chain(self.static_self) {
            if !concrete.is_concrete(signatures.types(), ty)? {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

struct Template {
    parameters: Vec<UnitSymbolId>,
    contextual: bool,
    types: BTreeSet<UnitTypeId>,
    calls: Vec<usize>,
    display_reads: Vec<DisplayRead>,
}

#[derive(Clone, Copy)]
struct DisplayRead {
    operand: UnitExpressionId,
    receiver: UnitTypeId,
    requirement: UnitCallableTarget,
}

#[cfg(test)]
#[path = "generic_body_tests.rs"]
mod tests;

impl BodyChecker<'_> {
    /// Run after all trials, before field layouts and sealed publication. Errors retain recovery facts.
    pub(super) fn materialize_generic_body_types_with_limit(
        &mut self,
        limit: usize,
    ) -> Result<(), CompilationUnitTypeError> {
        self.materialize_generic_body_types(limit, None)
    }

    pub(super) fn materialize_completed_display_bodies(
        &mut self,
        limit: usize,
        completed: &CompletedUnitCalls,
    ) -> Result<(), CompilationUnitTypeError> {
        self.materialize_generic_body_types(limit, Some(completed))
    }

    fn materialize_generic_body_types(
        &mut self,
        limit: usize,
        completed: Option<&CompletedUnitCalls>,
    ) -> Result<(), CompilationUnitTypeError> {
        if self
            .signatures
            .diagnostics()
            .iter()
            .chain(&self.diagnostics)
            .any(|diagnostic| diagnostic.severity() == Severity::Error)
        {
            return Ok(());
        }
        let display_reads = match completed {
            Some(completed) => self.display_reads(completed)?,
            None => Vec::new(),
        };
        let templates = self.generic_body_templates(&display_reads)?;
        let mut dispatch = StaticDispatchOwners::new(&self.signatures)?;
        let mut concrete = ConcreteTypes::for_closed_arguments();
        let mut seeds = BTreeMap::new();
        let contextual_calls = completed.map(|_| {
            templates
                .values()
                .flat_map(|template| template.calls.iter().copied())
                .collect::<BTreeSet<_>>()
        });
        for (index, call) in self.parts.calls.iter().enumerate() {
            if completed.is_some_and(|calls| !calls.explicit.contains(&call.expression()))
                || contextual_calls
                    .as_ref()
                    .is_some_and(|calls| calls.contains(&index))
            {
                continue;
            }
            if let Some((receiver, target)) = dispatch.call(call) {
                dispatch.record(self.signatures.types(), receiver, target);
                if !concrete.is_concrete(self.signatures.types(), receiver)? {
                    continue;
                }
            } else if !templates.contains_key(&call.target()) {
                continue;
            }
            let Some(instance) = body_call_instance(
                &mut self.signatures,
                &dispatch,
                &templates,
                call,
                call.receiver().map(|receiver| receiver.ty()),
                call.instance().type_arguments().to_vec(),
            )?
            else {
                continue;
            };
            if instance.is_closed(&mut concrete, &self.signatures)? {
                let expression = call.expression();
                let span = self
                    .file(expression.source_unit())
                    .ast()
                    .expressions()
                    .get(expression.expression())
                    .map_err(TypeCheckingError::from)?
                    .span();
                seeds.entry(instance).or_insert(span);
            }
        }
        let contextual_displays = templates
            .values()
            .flat_map(|template| template.display_reads.iter().map(|read| read.operand))
            .collect::<BTreeSet<_>>();
        for read in display_reads {
            if contextual_displays.contains(&read.operand) {
                continue;
            }
            dispatch.record(self.signatures.types(), read.receiver, read.requirement);
            if !concrete.is_concrete(self.signatures.types(), read.receiver)? {
                continue;
            }
            if let Some(instance) = display_body_instance(
                &mut self.signatures,
                &dispatch,
                &templates,
                read.requirement,
                read.receiver,
            )? {
                let span = self
                    .file(read.operand.source_unit())
                    .ast()
                    .expressions()
                    .get(read.operand.expression())
                    .map_err(TypeCheckingError::from)?
                    .span();
                seeds.entry(instance).or_insert(span);
            }
        }
        for (seed, call_span) in seeds {
            let mut pending = BTreeSet::from([seed]);
            let mut planned = BTreeSet::new();
            let mut count = 0;
            while let Some(key) = pending.pop_first() {
                if planned.contains(&key) {
                    continue;
                }
                if key.is_specialized() && count >= limit {
                    // The original planner owns budget/recipe arbitration. Publish only its finite frontier.
                    if self
                        .materialize_body_instance(
                            &templates,
                            &key,
                            call_span,
                            &mut dispatch,
                            completed,
                        )?
                        .is_some()
                    {
                        for next in pending.difference(&planned) {
                            if self
                                .materialize_body_instance(
                                    &templates,
                                    next,
                                    call_span,
                                    &mut dispatch,
                                    completed,
                                )?
                                .is_none()
                            {
                                break;
                            }
                        }
                    }
                    break;
                }
                let Some(callees) = self.materialize_body_instance(
                    &templates,
                    &key,
                    call_span,
                    &mut dispatch,
                    completed,
                )?
                else {
                    break;
                };
                for callee in callees {
                    if callee.is_closed(&mut concrete, &self.signatures)?
                        && !planned.contains(&callee)
                    {
                        pending.insert(callee);
                    }
                }
                if key.is_specialized() {
                    count += 1;
                }
                planned.insert(key);
            }
        }
        dispatch.publish(&mut self.signatures, limit)
    }

    fn materialize_body_instance(
        &mut self,
        templates: &BTreeMap<UnitCallTarget, Template>,
        key: &Instance,
        call_span: Span,
        dispatch: &mut StaticDispatchOwners,
        completed: Option<&CompletedUnitCalls>,
    ) -> Result<Option<Vec<Instance>>, CompilationUnitTypeError> {
        let template = templates
            .get(&key.target)
            .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?;
        if template.parameters.len() != key.arguments.len() {
            return Err(CompilationUnitTypeError::MissingDeclarationSymbol);
        }
        let substitutions = template
            .parameters
            .iter()
            .copied()
            .zip(key.arguments.iter().copied())
            .collect();
        let checkpoint = self.signatures.types().len();
        let mut memo = BTreeMap::new();
        for &ty in &template.types {
            substitute_for_receiver(
                self.signatures.types_mut(),
                ty,
                &substitutions,
                key.static_self,
                &mut memo,
            )?;
        }
        let mut callees = Vec::new();
        for &index in &template.calls {
            let call = &self.parts.calls[index];
            if completed.is_some_and(|calls| !calls.explicit.contains(&call.expression())) {
                continue;
            }
            let arguments = call
                .instance()
                .type_arguments()
                .iter()
                .map(|&ty| {
                    substitute_for_receiver(
                        self.signatures.types_mut(),
                        ty,
                        &substitutions,
                        key.static_self,
                        &mut memo,
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
            let receiver = call
                .receiver()
                .and_then(|receiver| memo.get(&receiver.ty()).copied());
            if let Some(callee) = body_call_instance(
                &mut self.signatures,
                dispatch,
                templates,
                call,
                receiver,
                arguments,
            )? {
                callees.push(callee);
            }
        }
        for read in &template.display_reads {
            let receiver = substitute_for_receiver(
                self.signatures.types_mut(),
                read.receiver,
                &substitutions,
                key.static_self,
                &mut memo,
            )?;
            if let Some(callee) = display_body_instance(
                &mut self.signatures,
                dispatch,
                templates,
                read.requirement,
                receiver,
            )? {
                callees.push(callee);
            }
        }
        if let Some(message) = self.invalid_storage_in_substitution(&memo) {
            self.signatures.types_mut().discard_after(checkpoint);
            self.emit(codes::INVALID_CONTAINER_ELEMENT, message, call_span)?;
            return Ok(None);
        }
        for &index in &template.calls {
            if let Some((receiver, target)) = dispatch.call(&self.parts.calls[index]) {
                // Template.types includes each committed call's receiver type.
                let receiver = memo
                    .get(&receiver)
                    .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?;
                dispatch.record(self.signatures.types(), *receiver, target);
            }
        }
        for read in &template.display_reads {
            let receiver = memo
                .get(&read.receiver)
                .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?;
            dispatch.record(self.signatures.types(), *receiver, read.requirement);
        }
        Ok(Some(callees))
    }

    pub(super) fn invalid_storage_in_substitution(
        &self,
        instantiated: &BTreeMap<UnitTypeId, UnitTypeId>,
    ) -> Option<&'static str> {
        let types = self.signatures.types();
        // The substitution memo already visits every changed node. Check only actual
        // template storage sites so explicit invalid TypeRefs keep their own diagnostics.
        instantiated.iter().find_map(|(&template, &actual)| {
            if template == actual {
                return None;
            }
            if let Some(UnitTypeKind::Nominal {
                declaration,
                arguments,
            }) = types.get(actual)
                && self
                    .invalid_nominal_storage_argument(*declaration, arguments)
                    .is_some()
            {
                return Some("type argument cannot satisfy instantiated storage requirements");
            }
            let Some(UnitTypeKind::Intrinsic { constructor, .. }) = types.get(template) else {
                return None;
            };
            let message = match constructor {
                IntrinsicTypeConstructor::Array
                | IntrinsicTypeConstructor::List
                | IntrinsicTypeConstructor::MutableList => {
                    "instantiated sequential container element type is not structurally storable"
                }
                IntrinsicTypeConstructor::Rc => {
                    "instantiated Rc payload type is not structurally storable"
                }
                _ => return None,
            };
            let Some(UnitTypeKind::Intrinsic { arguments, .. }) = types.get(actual) else {
                return None;
            };
            let [argument] = arguments.as_slice() else {
                return None;
            };
            // Runtime Any is a known missing representation, unlike unresolved recovery.
            if matches!(
                types.get(*argument),
                Some(UnitTypeKind::Deferred(
                    DeferredReason::AnyValueRepresentation
                ))
            ) {
                return Some(message);
            }
            (!self.is_error(*argument)
                && !self.is_deferred(*argument)
                && !self.could_be_structurally_storable_type(*argument))
            .then_some(message)
        })
    }

    pub(super) fn invalid_nominal_storage_argument(
        &self,
        declaration: DeclarationId,
        arguments: &[UnitTypeId],
    ) -> Option<UnitSymbolId> {
        let nominal = self.signatures.declaration(declaration)?.nominal()?;
        let required = self
            .signatures
            .storage_requirements()
            .required(declaration)?;
        nominal
            .type_parameters()
            .iter()
            .zip(arguments)
            .find_map(|(parameter, argument)| {
                (required.contains(parameter)
                    && (matches!(
                        self.signatures.types().get(*argument),
                        Some(UnitTypeKind::Deferred(
                            DeferredReason::AnyValueRepresentation
                        ))
                    ) || (!self.is_error(*argument)
                        && !self.is_deferred(*argument)
                        && !self.could_be_structurally_storable_type(*argument))))
                .then_some(*parameter)
            })
    }

    fn display_reads(
        &self,
        completed: &CompletedUnitCalls,
    ) -> Result<Vec<DisplayRead>, CompilationUnitTypeError> {
        let mut reads = Vec::new();
        for parts in self.parts.interpolation_plans.values() {
            for part in parts {
                let UnitInterpolationPart::Format {
                    operand,
                    target: UnitDisplayTarget::Requirement(requirement),
                } = *part
                else {
                    continue;
                };
                if completed.display.get(&operand) != Some(&requirement) {
                    continue;
                }
                let &ty = self
                    .parts
                    .expression_types
                    .get(&operand)
                    .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?;
                let receiver = match self.signatures.types().get(ty) {
                    Some(UnitTypeKind::EnumCase { root, .. }) => *root,
                    _ => ty,
                };
                reads.push(DisplayRead {
                    operand,
                    receiver,
                    requirement,
                });
            }
        }
        Ok(reads)
    }

    fn generic_body_templates(
        &self,
        display_reads: &[DisplayRead],
    ) -> Result<BTreeMap<UnitCallTarget, Template>, CompilationUnitTypeError> {
        let mut definitions = BTreeMap::<SourceUnitId, BTreeMap<usize, _>>::new();
        for declaration in self.names.index().declarations() {
            let Some(signature) = self.signatures.declaration(declaration.id()) else {
                continue;
            };
            let mut register = |callable: &UnitCallableSignature, owner: &[UnitSymbolId]| {
                let contextual = callable.receiver().is_some_and(|receiver| {
                    matches!(
                        self.signatures.types().get(receiver.ty()),
                        Some(UnitTypeKind::StaticSelf(_))
                    )
                });
                if owner.is_empty()
                    && callable.type_parameters().is_empty()
                    && !(contextual && callable.has_body())
                {
                    return;
                }
                let target = match callable.target() {
                    UnitCallableTarget::Declaration(id) => UnitCallTarget::Declaration(id),
                    UnitCallableTarget::Symbol(id) => UnitCallTarget::Symbol(id),
                };
                let template = Template {
                    contextual,
                    parameters: owner
                        .iter()
                        .chain(callable.type_parameters())
                        .copied()
                        .collect(),
                    types: callable
                        .parameters()
                        .iter()
                        .map(|p| p.ty())
                        .chain(std::iter::once(callable.return_type()))
                        .collect(),
                    calls: Vec::new(),
                    display_reads: Vec::new(),
                };
                definitions
                    .entry(declaration.source_unit())
                    .or_default()
                    .insert(callable.name_span().start(), (target, template));
            };
            if let Some(callable) = signature.callable() {
                register(callable, &[]);
            }
            if let Some(nominal) = signature.nominal() {
                for callable in nominal.members().iter().chain(nominal.companion_members()) {
                    let owner = if callable.receiver().is_some() {
                        nominal.type_parameters()
                    } else {
                        &[]
                    };
                    register(callable, owner);
                }
            }
        }

        let mut templates = BTreeMap::new();
        let mut ranges = BTreeMap::<SourceUnitId, BTreeMap<usize, (Span, UnitCallTarget)>>::new();
        for (source, mut definitions) in definitions {
            for (_, node) in self.file(source).ast().items().iter() {
                let Item::Function {
                    name: NameMarker::Present(name),
                    ..
                } = node.payload()
                else {
                    continue;
                };
                let Some((target, template)) = definitions.remove(&name.start()) else {
                    continue;
                };
                let span = node.span();
                ranges
                    .entry(source)
                    .or_default()
                    .insert(span.start(), (span, target));
                templates.insert(target, template);
            }
        }

        if templates.is_empty() {
            return Ok(templates);
        }

        // Source callable item spans are disjoint. Lambda facts are inside their
        // enclosing source callable, so each fact needs only one range lookup.
        let locate = |source: SourceUnitId, span: Span| {
            let (_, &(owner, target)) = ranges.get(&source)?.range(..=span.start()).next_back()?;
            (owner.source_id() == span.source_id() && span.end() <= owner.end()).then_some(target)
        };
        for (expression, &ty) in &self.parts.expression_types {
            let source = expression.source_unit();
            if ranges.contains_key(&source) {
                let span = self
                    .file(source)
                    .ast()
                    .expressions()
                    .get(expression.expression())
                    .map_err(TypeCheckingError::from)?
                    .span();
                if let Some(template) =
                    locate(source, span).and_then(|target| templates.get_mut(&target))
                {
                    template.types.insert(ty);
                }
            }
        }
        for (type_ref, &ty) in &self.parts.type_ref_types {
            let source = type_ref.source_unit();
            if ranges.contains_key(&source) {
                let span = self
                    .file(source)
                    .ast()
                    .type_refs()
                    .get(type_ref.type_ref())
                    .map_err(TypeCheckingError::from)?
                    .span();
                if let Some(template) =
                    locate(source, span).and_then(|target| templates.get_mut(&target))
                {
                    template.types.insert(ty);
                }
            }
        }
        for (symbol, &ty) in &self.parts.symbol_types {
            let source = symbol.source_unit();
            if ranges.contains_key(&source) {
                let span = self.unit_symbol_span(*symbol)?;
                if let Some(template) =
                    locate(source, span).and_then(|target| templates.get_mut(&target))
                {
                    template.types.insert(ty);
                }
            }
        }
        for (index, call) in self.parts.calls.iter().enumerate() {
            let expr = call.expression();
            let source = expr.source_unit();
            if ranges.contains_key(&source) {
                let span = self
                    .file(source)
                    .ast()
                    .expressions()
                    .get(expr.expression())
                    .map_err(TypeCheckingError::from)?
                    .span();
                if let Some(template) =
                    locate(source, span).and_then(|target| templates.get_mut(&target))
                {
                    template.types.extend(call.instance().type_arguments());
                    template.types.insert(call.return_type());
                    template
                        .types
                        .extend(call.receiver().map(|receiver| receiver.ty()));
                    template
                        .types
                        .extend(call.arguments().iter().map(|arg| arg.parameter_type()));
                    template.calls.push(index);
                }
            }
        }
        for &read in display_reads {
            let source = read.operand.source_unit();
            if ranges.contains_key(&source) {
                let span = self
                    .file(source)
                    .ast()
                    .expressions()
                    .get(read.operand.expression())
                    .map_err(TypeCheckingError::from)?
                    .span();
                if let Some(template) =
                    locate(source, span).and_then(|target| templates.get_mut(&target))
                {
                    template.display_reads.push(read);
                }
            }
        }
        Ok(templates)
    }
}

fn body_call_instance(
    signatures: &mut CompilationUnitSignatures,
    dispatch: &StaticDispatchOwners,
    templates: &BTreeMap<UnitCallTarget, Template>,
    call: &UnitCallDescriptor,
    mut receiver: Option<UnitTypeId>,
    arguments: Vec<UnitTypeId>,
) -> Result<Option<Instance>, CompilationUnitTypeError> {
    let mut target = call.target();
    let mut arguments = arguments;
    if let Some((declared_receiver, requirement)) = dispatch.call(call) {
        let Some(actual) = receiver else {
            return Ok(None);
        };
        let contextual_dispatch = matches!(
            signatures.types().get(declared_receiver),
            Some(UnitTypeKind::TypeParameter(_) | UnitTypeKind::StaticSelf(_))
        );
        if contextual_dispatch
            && matches!(
                signatures.types().get(actual),
                Some(UnitTypeKind::TypeParameter(_) | UnitTypeKind::StaticSelf(_))
            )
        {
            // Defer selection until the enclosing body is instantiated.
            return Ok(None);
        }
        if !call
            .receiver()
            .is_some_and(|receiver| receiver.is_qualified_super())
        {
            let Some(route) = dispatch.body_route(
                signatures,
                actual,
                requirement,
                arguments,
                contextual_dispatch,
            )?
            else {
                return Ok(None);
            };
            target = route.target;
            arguments = route.arguments;
            receiver = Some(route.receiver);
        }
    }
    Ok(body_instance(templates, target, arguments, receiver))
}

fn display_body_instance(
    signatures: &mut CompilationUnitSignatures,
    dispatch: &StaticDispatchOwners,
    templates: &BTreeMap<UnitCallTarget, Template>,
    requirement: UnitCallableTarget,
    receiver: UnitTypeId,
) -> Result<Option<Instance>, CompilationUnitTypeError> {
    if !matches!(
        signatures.types().get(receiver),
        Some(UnitTypeKind::Nominal { .. })
    ) {
        return Ok(None);
    }
    Ok(dispatch
        .body_route(signatures, receiver, requirement, Vec::new(), false)?
        .and_then(|route| {
            body_instance(
                templates,
                route.target,
                route.arguments,
                Some(route.receiver),
            )
        }))
}

fn body_instance(
    templates: &BTreeMap<UnitCallTarget, Template>,
    target: UnitCallTarget,
    arguments: Vec<UnitTypeId>,
    receiver: Option<UnitTypeId>,
) -> Option<Instance> {
    let template = templates.get(&target)?;
    let static_self = if template.contextual {
        Some(receiver?)
    } else {
        None
    };
    Some(Instance {
        target,
        arguments,
        static_self,
    })
}
