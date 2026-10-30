//! SPEC-0329: ordinary source observation only; no facts mutation or execution.
use lang_frontend::{
    diagnostic::{Diagnostic, Severity},
    lexer::lex,
    name_resolution::{
        SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    parser::{Statement, parse_file},
    source::{SourceMap, Span},
    type_checking::{
        BuiltinType, FinalTypeAdmissionError, IterationProvider, ParameterMode, TypeKind,
        UnitExpressionId, UnitStatementId, UnitTypeKind, check_compilation_unit_types, check_types,
        complete_compilation_unit_types, complete_file_types, standard_environments,
    },
};

#[derive(Debug)]
struct Observation {
    mode: &'static str,
    source_span: Span,
    names: Vec<Diagnostic>,
    raw: Option<Vec<Diagnostic>>,
    source_kind: String,
    plan: String,
    identity: bool,
    element: Option<BuiltinType>,
    provider: Option<IterationProvider>,
    source_builtin: Option<BuiltinType>,
    delivery: Option<ParameterMode>,
    complete: Option<Result<bool, FinalTypeAdmissionError>>,
}

fn observe(text: &str) -> Vec<Observation> {
    let mut sources = SourceMap::new();
    let source = sources.add_source("iteration.ko", text).unwrap();
    let lexed = lex(&sources, source).unwrap();
    assert!(lexed.diagnostics().is_empty(), "{:?}", lexed.diagnostics());
    let file = parse_file(&sources, &lexed).unwrap();
    assert!(file.diagnostics().is_empty(), "{:?}", file.diagnostics());
    let loops: Vec<_> = file
        .ast()
        .statements()
        .iter()
        .filter_map(|(id, node)| {
            if let Statement::For { source, .. } = node.payload() {
                Some((id, *source))
            } else {
                None
            }
        })
        .collect();
    assert_eq!(loops.len(), 1, "one source loop");
    let (statement, source_expr) = loops[0];
    let source_span = file.ast().expressions().get(source_expr).unwrap().span();
    let (environment, types) = standard_environments();
    let names = resolve_names(&sources, &file, &environment).unwrap();
    let typed = check_types(&sources, &file, &names, &types).unwrap();
    let plan = typed.sequential_iteration(statement);
    let single = Observation {
        mode: "single",
        source_span,
        names: names.diagnostics().to_vec(),
        raw: Some(typed.diagnostics().to_vec()),
        source_kind: format!(
            "{:?}",
            typed
                .expression_type(source_expr)
                .and_then(|id| typed.types().get(id))
        ),
        plan: format!("{plan:?}"),
        identity: plan.is_some_and(|p| {
            p.statement() == statement
                && p.source() == source_expr
                && Some(p.source_type()) == typed.expression_type(source_expr)
                && typed.sequential_iterations().len() == 1
        }),
        element: plan.and_then(|p| match typed.types().get(p.element_type()) {
            Some(TypeKind::Builtin(builtin)) => Some(*builtin),
            _ => None,
        }),
        provider: plan.map(|p| p.provider()),
        source_builtin: plan.and_then(|p| match typed.types().get(p.source_type()) {
            Some(TypeKind::Builtin(b)) => Some(*b),
            _ => None,
        }),
        delivery: plan.map(|p| p.delivery()),
        complete: Some(
            complete_file_types(&sources, &file, &names, &types, &typed)
                .map(|complete| complete.is_same_analysis(&typed)),
        ),
    };
    // Keep the legacy witness; both Unit keys consume this exact parsed source.
    let mut observations = vec![single];
    for (mode, input) in [
        (
            "unit",
            SourceUnitInput::new("root", "iteration.ko", source, &file),
        ),
        (
            "memory unit",
            SourceUnitInput::memory_document(source, &file),
        ),
    ] {
        let inputs = [input];
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names =
            resolve_compilation_unit_names(&sources, &inputs, &index, &environment).unwrap();
        let diagnostics = names.diagnostics().to_vec();
        let unit = match names.validate() {
            Err(_) => Observation {
                mode,
                source_span,
                names: diagnostics,
                raw: None,
                source_kind: String::new(),
                plan: String::new(),
                identity: false,
                element: None,
                provider: None,
                source_builtin: None,
                delivery: None,
                complete: None,
            },
            Ok(names) => {
                let unit = names.names().index().source_units()[0].id();
                let typed =
                    check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
                let statement = UnitStatementId::new(unit, statement);
                let source_expr = UnitExpressionId::new(unit, source_expr);
                let plan = typed.sequential_iteration(statement);
                Observation {
                    mode,
                    source_span,
                    names: diagnostics,
                    raw: Some(typed.diagnostics().to_vec()),
                    source_kind: format!(
                        "{:?}",
                        typed
                            .expression_type(source_expr)
                            .and_then(|id| typed.types().get(id))
                    ),
                    plan: format!("{plan:?}"),
                    identity: plan.is_some_and(|p| {
                        p.statement() == statement
                            && p.source() == source_expr
                            && Some(p.source_type()) == typed.expression_type(source_expr)
                            && typed.sequential_iterations().len() == 1
                    }),
                    element: plan.and_then(|p| match typed.types().get(p.element_type()) {
                        Some(UnitTypeKind::Builtin(builtin)) => Some(*builtin),
                        _ => None,
                    }),
                    provider: plan.map(|p| p.provider()),
                    source_builtin: plan.and_then(|p| match typed.types().get(p.source_type()) {
                        Some(UnitTypeKind::Builtin(b)) => Some(*b),
                        _ => None,
                    }),
                    delivery: plan.map(|p| p.delivery()),
                    complete: Some(
                        complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed)
                            .map(|complete| complete.is_same_analysis(&typed)),
                    ),
                }
            }
        };
        observations.push(unit);
    }
    for o in &observations {
        println!(
            "SOURCE {text:?} MODE {} SOURCE_SPAN {:?} SOURCE_KIND {} PLAN {} OBS {o:?}",
            o.mode, o.source_span, o.source_kind, o.plan
        );
    }
    observations
}

fn accepted(o: &Observation, endpoint: BuiltinType, mode: ParameterMode) {
    assert!(o.names.is_empty(), "{o:?}");
    assert_eq!(o.raw.as_deref(), Some([].as_slice()), "{o:?}");
    assert!(o.identity, "{o:?}");
    assert_eq!(o.element, Some(endpoint), "{o:?}");
    assert_eq!(o.delivery, Some(mode), "{o:?}");
    if mode == ParameterMode::Value {
        assert_eq!(o.provider, Some(IterationProvider::NumericRange(endpoint)));
        let range = match endpoint {
            BuiltinType::Int => BuiltinType::IntRange,
            BuiltinType::Long => BuiltinType::LongRange,
            _ => panic!("numeric endpoint"),
        };
        assert_eq!(o.source_builtin, Some(range));
    }
    assert!(matches!(o.complete, Some(Ok(true))), "{o:?}");
}

#[test]
fn range_sources_publish_owned_endpoint_delivery() {
    let mut all = Vec::new();
    for (endpoint, range, builtin) in [
        ("Int", "IntRange", BuiltinType::Int),
        ("Long", "LongRange", BuiltinType::Long),
    ] {
        for operator in ["..", "..<"] {
            for source in [
                "value".to_owned(),
                format!("a{operator}b"),
                "make(a,b)".to_owned(),
                "holder.value".to_owned(),
            ] {
                let text = format!(
                    "class Holder(val value:{range})\nfun make(a:{endpoint},b:{endpoint}):{range}=a{operator}b\nfun entry(a:{endpoint},b:{endpoint},value:{range},holder:Holder):Unit{{for(item in {source}){{val copy:{endpoint}=item}}}}"
                );
                all.extend(observe(&text).into_iter().map(|o| (o, builtin)));
            }
        }
    }
    for (o, endpoint) in all {
        accepted(&o, endpoint, ParameterMode::Value);
    }
}

#[test]
fn range_iteration_accepts_existing_control_exit_syntax() {
    let mut all = Vec::new();
    for (endpoint, range, builtin) in [
        ("Int", "IntRange", BuiltinType::Int),
        ("Long", "LongRange", BuiltinType::Long),
    ] {
        for jump in ["", "continue", "break", "return"] {
            let text = format!(
                "fun entry(value:{range}):Unit{{for(item in value){{val copy:{endpoint}=item; {jump}}}}}"
            );
            all.extend(observe(&text).into_iter().map(|o| (o, builtin)));
        }
        all.extend(
            observe(&format!(
                "fun entry(value:{range}):Unit{{for(_ in value){{}}}}"
            ))
            .into_iter()
            .map(|o| (o, builtin)),
        );
    }
    for (o, endpoint) in all {
        accepted(&o, endpoint, ParameterMode::Value);
    }
}

#[test]
fn invalid_numeric_sources_preserve_borrow_provider_controls() {
    let mut rejected = Vec::new();
    for ty in ["Boolean", "IntRange?", "LongRange?"] {
        rejected.extend(observe(&format!(
            "fun entry(value:{ty}):Unit{{for(_ in value){{}}}}"
        )));
    }
    let mut borrowed = Vec::new();
    for ty in ["Array<Int>", "List<Int>", "MutableList<Int>", "View<Int>"] {
        borrowed.extend(observe(&format!(
            "fun entry(value:{ty}):Unit{{for(item in value){{val copy:Int=item}}}}"
        )));
    }
    for o in rejected {
        assert!(o.names.is_empty(), "{o:?}");
        let raw = o.raw.as_ref().expect("raw stage reached");
        assert_eq!(raw.len(), 1, "{o:?}");
        assert_eq!(raw[0].code().to_string(), "L0159", "{o:?}");
        assert_eq!(raw[0].primary_span(), o.source_span, "{o:?}");
        // Independent frozen contract: do not derive expected text/details from another entry.
        assert_eq!(raw[0].severity(), Severity::Error, "{o:?}");
        assert_eq!(
            raw[0].message(),
            "for source requires a compiler-bound sequential container, View, IntRange or LongRange",
            "{o:?}"
        );
        assert!(raw[0].details().is_empty(), "{o:?}");
        assert!(o.delivery.is_none(), "{o:?}");
        match &o.complete {
            Some(Err(FinalTypeAdmissionError::Diagnostics(diagnostics))) => {
                assert_eq!(
                    diagnostics, raw,
                    "complete preserves full raw diagnostics: {o:?}"
                );
            }
            other => panic!("expected exact L0159 completion diagnostics, got {other:?}: {o:?}"),
        }
    }
    for o in borrowed {
        accepted(&o, BuiltinType::Int, ParameterMode::Borrow);
    }
}
