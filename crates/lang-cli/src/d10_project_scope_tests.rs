//! Audit the real project loader/full frontend boundary, not a new project capability.
use super::*;
use std::{
    fs,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
struct Directory(std::path::PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "koven-d10-project-scope-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(path.join("src/app")).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("project audit retained: {}", self.0.display());
        } else {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
}
#[test]
fn real_project_keeps_standard_facts_outside_the_split_envelope() {
    for body in [
        "val parentOld = replace(&a, \"parent\"); var inner = childCount; while(inner > 0) { val old = replace(&a, \"next\"); inner -= 1 }; outer -= 1",
        "var inner = childCount; while(inner > 0) { val old = replace(&a, \"next\"); inner -= 1 }; val parentOld = replace(&a, \"parent\"); outer -= 1",
    ] {
        let d = Directory::new();
        let manifest = d.0.join("project.toml");
        fs::write(&manifest, "schema = \"koven.project\"\nversion = 1\n[project]\nname = \"d10-scope\"\nsource-roots = [\"src\"]\n").unwrap();
        fs::write(d.0.join("src/app/a.ko"), format!("package app\nfun run(count: Int, childCount: Int) {{ var a = \"first\"; var b = \"kept\"; var outer = count; while(outer > 0) {{ {body} }}; println(a); println(b) }}")).unwrap();
        fs::write(
            d.0.join("src/app/z.ko"),
            "package app\nfun entry() { val p = 1; val c = 2; run(p,c) }",
        )
        .unwrap();
        let object = d.0.join("entry.o");
        let (snapshot, types) =
            crate::project_build::prepare_project_unit(&manifest, &[&object]).unwrap();
        let inputs = snapshot.inputs();
        assert_eq!(inputs.len(), 5);
        let checked = check_unit_frontend(&snapshot, &types).unwrap();
        let CheckedProducts::Basic { typed, owned } = &checked.products else {
            panic!("record real admission mode; no const coercion")
        };
        let facts = owned.ownership();
        let ledger =
            lang_frontend::ownership_checking::d10_catalog_experiment::source_collection_ledger(
                owned,
            )
            .unwrap();
        assert!(ledger.matches(owned));
        assert_eq!(ledger.argument_contracts(), facts.call_argument_contracts());
        assert_eq!(ledger.receiver_contracts(), facts.call_receiver_contracts());
        assert_eq!(ledger.argument_contracts().len(), 67);

        assert!(ledger.matches(&owned.clone()));
        assert_eq!(
            ledger
                .records()
                .iter()
                .map(|record| record.source())
                .collect::<Vec<_>>(),
            checked
                .names
                .names()
                .index()
                .source_units()
                .iter()
                .map(|source| source.id())
                .collect::<Vec<_>>()
        );
        assert!(
            ledger
                .records()
                .iter()
                .flat_map(|record| record.loans())
                .eq(facts.loans())
        );
        assert!(
            ledger
                .records()
                .iter()
                .flat_map(|record| record.receivers())
                .eq(facts.receiver_facts())
        );
        assert!(
            ledger
                .records()
                .iter()
                .flat_map(|record| record.constructions())
                .eq(facts.construction_plans())
        );
        assert!(
            ledger
                .records()
                .iter()
                .flat_map(|record| record.drops())
                .eq(facts.drops())
        );

        assert!(facts.diagnostics().is_empty());
        assert!(facts.deferred().is_empty());
        for source in checked.names.names().index().source_units() {
            let id = source.id();
            let input = inputs
                .iter()
                .find(|input| input.source_id() == source.source_id())
                .unwrap();
            let counts = (
                input.parsed().roots().len(),
                facts
                    .loans()
                    .iter()
                    .filter(|fact| fact.call().source_unit() == id)
                    .count(),
                facts
                    .call_argument_contracts()
                    .iter()
                    .filter(|fact| fact.call().source_unit() == id)
                    .count(),
                facts
                    .drops()
                    .iter()
                    .filter(|fact| fact.value_origin().source_id() == source.source_id())
                    .count(),
                facts
                    .bindings()
                    .iter()
                    .filter(|fact| fact.symbol().source_unit() == id)
                    .count(),
            );
            // These are executable audit expectations for this exact loader fixture, not a
            // production authorization inferred from a path/name or a per-callable ledger.
            let expected = match source.key().logical_path().unwrap().as_str() {
                "koven/algorithms/ranges.ko" => (12, 42, 36, 0, 24),
                "koven/core/result.ko" => (1, 0, 0, 0, 0),
                "koven/text/text.ko" => (8, 23, 23, 1, 13),
                "app/a.ko" => (1, 4, 6, 4, 2),
                "app/z.ko" => (1, 2, 2, 0, 0),
                other => panic!("unexpected original source {other}"),
            };
            assert_eq!(counts, expected, "{:?}", source.key());
        }
        assert_eq!(
            (
                facts.drops().len(),
                facts.loans().len(),
                facts.call_argument_contracts().len(),
                facts.receiver_facts().len(),
                facts.closures().len(),
                facts.construction_plans().len(),
                facts.ownership_primitives().len()
            ),
            (5, 71, 67, 0, 1, 8, 2)
        );
        let entry = select_unit_entry(
            UnitEntrySelection::Project(&["app".to_owned(), "entry".to_owned()]),
            &inputs,
            checked.names,
            typed.types(),
        )
        .unwrap();
        let unit = owned_compilation_unit_view(
            snapshot.sources(),
            &inputs,
            checked.names,
            &types,
            typed,
            owned,
        )
        .unwrap();
        // Actual ordinary whole-project publication, keeping all inputs and owned facts.
        emit_native_owned_unit_object(&unit, entry, &object).unwrap();
        let nm = Command::new("/usr/bin/nm")
            .args(if cfg!(target_os = "macos") {
                ["-j", "-U"]
            } else {
                ["-j", "--defined-only"]
            })
            .arg(&object)
            .output()
            .unwrap();
        assert!(nm.status.success());
        let symbols = String::from_utf8(nm.stdout).unwrap();
        let source_functions = symbols
            .lines()
            .filter(|symbol| {
                let symbol = symbol.strip_prefix('_').unwrap_or(symbol);
                symbol.strip_prefix('f').is_some_and(|rest| {
                    rest.chars().next().is_some_and(|c| c.is_ascii_digit())
                        && rest.contains(".koven.")
                })
            })
            .collect::<Vec<_>>();
        assert_eq!(source_functions.len(), 2, "{symbols}");
        for expected in [".koven.app.run.d", ".koven.app.entry.d"] {
            assert_eq!(
                source_functions
                    .iter()
                    .filter(|symbol| symbol.contains(expected))
                    .count(),
                1,
                "{symbols}"
            );
        }
        assert_eq!(
            symbols
                .lines()
                .filter(|s| *s
                    == if cfg!(target_os = "macos") {
                        "_main"
                    } else {
                        "main"
                    })
                .count(),
            1
        );
        assert_eq!(
            unit.inputs().len(),
            5,
            "object emission never crops the source view"
        );
        // A separate analysis is only a scope-refusal observation; never combine its facts or
        // nonce with the ordinary project product above to manufacture a sealed capability.
        let current =
            check_compilation_unit_types(snapshot.sources(), &inputs, checked.names, &types)
                .unwrap()
                .validate()
                .unwrap();
        let prepared =
            lang_frontend::ownership_checking::d10_catalog_experiment::prepare_split_named_entry(
                snapshot.sources(),
                &inputs,
                checked.names,
                &types,
                current,
            );
        assert!(matches!(prepared, Err(lang_frontend::ownership_checking::d10_catalog_experiment::PrepareNamedEntryError::Unsupported)));
    }
}
