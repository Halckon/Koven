//! SPEC-0332 first evidence: ordinary source, both frontends, no native execution.
//! A passing collector proves complete/ownership and a real lowering attempt only.
use std::fmt::Debug;

use lang_frontend::{
    lexer::lex,
    name_resolution::{
        SourceUnitInput, SymbolKind, index_compilation_unit, resolve_compilation_unit_names,
        resolve_names,
    },
    ownership_checking::{check_compilation_unit_ownership, check_ownership},
    parser::parse_file,
    source::{SourceMap, Span},
    type_checking::{
        BuiltinType, Copyability, IntrinsicTypeConstructor, ParameterMode, TypeKind,
        UnitExpressionId, UnitTypeKind, check_compilation_unit_types, check_types,
        complete_compilation_unit_types, complete_file_types, standard_environments,
    },
};

use super::{
    LoweringError, lower_scalar_file_with_entry, render::render_program,
    unit_lower::lower_scalar_unit_with_entry, verify::verify_program,
};

#[path = "rc_nullable_while_identity_queries.rs"]
mod identity;

const SOURCE: &str =
    "fun inspect(own owner: Rc<Int>?): Rc<Int>? { while (owner != null) { break }; return owner }";

struct Failure {
    stage: &'static str,
    detail: String,
    lower: Option<LoweringError>,
    span_text: Option<String>,
}

fn failure(stage: &'static str, error: impl Debug) -> Failure {
    Failure {
        stage,
        detail: format!("{error:?}"),
        lower: None,
        span_text: None,
    }
}

struct Observation {
    unit: bool,
    complete_ok: bool,
    ownership_ok: bool,
    frontend_anchor_ok: bool,
    lower_attempted: bool,
    outcome: Result<(), Failure>,
}

fn observe(unit: bool) -> Observation {
    observe_with_inspector(unit, false).0
}

fn observe_with_inspector(unit: bool, inspect_identity: bool) -> (Observation, Result<(), String>) {
    let mut complete_ok = false;
    let mut ownership_ok = false;
    let mut frontend_anchor_ok = false;
    let mut lower_attempted = false;
    let mut source_identity = Err("source identity query not reached".to_owned());
    let mut identity = Err("SSA identity query not reached".to_owned());
    let outcome = (|| {
        let mut sources = SourceMap::new();
        let (path, text) = if unit {
            ("p/inspect.ko", format!("package p\n{SOURCE}"))
        } else {
            ("inspect.ko", SOURCE.to_owned())
        };
        println!("SPEC0332 INPUT unit={unit} path={path:?} source={text:?}");
        let source = sources
            .add_source(path, &text)
            .map_err(|error| failure("source-map", error))?;
        let lexed = lex(&sources, source).map_err(|error| failure("lex-api", error))?;
        if !lexed.diagnostics().is_empty() {
            return Err(failure("lex-diagnostics", lexed.diagnostics()));
        }
        let file = parse_file(&sources, &lexed).map_err(|error| failure("parse-api", error))?;
        if !file.diagnostics().is_empty() {
            return Err(failure("parse-diagnostics", file.diagnostics()));
        }
        println!("SPEC0332 STAGE unit={unit} parse=ok");
        let source_shape_ok = file.roots().len() == 1;
        let conditions = file
            .ast()
            .expressions()
            .iter()
            .filter(|(_, node)| sources.slice(node.span()).ok() == Some("owner != null"))
            .map(|(id, node)| (id, node.span()))
            .collect::<Vec<_>>();
        let condition = match conditions.as_slice() {
            [condition] => Some(*condition),
            _ => None,
        };
        println!(
            "SPEC0332 SOURCE_SHAPE unit={unit} roots={} conditions={conditions:?}",
            file.roots().len()
        );
        let (environment, types) = standard_environments();
        let lowered = if unit {
            let inputs = [SourceUnitInput::new("root", path, source, &file)];
            let index = index_compilation_unit(&sources, &inputs)
                .map_err(|error| failure("index-api", error))?;
            let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
                .map_err(|error| failure("names-api", error))?
                .validate()
                .map_err(|error| failure("names-validation", error))?;
            println!("SPEC0332 STAGE unit={unit} names=ok");
            let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
                .map_err(|error| failure("types-api", error))?;
            complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed)
                .map_err(|error| failure("complete", error))?;
            complete_ok = true;
            println!("SPEC0332 STAGE unit={unit} complete=ok");
            let typed = typed
                .validate()
                .map_err(|error| failure("types-validation", error))?;
            let owned = check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                .map_err(|error| failure("ownership-api", error))?;
            if !owned.diagnostics().is_empty() || !owned.deferred().is_empty() {
                return Err(failure(
                    "ownership-diagnostics-or-deferred",
                    (owned.diagnostics(), owned.deferred()),
                ));
            }
            let owned = owned
                .validate()
                .map_err(|error| failure("ownership-validation", error))?;
            ownership_ok = true;
            println!("SPEC0332 STAGE unit={unit} ownership=ok");
            let declarations = names
                .names()
                .index()
                .declarations()
                .iter()
                .filter(|declaration| {
                    declaration.name() == "inspect"
                        && declaration.kind() == SymbolKind::Function
                        && names.names().index().packages()[declaration.package().index()]
                            .name()
                            .expect("named package")
                            .segments()
                            .iter()
                            .map(String::as_str)
                            .eq(["p"])
                })
                .collect::<Vec<_>>();
            let [declaration] = declarations.as_slice() else {
                return Err(failure(
                    "entry-selection",
                    "expected exactly one p.inspect declaration",
                ));
            };
            let entry = declaration.id();
            let facts = typed.types();
            frontend_anchor_ok = condition.and_then(|(id, span)| {
                let expression = UnitExpressionId::new(declaration.source_unit(), id);
                let descriptor = facts.null_comparison(expression)?;
                let callable = facts.signatures().declaration(entry)?.callable()?;
                let parameter = callable.parameters().iter()
                    .find(|parameter| parameter.symbol() == Some(descriptor.symbol()))?;
                let nullable = facts.types().get(descriptor.nullable_type())?;
                println!(
                    "SPEC0332 FACT unit={unit} condition={:?} descriptor={descriptor:?} parameter={parameter:?} nullable={nullable:?} copyability={:?}",
                    span, facts.copyability(descriptor.nullable_type())
                );
                let UnitTypeKind::Nullable(inner) = nullable else { return Some(false) };
                let inner = facts.types().get(*inner)?;
                println!("SPEC0332 INNER unit={unit} type={inner:?}");
                let UnitTypeKind::Intrinsic {
                    constructor: IntrinsicTypeConstructor::Rc, arguments,
                } = inner else { return Some(false) };
                let [argument] = arguments.as_slice() else { return Some(false) };
                let payload = facts.types().get(*argument)?;
                println!("SPEC0332 PAYLOAD unit={unit} type={payload:?}");
                Some(source_shape_ok && descriptor.expression() == expression
                    && descriptor.non_null_when_true()
                    && parameter.mode() == ParameterMode::Value
                    && parameter.ty() == descriptor.nullable_type()
                    && facts.symbol_type(descriptor.symbol()) == Some(descriptor.nullable_type())
                    && facts.copyability(descriptor.nullable_type()) == Copyability::MoveOnly
                    && *payload == UnitTypeKind::Builtin(BuiltinType::Int))
            }).unwrap_or(false);
            println!("SPEC0332 ANCHOR unit={unit} valid={frontend_anchor_ok}");
            if inspect_identity {
                source_identity =
                    identity::unit_source(&file, &names, typed.types(), owned.ownership(), entry);
                println!(
                    "SPEC0332 SOURCE OWNER QUERY unit={unit} result={:?}",
                    source_identity.as_ref().map(|_| ())
                );
            }
            lower_attempted = true;
            println!("SPEC0332 STAGE unit={unit} lower=attempt");
            crate::ssa::with_per_family_lowering(|| {
                lower_scalar_unit_with_entry(
                    &sources, &inputs, &names, &types, &typed, &owned, entry,
                )
            })
        } else {
            let names = resolve_names(&sources, &file, &environment)
                .map_err(|error| failure("names-api", error))?;
            if !names.diagnostics().is_empty() {
                return Err(failure("names-diagnostics", names.diagnostics()));
            }
            println!("SPEC0332 STAGE unit={unit} names=ok");
            let typed = check_types(&sources, &file, &names, &types)
                .map_err(|error| failure("types-api", error))?;
            complete_file_types(&sources, &file, &names, &types, &typed)
                .map_err(|error| failure("complete", error))?;
            complete_ok = true;
            println!("SPEC0332 STAGE unit={unit} complete=ok");
            let owned = check_ownership(&sources, &file, &names, &typed)
                .map_err(|error| failure("ownership-api", error))?;
            if !owned.diagnostics().is_empty() || !owned.deferred().is_empty() {
                return Err(failure(
                    "ownership-diagnostics-or-deferred",
                    (owned.diagnostics(), owned.deferred()),
                ));
            }
            ownership_ok = true;
            println!("SPEC0332 STAGE unit={unit} ownership=ok");
            let entries = names
                .symbols()
                .iter()
                .filter(|symbol| {
                    symbol.name() == "inspect" && symbol.kind() == SymbolKind::Function
                })
                .collect::<Vec<_>>();
            let [entry] = entries.as_slice() else {
                return Err(failure(
                    "entry-selection",
                    "expected exactly one inspect function",
                ));
            };
            let entry = entry.id();
            frontend_anchor_ok = condition.and_then(|(id, span)| {
                let descriptor = typed.null_comparison(id)?;
                let nullable = typed.types().get(descriptor.nullable_type())?;
                println!(
                    "SPEC0332 FACT unit={unit} condition={:?} descriptor={descriptor:?} parameter_mode={:?} nullable={nullable:?} copyability={:?}",
                    span, typed.parameter_mode(descriptor.symbol()),
                    typed.copyability(descriptor.nullable_type())
                );
                let TypeKind::Nullable(inner) = nullable else { return Some(false) };
                let inner = typed.types().get(*inner)?;
                println!("SPEC0332 INNER unit={unit} type={inner:?}");
                let TypeKind::Intrinsic {
                    constructor: IntrinsicTypeConstructor::Rc, arguments,
                } = inner else { return Some(false) };
                let [argument] = arguments.as_slice() else { return Some(false) };
                let payload = typed.types().get(*argument)?;
                println!("SPEC0332 PAYLOAD unit={unit} type={payload:?}");
                Some(source_shape_ok && descriptor.expression() == id
                    && descriptor.non_null_when_true()
                    && typed.parameter_mode(descriptor.symbol()) == Some(ParameterMode::Value)
                    && typed.symbol_type(descriptor.symbol()) == Some(descriptor.nullable_type())
                    && typed.copyability(descriptor.nullable_type()) == Some(Copyability::MoveOnly)
                    && *payload == TypeKind::Builtin(BuiltinType::Int))
            }).unwrap_or(false);
            println!("SPEC0332 ANCHOR unit={unit} valid={frontend_anchor_ok}");
            if inspect_identity {
                source_identity = identity::single_source(&file, &names, &typed, &owned, entry);
                println!(
                    "SPEC0332 SOURCE OWNER QUERY unit={unit} result={:?}",
                    source_identity.as_ref().map(|_| ())
                );
            }
            lower_attempted = true;
            println!("SPEC0332 STAGE unit={unit} lower=attempt");
            lower_scalar_file_with_entry(&sources, &file, &names, &typed, &owned, entry)
        };
        let (program, entry) = lowered.map_err(|error| Failure {
            stage: "lower",
            detail: format!("{error:?}"),
            lower: Some(error),
            span_text: error.span.map(|span| format!("{:?}", sources.slice(span))),
        })?;
        println!("SPEC0332 STAGE unit={unit} lower=ok");
        let function = program
            .module(entry.module())
            .and_then(|module| module.function(entry))
            .ok_or_else(|| failure("entry-identity", "returned FunctionId is absent"))?;
        let origin: Span = function.origin.span();
        println!(
            "SPEC0332 ENTRY unit={unit} id={entry:?} name={:?} origin={origin:?} text={:?}",
            function.name,
            sources.slice(origin)
        );
        // Read-only source identity check; no synthetic fact, block or operation.
        if sources.slice(origin).ok() != Some(SOURCE) {
            return Err(failure(
                "entry-identity",
                "entry origin differs from inspect source",
            ));
        }
        println!("SPEC0332 SSA unit={unit}\n{}", render_program(&program));
        if inspect_identity {
            // A failed independent query must not suppress the real verifier/LLVM outcome.
            identity =
                source_identity.and_then(|source| identity::inspect(&program, entry, source));
            println!("SPEC0332 ORIGINAL OWNER QUERY unit={unit} result={identity:?}");
        }
        verify_program(&program).map_err(|error| failure("ssa-verify", error))?;
        println!("SPEC0332 STAGE unit={unit} ssa-verify=ok");
        // with_entry would add a NoArguments process bridge, unrelated to this function.
        let llvm = crate::llvm::render_verified_program(&program)
            .map_err(|error| failure("llvm-render-verify", error))?;
        println!("SPEC0332 LLVM unit={unit}\n{llvm}");
        println!("SPEC0332 STAGE unit={unit} llvm-render-verify=ok");
        Ok(())
    })();
    (
        Observation {
            unit,
            complete_ok,
            ownership_ok,
            frontend_anchor_ok,
            lower_attempted,
            outcome,
        },
        identity,
    )
}

#[test]
fn observe_rc_nullable_while_both_frontends() {
    // Finish both actual Result pipelines before any harness assertion.
    let observations = [observe(false), observe(true)];
    for observation in &observations {
        let unit = observation.unit;
        println!(
            "SPEC0332 PRECONDITIONS unit={unit} complete={} ownership={} frontend_anchor={} lower_attempted={}",
            observation.complete_ok,
            observation.ownership_ok,
            observation.frontend_anchor_ok,
            observation.lower_attempted
        );
        match &observation.outcome {
            Ok(()) => println!("SPEC0332 OUTCOME unit={unit} stage=llvm-render-verify result=ok"),
            Err(error) => println!(
                "SPEC0332 OUTCOME unit={unit} stage={} result=err detail={} lower={:?} span_text={:?}",
                error.stage, error.detail, error.lower, error.span_text
            ),
        }
    }
    assert!(
        observations.iter().all(|observation| {
            observation.complete_ok
                && observation.ownership_ok
                && observation.frontend_anchor_ok
                && observation.lower_attempted
        }),
        "both ordinary-source pipelines must finish frontend checks and attempt real lowering"
    );
    println!(
        "SPEC0332 COLLECTOR contract=preconditions-and-lower-attempt-only behavior_oracle=not-asserted"
    );
}

#[test]
fn rc_nullable_while_both_frontends_reach_verified_llvm() {
    // Preserve both real pipeline outcomes before applying the success oracle.
    let observations = [observe(false), observe(true)];
    for observation in &observations {
        let unit = observation.unit;
        println!(
            "SPEC0332 STRICT PRECONDITIONS unit={unit} complete={} ownership={} frontend_anchor={} lower_attempted={}",
            observation.complete_ok,
            observation.ownership_ok,
            observation.frontend_anchor_ok,
            observation.lower_attempted
        );
        match &observation.outcome {
            Ok(()) => {
                println!("SPEC0332 STRICT OUTCOME unit={unit} stage=llvm-render-verify result=ok");
            }
            Err(error) => println!(
                "SPEC0332 STRICT OUTCOME unit={unit} stage={} result=err detail={} lower={:?} span_text={:?}",
                error.stage, error.detail, error.lower, error.span_text
            ),
        }
    }
    assert!(
        observations.iter().all(|observation| {
            observation.complete_ok
                && observation.ownership_ok
                && observation.frontend_anchor_ok
                && observation.lower_attempted
                && observation.outcome.is_ok()
        }),
        "both real inspect pipelines must pass frontend anchors, lowering, SSA verification and LLVM rendering"
    );
}
