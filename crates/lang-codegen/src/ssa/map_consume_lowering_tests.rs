//! SPEC-0291: ordinary source through complete FE, ownership, SSA and in-memory LLVM.
//! This first-red observer collects all twelve paths before asserting success.

use lang_frontend::{
    lexer::lex,
    name_resolution::{
        SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    ownership_checking::{check_compilation_unit_ownership, check_ownership},
    parser::{ParsedFile, parse_file},
    source::SourceMap,
    type_checking::{
        check_compilation_unit_types, check_types, complete_compilation_unit_types,
        complete_file_types, standard_environments,
    },
};

use super::model::Program;

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

#[derive(Debug)]
struct Observation {
    label: String,
    stages: Vec<String>,
    outcome: Result<(), String>,
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

fn lower_single(
    sources: &SourceMap,
    parsed: &ParsedFile,
    stages: &mut Vec<String>,
) -> Result<Program, String> {
    let (environment, types) = standard_environments();
    let names = step(
        "names",
        resolve_names(sources, parsed, &environment),
        stages,
    )?;
    gate(
        "names diagnostics",
        names.diagnostics().is_empty(),
        names.diagnostics(),
        stages,
    )?;
    let typed = step(
        "raw types",
        check_types(sources, parsed, &names, &types),
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
        complete_file_types(sources, parsed, &names, &types, &typed),
        stages,
    )?;
    gate(
        "complete same analysis",
        complete.is_same_analysis(&typed),
        "original typed analysis",
        stages,
    )?;
    let owned = step(
        "ownership",
        check_ownership(sources, parsed, &names, complete.types()),
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
    let result = super::lower_frontend::orchestrate::lower_scalar_file(
        sources,
        parsed,
        &names,
        complete.types(),
        &owned,
    );
    step(
        "lower SSA",
        result.map_err(|error| lowering_error(sources, error)),
        stages,
    )
}

fn lower_unit(
    sources: &SourceMap,
    parsed: &ParsedFile,
    stages: &mut Vec<String>,
) -> Result<Program, String> {
    let (environment, types) = standard_environments();
    let inputs = [SourceUnitInput::new(
        "root",
        "consume.ko",
        parsed.source_id(),
        parsed,
    )];
    let index = step("index", index_compilation_unit(sources, &inputs), stages)?;
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
        .filter(|declaration| declaration.name() == "entry")
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

fn observe(text: &str, unit: bool, stages: &mut Vec<String>) -> Result<(), String> {
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
    let program = if unit {
        lower_unit(&sources, &parsed, stages)?
    } else {
        lower_single(&sources, &parsed, stages)?
    };
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
    Ok(())
}

#[test]
fn map_consume_owned_locals_reach_verified_llvm() {
    let mut observations = Vec::with_capacity(12);
    for (name, text) in CASES {
        for unit in [false, true] {
            let mut stages = Vec::new();
            let outcome = observe(text, unit, &mut stages);
            observations.push(Observation {
                label: format!("{name}/{}", if unit { "unit" } else { "single" }),
                stages,
                outcome,
            });
        }
    }
    // Explicitly read every field so strict Clippy does not depend on derived Debug.
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
        "all twelve ordinary-source observations must reach verified LLVM: {observations:#?}"
    );
}
