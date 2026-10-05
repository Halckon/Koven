//! Finite canonical publication from committed ordinary source calls, without rechecking bodies.

use super::type_graph::{ConcreteTypes, substitute};
use super::*;
use crate::type_checking::{UnitCallDescriptor, UnitCallTarget};

type Instance = (DeclarationId, Vec<UnitTypeId>);

struct Template {
    parameters: Vec<UnitSymbolId>,
    types: BTreeSet<UnitTypeId>,
    calls: Vec<UnitCallDescriptor>,
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
        let mut concrete = ConcreteTypes::for_closed_arguments();
        let mut seeds = BTreeSet::new();
        for call in &self.parts.calls {
            let UnitCallTarget::Declaration(target) = call.target() else {
                continue;
            };
            if !templates.contains_key(&target) {
                continue;
            }
            let args = call.instance().type_arguments();
            let mut closed = true;
            for &arg in args {
                closed &= concrete.is_concrete(self.signatures.types(), arg)?;
            }
            if closed {
                seeds.insert((target, args.to_vec()));
            }
        }
        for seed in seeds {
            let mut pending = BTreeSet::from([seed]);
            let mut planned = BTreeSet::new();
            let mut count = 0;
            while let Some(key) = pending.pop_first() {
                if planned.contains(&key) {
                    continue;
                }
                if !key.1.is_empty() && count >= limit {
                    // The original planner owns budget/recipe arbitration. Publish only its finite frontier.
                    self.materialize_body_instance(&templates, &key)?;
                    for next in pending.difference(&planned) {
                        self.materialize_body_instance(&templates, next)?;
                    }
                    break;
                }
                let callees = self.materialize_body_instance(&templates, &key)?;
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
        Ok(())
    }

    fn materialize_body_instance(
        &mut self,
        templates: &BTreeMap<DeclarationId, Template>,
        key: &Instance,
    ) -> Result<Vec<Instance>, CompilationUnitTypeError> {
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
        let mut memo = BTreeMap::new();
        for &ty in &template.types {
            substitute(self.signatures.types_mut(), ty, &substitutions, &mut memo)?;
        }
        let mut callees = Vec::new();
        for call in &template.calls {
            let arguments = call
                .instance()
                .type_arguments()
                .iter()
                .map(|&ty| substitute(self.signatures.types_mut(), ty, &substitutions, &mut memo))
                .collect::<Result<Vec<_>, _>>()?;
            if let UnitCallTarget::Declaration(target) = call.target()
                && templates.contains_key(&target)
            {
                callees.push((target, arguments));
            }
        }
        Ok(callees)
    }

    fn generic_body_templates(
        &self,
    ) -> Result<BTreeMap<DeclarationId, Template>, CompilationUnitTypeError> {
        let mut templates = BTreeMap::new();
        for declaration in self.names.names().index().declarations() {
            let Some(signature) = self
                .signatures
                .declaration(declaration.id())
                .and_then(|s| s.callable())
            else {
                continue;
            };
            let file = self.file(declaration.source_unit());
            let span = file
                .ast()
                .items()
                .get(declaration.root())
                .map_err(TypeCheckingError::from)?
                .span();
            let contains = |candidate: Span| {
                candidate.source_id() == span.source_id()
                    && span.start() <= candidate.start()
                    && candidate.end() <= span.end()
            };
            let mut types = signature
                .parameters()
                .iter()
                .map(|p| p.ty())
                .chain(std::iter::once(signature.return_type()))
                .collect::<BTreeSet<_>>();
            for (expression, &ty) in &self.parts.expression_types {
                if expression.source_unit() == declaration.source_unit()
                    && contains(
                        file.ast()
                            .expressions()
                            .get(expression.expression())
                            .map_err(TypeCheckingError::from)?
                            .span(),
                    )
                {
                    types.insert(ty);
                }
            }
            for (type_ref, &ty) in &self.parts.type_ref_types {
                if type_ref.source_unit() == declaration.source_unit()
                    && contains(
                        file.ast()
                            .type_refs()
                            .get(type_ref.type_ref())
                            .map_err(TypeCheckingError::from)?
                            .span(),
                    )
                {
                    types.insert(ty);
                }
            }
            for (symbol, &ty) in &self.parts.symbol_types {
                if symbol.source_unit() == declaration.source_unit() {
                    let table = self.names.names().source_units()[symbol.source_unit().index()]
                        .resolution();
                    if contains(
                        table
                            .symbols()
                            .get(symbol.symbol().index())
                            .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?
                            .span(),
                    ) {
                        types.insert(ty);
                    }
                }
            }
            let mut calls = Vec::new();
            for call in &self.parts.calls {
                let expr = call.expression();
                if expr.source_unit() == declaration.source_unit()
                    && contains(
                        file.ast()
                            .expressions()
                            .get(expr.expression())
                            .map_err(TypeCheckingError::from)?
                            .span(),
                    )
                {
                    types.extend(call.instance().type_arguments());
                    types.insert(call.return_type());
                    types.extend(call.receiver().map(|receiver| receiver.ty()));
                    types.extend(
                        call.arguments()
                            .iter()
                            .map(|argument| argument.parameter_type()),
                    );
                    calls.push(call.clone());
                }
            }
            templates.insert(
                declaration.id(),
                Template {
                    parameters: signature.type_parameters().to_vec(),
                    types,
                    calls,
                },
            );
        }
        Ok(templates)
    }
}
