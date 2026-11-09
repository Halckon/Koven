//! Overload applicability and contextual lambda trials.

use std::collections::BTreeMap;

use crate::{
    ast::{ExpressionId, TypeRefId},
    diagnostic::Severity,
    name_resolution::SourceUnitId,
    parser::CallArgument,
    source::Span,
    type_checking::{CompilationUnitTypeError, DeferredReason, UnitExpressionId, UnitTypeId},
};

use super::super::type_graph::CandidateTypes;
use super::{
    ArgumentCheckStart, BodyChecker, CallCandidate, ExpressionCheck,
    generic::{InstantiatedCandidate, UnitInstantiationFailure},
};

#[derive(Clone, Copy)]
pub(super) struct CallSite<'a> {
    pub(super) source: SourceUnitId,
    pub(super) expression: ExpressionId,
    pub(super) callee: ExpressionId,
    pub(super) callee_span: Span,
    pub(super) arguments: &'a [CallArgument],
    pub(super) return_type: UnitTypeId,
    pub(super) explicit: (&'a [TypeRefId], &'a [UnitTypeId]),
    pub(super) explicit_storage_failures: &'a BTreeMap<UnitTypeId, Span>,
    pub(super) argument_start: ArgumentCheckStart,
}

type MappedCandidate = (CallCandidate, Vec<usize>);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Applicability {
    Applicable,
    Pending,
    Rejected,
}

impl CallSite<'_> {
    fn instantiate(
        &self,
        checker: &mut BodyChecker<'_>,
        candidate: CallCandidate,
        mapping: &[usize],
        inference_types: &[Option<UnitTypeId>],
    ) -> Result<Result<InstantiatedCandidate, UnitInstantiationFailure>, CompilationUnitTypeError>
    {
        checker.instantiate_source_candidate(
            self.source,
            candidate,
            mapping,
            self.arguments,
            inference_types,
            self.explicit,
            self.callee_span,
        )
    }

    fn probe(
        &self,
        checker: &mut BodyChecker<'_>,
        (template, mapping): &MappedCandidate,
        inference_types: &[Option<UnitTypeId>],
        actuals: Option<&[Option<UnitTypeId>]>,
    ) -> Result<Applicability, CompilationUnitTypeError> {
        // Instantiation only interns types. No candidate-local ID may escape this probe.
        let checkpoint = checker.signatures.types().len();
        let result = self
            .instantiate(checker, template.clone(), mapping, inference_types)
            .and_then(|result| {
                let candidate = match result {
                    Ok(candidate) => candidate,
                    Err(_) => return Ok(Applicability::Rejected),
                };
                let mut pending = candidate.pending;
                if let Some(actuals) = actuals {
                    for ((actual, &parameter), argument) in
                        actuals.iter().zip(mapping).zip(self.arguments)
                    {
                        let Some(actual) = *actual else { continue };
                        let unresolved = checker.contains_unresolved_call_type(actual);
                        pending |= unresolved || checker.is_deferred(actual);
                        let required = checker
                            .contextual_argument_requirement(self.source, argument.value)?
                            .unwrap_or(actual);
                        if (!checker.is_deferred(required)
                            || checker.has_contextual_integer(required))
                            && !checker.call_argument_matches_mode(
                                UnitExpressionId::new(self.source, argument.value),
                                required,
                                candidate.call.parameters[parameter].ty,
                                candidate.call.parameters[parameter].mode,
                                candidate.pending || unresolved,
                            )
                        {
                            return Ok(Applicability::Rejected);
                        }
                    }
                    if let Some(constraints) = &candidate.constraints
                        && constraints
                            .first_conflict(
                                checker,
                                None,
                                self.source,
                                self.arguments,
                                mapping,
                                actuals,
                            )?
                            .is_some()
                    {
                        return Ok(Applicability::Rejected);
                    }
                }
                Ok(if pending {
                    Applicability::Pending
                } else {
                    Applicability::Applicable
                })
            });
        checker.signatures.types_mut().discard_after(checkpoint);
        result
    }

    fn finish(
        &self,
        checker: &mut BodyChecker<'_>,
        candidate: InstantiatedCandidate,
        mapping: &[usize],
        prechecked: Option<&[Option<UnitTypeId>]>,
        checkpoint: usize,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        let candidate_types = CandidateTypes::detach(checker.signatures.types_mut(), checkpoint);
        checker.finish_candidate(
            self.source,
            self.expression,
            self.callee,
            self.arguments,
            self.return_type,
            candidate.call,
            mapping,
            prechecked,
            candidate_types,
            self.argument_start,
            candidate.pending,
            candidate.constraints,
        )
    }

    fn finish_selected(
        &self,
        checker: &mut BodyChecker<'_>,
        (template, mapping): MappedCandidate,
        inference_types: &[Option<UnitTypeId>],
        prechecked: Option<&[Option<UnitTypeId>]>,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        let checkpoint = checker.signatures.types().len();
        match self.instantiate(checker, template, &mapping, inference_types)? {
            Ok(candidate) => self.finish(checker, candidate, &mapping, prechecked, checkpoint),
            Err(failure) => {
                checker.emit_unit_instantiation_failure(failure)?;
                Ok(ExpressionCheck {
                    ty: checker.error_type(),
                    falls_through: true,
                })
            }
        }
    }
}

impl BodyChecker<'_> {
    pub(super) fn select_mapped_call(
        &mut self,
        site: CallSite<'_>,
        initial_candidate_count: usize,
        mut mapped: Vec<MappedCandidate>,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        if initial_candidate_count > 1
            && site.explicit.1.iter().any(|&ty| self.is_error(ty))
            && mapped
                .iter()
                .any(|(candidate, _)| candidate.type_parameters.len() == site.explicit.1.len())
        {
            return self.reject_after_argument_precheck(&site);
        }
        let needs_inference = site.explicit.1.is_empty()
            && mapped
                .iter()
                .any(|(candidate, _)| !candidate.type_parameters.is_empty());
        let inference_types = if needs_inference {
            self.check_generic_inference_arguments(
                site.source,
                site.arguments,
                None,
                site.return_type,
            )?
        } else {
            vec![None; site.arguments.len()]
        };
        let prechecked = needs_inference.then_some(inference_types.as_slice());
        if initial_candidate_count > 1
            && let Some(types) = prechecked
            && self.call_argument_precheck_failed(&site, types)
        {
            return self.reject_after_argument_precheck(&site);
        }

        // A unique mapped candidate keeps the existing contextual argument check and diagnostics.
        if mapped.len() == 1 {
            let (template, mapping) = mapped.pop().expect("one mapped candidate");
            let checkpoint = self.signatures.types().len();
            return match site.instantiate(self, template, &mapping, &inference_types)? {
                Ok(candidate) => site.finish(self, candidate, &mapping, prechecked, checkpoint),
                Err(failure) if initial_candidate_count == 1 => {
                    let mut uncommitted = false;
                    if matches!(failure, UnitInstantiationFailure::Arity { .. }) {
                        self.with_explicit_storage_recovery(
                            site.explicit_storage_failures,
                            |checker| {
                                checker.check_call_arguments_without_expected(
                                    site.source,
                                    site.arguments,
                                    site.return_type,
                                )
                            },
                        )?;
                    } else {
                        uncommitted = self.recover_untyped_call_arguments(&site)?;
                    }
                    self.emit_unit_instantiation_failure(failure)?;
                    if uncommitted {
                        return Ok(self.rejected_call_recovery(site.source, site.expression));
                    }
                    Ok(ExpressionCheck {
                        ty: self.error_type(),
                        falls_through: true,
                    })
                }
                Err(_) => self.reject_overload_before_context(&site),
            };
        }

        let mut eligible = Vec::with_capacity(mapped.len());
        for candidate in mapped {
            if site.probe(self, &candidate, &inference_types, None)? != Applicability::Rejected {
                eligible.push(candidate);
            }
        }
        match eligible.len() {
            0 => return self.reject_overload_before_context(&site),
            1 => {
                let candidate = eligible.pop().expect("one eligible candidate");
                return site.finish_selected(self, candidate, &inference_types, prechecked);
            }
            _ => {}
        }

        let checked;
        let argument_types = if let Some(prechecked) = prechecked {
            prechecked
        } else {
            let expected = site
                .explicit
                .1
                .iter()
                .any(|&ty| self.contains_unresolved_call_type(ty))
                .then(|| self.deferred_type(DeferredReason::ContextualArgument));
            checked = self.check_generic_inference_arguments(
                site.source,
                site.arguments,
                expected,
                site.return_type,
            )?;
            &checked
        };
        if self.call_argument_precheck_failed(&site, argument_types) {
            return self.reject_after_argument_precheck(&site);
        }
        let mut applicable = Vec::new();
        let mut pending = false;
        for candidate in eligible {
            match site.probe(self, &candidate, &inference_types, Some(argument_types))? {
                Applicability::Applicable => applicable.push(candidate),
                Applicability::Pending => {
                    pending = true;
                    applicable.push(candidate);
                }
                Applicability::Rejected => {}
            }
        }
        match applicable.len() {
            0 => self.reject_overload_before_context(&site),
            1 => {
                let candidate = applicable.pop().expect("one applicable candidate");
                site.finish_selected(self, candidate, &inference_types, Some(argument_types))
            }
            _ if pending && argument_types.iter().all(Option::is_some) => {
                Ok(self.unresolved_call())
            }
            _ if argument_types.iter().all(Option::is_some) => {
                self.overload_failure(site.callee_span, false)
            }
            _ => self.select_lambda_overload(
                &site,
                applicable,
                &inference_types,
                argument_types,
                pending,
            ),
        }
    }

    fn call_argument_precheck_failed(
        &self,
        site: &CallSite<'_>,
        types: &[Option<UnitTypeId>],
    ) -> bool {
        types.iter().flatten().any(|&ty| self.is_error(ty))
            || self.has_argument_errors_since(site.argument_start)
    }

    fn reject_after_argument_precheck(
        &mut self,
        site: &CallSite<'_>,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        if self.recover_untyped_call_arguments(site)? {
            return Ok(self.rejected_call_recovery(site.source, site.expression));
        }
        Ok(ExpressionCheck {
            ty: self.error_type(),
            falls_through: true,
        })
    }

    fn reject_overload_before_context(
        &mut self,
        site: &CallSite<'_>,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        let uncommitted = self.recover_untyped_call_arguments(site)?;
        let failure = self.overload_failure(site.callee_span, true)?;
        if uncommitted {
            Ok(self.rejected_call_recovery(site.source, site.expression))
        } else {
            Ok(failure)
        }
    }

    fn recover_untyped_call_arguments(
        &mut self,
        site: &CallSite<'_>,
    ) -> Result<bool, CompilationUnitTypeError> {
        self.with_explicit_storage_recovery(site.explicit_storage_failures, |checker| {
            checker.check_untyped_call_arguments(site)
        })
    }

    fn check_untyped_call_arguments(
        &mut self,
        site: &CallSite<'_>,
    ) -> Result<bool, CompilationUnitTypeError> {
        let mut uncommitted = false;
        let track_arguments = self.tracks_pending_call_flow();
        let mut ordinary = true;
        let mut arguments_exit = true;
        for argument in site.arguments {
            let key = crate::type_checking::UnitExpressionId::new(site.source, argument.value);
            if !self.parts.expression_types.contains_key(&key) {
                if self.is_lambda_syntax(site.source, argument.value) {
                    uncommitted = true;
                } else {
                    let context = self.deferred_type(DeferredReason::Call);
                    self.check_loop_paths(ordinary, arguments_exit, |checker| {
                        checker.check_expression(
                            site.source,
                            argument.value,
                            Some(context),
                            None,
                            site.return_type,
                        )
                    })?;
                }
            }
            ordinary &= self.operand_falls_through(site.source, argument.value);
            if track_arguments && arguments_exit {
                arguments_exit = self.checked_call_operand_exits(site.source, argument.value)?;
            }
        }
        Ok(uncommitted)
    }

    fn select_lambda_overload(
        &mut self,
        site: &CallSite<'_>,
        candidates: Vec<MappedCandidate>,
        inference_types: &[Option<UnitTypeId>],
        argument_types: &[Option<UnitTypeId>],
        pending: bool,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        let baseline = self.trial_state();
        let baseline_diagnostics = self.diagnostics.len();
        let baseline_deferred = self.deferred_facts.len();
        let precheck_deferred =
            self.has_unresolved_argument_facts_since(site.argument_start.deferred_facts);
        // Independently checked arguments can prevent publication, but their
        // hidden deferred facts do not change a trial's contextual applicability.
        let mut trial_site = *site;
        trial_site.argument_start.deferred_facts = baseline_deferred;
        let mut successes = Vec::new();
        let mut has_deferred_trial = false;
        let mut shared_failure = None;
        for candidate in candidates {
            self.restore_trial_state(baseline.clone());
            let declaration_span = candidate.0.declaration_span;
            let retry = precheck_deferred.then(|| candidate.clone());
            // Instantiate inside the same transaction as contextual lambda facts.
            let result = self.check_lambda_candidate(
                &trial_site,
                candidate,
                inference_types,
                argument_types,
            );
            let result = match result {
                Ok(result) => result,
                Err(error) => {
                    self.restore_trial_state(baseline);
                    return Err(error);
                }
            };
            let trial_deferred =
                self.is_deferred(result.ty) || self.has_deferred_facts_since(baseline_deferred);
            let trial_has_errors = self.diagnostics[baseline_diagnostics..]
                .iter()
                .any(|diagnostic| diagnostic.severity() == Severity::Error);
            if shared_failure.is_none() && trial_has_errors {
                shared_failure = self.repeated_storage_failure_since(baseline_diagnostics);
            }
            // 已由错误淘汰的候选可能留下 recovery Deferred，不能否决另一完整候选。
            // 无错误的未定候选仍保留保守门禁，防止过早提交 intrinsic facts。
            has_deferred_trial |= trial_deferred && !trial_has_errors;
            if !trial_has_errors
                && self.diagnostics.len() == baseline_diagnostics
                && !self.is_error(result.ty)
                && !trial_deferred
            {
                successes.push((self.trial_state(), result, declaration_span, retry));
            }
        }
        self.restore_trial_state(baseline);
        if has_deferred_trial {
            return Ok(ExpressionCheck {
                ty: self.deferred_type(DeferredReason::Call),
                falls_through: true,
            });
        }
        if successes.is_empty()
            && let Some((diagnostic, origin)) = shared_failure
        {
            // The same explicit storage error already explains this failed
            // trial. Its lambda facts were rolled back with the other candidates.
            let index = self.diagnostics.len();
            self.diagnostics.push(diagnostic);
            self.note_repeated_storage_error(
                crate::type_checking::UnitExpressionId::new(site.source, site.expression),
                index,
                origin,
            );
            return Ok(self.rejected_call_recovery(site.source, site.expression));
        }
        let failure = match successes.len() {
            0 => self.overload_failure(site.callee_span, true),
            1 => {
                let (state, result, _, retry) =
                    successes.pop().expect("one successful candidate trial");
                if let Some(candidate) = retry {
                    return self.check_lambda_candidate(
                        site,
                        candidate,
                        inference_types,
                        argument_types,
                    );
                }
                self.restore_trial_state(state);
                return Ok(result);
            }
            _ => {
                let declarations = successes
                    .iter()
                    .filter_map(|(_, _, span, _)| *span)
                    .take(2)
                    .collect::<Vec<_>>();
                self.ambiguous_overload(site.callee_span, &declarations)
            }
        }?;
        let callee = self
            .file(site.source)
            .ast()
            .expressions()
            .get(site.callee)
            .map_err(crate::type_checking::TypeCheckingError::from)?;
        if pending || !matches!(callee.payload(), crate::parser::Expression::Name) {
            // Named overload errors have an existing final-admission proof.
            // Other selectors need the unresolved anchor for rolled-back lambdas.
            Ok(self.rejected_call_recovery(site.source, site.expression))
        } else {
            Ok(failure)
        }
    }

    fn check_lambda_candidate(
        &mut self,
        site: &CallSite<'_>,
        candidate: MappedCandidate,
        inference_types: &[Option<UnitTypeId>],
        argument_types: &[Option<UnitTypeId>],
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        let previous = self.candidate_local_expected;
        self.candidate_local_expected = true;
        let result = site.finish_selected(self, candidate, inference_types, Some(argument_types));
        self.candidate_local_expected = previous;
        result
    }
}
