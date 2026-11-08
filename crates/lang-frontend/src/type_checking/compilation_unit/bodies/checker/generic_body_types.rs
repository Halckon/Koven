//! Finite canonical publication from committed ordinary source calls, without rechecking bodies.

use super::dispatch_owner_types::StaticDispatchOwners;
use super::type_graph::{ConcreteTypes, substitute};
use super::*;
use crate::parser::NameMarker;
use crate::type_checking::{IntrinsicTypeConstructor, UnitCallTarget, UnitCallableTarget};

type Instance = (UnitCallTarget, Vec<UnitTypeId>);

struct Template {
    parameters: Vec<UnitSymbolId>,
    types: BTreeSet<UnitTypeId>,
    calls: Vec<usize>,
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
        if self
            .signatures
            .diagnostics()
            .iter()
            .chain(&self.diagnostics)
            .any(|diagnostic| diagnostic.severity() == Severity::Error)
        {
            return Ok(());
        }
        let templates = self.generic_body_templates()?;
        let mut dispatch = StaticDispatchOwners::new(&self.signatures);
        let mut concrete = ConcreteTypes::for_closed_arguments();
        let mut seeds = BTreeMap::new();
        for call in &self.parts.calls {
            if let Some((receiver, target)) = dispatch.call(call) {
                dispatch.record(self.signatures.types(), receiver, target);
            }
            let target = call.target();
            if !templates.contains_key(&target) {
                continue;
            }
            let args = call.instance().type_arguments();
            let mut closed = true;
            for &arg in args {
                closed &= concrete.is_concrete(self.signatures.types(), arg)?;
            }
            if closed {
                let expression = call.expression();
                let span = self
                    .file(expression.source_unit())
                    .ast()
                    .expressions()
                    .get(expression.expression())
                    .map_err(TypeCheckingError::from)?
                    .span();
                seeds.entry((target, args.to_vec())).or_insert(span);
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
                if !key.1.is_empty() && count >= limit {
                    // The original planner owns budget/recipe arbitration. Publish only its finite frontier.
                    if self
                        .materialize_body_instance(&templates, &key, call_span, &mut dispatch)?
                        .is_some()
                    {
                        for next in pending.difference(&planned) {
                            if self
                                .materialize_body_instance(
                                    &templates,
                                    next,
                                    call_span,
                                    &mut dispatch,
                                )?
                                .is_none()
                            {
                                break;
                            }
                        }
                    }
                    break;
                }
                let Some(callees) =
                    self.materialize_body_instance(&templates, &key, call_span, &mut dispatch)?
                else {
                    break;
                };
                for callee in callees {
                    let mut closed = true;
                    for &arg in &callee.1 {
                        closed &= concrete.is_concrete(self.signatures.types(), arg)?;
                    }
                    if closed && !planned.contains(&callee) {
                        pending.insert(callee);
                    }
                }
                if !key.1.is_empty() {
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
    ) -> Result<Option<Vec<Instance>>, CompilationUnitTypeError> {
        let template = templates
            .get(&key.0)
            .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?;
        if template.parameters.len() != key.1.len() {
            return Err(CompilationUnitTypeError::MissingDeclarationSymbol);
        }
        let substitutions = template
            .parameters
            .iter()
            .copied()
            .zip(key.1.iter().copied())
            .collect();
        let checkpoint = self.signatures.types().len();
        let mut memo = BTreeMap::new();
        for &ty in &template.types {
            substitute(self.signatures.types_mut(), ty, &substitutions, &mut memo)?;
        }
        let mut callees = Vec::new();
        for &index in &template.calls {
            let call = &self.parts.calls[index];
            let arguments = call
                .instance()
                .type_arguments()
                .iter()
                .map(|&ty| substitute(self.signatures.types_mut(), ty, &substitutions, &mut memo))
                .collect::<Result<Vec<_>, _>>()?;
            if templates.contains_key(&call.target()) {
                callees.push((call.target(), arguments));
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

    fn generic_body_templates(
        &self,
    ) -> Result<BTreeMap<UnitCallTarget, Template>, CompilationUnitTypeError> {
        let mut definitions = BTreeMap::<SourceUnitId, BTreeMap<usize, _>>::new();
        for declaration in self.names.index().declarations() {
            let Some(signature) = self.signatures.declaration(declaration.id()) else {
                continue;
            };
            let mut register = |callable: &UnitCallableSignature, owner: &[UnitSymbolId]| {
                if owner.is_empty() && callable.type_parameters().is_empty() {
                    return;
                }
                let target = match callable.target() {
                    UnitCallableTarget::Declaration(id) => UnitCallTarget::Declaration(id),
                    UnitCallableTarget::Symbol(id) => UnitCallTarget::Symbol(id),
                };
                let template = Template {
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
        Ok(templates)
    }
}
