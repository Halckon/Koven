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
    prefix_summary_keys: usize,
    bypassed_keys_materialized: usize,
    bypassed_restore_keys: usize,
    cached_flow_update_keys: usize,
    pending_scan_keys: usize,
    pending_snapshot_keys: usize,
    version_snapshot_copies: usize,
    flow_fact_copies: usize,
    write_version_copies: usize,
    invalidation_fact_copies: usize,
    version_merge_visits: usize,
    fact_intersection_visits: usize,
    logical_publish_version_copies: usize,
    logical_publish_fact_copies: usize,
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

pub(super) fn record_prefix_summary_keys(keys: usize) {
    record(|visits| visits.prefix_summary_keys += keys);
}

pub(super) fn record_bypassed_keys_materialized(count: usize) {
    record(|visits| visits.bypassed_keys_materialized += count);
}

pub(super) fn record_bypassed_restore_key() {
    record(|visits| visits.bypassed_restore_keys += 1);
}

pub(super) fn record_cached_flow_update_keys(count: usize) {
    record(|visits| visits.cached_flow_update_keys += count);
}

pub(super) fn record_pending_scan_keys(count: usize) {
    record(|visits| visits.pending_scan_keys += count);
}

pub(super) fn record_pending_snapshot_keys(count: usize) {
    record(|visits| visits.pending_snapshot_keys += count);
}

pub(super) fn record_flow_version_copies(count: usize) {
    record(|visits| visits.version_snapshot_copies += count);
}

pub(super) fn record_flow_fact_copies(count: usize) {
    record(|visits| visits.flow_fact_copies += count);
}

pub(super) fn record_invalidation_copies(versions: usize, facts: usize) {
    record(|visits| {
        visits.write_version_copies += versions;
        visits.invalidation_fact_copies += facts;
    });
}

pub(super) fn record_flow_merge_visits(versions: usize, facts: usize) {
    record(|visits| {
        visits.version_merge_visits += versions;
        visits.fact_intersection_visits += facts;
    });
}

pub(super) fn current_flow_copies() -> (usize, usize) {
    VISITS.with(|counter| {
        let visits = counter.get().unwrap_or_default();
        (visits.version_snapshot_copies, visits.flow_fact_copies)
    })
}

pub(super) fn record_logical_publish_copies((versions, facts): (usize, usize)) {
    record(|visits| {
        visits.logical_publish_version_copies += visits.version_snapshot_copies - versions;
        visits.logical_publish_fact_copies += visits.flow_fact_copies - facts;
    });
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

#[test]
fn short_circuit_updates_summary_using_only_rhs_write_keys() {
    for operator in ["&&", "||"] {
        for writes_on_true in [false, true] {
            for distinct in [false, true] {
                for terms in [16, 32, 64, 128] {
                    let mut source = String::from(
                        "fun clear(inout value: Int?): Boolean { value = null; return true }
                         fun accept(value: Int): Boolean = true
                         fun probe(input: Int?, flag: Boolean): Boolean {
",
                    );
                    for key in 0..terms {
                        source.push_str(&format!(
                            "var x{key} = input; if (x{key} == null) return false; "
                        ));
                    }
                    let chain = (0..terms)
                        .map(|term| {
                            let key = if distinct { term } else { 0 };
                            if writes_on_true {
                                format!("(flag && (clear(&x{key}) || true))")
                            } else {
                                format!("(flag || (clear(&x{key}) && false))")
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(&format!(" {operator} "));
                    let bypasses_writes = (operator == "&&") != writes_on_true;
                    let tail = if bypasses_writes {
                        format!("accept(x0) {operator} accept(x{})", terms - 1)
                    } else {
                        format!("flag {operator} flag")
                    };
                    source.push_str(&format!("return {chain} {operator} {tail}; }}"));
                    let visits = measure_source(source);
                    eprintln!(
                        "{operator}, writes_on_true={writes_on_true}, distinct={distinct}, {terms}: {} summary keys, {} restore visits, {} cached updates; pending scans={}, pending snapshots={}, version node copies={}, fact node copies={}",
                        visits.prefix_summary_keys,
                        visits.bypassed_restore_keys,
                        visits.cached_flow_update_keys,
                        visits.pending_scan_keys,
                        visits.pending_snapshot_keys,
                        visits.version_snapshot_copies,
                        visits.flow_fact_copies,
                    );
                    if distinct && !bypasses_writes {
                        eprintln!(
                            "actual-write {operator} {terms}: version node copies={}, invalidation fact node copies={}, version merge visits={}, fact intersection visits={}, logical publish node copies={}/{}",
                            visits.write_version_copies,
                            visits.invalidation_fact_copies,
                            visits.version_merge_visits,
                            visits.fact_intersection_visits,
                            visits.logical_publish_version_copies,
                            visits.logical_publish_fact_copies,
                        );
                    }
                    assert_eq!(
                        visits.bypassed_keys_materialized, 0,
                        "{operator}, writes_on_true={writes_on_true}, distinct={distinct}, {terms}: {visits:?}"
                    );
                    if bypasses_writes && distinct {
                        assert!(
                            visits.bypassed_restore_keys <= 2 * terms
                                && (1..=2 * terms).contains(&visits.cached_flow_update_keys),
                            "{visits:?}"
                        );
                    } else if !bypasses_writes && distinct {
                        assert_eq!(visits.cached_flow_update_keys, 0, "{visits:?}");
                        let all_copies = visits.version_snapshot_copies + visits.flow_fact_copies;
                        let publication_copies = visits.logical_publish_version_copies
                            + visits.logical_publish_fact_copies;
                        assert!(
                            publication_copies <= 16 * terms
                                && all_copies - publication_copies <= 16 * terms
                                && all_copies
                                    + visits.version_merge_visits
                                    + visits.fact_intersection_visits
                                    <= 32 * terms,
                            "{visits:?}"
                        );
                    }
                    assert!(
                        (1..=2 * terms).contains(&visits.prefix_summary_keys),
                        "{operator}, writes_on_true={writes_on_true}, distinct={distinct}, {terms}: {visits:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn literal_suffix_does_not_restore_bypassed_writes() {
    for (operator, piece, literal) in [
        ("&&", "(flag || (clear(&x{key}) && false))", "true"),
        ("||", "(flag && (clear(&x{key}) || true))", "false"),
    ] {
        for terms in [16, 32, 64] {
            let mut source = String::from(
                "fun clear(inout value: Int?): Boolean { value = null; return true }
                 fun accept(value: Int): Boolean = true
                 fun probe(input: Int?, flag: Boolean): Boolean {
                 ",
            );
            for key in 0..terms {
                source.push_str(&format!(
                    "var x{key} = input; if (x{key} == null) return false; "
                ));
            }
            let separator = format!(" {operator} ");
            let chain = (0..terms)
                .map(|key| piece.replace("{key}", &key.to_string()))
                .collect::<Vec<_>>()
                .join(&separator);
            let tail = separator.clone() + literal;
            let probe = |literals| {
                measure_source(format!(
                    "{source} return {chain}{}{separator}accept(x0){separator}accept(x{}); }}",
                    tail.repeat(literals),
                    terms - 1,
                ))
            };
            let bare = probe(0).bypassed_restore_keys;
            let padded = probe(terms).bypassed_restore_keys;
            eprintln!("{operator}, {terms} literals: {bare} -> {padded} restore visits");
            assert!(bare > 0);
            assert_eq!(bare, padded);
        }
    }
}

#[test]
fn short_circuit_pending_cost_excludes_resolved_variables() {
    for operator in ["&&", "||"] {
        for terms in [16, 32, 64, 128] {
            let mut source = String::from("fun probe(flag: Boolean): Boolean { ");
            for key in 0..terms {
                source.push_str(&format!("var v{key} = flag; "));
            }
            let expression = (0..terms)
                .map(|key| format!("v{key}"))
                .collect::<Vec<_>>()
                .join(&format!(" {operator} "));
            source.push_str(&format!("return {expression}; }}"));
            let visits = measure_source(source);
            eprintln!(
                "{operator}, {terms} resolved variables: {} pending scan keys, {} pending snapshot keys",
                visits.pending_scan_keys, visits.pending_snapshot_keys,
            );
            assert_eq!(
                (visits.pending_scan_keys, visits.pending_snapshot_keys),
                (0, 0)
            );
        }
    }
}

#[test]
fn short_circuit_reads_share_existing_flow_versions() {
    for operator in ["&&", "||"] {
        for terms in [16, 32, 64, 128] {
            let mut source = String::from("fun probe(flag: Boolean): Boolean { ");
            for key in 0..terms {
                source.push_str(&format!("var v{key} = false; v{key} = flag; "));
            }
            let expression = (0..terms)
                .map(|key| format!("v{key}"))
                .collect::<Vec<_>>()
                .join(&format!(" {operator} "));
            source.push_str(&format!("return {expression}; }}"));
            let visits = measure_source(source);
            eprintln!(
                "{operator}, {terms} reads after writes: {} copied version keys",
                visits.version_snapshot_copies,
            );
            assert_eq!(visits.version_snapshot_copies, 0, "{visits:?}");
        }
    }
}

#[test]
fn literal_rhs_leaves_wide_flow_facts_in_place() {
    for (operator, literal) in [("&&", "true"), ("||", "false")] {
        for terms in [16, 32, 64, 128] {
            let mut source = String::from(
                "fun accept(value: Int): Boolean = true
                 fun probe(input: Int?, flag: Boolean): Boolean { ",
            );
            for key in 0..terms {
                source.push_str(&format!(
                    "var x{key} = input; if (x{key} == null) return false; "
                ));
            }
            let suffix = format!(
                "{operator} accept(x0) {operator} accept(x{}); }}",
                terms - 1
            );
            let literals = format!(" {operator} {literal} ").repeat(terms);
            let bare = measure_source(format!("{source} return flag {suffix}"));
            let padded = measure_source(format!("{source} return flag{literals}{suffix}"));
            eprintln!(
                "{operator}, {terms} literal RHS with live facts: {} -> {} copied fact keys",
                bare.flow_fact_copies, padded.flow_fact_copies,
            );
            assert_eq!(bare.flow_fact_copies, padded.flow_fact_copies, "{padded:?}");
        }
    }
}

#[test]
fn readonly_rhs_shares_wide_flow_facts() {
    for operator in ["&&", "||"] {
        for terms in [16, 32, 64, 128] {
            let mut source = String::from(
                "fun accept(value: Int): Boolean = true
                 fun probe(input: Int?): Boolean { ",
            );
            for key in 0..terms {
                source.push_str(&format!(
                    "var x{key} = input; if (x{key} == null) return false; "
                ));
            }
            let expression = (0..terms)
                .map(|key| format!("accept(x{key})"))
                .collect::<Vec<_>>()
                .join(&format!(" {operator} "));
            let bare = measure_source(format!("{source} return accept(x0); }}"));
            let chain = measure_source(format!("{source} return {expression}; }}"));
            eprintln!(
                "{operator}, {terms} calls with live facts: {} -> {} copied fact keys",
                bare.flow_fact_copies, chain.flow_fact_copies,
            );
            assert_eq!(bare.flow_fact_copies, 0, "{bare:?}");
            assert_eq!(bare.flow_fact_copies, chain.flow_fact_copies);
        }
    }
}
