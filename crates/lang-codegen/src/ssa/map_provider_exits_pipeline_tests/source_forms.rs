//! Source mode and transparent Group must preserve the original provider program.
pub(super) mod facts;
pub(super) mod graph;
use super::{
    Report,
    fixture::{Failure, clean, failure},
    pipeline,
};
use lang_frontend::{
    lexer::lex,
    parser::{Statement, parse_file},
    source::SourceMap,
};

pub(crate) struct Case {
    pub label: String,
    pub source: String,
    pub golden: String,
    direct: Option<String>,
}
fn owned_mutable(text: &str) -> String {
    let mut result = String::new();
    for line in text.lines() {
        let line = if line.contains("MutableMap<") {
            line.to_owned()
        } else {
            line.replace("Map<String, String>", "MutableMap<String, String>")
        };
        if line.contains("val result = source.consume()") {
            continue;
        }
        let line = line
            .replace("return result", "return source")
            .replace("mapOf<String, String>()", "mutableMapOf<String, String>()");
        result.push_str(&line);
        result.push('\n');
    }
    result
}
fn grouped(text: &str) -> String {
    let mut sources = SourceMap::new();
    let id = sources.add_source("provider.ko", text).unwrap();
    let lexed = lex(&sources, id).unwrap();
    assert!(lexed.diagnostics().is_empty());
    let parsed = parse_file(&sources, &lexed).unwrap();
    assert!(parsed.diagnostics().is_empty());
    let mut edits = parsed
        .ast()
        .statements()
        .iter()
        .filter_map(|(_, n)| {
            let Statement::For { source, .. } = n.payload() else {
                return None;
            };
            let span = parsed.ast().expressions().get(*source).unwrap().span();
            Some((span.start(), span.end()))
        })
        .collect::<Vec<_>>();
    edits.sort();
    let mut result = text.to_owned();
    for (start, end) in edits.into_iter().rev() {
        result.insert_str(end, ")))");
        result.insert_str(start, "(((");
    }
    result
}
pub(crate) fn sources() -> Vec<Case> {
    let normal_map = include_str!("../../../tests/fixtures/map-provider-0331/map.ko");
    let normal_map_golden = include_str!("../../../tests/fixtures/map-provider-0331/map.stdout");
    let mut direct = vec![
        (
            "map-normal".to_owned(),
            normal_map.to_owned(),
            normal_map_golden.to_owned(),
        ),
        (
            "borrow-map-normal".to_owned(),
            normal_map.replace("own source:", "borrow source:"),
            normal_map_golden.to_owned(),
        ),
        (
            "borrow-mutable-normal".to_owned(),
            include_str!("../../../tests/fixtures/map-provider-0331/mutable-map.ko").to_owned(),
            include_str!("../../../tests/fixtures/map-provider-0331/mutable-map.stdout").to_owned(),
        ),
    ];
    direct.extend(
        super::sources()
            .into_iter()
            .map(|(l, s, g)| (l.to_owned(), s.to_owned(), g.to_owned())),
    );
    direct.extend(
        super::borrowed::sources()
            .into_iter()
            .map(|(l, s, g)| (l.to_owned(), s.to_owned(), g.to_owned())),
    );
    direct.extend(
        super::conditional::sources()
            .into_iter()
            .map(|(l, _, _, s, g)| (l.to_owned(), s.to_owned(), g.to_owned())),
    );
    direct.extend(
        super::temporary::sources()
            .into_iter()
            .map(|(l, _, _, s, g)| (l.to_owned(), s.to_owned(), g.to_owned())),
    );
    direct.extend(
        super::temporary_exits::native_sources()
            .into_iter()
            .map(|(l, s, g)| (l.to_owned(), s.to_owned(), g.to_owned())),
    );
    direct.extend(
        super::local_sources::native_sources()
            .into_iter()
            .map(|(l, s, g)| (l.to_owned(), s.to_owned(), g.to_owned())),
    );
    direct.extend(
        super::nested::sources()
            .into_iter()
            .map(|(l, _, _, s, g)| (l.to_owned(), s.to_owned(), g.to_owned())),
    );
    direct.extend(
        super::nested_exits::native_sources()
            .into_iter()
            .map(|(l, s, g)| (l.to_owned(), s.to_owned(), g.to_owned())),
    );
    let mut cases = Vec::new();
    let mut mutable = vec![(
        "own-mutable-normal".to_owned(),
        owned_mutable(normal_map),
        normal_map_golden.to_owned(),
    )];
    mutable.extend(
        super::sources()
            .into_iter()
            .filter(|(l, _, _)| l.starts_with("map-"))
            .map(|(l, s, g)| {
                (
                    format!("own-mutable-{}", l.trim_start_matches("map-")),
                    owned_mutable(s),
                    g.to_owned(),
                )
            }),
    );
    for (l, s, g) in mutable {
        cases.push(Case {
            label: l.clone(),
            source: s.clone(),
            golden: g.clone(),
            direct: None,
        });
        direct.push((l, s, g));
    }
    cases.extend(direct.into_iter().map(|(l, s, g)| Case {
        label: format!("group-{l}"),
        source: grouped(&s),
        golden: g,
        direct: Some(s),
    }));
    assert_eq!(cases.len(), 41);
    assert_eq!(cases.iter().filter(|c| c.direct.is_some()).count(), 37);
    let labels = cases
        .iter()
        .map(|c| c.label.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(labels.len(), 41);
    for label in [
        "own-mutable-normal",
        "own-mutable-continue",
        "own-mutable-break",
        "own-mutable-return",
    ] {
        assert!(labels.contains(label), "missing {label}");
    }
    cases
}
fn observe(text: &str, unit: bool, expected_owned_mutable: bool) -> Result<String, Failure> {
    let mut sources = SourceMap::new();
    let id = sources
        .add_source("provider.ko", text)
        .map_err(|e| failure("source", e))?;
    let lexed = lex(&sources, id).map_err(|e| failure("lex", e))?;
    clean(&sources, "lex", lexed.diagnostics())?;
    let parsed = parse_file(&sources, &lexed).map_err(|e| failure("parse", e))?;
    clean(&sources, "parse", parsed.diagnostics())?;
    let mut report = Report::default();
    let result = if unit {
        pipeline::unit_observed(
            &sources,
            &parsed,
            "provider.ko",
            &mut report,
            |names, typed, owned, report| {
                facts::unit(&sources, &parsed, names, typed, owned, report)
            },
        )
    } else {
        pipeline::single_observed(
            &sources,
            &parsed,
            &mut report,
            |names, typed, owned, report| {
                facts::single(&sources, &parsed, names, typed, owned, report)
            },
        )
    };
    println!(
        "SOURCE FORMS unit={unit} report={report:#?} outcome={:?}",
        result.as_ref().map(|(_, id)| id)
    );
    let (program, _) = result?;
    if expected_owned_mutable {
        graph::owned_map(&sources, &parsed, &program, true)?;
    }
    assert!(report.passed(), "{report:#?}");
    crate::ssa::verify::verify_program(&program).map_err(|e| failure("SSA", e))?;
    crate::llvm::render_verified_program(&program).map_err(|e| failure("LLVM", e))
}
#[test]
fn owned_mutable_and_group_sources_preserve_verified_programs() {
    let mut results = Vec::new();
    for case in sources() {
        for unit in [false, true] {
            let owned_mutable = case.label.starts_with("own-mutable-")
                || case.label.starts_with("group-own-mutable-");
            let direct = case
                .direct
                .as_ref()
                .map(|s| observe(s, unit, owned_mutable));
            let result = observe(&case.source, unit, owned_mutable);
            let same = match (&direct, &result) {
                (Some(Ok(a)), Ok(b)) => a == b,
                (None, Ok(_)) => true,
                _ => false,
            };
            println!(
                "SOURCE FORMS CASE {} unit={unit} equivalent={same} result={:?}",
                case.label,
                result.as_ref().map(|s| s.len())
            );
            results.push((case.label.clone(), unit, same, result.map(|s| s.len())));
        }
    }
    println!("SOURCE FORMS total={}", results.len());
    assert!(
        results.len() == 82 && results.iter().all(|(_, _, same, r)| *same && r.is_ok()),
        "{results:#?}"
    );
}
