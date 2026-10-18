//! Shared ordinary-source bridge and complete rejection fact registry; no fact mutation.
#![allow(dead_code)]
use lang_frontend::{
    diagnostic::Diagnostic,
    lexer::lex,
    name_resolution::{
        SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    ownership_checking::{
        check_compilation_unit_ownership, check_ownership, validate_iteration_facts,
    },
    parser::parse_file,
    source::{SourceMap, Span},
    type_checking::{
        check_compilation_unit_types, check_types, complete_compilation_unit_types,
        complete_file_types, standard_environments,
    },
};

const PRELUDE: &str = r#"
class Resource { deinit() {} }
class Holder(val action: () -> Unit)
fun read(item: String): Unit {}
fun readResource(item: Resource): Unit {}
fun take(own item: String): Unit {}
fun hold(own action: () -> Unit): Unit {}
"#;

pub struct Case {
    pub body: String,
    pub rejected: Option<(&'static str, &'static str, &'static str)>,
}

fn clean(items: &[Diagnostic], stage: &str) -> Result<(), String> {
    if items.is_empty() {
        Ok(())
    } else {
        Err(format!("{stage}: {items:#?}"))
    }
}

fn assess(
    case: &Case,
    diagnostics: &[Diagnostic],
    expected_span: Option<Span>,
    captures: usize,
    facts_empty: bool,
    qualified: bool,
) -> bool {
    match case.rejected {
        Some((code, _, _)) => {
            matches!(diagnostics, [d] if d.code().to_string() == code && Some(d.primary_span()) == expected_span)
                && facts_empty
                && !qualified
        }
        None => diagnostics.is_empty() && captures > 0 && qualified,
    }
}

pub fn observe(case: &Case, unit: bool) -> Result<bool, String> {
    observe_with(case, unit, |_, _| Ok(()))
}

pub enum Facts<'a> {
    Single(
        &'a lang_frontend::parser::ParsedFile,
        &'a lang_frontend::name_resolution::NameResolution,
        &'a lang_frontend::ownership_checking::OwnershipCheckedFile,
    ),
    Unit(
        &'a lang_frontend::parser::ParsedFile,
        &'a lang_frontend::name_resolution::ValidatedCompilationUnitNames,
        &'a lang_frontend::ownership_checking::CompilationUnitOwnership,
    ),
}

pub fn observe_with(
    case: &Case,
    unit: bool,
    query: impl FnOnce(&SourceMap, Facts<'_>) -> Result<(), String>,
) -> Result<bool, String> {
    let text = format!("{PRELUDE}{}\n", case.body);
    println!("CLOSURE OBLIGATION INPUT unit={unit} source={text:?}");
    let mut sources = SourceMap::new();
    let source = sources
        .add_source("safety-rebuild.ko", &text)
        .map_err(|e| format!("source {e:?}"))?;
    let expected_span = case.rejected.map(|(_, needle, primary)| {
        let at = text.rfind(needle).expect("fixed offending expression")
            + needle
                .rfind(primary)
                .expect("fixed primary within expression");
        sources
            .span(source, at, at + primary.len())
            .expect("original source span")
    });
    let lexed = lex(&sources, source).map_err(|e| format!("lex {e:?}"))?;
    clean(lexed.diagnostics(), "lex")?;
    let file = parse_file(&sources, &lexed).map_err(|e| format!("parse {e:?}"))?;
    clean(file.diagnostics(), "parse")?;
    let (name_environment, environment) = standard_environments();
    if !unit {
        let names = resolve_names(&sources, &file, &name_environment)
            .map_err(|e| format!("names {e:?}"))?;
        clean(names.diagnostics(), "names")?;
        let typed =
            check_types(&sources, &file, &names, &environment).map_err(|e| format!("raw {e:?}"))?;
        clean(typed.diagnostics(), "raw")?;
        let complete = complete_file_types(&sources, &file, &names, &environment, &typed)
            .map_err(|e| format!("complete {e:?}"))?;
        if !complete.is_same_analysis(&typed) {
            return Err("complete identity".into());
        }
        let owned = check_ownership(&sources, &file, &names, complete.types())
            .map_err(|e| format!("ownership {e:?}"))?;
        if !owned.deferred().is_empty() {
            return Err(format!("ownership deferred {:?}", owned.deferred()));
        }
        if !owned.is_compatible_with(&names, &typed) {
            return Err("ownership original analysis identity".into());
        }
        let qualified = owned.diagnostics().is_empty()
            && validate_iteration_facts(&file, &names, &typed, &owned).is_ok();
        // Declaration bindings, closure descriptors and callable provenance are metadata, not execution permits.
        // Default cleanup conditions contain only Always/Never sentinels, with no owners or choices.
        let facts = [
            ("captures", owned.captures().is_empty()),
            ("drops", owned.drops().is_empty()),
            ("loans", owned.loans().is_empty()),
            ("map iterations", owned.map_iterations().is_empty()),
            ("sequential iterations", owned.iterations().is_empty()),
            ("formed locals", owned.formed_owned_locals().is_empty()),
            ("construction", owned.construction_plans().is_empty()),
            ("rc effects", owned.rc_effects().is_empty()),
            ("string effects", owned.string_effects().is_empty()),
            (
                "non-null assertions",
                owned.non_null_assertions().is_empty(),
            ),
            (
                "ownership primitives",
                owned.ownership_primitives().is_empty(),
            ),
            ("field replacement", owned.field_replacements().is_empty()),
            (
                "borrow return origins",
                owned.borrow_return_origins().is_empty(),
            ),
            (
                "borrow bindings",
                owned.borrow_results().bindings().is_empty(),
            ),
            ("borrow ends", owned.borrow_results().ends().is_empty()),
            ("borrow uses", owned.borrow_results().call_uses().is_empty()),
            (
                "range return origins",
                owned.borrow_results().range_return_origins().is_empty(),
            ),
            (
                "forwarded source loans",
                owned.borrow_results().forwarded_source_loans().is_empty(),
            ),
            ("range uses", owned.borrow_results().range_uses().is_empty()),
            (
                "constant materializations",
                owned.constant_materializations().is_none(),
            ),
            ("loan ends", owned.loan_ends().is_empty()),
            ("cleanup steps", owned.cleanup_steps().is_empty()),
            (
                "cleanup conditions",
                owned.cleanup_conditions() == &Default::default(),
            ),
            (
                "cleanup selectors",
                owned.cleanup_conditions().selectors().is_empty(),
            ),
            ("nullable when", owned.nullable_whens().is_empty()),
        ];
        let facts_empty = facts.iter().all(|(_, empty)| *empty);
        println!("single executable fact registry {facts:?}");
        let matched = assess(
            case,
            owned.diagnostics(),
            expected_span,
            owned.captures().len(),
            facts_empty,
            qualified,
        );
        println!(
            "REBUILT single expected_span={expected_span:?} matched={matched} captures={} facts_empty={facts_empty} qualified={qualified} diagnostics={:#?}",
            owned.captures().len(),
            owned.diagnostics()
        );
        if case.rejected.is_none() {
            query(&sources, Facts::Single(&file, &names, &owned))?;
        }
        Ok(matched)
    } else {
        let inputs = [SourceUnitInput::new(
            "root",
            "safety-rebuild.ko",
            source,
            &file,
        )];
        let index =
            index_compilation_unit(&sources, &inputs).map_err(|e| format!("index {e:?}"))?;
        clean(index.diagnostics(), "index")?;
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &name_environment)
            .map_err(|e| format!("names {e:?}"))?
            .validate()
            .map_err(|e| format!("names gate {e:?}"))?;
        let raw = check_compilation_unit_types(&sources, &inputs, &names, &environment)
            .map_err(|e| format!("raw {e:?}"))?;
        clean(raw.diagnostics(), "raw")?;
        let complete =
            complete_compilation_unit_types(&sources, &inputs, &names, &environment, &raw)
                .map_err(|e| format!("complete {e:?}"))?;
        if !complete.is_same_analysis(&raw) {
            return Err("complete identity".into());
        }
        let typed = raw.validate().map_err(|e| format!("type gate {e:?}"))?;
        let owned =
            check_compilation_unit_ownership(&sources, &inputs, &names, &environment, &typed)
                .map_err(|e| format!("ownership {e:?}"))?;
        if !owned.is_compatible_with(&typed) || !owned.deferred().is_empty() {
            return Err(format!(
                "ownership identity/deferred {:?}",
                owned.deferred()
            ));
        }
        // Declaration bindings, closure descriptors and callable provenance are metadata, not execution permits.
        // Default cleanup conditions contain only Always/Never sentinels, with no owners or choices.
        let facts = [
            ("captures", owned.captures().is_empty()),
            ("drops", owned.drops().is_empty()),
            ("loans", owned.loans().is_empty()),
            ("map iterations", owned.map_iterations().is_empty()),
            ("sequential iterations", owned.iterations().is_empty()),
            ("formed locals", owned.formed_owned_locals().is_empty()),
            ("construction", owned.construction_plans().is_empty()),
            ("rc effects", owned.rc_effects().is_empty()),
            ("string effects", owned.string_effects().is_empty()),
            (
                "non-null assertions",
                owned.non_null_assertions().is_empty(),
            ),
            (
                "ownership primitives",
                owned.ownership_primitives().is_empty(),
            ),
            ("field replacement", owned.field_replacements().is_empty()),
            (
                "borrow return origins",
                owned.borrow_return_origins().is_empty(),
            ),
            (
                "borrow bindings",
                owned.borrow_results().bindings().is_empty(),
            ),
            ("borrow ends", owned.borrow_results().ends().is_empty()),
            ("borrow uses", owned.borrow_results().call_uses().is_empty()),
            (
                "range return origins",
                owned.borrow_results().range_return_origins().is_empty(),
            ),
            (
                "forwarded source loans",
                owned.borrow_results().forwarded_source_loans().is_empty(),
            ),
            ("range uses", owned.borrow_results().range_uses().is_empty()),
            ("value deliveries", owned.value_deliveries().is_empty()),
            ("receiver facts", owned.receiver_facts().is_empty()),
            (
                "conditional receiver delivery",
                owned.conditional_receiver_deliveries().is_empty(),
            ),
            (
                "conditional receiver drop",
                owned.conditional_receiver_drops().is_empty(),
            ),
            ("delegations", owned.delegations().is_empty()),
        ];
        let facts_empty = facts.iter().all(|(_, empty)| *empty);
        println!("unit executable fact registry {facts:?}");
        let diagnostics = owned.diagnostics().to_vec();
        let captures = owned.captures().len();
        if case.rejected.is_none() {
            query(&sources, Facts::Unit(&file, &names, &owned))?;
        }
        let qualified = owned.validate().is_ok();
        let matched = assess(
            case,
            &diagnostics,
            expected_span,
            captures,
            facts_empty,
            qualified,
        );
        println!(
            "REBUILT unit expected_span={expected_span:?} matched={matched} captures={captures} facts_empty={facts_empty} qualified={qualified} diagnostics={diagnostics:#?}"
        );
        Ok(matched)
    }
}
