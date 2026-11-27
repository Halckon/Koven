//! Operation counts for the ordinary Unit type-checking entry.

use std::cell::Cell;

use crate::{
    lexer::lex,
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    parser::parse_file,
    source::SourceMap,
};

use super::{check_compilation_unit_types, standard_environments};

#[derive(Clone, Copy, Debug, Default)]
struct Visits {
    condition: usize,
    condition_write_scans: usize,
    writes: usize,
    write_index: usize,
    rhs_reused: usize,
    rhs_abrupt: usize,
    rhs_reachability_mismatch: usize,
}

thread_local! {
    static VISITS: Cell<Option<Visits>> = const { Cell::new(None) };
}

pub(super) fn record_condition_visit() {
    record(|visits| visits.condition += 1);
}

pub(super) fn record_condition_write_scan() {
    record(|visits| visits.condition_write_scans += 1);
}

pub(super) fn record_write_visit() {
    record(|visits| visits.writes += 1);
}

pub(super) fn record_write_index_visit() {
    record(|visits| visits.write_index += 1);
}

pub(super) fn record_rhs_summary(same_reachability: bool, completes: bool) {
    record(|visits| match (same_reachability, completes) {
        (false, _) => visits.rhs_reachability_mismatch += 1,
        (true, false) => visits.rhs_abrupt += 1,
        (true, true) => visits.rhs_reused += 1,
    });
}

fn record(update: impl FnOnce(&mut Visits)) {
    VISITS.with(|counter| {
        if let Some(mut visits) = counter.get() {
            update(&mut visits);
            counter.set(Some(visits));
        }
    });
}

fn measure_chain(terms: usize, setup: &str, seed: &str, operator: &str) -> Visits {
    let flags = format!(" {operator} flag").repeat(terms - 1);
    let text = format!(
        "fun clear(inout value: Int?): Boolean {{ value = null; return true }}
         fun accept(value: Int): Boolean = true
         fun probe(input: Int?, flag: Boolean): Boolean {{
             var x = input
             {setup}
             return {seed}{flags} {operator} accept(x)
         }}"
    );
    measure_source(text)
}

fn measure_source(text: String) -> Visits {
    let mut sources = SourceMap::new();
    let source = sources.add_source("main.ko", text).unwrap();
    let lexed = lex(&sources, source).unwrap();
    let parsed = parse_file(&sources, &lexed).unwrap();
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let inputs = [SourceUnitInput::memory_document(source, &parsed)];
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let (names, types) = standard_environments();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &names)
        .unwrap()
        .validate()
        .unwrap();

    VISITS.set(Some(Visits::default()));
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &types);
    let visits = VISITS.take().unwrap();
    let typed = typed.unwrap();
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    visits
}

#[test]
fn grouped_right_short_circuit_writes() {
    for (seed, operator) in [("x != null", "&&"), ("x == null", "||")] {
        let mut previous = None;
        for terms in [8, 16, 32] {
            let prefix = format!("flag {operator} (").repeat(terms - 1);
            let suffix = ")".repeat(terms - 1);
            let visits = measure_source(format!(
                "fun accept(value: Int): Boolean = true
                 fun probe(input: Int?, flag: Boolean): Boolean {{
                     var x = input
                     if ({seed} {operator} ({prefix}accept(x){suffix})) {{ return true }}
                     return false
                 }}"
            ));
            eprintln!("{operator}, {terms} terms: {visits:?}");
            let work = visits.writes + visits.write_index;
            if let Some(short) = previous {
                assert!(short < work && work <= 3 * short, "{short} -> {work}");
            }
            previous = Some(work);
        }
    }
}

#[test]
fn grouped_right_short_circuit_with_a_write() {
    for (seed, operator) in [("x != null", "&&"), ("x == null", "||")] {
        let mut previous = None;
        for terms in [8, 16, 32] {
            let prefix = format!("flag {operator} (").repeat(terms - 1);
            let suffix = ")".repeat(terms - 1);
            let visits = measure_source(format!(
                "fun clear(inout value: Int?): Boolean {{ value = null; return true }}
                 fun probe(input: Int?, flag: Boolean): Boolean {{
                     var x = input
                     if ({seed} {operator} ({prefix}clear(&x){suffix})) {{ return true }}
                     return false
                 }}"
            ));
            eprintln!("{operator}, {terms} terms with inout: {visits:?}");
            // Only the null comparison carries a narrowing fact into its RHS.
            assert_eq!(visits.condition_write_scans, 1, "{operator}, {terms} terms");
            let work = visits.writes + visits.write_index;
            if let Some(short) = previous {
                assert!(short < work && work <= (5 * short) / 2, "{short} -> {work}");
            }
            previous = Some(work);
        }
    }
}

#[test]
fn grouped_rhs_summary_checks_completion_and_reachability() {
    for (predicate, abrupt, mismatch) in [
        ("true && (true && keep(clear(&x), return 0))", 1, 0),
        (
            "keep(true, return 0) && (flag && (if (flag) {
                loop { clear(&x); break }
                true
            } else { false }))",
            0,
            1,
        ),
    ] {
        let visits = measure_source(format!(
            "fun clear(inout value: Int?): Boolean {{ value = null; return true }}
             fun keep(flag: Boolean, number: Int): Boolean = flag
             fun probe(input: Int?, flag: Boolean): Int {{
                 var x = input
                 if ({predicate}) {{ return 1 }}
                 return 2
             }}"
        ));
        eprintln!("RHS before return: {visits:?}");
        assert_eq!(visits.rhs_abrupt, abrupt, "{visits:?}");
        assert_eq!(visits.rhs_reachability_mismatch, mismatch, "{visits:?}");
        assert_eq!(visits.rhs_reused, 0, "{visits:?}");
    }
}

#[test]
fn grouped_rhs_summary_is_recomputed_in_overload_trials() {
    let visits = measure_source(
        "fun resolve(callback: (String) -> String): String = \"unused\"
         fun resolve(callback: (Int) -> Int): Int = 1
         fun clear(inout value: Int?): Boolean { value = null; return true }
         fun intResult(value: Int): Int = value
         fun probe(flag: Boolean): Int {
             val selected = resolve { value ->
                 var x: Int? = 1
                 if (flag && (flag && clear(&x))) { println(\"hit\") }
                 intResult(value)
             }
             return selected
         }"
        .into(),
    );
    eprintln!("RHS in overload trials: {visits:?}");
    assert!(visits.rhs_reused >= 2, "{visits:?}");
}

#[test]
fn short_circuit_condition_visits_grow_with_added_operands() {
    for (seed, operator) in [("x != null", "&&"), ("x == null", "||")] {
        let short = measure_chain(32, "", seed, operator).condition;
        let long = measure_chain(128, "", seed, operator).condition;
        eprintln!("{operator} condition visits: 32 terms {short}, 128 terms {long}");
        // Four times the input must not rescan every completed prefix.
        assert!(
            0 < short && short < long && long <= 5 * short,
            "{operator}: unexpected condition visits: {short} -> {long}"
        );
    }
}

#[test]
fn short_circuit_write_visits_grow_with_added_operands() {
    let guard = "if (x == null) { return false }";
    for (seed, operator) in [
        ("(flag || (clear(&x) && false))", "&&"),
        ("(flag && (clear(&x) || true))", "||"),
    ] {
        let short = measure_chain(32, guard, seed, operator).writes;
        let long = measure_chain(128, guard, seed, operator).writes;
        eprintln!("{operator} write visits: 32 terms {short}, 128 terms {long}");
        assert!(
            0 < short && short < long && long <= 5 * short,
            "{operator}: unexpected write visits: {short} -> {long}"
        );
    }
}
