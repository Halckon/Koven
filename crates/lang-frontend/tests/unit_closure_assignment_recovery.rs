//! SPEC-0331: newly written regression harness for four recovered ordinary source inputs.
//! This is not a byte-for-byte restoration of the unavailable original collector.

use lang_frontend::{
    lexer::lex,
    name_resolution::{
        SourceUnitId, SourceUnitInput, SymbolKind, UnitReferenceTarget, UnitSymbolId,
        ValidatedCompilationUnitNames, index_compilation_unit, resolve_compilation_unit_names,
    },
    ownership_checking::{
        CompilationUnitOwnership, UnitClosureCaptureSource, UnitDropPoint, UnitDropTarget,
        UnitIterationCleanupAction, UnitIterationExitKind, check_compilation_unit_ownership,
    },
    parser::{ParsedFile, parse_file},
    source::SourceMap,
    type_checking::{
        UnitExpressionId, check_compilation_unit_types, complete_compilation_unit_types,
        standard_environments,
    },
};

const OWNED: &str = r#"
class Resource { deinit() {} }
fun readResource(item: Resource): Unit {}
fun read(item: String): Unit {}
fun run(own source: String, own resource: Resource): Unit {
  var saved: () -> Unit = move { readResource(resource) }
  saved = ({ read(source) })
  saved()
}
"#;
const ALIAS: &str = r#"
fun read(item: String): Unit {}
fun run(own source: String): Unit {
  var saved: () -> Unit = { read(source) }
  val replacement: () -> Unit = { read(source) }
  saved = (replacement)
  saved()
}
"#;
const SELF: &str = r#"
fun read(item: String): Unit {}
fun run(own source: String): Unit {
  var saved: () -> Unit = { read(source) }
  saved = (saved)
  saved()
}
"#;
const COMPONENTS: &str = r#"
fun read(item: String): Unit {}
fun run(source: Map<String, String>, flag: Boolean): Unit {
  for ((key, value) in source) {
    var saved: () -> Unit = { read(key) }
    saved = ({ read(value) })
    if (flag) { return }
    saved()
  }
}
"#;

fn observe(
    text: &str,
    query: impl FnOnce(
        &SourceMap,
        &ParsedFile,
        &ValidatedCompilationUnitNames,
        SourceUnitId,
        &CompilationUnitOwnership,
    ),
) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("recovery.ko", text).expect("source");
    let lexed = lex(&sources, source).expect("lex");
    assert!(lexed.diagnostics().is_empty());
    let file = parse_file(&sources, &lexed).expect("parse");
    assert!(file.diagnostics().is_empty());
    let inputs = [SourceUnitInput::new("root", "recovery.ko", source, &file)];
    let (name_environment, environment) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).expect("index");
    assert!(index.diagnostics().is_empty());
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &name_environment)
        .expect("names")
        .validate()
        .expect("validated names");
    let raw =
        check_compilation_unit_types(&sources, &inputs, &names, &environment).expect("raw types");
    assert!(raw.diagnostics().is_empty(), "{:?}", raw.diagnostics());
    let complete = complete_compilation_unit_types(&sources, &inputs, &names, &environment, &raw)
        .expect("complete types");
    assert!(complete.is_same_analysis(&raw));
    let typed = raw.validate().expect("typed gate");
    let owned = check_compilation_unit_ownership(&sources, &inputs, &names, &environment, &typed)
        .expect("ownership");
    println!(
        "diagnostics={:#?}\ncaptures={:#?}\ndrops={:#?}\nmap={:#?}",
        owned.diagnostics(),
        owned.captures(),
        owned.drops(),
        owned.map_iterations()
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    let unit = names.names().index().source_units()[0].id();
    query(&sources, &file, &names, unit, &owned);
    owned.validate().expect("complete ownership gate");
}

fn expression(
    sources: &SourceMap,
    file: &ParsedFile,
    unit: SourceUnitId,
    text: &str,
) -> UnitExpressionId {
    let found = file
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| (sources.slice(node.span()) == Ok(text)).then_some(id))
        .collect::<Vec<_>>();
    assert_eq!(found.len(), 1, "unique expression {text}");
    UnitExpressionId::new(unit, found[0])
}
fn symbol(
    names: &ValidatedCompilationUnitNames,
    unit: SourceUnitId,
    name: &str,
    kind: SymbolKind,
) -> UnitSymbolId {
    let found = names.names().source_units()[unit.index()]
        .resolution()
        .symbols()
        .iter()
        .filter(|s| s.name() == name && s.kind() == kind)
        .map(|s| s.id())
        .collect::<Vec<_>>();
    assert_eq!(found.len(), 1, "unique {kind:?} {name}");
    names
        .names()
        .references()
        .iter()
        .find_map(|reference| match reference.target() {
            UnitReferenceTarget::Symbol(symbol)
                if symbol.source_unit() == unit && symbol.symbol() == found[0] =>
            {
                Some(*symbol)
            }
            _ => None,
        })
        .expect("original qualified reference to source symbol")
}
fn source_survives_call(
    sources: &SourceMap,
    file: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    unit: SourceUnitId,
    owned: &CompilationUnitOwnership,
) {
    let source = symbol(names, unit, "source", SymbolKind::ValueParameter);
    let call = expression(sources, file, unit, "saved()");
    let drops = owned
        .drops()
        .iter()
        .filter(|f| f.target() == UnitDropTarget::Named(source))
        .map(|f| f.point())
        .collect::<Vec<_>>();
    assert_eq!(
        drops,
        [UnitDropPoint::CallReturn(call)],
        "captured source must survive replacement until the saved closure returns"
    );
}

#[test]
fn owned_resource_replacement_cleans_old_capture_after_rhs() {
    observe(OWNED, |sources, file, names, unit, owned| {
        let old = expression(sources, file, unit, "move { readResource(resource) }");
        let rhs = expression(sources, file, unit, "({ read(source) })");
        let resource = symbol(names, unit, "resource", SymbolKind::ValueParameter);
        let drops = owned
            .drops()
            .iter()
            .filter(|f| {
                f.target()
                    == UnitDropTarget::Captured {
                        closure: old,
                        source: UnitClosureCaptureSource::Symbol(resource),
                    }
            })
            .map(|f| f.point())
            .collect::<Vec<_>>();
        assert_eq!(
            drops,
            [UnitDropPoint::AfterExpression(rhs)],
            "old Resource capture must be released at the original RHS point"
        );
        source_survives_call(sources, file, names, unit, owned);
    });
}
#[test]
fn shared_alias_assignment_preserves_capture_source_lifetime() {
    observe(ALIAS, source_survives_call);
}
#[test]
fn shared_self_assignment_preserves_capture_source_lifetime() {
    observe(SELF, source_survives_call);
}
#[test]
fn map_component_replacement_ends_new_capture_before_returned_provider() {
    observe(COMPONENTS, |sources, file, names, unit, owned| {
        let new = expression(sources, file, unit, "{ read(value) }");
        let value = symbol(names, unit, "value", SymbolKind::ForBinding);
        let [plan] = owned.map_iterations() else {
            panic!("one actual Map provider")
        };
        let returned = plan
            .exits()
            .iter()
            .find(|e| matches!(e.kind(), UnitIterationExitKind::Return(_)))
            .expect("return exit");
        let end_capture = returned.actions().iter().position(|a| matches!(a, UnitIterationCleanupAction::EndCaptureLoan { closure, source } if *closure == new && *source == UnitClosureCaptureSource::Symbol(value))).expect("return ends the replacement capture");
        let end_value = returned.actions().iter().position(|a| matches!(a, UnitIterationCleanupAction::EndBinding { symbol, .. } if *symbol == value)).expect("return ends value loan");
        assert!(
            end_capture < end_value,
            "closure capture ends before its Map component source"
        );
    });
}

#[test]
fn nested_owned_resource_environment_survives_until_replacement_rhs() {
    let text = r#"
class Resource { deinit() {} }
fun readResource(item: Resource): Unit {}
fun tick(): Unit {}
fun run(own resource: Resource): Unit {
  val inner: () -> Unit = move { readResource(resource) }
  var saved: () -> Unit = move { inner() }
  tick()
  saved = ({})
  saved()
}
"#;
    observe(text, |sources, file, names, unit, owned| {
        let outer = expression(sources, file, unit, "move { inner() }");
        let rhs = expression(sources, file, unit, "({})");
        let inner = symbol(names, unit, "inner", SymbolKind::Variable);
        let drops = owned
            .drops()
            .iter()
            .filter(|fact| {
                fact.target()
                    == UnitDropTarget::Captured {
                        closure: outer,
                        source: UnitClosureCaptureSource::Symbol(inner),
                    }
            })
            .map(|fact| fact.point())
            .collect::<Vec<_>>();
        assert_eq!(
            drops,
            [UnitDropPoint::AfterExpression(rhs)],
            "indirect Resource environment must survive tick and complete replacement RHS"
        );
    });
}

#[test]
fn nested_pure_environment_keeps_asap_cleanup_with_resource_types_in_scope() {
    let text = r#"
class Resource { deinit() {} }
fun read(item: String): Unit {}
fun tick(): Unit {}
fun run(own source: String): Unit {
  val inner: () -> Unit = move { read(source) }
  var saved: () -> Unit = move { inner() }
  tick()
  saved = ({})
  saved()
}
"#;
    observe(text, |sources, file, names, unit, owned| {
        let outer = expression(sources, file, unit, "move { inner() }");
        let inner = symbol(names, unit, "inner", SymbolKind::Variable);
        let declaration = file
            .ast()
            .statements()
            .iter()
            .find_map(|(id, node)| {
                (sources.slice(node.span()) == Ok("var saved: () -> Unit = move { inner() }"))
                    .then_some(id)
            })
            .expect("actual saved declaration statement");
        let points = owned
            .drops()
            .iter()
            .filter(|fact| {
                fact.target()
                    == UnitDropTarget::Captured {
                        closure: outer,
                        source: UnitClosureCaptureSource::Symbol(inner),
                    }
            })
            .map(|fact| fact.point())
            .collect::<Vec<_>>();
        assert_eq!(
            points,
            [UnitDropPoint::AfterStatement(
                lang_frontend::type_checking::UnitStatementId::new(unit, declaration)
            )],
            "actual pure environment must not be delayed merely because another type has deinit"
        );
    });
}

#[test]
fn nested_resource_environment_keeps_lexical_cleanup_after_last_call() {
    let text = r#"
class Resource { deinit() {} }
fun readResource(item: Resource): Unit {}
fun tick(): Unit {}
fun run(own resource: Resource): Unit {
  val inner: () -> Unit = move { readResource(resource) }
  val saved: () -> Unit = move { inner() }
  saved()
  tick()
}
"#;
    observe(text, |sources, file, names, unit, owned| {
        let outer = expression(sources, file, unit, "move { inner() }");
        let inner = symbol(names, unit, "inner", SymbolKind::Variable);
        let body = file
            .ast()
            .statements()
            .iter()
            .find_map(|(id, node)| {
                (matches!(
                    node.payload(),
                    lang_frontend::parser::Statement::Block { .. }
                ) && sources
                    .slice(node.span())
                    .is_ok_and(|text| text.contains("val inner:")))
                .then_some(id)
            })
            .expect("actual run body block");
        let points = owned
            .drops()
            .iter()
            .filter(|fact| {
                fact.target()
                    == UnitDropTarget::Captured {
                        closure: outer,
                        source: UnitClosureCaptureSource::Symbol(inner),
                    }
            })
            .map(|fact| fact.point())
            .collect::<Vec<_>>();
        assert_eq!(
            points,
            [UnitDropPoint::AfterStatement(
                lang_frontend::type_checking::UnitStatementId::new(unit, body)
            )],
            "indirect Resource must survive the call and subsequent tick to lexical scope exit"
        );
    });
}
