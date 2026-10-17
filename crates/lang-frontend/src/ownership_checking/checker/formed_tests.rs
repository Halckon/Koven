//! CI-only private publication guard; never execute this mutation locally.
use super::*;
use crate::{
    lexer::lex,
    name_resolution::resolve_names,
    parser::parse_file,
    type_checking::{
        FinalTypeAdmissionError, check_types, complete_file_types, standard_environments,
    },
};

const SOURCE: &str = r#"
fun identity(own item: String): String = item
fun run(): Unit {
    val seed = "kept"
    var source = identity(seed)
    val delivered = identity(source)
    var counter = 0
    counter = 1
}
fun dead(): Unit { return; val deadTail = "dead" }
fun stop(): Nothing = stop()
fun abort(): Unit { val stopped = stop(); val abortTail = "dead" }
"#;

fn local(parsed: &ParsedFile, names: &NameResolution, text: &str) -> SymbolId {
    let markers = parsed
        .ast()
        .items()
        .iter()
        .filter_map(|(_, node)| {
            let Item::Variable {
                name: NameMarker::Present(span),
                ..
            } = node.payload()
            else {
                return None;
            };
            names
                .symbols()
                .iter()
                .find(|symbol| symbol.span() == *span && symbol.name() == text)
        })
        .collect::<Vec<_>>();
    assert_eq!(markers.len(), 1, "unique real variable marker: {text}");
    markers[0].id()
}

#[test]
fn formed_history_is_published_only_after_successful_completion() {
    let mut sources = SourceMap::new();
    let source = sources.add_source("single.ko", SOURCE).unwrap();
    let lexed = lex(&sources, source).unwrap();
    assert!(lexed.diagnostics().is_empty());
    let parsed = parse_file(&sources, &lexed).unwrap();
    assert!(parsed.diagnostics().is_empty());
    let (ne, te) = standard_environments();
    let names = resolve_names(&sources, &parsed, &ne).unwrap();
    assert!(names.diagnostics().is_empty());
    let typed = check_types(&sources, &parsed, &names, &te).unwrap();
    assert!(typed.diagnostics().is_empty());
    let complete = complete_file_types(&sources, &parsed, &names, &te, &typed).unwrap();
    assert!(complete.is_same_analysis(&typed));
    let fresh = || Checker::new(&sources, &parsed, &names, complete.types()).unwrap();
    let baseline = fresh().run().unwrap();
    assert!(baseline.diagnostics().is_empty());
    assert!(baseline.deferred().is_empty());
    let mut expected = ["seed", "source", "delivered", "counter"]
        .map(|name| local(&parsed, &names, name))
        .to_vec();
    expected.sort();
    assert_eq!(baseline.formed_owned_locals(), expected);
    for excluded in ["deadTail", "stopped", "abortTail"] {
        assert!(!expected.contains(&local(&parsed, &names, excluded)));
    }
    // seed and source have moved; formation history must retain both identities.
    assert!(baseline.drops().iter().all(|drop| {
        !matches!(drop.target(), super::super::DropTarget::Named(symbol)
            if symbol == local(&parsed, &names, "seed") || symbol == local(&parsed, &names, "source"))
    }));
    let expression = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| {
            (sources.slice(node.span()).unwrap() == "identity(source)").then_some(id)
        })
        .expect("real successful call expression");
    let deferred = OwnershipDeferredFact::new(expression, OwnershipDeferredReason::MemberReceiver);
    assert_eq!(
        parsed
            .ast()
            .expressions()
            .get(deferred.expression())
            .unwrap()
            .span()
            .source_id(),
        source
    );

    // Obtain a complete production diagnostic on the same parsed/name/typed identity.
    let counter = local(&parsed, &names, "counter");
    let mut rejecting = fresh();
    assert_eq!(
        rejecting.variable_kinds.insert(counter, VariableKind::Val),
        Some(VariableKind::Var)
    );
    let rejection = rejecting.run().unwrap();
    assert_eq!(rejection.diagnostics().len(), 1);
    let diagnostic = rejection.diagnostics()[0].clone();
    assert_eq!(diagnostic.code().to_string(), "L0134");
    assert_eq!(sources.slice(diagnostic.primary_span()).unwrap(), "counter");
    assert_eq!(diagnostic.primary_span().source_id(), source);
    assert!(rejection.deferred().is_empty());

    for (has_diagnostic, has_deferred) in
        [(false, false), (true, false), (false, true), (true, true)]
    {
        let mut checker = fresh();
        checker.formed_owned_locals = baseline
            .formed_owned_locals()
            .iter()
            .rev()
            .copied()
            .collect();
        checker
            .formed_owned_locals
            .extend_from_slice(baseline.formed_owned_locals());
        assert!(!checker.formed_owned_locals.is_empty());
        let diagnostics = if has_diagnostic {
            vec![diagnostic.clone()]
        } else {
            Vec::new()
        };
        let deferred = if has_deferred {
            vec![deferred]
        } else {
            Vec::new()
        };
        checker.diagnostics = diagnostics.clone();
        checker.deferred = deferred.clone();
        let product = checker.run().unwrap();
        assert_eq!(product.diagnostics(), diagnostics);
        assert_eq!(product.deferred(), deferred);
        if has_diagnostic || has_deferred {
            assert!(product.formed_owned_locals().is_empty());
        } else {
            assert_eq!(
                product.formed_owned_locals(),
                expected,
                "sort and deduplicate history"
            );
        }
    }

    // A real typed-error source exercises the separate typed-diagnostics gate.
    let mut sources = SourceMap::new();
    let source = sources
        .add_source(
            "typed_error.ko",
            format!("{SOURCE}\nfun wrong(): Int = true\n"),
        )
        .unwrap();
    let lexed = lex(&sources, source).unwrap();
    assert!(lexed.diagnostics().is_empty());
    let parsed = parse_file(&sources, &lexed).unwrap();
    assert!(parsed.diagnostics().is_empty());
    let (ne, te) = standard_environments();
    let names = resolve_names(&sources, &parsed, &ne).unwrap();
    assert!(names.diagnostics().is_empty());
    let typed = check_types(&sources, &parsed, &names, &te).unwrap();
    assert_eq!(typed.diagnostics().len(), 1);
    assert_eq!(typed.diagnostics()[0].code().to_string(), "L0084");
    assert_eq!(
        sources
            .slice(typed.diagnostics()[0].primary_span())
            .unwrap(),
        "true"
    );
    let original = typed.diagnostics().to_vec();
    let Err(FinalTypeAdmissionError::Diagnostics(rejected)) =
        complete_file_types(&sources, &parsed, &names, &te, &typed)
    else {
        panic!("real type mismatch must retain its diagnostics");
    };
    assert_eq!(rejected, original);
    let product = Checker::new(&sources, &parsed, &names, &typed)
        .unwrap()
        .run()
        .unwrap();
    assert!(
        product.diagnostics().is_empty(),
        "isolate typed gate from ownership errors"
    );
    assert!(product.deferred().is_empty());
    assert!(product.formed_owned_locals().is_empty());
    assert_eq!(typed.diagnostics(), original);
}
