//! SPEC-0331：source return 与 dead-tail 的独立四路阶段观察。
//! 不套用 live-loop 的 local/read 假设；未来 Map 的未 Acquire 计划查询另补。
use super::{
    Report,
    fixture::{Blocked, Fixture, LOGICAL_PATH, blocked, diagnostics, require_clean},
    pipeline::Entry,
};
use lang_frontend::{
    ast::{ExpressionId, StatementId},
    name_resolution::{
        NameResolution, SourceUnitInput, ValidatedCompilationUnitNames, index_compilation_unit,
        resolve_compilation_unit_names, resolve_names,
    },
    ownership_checking::{
        CompilationUnitOwnership, OwnershipCheckedFile, check_compilation_unit_ownership,
        check_ownership,
    },
    parser::{Expression, ForBinding, FunctionBody, FunctionForm, Item, NameMarker, Statement},
    source::Span,
    type_checking::{
        CompilationUnitTypes, FinalTypeAdmissionError, TypeEnvironment, TypedFile,
        UnitExpressionId, check_compilation_unit_types, check_types,
        complete_compilation_unit_types, complete_file_types, standard_environments,
    },
};

const SOURCE_RETURN: &str = r#"fun scan(): Unit {
  for ((key, value) in if (return) mapOf<Int, Int>() else mapOf<Int, Int>()) {}
}
"#;
const DEAD_TAIL: &str = r#"fun scan(): Unit {
  return
  for ((key, value) in mutableMapOf<Int, Int>()) {}
}
"#;

#[derive(Clone, Copy, Debug)]
enum Case {
    SourceReturn,
    DeadTail,
}

impl Case {
    fn text(self) -> &'static str {
        match self {
            Self::SourceReturn => SOURCE_RETURN,
            Self::DeadTail => DEAD_TAIL,
        }
    }
}

#[derive(Debug)]
struct Sites {
    statement: StatementId,
    source: ExpressionId,
    bindings: [Span; 2],
    returned: ExpressionId,
    factories: Vec<ExpressionId>,
}

fn sites(fixture: &Fixture, case: Case) -> Result<Sites, Blocked> {
    let ast = fixture.file.ast();
    let functions = ast
        .items()
        .iter()
        .filter_map(|(_, node)| match node.payload() {
            Item::Function {
                name: NameMarker::Present(name),
                form:
                    FunctionForm::Explicit {
                        body: FunctionBody::Block(body),
                        ..
                    },
                ..
            } if fixture.sources.slice(*name) == Ok("scan") => Some(*body),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [body] = functions.as_slice() else {
        return Err(blocked("AST scan", functions));
    };
    let body = ast
        .statements()
        .get(*body)
        .map_err(|error| blocked("AST scan", error))?;
    let Statement::Block { elements } = body.payload() else {
        return Err(blocked("AST scan", "block required"));
    };
    let (statement, preceding) = match (case, elements.as_slice()) {
        (Case::SourceReturn, [statement]) => (*statement, None),
        (Case::DeadTail, [preceding, statement]) => (*statement, Some(*preceding)),
        _ => return Err(blocked("AST statement order", elements)),
    };
    let node = ast
        .statements()
        .get(statement)
        .map_err(|error| blocked("AST for", error))?;
    let Statement::For {
        binding: ForBinding::Destructuring { names, .. },
        source,
        body,
        ..
    } = node.payload()
    else {
        return Err(blocked("AST for", "destructuring for required"));
    };
    let [NameMarker::Present(key), NameMarker::Present(value)] = names.as_slice() else {
        return Err(blocked("AST for", "two real bindings required"));
    };
    if fixture.sources.slice(*key) != Ok("key") || fixture.sources.slice(*value) != Ok("value") {
        return Err(blocked("AST binding spelling", names));
    }
    let body = ast
        .statements()
        .get(*body)
        .map_err(|error| blocked("AST for body", error))?;
    if !matches!(body.payload(), Statement::Block { elements } if elements.is_empty()) {
        return Err(blocked("AST for body", "empty body required"));
    }
    let source_node = ast
        .expressions()
        .get(*source)
        .map_err(|error| blocked("AST source", error))?;
    let returned = match (case, preceding, source_node.payload()) {
        (Case::SourceReturn, None, Expression::If { condition, .. }) => *condition,
        (Case::DeadTail, Some(preceding), Expression::Call { .. }) => {
            let preceding = ast
                .statements()
                .get(preceding)
                .map_err(|error| blocked("AST dead tail", error))?;
            let Statement::Expression { expression } = preceding.payload() else {
                return Err(blocked(
                    "AST dead tail",
                    "preceding return statement required",
                ));
            };
            *expression
        }
        _ => return Err(blocked("AST source order", source_node.payload())),
    };
    let returned_node = ast
        .expressions()
        .get(returned)
        .map_err(|error| blocked("AST return", error))?;
    if !matches!(
        returned_node.payload(),
        Expression::Return { value: None, .. }
    ) {
        return Err(blocked("AST return", "bare return required"));
    }
    let spelling = match case {
        Case::SourceReturn => "mapOf",
        Case::DeadTail => "mutableMapOf",
    };
    let factories = ast
        .expressions()
        .iter()
        .filter_map(|(id, node)| {
            let Expression::Call {
                callee, arguments, ..
            } = node.payload()
            else {
                return None;
            };
            let callee = ast.expressions().get(*callee).ok()?;
            (arguments.is_empty() && fixture.sources.slice(callee.span()) == Ok(spelling))
                .then_some(id)
        })
        .collect::<Vec<_>>();
    let expected = match case {
        Case::SourceReturn => 2,
        Case::DeadTail => 1,
    };
    if factories.len() != expected {
        return Err(blocked("AST empty factories", factories));
    }
    Ok(Sites {
        statement,
        source: *source,
        bindings: [*key, *value],
        returned,
        factories,
    })
}

struct Single {
    names: NameResolution,
    raw: TypedFile,
    complete: Result<bool, FinalTypeAdmissionError>,
    owned: Result<Option<OwnershipCheckedFile>, String>,
}
struct Unit {
    names: ValidatedCompilationUnitNames,
    raw: CompilationUnitTypes,
    complete: Result<bool, FinalTypeAdmissionError>,
    owned: Result<Option<CompilationUnitOwnership>, String>,
}
enum Products {
    Single(Box<Single>),
    Unit(Box<Unit>),
}
struct Observed {
    fixture: Fixture,
    environment: TypeEnvironment,
    products: Products,
    report: Report,
}
struct Observation {
    case: Case,
    entry: Entry,
    result: Result<Observed, Blocked>,
}

fn record_complete(
    fixture: &Fixture,
    complete: &Result<bool, FinalTypeAdmissionError>,
    report: &mut Report,
) {
    match complete {
        Ok(same) => report.record("complete original identity", *same, same),
        Err(FinalTypeAdmissionError::Diagnostics(items)) => report.record(
            "complete diagnostics",
            false,
            diagnostics(&fixture.sources, "complete", items),
        ),
        Err(FinalTypeAdmissionError::Internal(error)) => {
            report.record("complete INTERNAL", false, error)
        }
    }
}

fn analyze(case: Case, entry: Entry) -> Result<Observed, Blocked> {
    let fixture = Fixture::new(case.text())?;
    let sites = sites(&fixture, case)?;
    let (name_environment, environment) = standard_environments();
    let sources = &fixture.sources;
    let file = &fixture.file;
    let mut report = Report::default();
    report.record("lex/parse", true, file.source_id());
    report.note(
        "actual for/source/return identities",
        (sites.statement, sites.source, sites.returned),
    );
    report.note("actual named binding spans", sites.bindings);
    report.note("actual empty factory expressions", &sites.factories);
    let products = match entry {
        Entry::Single => {
            let names = resolve_names(sources, file, &name_environment)
                .map_err(|error| blocked("single names infrastructure", error))?;
            require_clean(sources, "single names", names.diagnostics())?;
            report.record("names", true, names.source_id());
            let raw = check_types(sources, file, &names, &environment)
                .map_err(|error| blocked("single raw infrastructure", error))?;
            report.record(
                "raw diagnostics",
                raw.diagnostics().is_empty(),
                diagnostics(sources, "raw", raw.diagnostics()),
            );
            report.note(
                "actual source type",
                raw.expression_type(sites.source)
                    .and_then(|ty| raw.types().get(ty)),
            );
            report.note(
                "actual factory types",
                sites
                    .factories
                    .iter()
                    .map(|id| raw.expression_type(*id).and_then(|ty| raw.types().get(ty)))
                    .collect::<Vec<_>>(),
            );
            report.note(
                "existing typed sequential plan count",
                raw.sequential_iterations().len(),
            );
            let (complete, owned) =
                match complete_file_types(sources, file, &names, &environment, &raw) {
                    Ok(complete) => {
                        let same = complete.is_same_analysis(&raw);
                        let owned = if same && raw.diagnostics().is_empty() {
                            check_ownership(sources, file, &names, complete.types())
                                .map(Some)
                                .map_err(|error| format!("{error:?}"))
                        } else {
                            Ok(None)
                        };
                        (Ok(same), owned)
                    }
                    Err(error) => (Err(error), Ok(None)),
                };
            record_complete(&fixture, &complete, &mut report);
            match &owned {
                Ok(Some(owned)) => {
                    report.record(
                        "ownership reached/identity",
                        owned.is_compatible_with(&names, &raw),
                        "original complete.types()",
                    );
                    report.record(
                        "ownership diagnostics",
                        owned.diagnostics().is_empty(),
                        diagnostics(sources, "ownership", owned.diagnostics()),
                    );
                    report.record(
                        "ownership deferred",
                        owned.deferred().is_empty(),
                        owned.deferred(),
                    );
                    report.record(
                        "no formed owned local",
                        owned.formed_owned_locals().is_empty(),
                        owned.formed_owned_locals(),
                    );
                    report.record(
                        "no evaluated source owner cleanup",
                        owned.drops().is_empty(),
                        owned.drops(),
                    );
                    report.note(
                        "existing ownership sequential plans (not Map oracle)",
                        owned.iterations(),
                    );
                }
                Ok(None) => report.record(
                    "OWNERSHIP_NOT_REACHED",
                    false,
                    "raw/complete admission failed",
                ),
                Err(error) => report.record("ownership INTERNAL", false, error),
            }
            Products::Single(Box::new(Single {
                names,
                raw,
                complete,
                owned,
            }))
        }
        Entry::Unit => {
            let inputs = [SourceUnitInput::new(
                "root",
                LOGICAL_PATH,
                file.source_id(),
                file,
            )];
            let index = index_compilation_unit(sources, &inputs)
                .map_err(|error| blocked("unit index infrastructure", error))?;
            require_clean(sources, "unit index", index.diagnostics())?;
            let names = resolve_compilation_unit_names(sources, &inputs, &index, &name_environment)
                .map_err(|error| blocked("unit names infrastructure", error))?;
            require_clean(sources, "unit names", names.diagnostics())?;
            let names = names
                .validate()
                .map_err(|error| blocked("unit names qualification", error.diagnostics()))?;
            report.record("names", true, names.names().source_units().len());
            let raw = check_compilation_unit_types(sources, &inputs, &names, &environment)
                .map_err(|error| blocked("unit raw infrastructure", error))?;
            report.record(
                "raw diagnostics",
                raw.diagnostics().is_empty(),
                diagnostics(sources, "raw", raw.diagnostics()),
            );
            if let [source_names] = names.names().source_units() {
                let expression = |id| UnitExpressionId::new(source_names.source_unit(), id);
                report.note(
                    "actual source type",
                    raw.expression_type(expression(sites.source))
                        .and_then(|ty| raw.types().get(ty)),
                );
                report.note(
                    "actual factory types",
                    sites
                        .factories
                        .iter()
                        .map(|id| {
                            raw.expression_type(expression(*id))
                                .and_then(|ty| raw.types().get(ty))
                        })
                        .collect::<Vec<_>>(),
                );
            } else {
                report.record(
                    "unit source identity",
                    false,
                    names.names().source_units().len(),
                );
            }
            report.note(
                "existing typed sequential plan count",
                raw.sequential_iterations().len(),
            );
            let complete =
                complete_compilation_unit_types(sources, &inputs, &names, &environment, &raw)
                    .map(|complete| complete.is_same_analysis(&raw));
            record_complete(&fixture, &complete, &mut report);
            let (raw, owned) = if raw.diagnostics().is_empty() && matches!(complete, Ok(true)) {
                match raw.validate() {
                    Ok(typed) => {
                        report.record("basic type qualification", true, "original unit types");
                        let owned = check_compilation_unit_ownership(
                            sources,
                            &inputs,
                            &names,
                            &environment,
                            &typed,
                        )
                        .map(|owned| {
                            report.record(
                                "ownership reached/identity",
                                owned.is_compatible_with(&typed),
                                "original qualified unit",
                            );
                            report.record(
                                "ownership diagnostics",
                                owned.diagnostics().is_empty(),
                                diagnostics(sources, "ownership", owned.diagnostics()),
                            );
                            report.record(
                                "ownership deferred",
                                owned.deferred().is_empty(),
                                owned.deferred(),
                            );
                            report.record(
                                "no formed owned local",
                                owned.formed_owned_locals().is_empty(),
                                owned.formed_owned_locals(),
                            );
                            report.record(
                                "no evaluated source owner cleanup",
                                owned.drops().is_empty(),
                                owned.drops(),
                            );
                            report.note(
                                "existing ownership sequential plans (not Map oracle)",
                                owned.iterations(),
                            );
                            match owned.validate() {
                                Ok(qualified) => {
                                    report.record(
                                        "ownership basic qualification",
                                        true,
                                        "accepted",
                                    );
                                    qualified.into_ownership()
                                }
                                Err(owned) => {
                                    report.record(
                                        "ownership basic qualification",
                                        false,
                                        "rejected",
                                    );
                                    *owned
                                }
                            }
                        })
                        .map(Some)
                        .map_err(|error| format!("{error:?}"));
                        (typed.into_types(), owned)
                    }
                    Err(raw) => {
                        report.record("basic type qualification", false, raw.diagnostics());
                        (*raw, Ok(None))
                    }
                }
            } else {
                (raw, Ok(None))
            };
            match &owned {
                Ok(None) => report.record(
                    "OWNERSHIP_NOT_REACHED",
                    false,
                    "raw/complete/basic admission failed",
                ),
                Err(error) => report.record("ownership INTERNAL", false, error),
                Ok(Some(_)) => {}
            }
            Products::Unit(Box::new(Unit {
                names,
                raw,
                complete,
                owned,
            }))
        }
    };
    Ok(Observed {
        fixture,
        environment,
        products,
        report,
    })
}

impl Observation {
    fn assess(&self) -> (bool, String) {
        let label = format!("{:?}/{:?}", self.case, self.entry);
        let observed = match &self.result {
            Ok(observed) => observed,
            Err(error) => {
                return (
                    false,
                    format!(
                        "{label}: BLOCKED {}: {}\nSOURCE {:?}",
                        error.stage,
                        error.detail,
                        self.case.text()
                    ),
                );
            }
        };
        let fixture = &observed.fixture;
        let retained_source = fixture.lexed.source_id() == fixture.file.source_id();
        let (identity, phase2, internal, reached) = match &observed.products {
            Products::Single(product) => (
                product.raw.is_compatible_with_names(&product.names)
                    && product
                        .owned
                        .as_ref()
                        .ok()
                        .and_then(Option::as_ref)
                        .is_none_or(|owned| owned.is_compatible_with(&product.names, &product.raw)),
                product.raw.diagnostics().is_empty() && matches!(product.complete, Ok(true)),
                matches!(product.complete, Err(FinalTypeAdmissionError::Internal(_)))
                    || product.owned.is_err(),
                matches!(product.owned, Ok(Some(_))),
            ),
            Products::Unit(product) => {
                let inputs = [SourceUnitInput::new(
                    "root",
                    LOGICAL_PATH,
                    fixture.file.source_id(),
                    &fixture.file,
                )];
                (
                    product.raw.is_compatible_with(
                        &fixture.sources,
                        &inputs,
                        &product.names,
                        &observed.environment,
                    ),
                    product.raw.diagnostics().is_empty() && matches!(product.complete, Ok(true)),
                    matches!(product.complete, Err(FinalTypeAdmissionError::Internal(_)))
                        || product.owned.is_err(),
                    matches!(product.owned, Ok(Some(_))),
                )
            }
        };
        let passed = retained_source && identity && observed.report.passed();
        let status = if internal {
            "INTERNAL"
        } else if !phase2 {
            "PHASE_2_REJECTED / OWNERSHIP_NOT_REACHED"
        } else if !reached {
            "OWNERSHIP_NOT_REACHED"
        } else if passed {
            "TARGET_MET (Map no-Acquire query still pending)"
        } else {
            "CONTRACT_TARGET_FAILED"
        };
        (
            passed,
            format!(
                "{label}: {status} retained_source={retained_source} retained_analysis_identity={identity}\nSOURCE {:?}\nCHECKS {:#?}\nDETAILS {}",
                self.case.text(),
                observed.report.checks,
                observed.report.details.join("\n")
            ),
        )
    }
}

#[test]
fn source_return_and_dead_tail_do_not_acquire_map_providers() {
    let observations = [Case::SourceReturn, Case::DeadTail]
        .into_iter()
        .flat_map(|case| {
            [Entry::Single, Entry::Unit].map(|entry| Observation {
                case,
                entry,
                result: analyze(case, entry),
            })
        })
        .collect::<Vec<_>>();
    let assessments = observations
        .iter()
        .map(Observation::assess)
        .collect::<Vec<_>>();
    for (_, report) in &assessments {
        println!("{report}");
    }
    assert_eq!(assessments.len(), 4);
    assert!(
        assessments.iter().all(|(passed, _)| *passed),
        "SPEC-0331 source-before-acquire/dead-tail success target: ordinary raw/complete then ownership must accept without forming or cleaning an unevaluated source owner. Phase 2 failure is not ownership evidence. New Map plan absence requires the later real API.\n{}",
        assessments
            .iter()
            .map(|(_, report)| report.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[path = "unacquired_query.rs"]
mod exact;
