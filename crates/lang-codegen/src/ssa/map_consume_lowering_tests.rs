//! SPEC-0291: ordinary source through complete FE, ownership, SSA and in-memory LLVM.
//! All twelve original paths are collected before success or identity assertions.
//! The single-source column now uses MemoryDocument Unit facts, not Legacy analysis.

use lang_frontend::{
    lexer::lex,
    name_resolution::{
        SourceUnitInput, SourceUnitKey, SymbolKind, index_compilation_unit,
        resolve_compilation_unit_names,
    },
    ownership_checking::check_compilation_unit_ownership,
    parser::{ParsedFile, parse_file},
    source::SourceMap,
    type_checking::{
        check_compilation_unit_types, complete_compilation_unit_types, standard_environments,
    },
};

use super::model::Program;

mod identity;

const CASES: [(&str, &str); 6] = [
    (
        "int_int_val_empty_group",
        r#"fun entry(): Unit {
 val source = mutableMapOf<Int, Int>()
 val result = (source).consume()
 if (result.size == 0) { println("empty int map") }
}"#,
    ),
    (
        "int_int_var_nonempty_overwrite",
        r#"fun entry(): Unit {
 var source = mutableMapOf<Int, Int>()
 source.put(7, 70)
 source.put(7, 71)
 val result = source.consume()
 if (result.get(7)!! == 71) { println("overwritten int value") }
 if (result.size == 1) { println("one int entry") }
}"#,
    ),
    (
        "string_string_val_empty",
        r#"fun entry(): Unit {
 val source = mutableMapOf<String, String>()
 val result = source.consume()
 if (result.size == 0) { println("empty string map") }
}"#,
    ),
    (
        "string_string_var_nonempty_overwrite_group",
        r#"fun entry(): Unit {
 var source = mutableMapOf<String, String>()
 source.put("key".clone(), "old".clone())
 source.put("key".clone(), "new".clone())
 val result = (source).consume()
 if (result.size == 1) { println("one string entry") }
}"#,
    ),
    (
        "int_int_discard_nonempty",
        r#"fun entry(): Unit {
 var source = mutableMapOf<Int, Int>()
 source.put(3, 30)
 source.consume()
 println("discarded int map")
}"#,
    ),
    (
        "string_string_discard_nonempty",
        r#"fun entry(): Unit {
 var source = mutableMapOf<String, String>()
 source.put("key".clone(), "value".clone())
 source.consume()
 println("discarded string map")
}"#,
    ),
];

struct Observation {
    label: String,
    stages: Vec<String>,
    outcome: Result<(), String>,
    produced: Option<Produced>,
}

struct Produced {
    program: Program,
    llvm: String,
    sources: SourceMap,
}

fn step<T, E: std::fmt::Debug>(
    stage: &str,
    result: Result<T, E>,
    stages: &mut Vec<String>,
) -> Result<T, String> {
    match result {
        Ok(value) => {
            stages.push(format!("{stage}: reached"));
            Ok(value)
        }
        Err(error) => Err(format!("{stage}: {error:?}")),
    }
}

fn gate(
    stage: &str,
    valid: bool,
    detail: impl std::fmt::Debug,
    stages: &mut Vec<String>,
) -> Result<(), String> {
    if valid {
        stages.push(format!("{stage}: passed"));
        Ok(())
    } else {
        Err(format!("{stage}: {detail:?}"))
    }
}

fn lower_source(
    sources: &SourceMap,
    parsed: &ParsedFile,
    rooted: bool,
    stages: &mut Vec<String>,
) -> Result<Program, String> {
    let (environment, types) = standard_environments();
    let inputs = [if rooted {
        SourceUnitInput::new("root", "consume.ko", parsed.source_id(), parsed)
    } else {
        SourceUnitInput::memory_document(parsed.source_id(), parsed)
    }];
    let index = step("index", index_compilation_unit(sources, &inputs), stages)?;
    gate(
        "one original source with the selected Unit input identity",
        matches!(index.source_units(), [source]
        if source.source_id() == parsed.source_id()
            && if rooted {
                source.key().root().is_some_and(|root| root.as_str() == "root")
                    && source.key().logical_path().is_some_and(|path| path.as_str() == "consume.ko")
            } else {
                matches!(source.key(), SourceUnitKey::MemoryDocument)
            }),
        index.source_units(),
        stages,
    )?;
    let source_unit = index.source_units()[0].id();
    let names = step(
        "names",
        resolve_compilation_unit_names(sources, &inputs, &index, &environment),
        stages,
    )?;
    gate(
        "names diagnostics",
        names.diagnostics().is_empty(),
        names.diagnostics(),
        stages,
    )?;
    let names = step(
        "qualified names",
        names
            .validate()
            .map_err(|value| format!("{:?}", value.diagnostics())),
        stages,
    )?;
    let typed = step(
        "raw types",
        check_compilation_unit_types(sources, &inputs, &names, &types),
        stages,
    )?;
    gate(
        "raw type diagnostics",
        typed.diagnostics().is_empty(),
        typed.diagnostics(),
        stages,
    )?;
    gate(
        "one real consume descriptor",
        typed.map_consumes().len() == 1,
        typed.map_consumes(),
        stages,
    )?;
    let complete = step(
        "complete types",
        complete_compilation_unit_types(sources, &inputs, &names, &types, &typed),
        stages,
    )?;
    gate(
        "complete same analysis",
        complete.is_same_analysis(&typed),
        "original typed analysis",
        stages,
    )?;
    let typed = step(
        "qualified types",
        typed
            .validate()
            .map_err(|value| format!("{:?}", value.diagnostics())),
        stages,
    )?;
    let owned = step(
        "ownership",
        check_compilation_unit_ownership(sources, &inputs, &names, &types, &typed),
        stages,
    )?;
    gate(
        "ownership diagnostics",
        owned.diagnostics().is_empty(),
        owned.diagnostics(),
        stages,
    )?;
    gate(
        "ownership completeness",
        owned.deferred().is_empty(),
        owned.deferred(),
        stages,
    )?;
    let owned = step(
        "qualified ownership",
        owned
            .validate()
            .map_err(|value| format!("{:?}", value.diagnostics())),
        stages,
    )?;
    let entries: Vec<_> = names
        .names()
        .index()
        .declarations()
        .iter()
        .filter(|declaration| {
            declaration.name() == "entry"
                && declaration.kind() == SymbolKind::Function
                && declaration.source_unit() == source_unit
                && declaration.name_span().source_id() == parsed.source_id()
        })
        .map(|declaration| declaration.id())
        .collect();
    gate(
        "one actual entry declaration",
        entries.len() == 1,
        &entries,
        stages,
    )?;
    let result = super::unit_lower::lower_scalar_unit_with_entry(
        sources, &inputs, &names, &types, &typed, &owned, entries[0],
    );
    step(
        "lower SSA",
        result.map_err(|error| lowering_error(sources, error)),
        stages,
    )
    .map(|(program, _)| program)
}

fn lowering_error(sources: &SourceMap, error: super::LoweringError) -> String {
    let origin = error
        .span
        .map(|span| (sources.source_name(span.source_id()), sources.slice(span)));
    format!("{error:?}; real source origin={origin:?}")
}

fn observe(text: &str, unit: bool, stages: &mut Vec<String>) -> Result<Produced, String> {
    let mut sources = SourceMap::new();
    let source = step("source", sources.add_source("consume.ko", text), stages)?;
    let lexed = step("lex", lex(&sources, source), stages)?;
    let parsed = step("parse", parse_file(&sources, &lexed), stages)?;
    gate(
        "parse diagnostics",
        parsed.diagnostics().is_empty(),
        parsed.diagnostics(),
        stages,
    )?;
    let program = lower_source(&sources, &parsed, unit, stages)?;
    step(
        "verify SSA",
        super::verify::verify_program(&program),
        stages,
    )?;
    // Read only the real program. Do not infer or manufacture owner identities.
    stages.push(format!("actual SSA:\n{}", super::render_program(&program)));
    let llvm = step(
        "in-memory LLVM",
        crate::llvm::render_verified_program(&program),
        stages,
    )?;
    let headers: Vec<_> = llvm
        .lines()
        .filter(|line| line.starts_with("%koven.map.t"))
        .collect();
    stages.push(format!(
        "actual LLVM Map headers: {headers:?}; bytes={}",
        llvm.len()
    ));
    Ok(Produced {
        program,
        llvm,
        sources,
    })
}

#[test]
fn map_consume_owned_locals_reach_verified_llvm() {
    let mut observations = Vec::with_capacity(12);
    for (name, text) in CASES {
        for unit in [false, true] {
            let mut stages = Vec::new();
            let (outcome, produced) = match observe(text, unit, &mut stages) {
                Ok(produced) => (Ok(()), Some(produced)),
                Err(error) => (Err(error), None),
            };
            observations.push(Observation {
                label: format!("{name}/{}", if unit { "unit" } else { "single" }),
                stages,
                outcome,
                produced,
            });
        }
    }
    // Preserve the complete original stage observations before any assertions.
    for observation in &observations {
        println!(
            "MAP CONSUME {}\n{}\noutcome={:?}",
            observation.label,
            observation.stages.join("\n"),
            observation.outcome
        );
    }
    assert_eq!(observations.len(), 12);
    assert!(
        observations
            .iter()
            .all(|observation| observation.outcome.is_ok()),
        "all twelve ordinary-source observations must reach verified LLVM; see complete observations above"
    );
    // Identity queries run only after every source and both entries were observed.
    for (index, observation) in observations.iter().enumerate() {
        let produced = observation
            .produced
            .as_ref()
            .expect("successful real output");
        let (_, text) = CASES[index / 2];
        identity::assert_identity(
            &observation.label,
            text,
            &produced.sources,
            &produced.program,
            &produced.llvm,
        );
    }
}
