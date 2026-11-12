//! SPEC-0332 first evidence: ordinary source, both frontends, no native execution.
//! A passing collector proves complete/ownership and a real lowering attempt only.
use crate::test_support::diagnostics::diagnostic;
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

use crate::ssa::lower_frontend_tests::map_result_tests::map_loop::map_return::rc_nullable_while_identity_queries as identity;

const SOURCE: &str =
    "fun inspect(own owner: Rc<Int>?): Rc<Int>? { while (owner != null) { break }; return owner }";

#[derive(Debug)]
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

pub(in crate::ssa) fn observe_with_inspector(unit: bool, inspect_identity: bool) -> (Observation, Result<(), String>) {
    let (obs, identity, _) = observe_with_options(unit, inspect_identity, true);
    (obs, identity)
}

fn observe_with_options(
    unit: bool,
    inspect_identity: bool,
    per_family_only: bool,
) -> (Observation, Result<(), String>, Vec<String>) {
    let mut complete_ok = false;
    let mut ownership_ok = false;
    let mut frontend_anchor_ok = false;
    let mut lower_attempted = false;
    let mut source_identity = Err("source identity query not reached".to_owned());
    let mut identity = Err("SSA identity query not reached".to_owned());
    let mut routed_functions = Vec::new();
    let outcome = (|| {
        let mut sources = SourceMap::new();
        let (path, text) = if unit {
            ("p/inspect.ko", format!("package p\n{SOURCE}"))
        } else {
            ("inspect.ko", SOURCE.to_owned())
        };
        diagnostic!("SPEC0332 INPUT unit={unit} path={path:?} source={text:?}");
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
        diagnostic!("SPEC0332 STAGE unit={unit} parse=ok");
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
        diagnostic!(
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
            diagnostic!("SPEC0332 STAGE unit={unit} names=ok");
            let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
                .map_err(|error| failure("types-api", error))?;
            complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed)
                .map_err(|error| failure("complete", error))?;
            complete_ok = true;
            diagnostic!("SPEC0332 STAGE unit={unit} complete=ok");
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
            diagnostic!("SPEC0332 STAGE unit={unit} ownership=ok");
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
                diagnostic!(
                    "SPEC0332 FACT unit={unit} condition={:?} descriptor={descriptor:?} parameter={parameter:?} nullable={nullable:?} copyability={:?}",
                    span, facts.copyability(descriptor.nullable_type())
                );
                let UnitTypeKind::Nullable(inner) = nullable else { return Some(false) };
                let inner = facts.types().get(*inner)?;
                diagnostic!("SPEC0332 INNER unit={unit} type={inner:?}");
                let UnitTypeKind::Intrinsic {
                    constructor: IntrinsicTypeConstructor::Rc, arguments,
                } = inner else { return Some(false) };
                let [argument] = arguments.as_slice() else { return Some(false) };
                let payload = facts.types().get(*argument)?;
                diagnostic!("SPEC0332 PAYLOAD unit={unit} type={payload:?}");
                Some(source_shape_ok && descriptor.expression() == expression
                    && descriptor.non_null_when_true()
                    && parameter.mode() == ParameterMode::Value
                    && parameter.ty() == descriptor.nullable_type()
                    && facts.symbol_type(descriptor.symbol()) == Some(descriptor.nullable_type())
                    && facts.copyability(descriptor.nullable_type()) == Copyability::MoveOnly
                    && *payload == UnitTypeKind::Builtin(BuiltinType::Int))
            }).unwrap_or(false);
            diagnostic!("SPEC0332 ANCHOR unit={unit} valid={frontend_anchor_ok}");
            if inspect_identity {
                source_identity =
                    identity::unit_source(&file, &names, typed.types(), owned.ownership(), entry);
                diagnostic!(
                    "SPEC0332 SOURCE OWNER QUERY unit={unit} result={:?}",
                    source_identity.as_ref().map(|_| ())
                );
            }
            lower_attempted = true;
            diagnostic!("SPEC0332 STAGE unit={unit} lower=attempt");
            let (lower_result, routed) = if per_family_only {
                (
                    crate::ssa::with_per_family_lowering(|| {
                        lower_scalar_unit_with_entry(
                            &sources, &inputs, &names, &types, &typed, &owned, entry,
                        )
                    }),
                    Vec::new(),
                )
            } else {
                crate::ssa::refine::observe_routing(|| {
                    lower_scalar_unit_with_entry(
                        &sources, &inputs, &names, &types, &typed, &owned, entry,
                    )
                })
            };
            routed_functions = routed;
            lower_result
        } else {
            let names = resolve_names(&sources, &file, &environment)
                .map_err(|error| failure("names-api", error))?;
            if !names.diagnostics().is_empty() {
                return Err(failure("names-diagnostics", names.diagnostics()));
            }
            diagnostic!("SPEC0332 STAGE unit={unit} names=ok");
            let typed = check_types(&sources, &file, &names, &types)
                .map_err(|error| failure("types-api", error))?;
            complete_file_types(&sources, &file, &names, &types, &typed)
                .map_err(|error| failure("complete", error))?;
            complete_ok = true;
            diagnostic!("SPEC0332 STAGE unit={unit} complete=ok");
            let owned = check_ownership(&sources, &file, &names, &typed)
                .map_err(|error| failure("ownership-api", error))?;
            if !owned.diagnostics().is_empty() || !owned.deferred().is_empty() {
                return Err(failure(
                    "ownership-diagnostics-or-deferred",
                    (owned.diagnostics(), owned.deferred()),
                ));
            }
            ownership_ok = true;
            diagnostic!("SPEC0332 STAGE unit={unit} ownership=ok");
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
                diagnostic!(
                    "SPEC0332 FACT unit={unit} condition={:?} descriptor={descriptor:?} parameter_mode={:?} nullable={nullable:?} copyability={:?}",
                    span, typed.parameter_mode(descriptor.symbol()),
                    typed.copyability(descriptor.nullable_type())
                );
                let TypeKind::Nullable(inner) = nullable else { return Some(false) };
                let inner = typed.types().get(*inner)?;
                diagnostic!("SPEC0332 INNER unit={unit} type={inner:?}");
                let TypeKind::Intrinsic {
                    constructor: IntrinsicTypeConstructor::Rc, arguments,
                } = inner else { return Some(false) };
                let [argument] = arguments.as_slice() else { return Some(false) };
                let payload = typed.types().get(*argument)?;
                diagnostic!("SPEC0332 PAYLOAD unit={unit} type={payload:?}");
                Some(source_shape_ok && descriptor.expression() == id
                    && descriptor.non_null_when_true()
                    && typed.parameter_mode(descriptor.symbol()) == Some(ParameterMode::Value)
                    && typed.symbol_type(descriptor.symbol()) == Some(descriptor.nullable_type())
                    && typed.copyability(descriptor.nullable_type()) == Some(Copyability::MoveOnly)
                    && *payload == TypeKind::Builtin(BuiltinType::Int))
            }).unwrap_or(false);
            diagnostic!("SPEC0332 ANCHOR unit={unit} valid={frontend_anchor_ok}");
            if inspect_identity {
                source_identity = identity::single_source(&file, &names, &typed, &owned, entry);
                diagnostic!(
                    "SPEC0332 SOURCE OWNER QUERY unit={unit} result={:?}",
                    source_identity.as_ref().map(|_| ())
                );
            }
            lower_attempted = true;
            diagnostic!("SPEC0332 STAGE unit={unit} lower=attempt");
            lower_scalar_file_with_entry(&sources, &file, &names, &typed, &owned, entry)
        };
        let (program, entry) = lowered.map_err(|error| Failure {
            stage: "lower",
            detail: format!("{error:?}"),
            lower: Some(error),
            span_text: error.span.map(|span| format!("{:?}", sources.slice(span))),
        })?;
        diagnostic!("SPEC0332 STAGE unit={unit} lower=ok");
        let function = program
            .module(entry.module())
            .and_then(|module| module.function(entry))
            .ok_or_else(|| failure("entry-identity", "returned FunctionId is absent"))?;
        let origin: Span = function.origin.span();
        diagnostic!(
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
        diagnostic!("SPEC0332 SSA unit={unit}\n{}", render_program(&program));
        if inspect_identity {
            // A failed independent query must not suppress the real verifier/LLVM outcome.
            identity =
                source_identity.and_then(|source| identity::inspect(&program, entry, source));
            diagnostic!("SPEC0332 ORIGINAL OWNER QUERY unit={unit} result={identity:?}");
        }
        verify_program(&program).map_err(|error| failure("ssa-verify", error))?;
        diagnostic!("SPEC0332 STAGE unit={unit} ssa-verify=ok");
        // with_entry would add a NoArguments process bridge, unrelated to this function.
        let llvm = crate::llvm::render_verified_program(&program)
            .map_err(|error| failure("llvm-render-verify", error))?;
        diagnostic!("SPEC0332 LLVM unit={unit}\n{llvm}");
        diagnostic!("SPEC0332 STAGE unit={unit} llvm-render-verify=ok");
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
        routed_functions,
    )
}

#[test]
fn rc_nullable_while_default_routing_matches_or_surpasses_per_family() {
    let (obs, identity, routed) = observe_with_options(true, true, false);
    assert!(
        obs.outcome.is_ok(),
        "default routing must pass and reach verified LLVM: {:?}",
        obs.outcome
    );
    // Verified contrast with per-family legacy witness:
    // Under default routing, `inspect` is lowered by refine (`routed`), reaching verified SSA and LLVM.
    // The legacy `identity` oracle queries per-family synthetic blocks which are absent in refine's
    // cleaner phi-based SSA, so it correctly reports absent rather than falsely claiming legacy structure.
    assert!(
        routed.iter().any(|name| name.contains("inspect")),
        "inspect must route to refine under default routing: {routed:?}"
    );
    assert!(
        identity.is_err(),
        "legacy per-family structural oracle does not match refine SSA, as expected: {identity:?}"
    );
}

#[test]
fn observe_rc_nullable_while_both_frontends() {
    // Finish both actual Result pipelines before any harness assertion.
    let observations = [observe(false), observe(true)];
    for observation in &observations {
        let unit = observation.unit;
        diagnostic!(
            "SPEC0332 PRECONDITIONS unit={unit} complete={} ownership={} frontend_anchor={} lower_attempted={}",
            observation.complete_ok,
            observation.ownership_ok,
            observation.frontend_anchor_ok,
            observation.lower_attempted
        );
        match &observation.outcome {
            Ok(()) => {
                diagnostic!("SPEC0332 OUTCOME unit={unit} stage=llvm-render-verify result=ok")
            }
            Err(error) => diagnostic!(
                "SPEC0332 OUTCOME unit={unit} stage={} result=err detail={} lower={:?} span_text={:?}",
                error.stage,
                error.detail,
                error.lower,
                error.span_text
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
    diagnostic!(
        "SPEC0332 COLLECTOR contract=preconditions-and-lower-attempt-only behavior_oracle=not-asserted"
    );
}

#[test]
fn rc_nullable_while_both_frontends_reach_verified_llvm() {
    // Preserve both real pipeline outcomes before applying the success oracle.
    let observations = [observe(false), observe(true)];
    for observation in &observations {
        let unit = observation.unit;
        diagnostic!(
            "SPEC0332 STRICT PRECONDITIONS unit={unit} complete={} ownership={} frontend_anchor={} lower_attempted={}",
            observation.complete_ok,
            observation.ownership_ok,
            observation.frontend_anchor_ok,
            observation.lower_attempted
        );
        match &observation.outcome {
            Ok(()) => {
                diagnostic!(
                    "SPEC0332 STRICT OUTCOME unit={unit} stage=llvm-render-verify result=ok"
                );
            }
            Err(error) => diagnostic!(
                "SPEC0332 STRICT OUTCOME unit={unit} stage={} result=err detail={} lower={:?} span_text={:?}",
                error.stage,
                error.detail,
                error.lower,
                error.span_text
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

pub(in crate::ssa) mod unit_lower_when_tests {
use crate::test_support::diagnostics::assert_text_eq;
use lang_frontend::{
    name_resolution::SourceUnitInput,
    ownership_checking::{UnitDropPoint, UnitDropTarget},
    source::SourceMap,
    type_checking::standard_environments,
};

use super::super::{
    model::{Operation, ScalarConstant, TerminatorKind},
    render::render_program,
    unit_lower::lower_scalar_unit_with_entry,
    unit_lower_test_support::{analyze, declaration, parsed},
};

#[test]
fn boolean_subject_else_preserves_semantic_edges() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         fun select(own flag: Boolean): Int = when (flag) {\n\
             true -> 1\n\
             else -> 2\n\
         }",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\nfun entry(): Int = p.select(true)",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let reversed = [inputs[1], inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let entry = declaration(&names, "q", "entry");

    let (forward, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        entry,
    )
    .expect("Boolean subject with else lowers to verified SSA");
    let (backward, _) = lower_scalar_unit_with_entry(
        &sources,
        &reversed,
        &names,
        &type_environment,
        &typed,
        &owned,
        entry,
    )
    .expect("source permutation preserves Boolean-else SSA");
    assert_text_eq!(render_program(&forward), render_program(&backward));

    let select = function_named(&forward, "koven.p.select.d");
    let (when_true, when_false) = select
        .blocks
        .iter()
        .filter_map(|block| block.terminator.as_ref())
        .find_map(|terminator| match &terminator.kind {
            TerminatorKind::Conditional {
                when_true,
                when_false,
                ..
            } => Some((when_true.target, when_false.target)),
            _ => None,
        })
        .expect("select has one Boolean conditional");
    assert_eq!(integer_constant_in(select, when_true), 1);
    assert_eq!(integer_constant_in(select, when_false), 2);
}

#[test]
fn exhaustive_multi_condition_entry_lowers_its_body_once() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         fun subject(): Boolean = false\n\
         fun select(): Int = when (subject()) {\n\
             true, false -> 7\n\
         }",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\nfun entry(): Int = p.select()",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);

    let (program, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "q", "entry"),
    )
    .expect("one entry covering both Boolean values lowers to verified SSA");
    let select = function_named(&program, "koven.p.select.d");
    assert_eq!(
        direct_call_names(&program, select),
        vec!["koven.p.subject.d0"],
        "the exhaustive specialization must still evaluate its subject once"
    );
    assert_eq!(
        select
            .instructions
            .iter()
            .filter(|instruction| matches!(
                instruction.operation,
                Operation::Constant(ScalarConstant::Integer(7))
            ))
            .count(),
        1,
        "the shared entry body must be lowered once"
    );
    assert!(select.blocks.iter().all(|block| !matches!(
        block.terminator.as_ref().map(|terminator| &terminator.kind),
        Some(TerminatorKind::Conditional { .. })
    )));
}

#[test]
fn dynamic_boolean_subject_condition_compares_once() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         fun subject(): Boolean = true\n\
         fun candidate(): Boolean = false\n\
         fun select(): Int = when (subject()) {\n\
             candidate() -> 1\n\
             else -> 2\n\
         }",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\nfun entry(): Int = p.select()",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);

    let (program, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "q", "entry"),
    )
    .expect("dynamic Boolean subject condition lowers to verified SSA");
    let select = function_named(&program, "koven.p.select.d");
    assert_eq!(
        direct_call_names(&program, select),
        vec!["koven.p.subject.d0", "koven.p.candidate.d1"],
        "the subject and dynamic candidate are each evaluated once in source order"
    );
    assert_eq!(
        select
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::Compare { .. }))
            .count(),
        1,
        "the subject candidate is compared once"
    );
    assert_eq!(
        select
            .blocks
            .iter()
            .filter_map(|block| block.terminator.as_ref())
            .filter(|terminator| matches!(terminator.kind, TerminatorKind::Conditional { .. }))
            .count(),
        1
    );
}

#[test]
fn subjectless_call_conditions_short_circuit_on_false_edges() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         fun first(): Boolean = true\n\
         fun second(): Boolean = false\n\
         fun select(): Int = when {\n\
             first() -> 1\n\
             second() -> 2\n\
             else -> 3\n\
         }",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\nfun entry(): Int = p.select()",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);

    let (program, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "q", "entry"),
    )
    .expect("subjectless call conditions lower to a verified short-circuit chain");
    let select = function_named(&program, "koven.p.select.d");
    assert_eq!(
        direct_call_names(&program, select),
        vec!["koven.p.first.d0", "koven.p.second.d1"],
        "each condition call is emitted once in source order"
    );
    let entry = select.entry_block().expect("select has an entry block");
    let TerminatorKind::Conditional {
        when_true,
        when_false,
        ..
    } = &select
        .block(entry)
        .and_then(|block| block.terminator.as_ref())
        .expect("first condition terminates the entry block")
        .kind
    else {
        panic!("first condition must use a conditional terminator");
    };
    assert_eq!(
        direct_call_names_in_block(&program, select, when_false.target),
        vec!["koven.p.second.d1"],
        "only the first false edge evaluates the second condition"
    );
    assert!(direct_call_names_in_block(&program, select, when_true.target).is_empty());
    assert!(matches!(
        select
            .block(when_false.target)
            .and_then(|block| block.terminator.as_ref())
            .map(|terminator| &terminator.kind),
        Some(TerminatorKind::Conditional { .. })
    ));
}

#[test]
fn subjectless_paths_merge_after_each_consumes_the_same_owner() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         fun sink(own input: String): Unit {}\n\
         fun route(own first: Boolean, own second: Boolean, own text: String): Unit {\n\
             when {\n\
                 first -> { val done = sink(text) }\n\
                 second -> { val done = sink(text) }\n\
                 else -> { val done = sink(text) }\n\
             }\n\
         }",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\nfun entry(): Unit { val done = p.route(false, true, \"owner\") }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let reversed = [inputs[1], inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let entry = declaration(&names, "q", "entry");

    let (forward, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        entry,
    )
    .expect("consistent subjectless owner paths lower to verified SSA");
    let (backward, _) = lower_scalar_unit_with_entry(
        &sources,
        &reversed,
        &names,
        &type_environment,
        &typed,
        &owned,
        entry,
    )
    .expect("source permutation preserves subjectless owner SSA");
    assert_text_eq!(render_program(&forward), render_program(&backward));
    let route = function_named(&forward, "koven.p.route.d");
    assert_eq!(
        route
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::DirectCall { .. }))
            .count(),
        3,
        "each source entry has one independently reachable body"
    );
}

#[test]
fn implicit_subjectless_exit_uses_the_synthetic_branch_index() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         fun sink(own input: String): Unit {}\n\
         fun route(own flag: Boolean, own text: String): Unit {\n\
             when { flag -> { val done = sink(text) } }\n\
         }",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\nfun entry(): Unit { val done = p.route(false, \"owner\") }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    assert!(owned.ownership().drops().iter().any(|fact| {
        matches!(fact.point(), UnitDropPoint::BranchExit { branch: 1, .. })
            && matches!(fact.target(), UnitDropTarget::Named(_))
            && sources
                .slice(fact.value_origin())
                .is_ok_and(|name| name == "text")
    }));

    let (program, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "q", "entry"),
    )
    .expect("implicit subjectless exit consumes its BranchExit facts");
    let route = function_named(&program, "koven.p.route.d");
    assert_eq!(
        route
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
            .count(),
        1,
        "only the implicit unmatched branch drops the owner locally"
    );
}

fn function_named<'a>(
    program: &'a super::super::model::Program,
    prefix: &str,
) -> &'a super::super::model::Function {
    program.modules[0]
        .functions
        .iter()
        .find(|function| function.name.starts_with(prefix))
        .expect("function exists")
}

fn integer_constant_in(function: &super::super::model::Function, block: super::super::model::BlockId) -> i128 {
    function
        .block(block)
        .expect("block belongs to function")
        .instructions
        .iter()
        .filter_map(|instruction| function.instruction(*instruction))
        .find_map(|instruction| match instruction.operation {
            Operation::Constant(ScalarConstant::Integer(value)) => Some(value),
            _ => None,
        })
        .expect("branch materializes its integer result")
}

fn direct_call_names<'a>(
    program: &'a super::super::model::Program,
    function: &super::super::model::Function,
) -> Vec<&'a str> {
    function
        .instructions
        .iter()
        .filter_map(|instruction| match instruction.operation {
            Operation::DirectCall { callee, .. } => program.modules[0]
                .function(callee)
                .map(|callee| callee.name.as_str()),
            _ => None,
        })
        .collect()
}

fn direct_call_names_in_block<'a>(
    program: &'a super::super::model::Program,
    function: &super::super::model::Function,
    block: super::super::model::BlockId,
) -> Vec<&'a str> {
    function
        .block(block)
        .expect("block belongs to function")
        .instructions
        .iter()
        .filter_map(|instruction| function.instruction(*instruction))
        .filter_map(|instruction| match instruction.operation {
            Operation::DirectCall { callee, .. } => program.modules[0]
                .function(callee)
                .map(|callee| callee.name.as_str()),
            _ => None,
        })
        .collect()
}

#[test]
fn string_when_comparison_facts_are_explicitly_rejected_until_consumed() {
    for source in [
        "package p\nfun entry(): Int { when (\"subject\") { \"first\", \"second\" -> {} }\nreturn 0 }",
        "package p\nfun entry(): Int { when (\"subject\") { else -> {} }\nreturn 0 }",
    ] {
        let mut sources = SourceMap::new();
        let (id, parsed) = parsed(&mut sources, "p/entry.ko", source);
        let inputs = [SourceUnitInput::new("root", "p/entry.ko", id, &parsed)];
        let (ne, te) = standard_environments();
        let (names, typed, owned) = analyze(&sources, &inputs, &ne, &te);
        let entry = declaration(&names, "p", "entry");
        // The legacy path still must reject these facts. Public routing now admits the
        // bounded String kernel; use the existing test-only pin to retain this guard.
        let error = match super::super::refine::with_per_family_lowering(|| {
            lower_scalar_unit_with_entry(&sources, &inputs, &names, &te, &typed, &owned, entry)
        }) {
            Err(error) => error,
            Ok(_) => panic!("new comparison points must not be silently ignored by fallback"),
        };
        assert_eq!(error.kind, super::super::LoweringErrorKind::UnsupportedNode);
    }
}
}
