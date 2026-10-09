use super::{Checker, ExpressionUse, Flows, OwnershipCheckingError, State};
use crate::ownership_checking::{
    UnitLoanTarget as LoanTarget,
    borrow_result::{BorrowReturnOriginFact, ReturnSource, marker_span, origin_expression},
};
use crate::{
    ast::ExpressionId,
    diagnostic::{Diagnostic, Severity, codes},
    parser::{BorrowReturnSource, FunctionForm, NameMarker, ParameterModeMarker, ValueParameter},
};
impl Checker<'_> {
    pub(super) fn borrow_return_source(
        &self,
        form: FunctionForm,
        parameters: &[ValueParameter],
    ) -> Option<ReturnSource<crate::name_resolution::UnitSymbolId>> {
        let FunctionForm::Explicit {
            borrow_return: Some(syntax),
            ..
        } = form
        else {
            return None;
        };
        let (symbol, span) = match syntax.source {
            BorrowReturnSource::Parameter(NameMarker::Present(span)) => {
                let parameter = parameters.iter().find(|parameter| {
                    self.sources.slice(marker_span(parameter.name)).ok()
                        == self.sources.slice(span).ok()
                });
                let symbol = parameter
                    .filter(|p| {
                        !matches!(
                            p.mode_marker,
                            Some(ParameterModeMarker::Inout(_) | ParameterModeMarker::Own(_))
                        )
                    })
                    .and_then(|p| self.marker_symbol(p.name).copied());
                (symbol, parameter.map_or(span, |p| marker_span(p.name)))
            }
            BorrowReturnSource::Parameter(NameMarker::Missing(span) | NameMarker::Error(span))
            | BorrowReturnSource::Receiver(span) => (None, span),
        };
        Some(ReturnSource {
            symbol,
            span,
            marker: syntax.borrow_span,
        })
    }

    pub(super) fn check_borrow_call_use(
        &mut self,
        expression: ExpressionId,
        owned: bool,
    ) -> Result<(), OwnershipCheckingError> {
        let Some(contract) = self
            .typed
            .calls()
            .iter()
            .find(|call| call.expression() == self.unit_expression(expression))
            .and_then(|call| call.borrow_return())
        else {
            return Ok(());
        };
        let (code, message) = if owned {
            (
                codes::BORROW_RESULT_ESCAPE,
                "borrow result cannot be delivered to an owned destination",
            )
        } else {
            (
                codes::UNSUPPORTED_BORROW_FLOW,
                "caller borrow result continuation is not yet proven",
            )
        };
        let mut diagnostic = Diagnostic::new(
            self.sources,
            Severity::Error,
            codes::catalog()?.resolve(code)?,
            message,
            self.parsed.ast().expressions().get(expression)?.span(),
        )?;
        diagnostic.add_label(
            self.sources,
            contract.marker_span(),
            "borrow result declared here",
        )?;
        self.diagnostics.push(diagnostic);
        Ok(())
    }

    pub(super) fn check_borrow_return(
        &mut self,
        expression: ExpressionId,
        state: State,
    ) -> Result<Flows, OwnershipCheckingError> {
        let Some(source) = self.current_borrow_return else {
            return self.check_escaping_expression(
                expression,
                state,
                ExpressionUse::Consume {
                    parameter_span: None,
                },
            );
        };
        let mut value = expression;
        while let crate::parser::Expression::Group { expression: inner } =
            self.parsed.ast().expressions().get(value)?.payload()
        {
            value = *inner;
        }
        if matches!(
            self.parsed.ast().expressions().get(value)?.payload(),
            crate::parser::Expression::If { .. }
                | crate::parser::Expression::When { .. }
                | crate::parser::Expression::Call { .. }
        ) {
            self.diagnostics.push(Diagnostic::new(
                self.sources,
                Severity::Error,
                codes::catalog()?.resolve(codes::UNSUPPORTED_BORROW_FLOW)?,
                "borrow return control-flow or call continuation is not yet proven",
                self.parsed.ast().expressions().get(expression)?.span(),
            )?);
            return Ok(Flows::next(state));
        }
        let before = self.diagnostics.len();
        let flows = self.check_expression(expression, state, ExpressionUse::Read)?;
        if flows.next.is_none() {
            return Ok(flows);
        }
        let Some(symbol) = source.symbol else {
            self.diagnostics.push(Diagnostic::new(
                self.sources,
                Severity::Error,
                codes::catalog()?.resolve(codes::UNSUPPORTED_BORROW_FLOW)?,
                "borrow result source requires an unsupported receiver or inout continuation",
                source.marker,
            )?);
            return Ok(flows);
        };
        let origin = origin_expression(self.parsed, expression)?;
        let place = origin
            .map(|origin| self.place(origin))
            .transpose()?
            .flatten();
        if let Some(place) = place.filter(|place| place.root() == symbol) {
            if self.diagnostics.len() == before {
                self.borrow_return_origins.push(BorrowReturnOriginFact::new(
                    self.unit_expression(expression),
                    LoanTarget::Place(place),
                    source.marker,
                ));
            }
        } else {
            let mut diagnostic = Diagnostic::new(
                self.sources,
                Severity::Error,
                codes::catalog()?.resolve(codes::INVALID_BORROW_CONTRACT)?,
                "returned borrow does not originate from the declared source",
                self.parsed.ast().expressions().get(expression)?.span(),
            )?;
            diagnostic.add_label(
                self.sources,
                source.span,
                "required borrow source declared here",
            )?;
            self.diagnostics.push(diagnostic);
        }
        Ok(flows)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        diagnostic::DiagnosticDetail,
        lexer::lex,
        name_resolution::{
            SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names,
        },
        ownership_checking::{
            CompilationUnitOwnership, UnitLoanTarget, compilation_unit::analysis,
        },
        parser::parse_file,
        source::SourceMap,
        type_checking::{check_compilation_unit_types, standard_environments},
    };

    // Exercise the private producer with recovery types. Public type validation stays closed.
    fn checked(text: &str) -> (SourceMap, CompilationUnitOwnership) {
        let mut sources = SourceMap::new();
        let source = sources.add_source("origin.ko", text).unwrap();
        let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
        assert!(
            parsed.diagnostics().is_empty(),
            "{text}: {:?}",
            parsed.diagnostics()
        );
        let inputs = [SourceUnitInput::new("root", "origin.ko", source, &parsed)];
        let (ne, te) = standard_environments();
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &ne)
            .unwrap()
            .validate()
            .unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &te).unwrap();
        assert!(!typed.diagnostics().is_empty());
        assert!(
            typed
                .diagnostics()
                .iter()
                .all(|d| d.code().to_string() == "L0164"),
            "{text}: {:?}",
            typed.diagnostics()
        );
        assert!(typed.clone().validate().is_err());
        let owned = analysis::analyze(&sources, &inputs, &names, &te, &typed, false).unwrap();
        for fact in owned.borrow_return_origins() {
            let expression = parsed
                .ast()
                .expressions()
                .get(fact.expression().expression())
                .unwrap();
            let actual = sources.slice(expression.span()).unwrap();
            assert!(
                matches!(actual, "source" | "(source)" | "source.text"),
                "{actual}"
            );
        }
        (sources, owned)
    }

    #[test]
    fn stable_generic_nullable_move_only_and_projected_roots_are_proven() {
        for (text, fields, declaration) in [
            (
                "fun <T> view(source: T): borrow T from source = source",
                0,
                "source: T",
            ),
            (
                "fun view(source: String?): borrow String? from source = (source)",
                0,
                "source: String?",
            ),
            (
                "class Record(val text: String)\nfun view(source: Record): borrow Record from source = source",
                0,
                "source: Record",
            ),
            (
                "class Record(val text: String)\nfun view(source: Record): borrow String from source = source.text",
                1,
                "source: Record",
            ),
            (
                "fun view(aux: String, source: String): borrow String from source = source",
                0,
                "source: String",
            ),
            (
                "fun view(source: String): borrow String from source { return source }",
                0,
                "source: String",
            ),
        ] {
            let (sources, owned) = checked(text);
            assert!(
                owned.diagnostics().is_empty(),
                "{text}: {:?}",
                owned.diagnostics()
            );
            assert!(owned.deferred().is_empty());
            assert_eq!(owned.borrow_return_origins().len(), 1);
            let fact = &owned.borrow_return_origins()[0];
            let UnitLoanTarget::Place(place) = fact.origin() else {
                panic!("stable place required");
            };
            assert_eq!(place.root().source_unit(), fact.expression().source_unit());
            let binding = owned
                .bindings()
                .iter()
                .find(|binding| binding.symbol() == place.root())
                .unwrap();
            assert_eq!(
                sources.slice(binding.declaration_span()).unwrap(),
                declaration
            );
            assert_eq!(place.fields().len(), fields);
            assert_eq!(sources.slice(fact.declaration_span()).unwrap(), "borrow");
            assert!(owned.drops().is_empty());
            assert!(owned.value_deliveries().is_empty());
        }
    }

    #[test]
    fn wrong_same_type_roots_local_and_temporary_values_are_rejected_atomically() {
        for (text, primary) in [
            (
                "fun view(source: String, other: String): borrow String from source = other",
                "other",
            ),
            (
                "fun view(source: String): borrow String from source = \"temporary\"",
                "\"temporary\"",
            ),
            (
                "fun view(source: String): borrow String from source { val local = \"local\"; return local }",
                "local",
            ),
            (
                "class Record(val text: String)\nfun view(source: Record, other: Record): borrow String from source = other.text",
                "other.text",
            ),
        ] {
            let (sources, owned) = checked(text);
            let error = owned
                .diagnostics()
                .iter()
                .find(|d| d.code().to_string() == "L0162")
                .unwrap();
            assert_eq!(sources.slice(error.primary_span()).unwrap(), primary);
            assert!(error.details().iter().any(|detail| matches!(detail, DiagnosticDetail::Label(label) if sources.slice(label.span()).unwrap() == "source")));
            assert!(owned.borrow_return_origins().is_empty());
            assert!(owned.loans().is_empty());
            assert!(owned.drops().is_empty());
        }
        let (_, owned) = checked(
            "fun valid(source: String): borrow String from source = source\nfun invalid(source: String, other: String): borrow String from source = other",
        );
        assert!(
            owned
                .diagnostics()
                .iter()
                .any(|d| d.code().to_string() == "L0162")
        );
        assert!(owned.borrow_return_origins().is_empty());
    }

    #[test]
    fn missing_continuation_control_inout_and_bodyless_proofs_stay_closed() {
        for text in [
            "fun view(source: String): borrow String from source",
            "fun view(inout source: String): borrow String from source = source",
            "fun view(source: String, flag: Boolean): borrow String from source = (if (flag) { source } else { source })",
            "fun view(source: String): borrow String from source = source\nfun wrap(source: String): borrow String from source = view(source)",
            "fun view(source: String): borrow String from source = source\nfun run(source: String) { view(source) }",
        ] {
            let (_, owned) = checked(text);
            assert!(
                owned
                    .diagnostics()
                    .iter()
                    .any(|d| d.code().to_string() == "L0164"),
                "{text}: {:?}",
                owned.diagnostics()
            );
            assert!(owned.borrow_return_origins().is_empty());
        }
    }

    #[test]
    fn owned_delivery_is_rejected_and_lambda_return_has_its_own_context() {
        let (_, owned) = checked(
            "fun view(source: String): borrow String from source = source\nfun run(source: String) { val item = view(source) }",
        );
        assert!(
            owned
                .diagnostics()
                .iter()
                .any(|d| d.code().to_string() == "L0163")
        );
        assert!(owned.borrow_return_origins().is_empty());
        let (_, owned) = checked(
            "fun view(source: String): borrow String from source { val local = { \"owned lambda value\" }; return source }",
        );
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert_eq!(owned.borrow_return_origins().len(), 1);
    }

    #[test]
    fn overlapping_local_ids_keep_real_source_qualification_and_canonical_order() {
        let mut sources = SourceMap::new();
        // Physical source order intentionally differs from canonical source-unit order.
        let b = sources
            .add_source(
                "beta.ko",
                "package beta\nfun view(source: String): borrow String from source = source",
            )
            .unwrap();
        let a = sources
            .add_source(
                "alpha.ko",
                "package alpha\nfun view(source: String): borrow String from source = source",
            )
            .unwrap();
        let pa = parse_file(&sources, &lex(&sources, a).unwrap()).unwrap();
        let pb = parse_file(&sources, &lex(&sources, b).unwrap()).unwrap();
        assert!(pa.diagnostics().is_empty() && pb.diagnostics().is_empty());
        let inputs = [
            SourceUnitInput::new("root", "beta/b.ko", b, &pb),
            SourceUnitInput::new("root", "alpha/a.ko", a, &pa),
        ];
        let (ne, te) = standard_environments();
        let mut previous = None;
        for inputs in [inputs, [inputs[1], inputs[0]]] {
            let index = index_compilation_unit(&sources, &inputs).unwrap();
            let names = resolve_compilation_unit_names(&sources, &inputs, &index, &ne)
                .unwrap()
                .validate()
                .unwrap();
            let typed = check_compilation_unit_types(&sources, &inputs, &names, &te).unwrap();
            assert_eq!(typed.diagnostics().len(), 2);
            assert!(
                typed
                    .diagnostics()
                    .iter()
                    .all(|d| d.code().to_string() == "L0164")
            );
            assert!(typed.clone().validate().is_err());
            let owned = analysis::analyze(&sources, &inputs, &names, &te, &typed, false).unwrap();
            assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
            let facts = owned.borrow_return_origins();
            assert_eq!(facts.len(), 2);
            let roots: Vec<_> = facts
                .iter()
                .map(|fact| {
                    let UnitLoanTarget::Place(place) = fact.origin() else {
                        panic!("stable place required");
                    };
                    assert_eq!(place.root().source_unit(), fact.expression().source_unit());
                    place.root()
                })
                .collect();
            assert_eq!(roots[0].symbol(), roots[1].symbol());
            assert_ne!(roots[0], roots[1]);
            assert_eq!(
                facts[0].expression().expression(),
                facts[1].expression().expression()
            );
            assert_ne!(facts[0].expression(), facts[1].expression());
            assert_eq!(
                sources
                    .slice(
                        pa.ast()
                            .expressions()
                            .get(facts[0].expression().expression())
                            .unwrap()
                            .span()
                    )
                    .unwrap(),
                "source"
            );
            assert_eq!(
                sources
                    .slice(
                        pb.ast()
                            .expressions()
                            .get(facts[1].expression().expression())
                            .unwrap()
                            .span()
                    )
                    .unwrap(),
                "source"
            );
            assert_eq!(facts[0].declaration_span().source_id(), a);
            assert_eq!(facts[1].declaration_span().source_id(), b);
            if let Some(previous) = previous {
                assert_eq!(facts, previous);
            }
            previous = Some(facts.to_vec());
        }
    }
}
