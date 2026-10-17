//! CI-only final publication guard and real-source inputs for the assignment guard.
use super::*;
use crate::{
    ast::ExpressionId,
    lexer::lex,
    name_resolution::{SourceUnitId, index_compilation_unit, resolve_compilation_unit_names},
    parser::{Item, NameMarker, ParsedFile, parse_file},
    type_checking::{
        check_compilation_unit_types, complete_compilation_unit_types, standard_environments,
    },
};
use std::collections::BTreeMap;

const BODY: &str = r#"
fun identity(own item: String): String = item
fun run(): Unit {
    val seed = "kept"
    var source = identity(seed)
    val delivered = identity(source)
}
"#;

// All helpers remain within this cfg(test) subtree. No test-only production mutator.
pub(super) struct Fixture {
    pub(super) sources: SourceMap,
    pub(super) parsed: Vec<ParsedFile>,
    paths: Vec<String>,
    pub(super) names: ValidatedCompilationUnitNames,
    pub(super) typed: ValidatedCompilationUnitTypes,
    environment: TypeEnvironment,
}

impl Fixture {
    pub(super) fn new(texts: &[(&str, &str)]) -> Self {
        let mut sources = SourceMap::new();
        let parsed = texts
            .iter()
            .map(|(path, text)| {
                let source = sources.add_source(*path, *text).unwrap();
                let lexed = lex(&sources, source).unwrap();
                assert!(lexed.diagnostics().is_empty(), "{:?}", lexed.diagnostics());
                let parsed = parse_file(&sources, &lexed).unwrap();
                assert!(
                    parsed.diagnostics().is_empty(),
                    "{:?}",
                    parsed.diagnostics()
                );
                parsed
            })
            .collect::<Vec<_>>();
        let paths = texts
            .iter()
            .map(|(path, _)| (*path).to_owned())
            .collect::<Vec<_>>();
        let inputs = parsed
            .iter()
            .zip(&paths)
            .map(|(parsed, path)| {
                SourceUnitInput::new("root", path.as_str(), parsed.source_id(), parsed)
            })
            .collect::<Vec<_>>();
        let (ne, environment) = standard_environments();
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &ne)
            .unwrap()
            .validate()
            .unwrap();
        let raw = check_compilation_unit_types(&sources, &inputs, &names, &environment).unwrap();
        assert!(raw.diagnostics().is_empty(), "{:?}", raw.diagnostics());
        let complete =
            complete_compilation_unit_types(&sources, &inputs, &names, &environment, &raw).unwrap();
        assert!(complete.is_same_analysis(&raw));
        let typed = raw.validate().unwrap();
        Self {
            sources,
            parsed,
            paths,
            names,
            typed,
            environment,
        }
    }

    pub(super) fn inputs(&self) -> Vec<SourceUnitInput<'_>> {
        self.parsed
            .iter()
            .zip(&self.paths)
            .map(|(parsed, path)| {
                SourceUnitInput::new("root", path.as_str(), parsed.source_id(), parsed)
            })
            .collect()
    }

    pub(super) fn source(&self, index: usize) -> SourceUnitId {
        self.names
            .names()
            .index()
            .source_units()
            .iter()
            .find(|source| source.source_id() == self.parsed[index].source_id())
            .unwrap()
            .id()
    }

    pub(super) fn symbol(&self, index: usize, name: &str) -> UnitSymbolId {
        let source = self.source(index);
        let resolution = self.names.names().source_units()[source.index()].resolution();
        let symbols = resolution
            .symbols()
            .iter()
            .filter(|symbol| symbol.name() == name)
            .collect::<Vec<_>>();
        assert_eq!(symbols.len(), 1, "unique real symbol: {name}");
        UnitSymbolId::new(source, symbols[0].id())
    }

    pub(super) fn local(&self, index: usize, name: &str) -> UnitSymbolId {
        let symbol = self.symbol(index, name);
        let resolution =
            self.names.names().source_units()[symbol.source_unit().index()].resolution();
        let span = resolution.symbols()[symbol.symbol().index()].span();
        assert_eq!(self.parsed[index].ast().items().iter().filter(|(_, node)| {
            matches!(node.payload(), Item::Variable { name: NameMarker::Present(marker), .. } if *marker == span)
        }).count(), 1, "local must have its exact real Variable NameMarker");
        symbol
    }

    pub(super) fn expression(&self, index: usize, text: &str) -> ExpressionId {
        let matches = self.parsed[index]
            .ast()
            .expressions()
            .iter()
            .filter_map(|(id, node)| {
                (self.sources.slice(node.span()).unwrap() == text).then_some(id)
            })
            .collect::<Vec<_>>();
        assert_eq!(matches.len(), 1, "unique real expression: {text}");
        matches[0]
    }

    pub(super) fn shared(&self) -> CompilationUnitOwnership {
        analysis::analyze(
            &self.sources,
            &self.inputs(),
            &self.names,
            &self.environment,
            self.typed.types(),
            false,
        )
        .unwrap()
    }

    pub(super) fn parts(&self, shared: &CompilationUnitOwnership) -> Parts {
        assert!(shared.is_compatible_with(&self.typed));
        let inputs = self.inputs();
        let arguments =
            contracts::collect_call_argument_contracts(&inputs, &self.names, self.typed.types())
                .unwrap();
        let receivers =
            contracts::collect_call_receiver_contracts(&inputs, &self.names, self.typed.types())
                .unwrap();
        assert_eq!(arguments, shared.call_argument_contracts());
        assert_eq!(receivers, shared.call_receiver_contracts());
        let capture = capture::analyze(&inputs, &self.names, self.typed.types()).unwrap();
        assert_eq!(capture.captures, shared.captures());
        assert_eq!(capture.closures, shared.closures());
        Parts {
            bindings: shared
                .bindings()
                .iter()
                .map(|binding| (binding.symbol(), *binding))
                .collect(),
            arguments,
            receivers,
            capture,
        }
    }

    pub(super) fn dataflow(&self, parts: &Parts) -> dataflow::Analysis {
        dataflow::analyze(
            &self.sources,
            &self.inputs(),
            &self.names,
            self.typed.types(),
            false,
            &parts.bindings,
            dataflow::CallInputs::new(&parts.arguments, &parts.receivers),
            dataflow::ClosureInputs::new(
                &parts.capture.captures,
                &parts.capture.closures,
                &parts.capture.transferabilities,
            ),
        )
        .unwrap()
    }

    fn publish(&self, parts: Parts, dataflow: dataflow::Analysis) -> CompilationUnitOwnership {
        CompilationUnitOwnership::new(
            self.typed.types(),
            parts.bindings.into_values().collect(),
            parts.arguments,
            parts.receivers,
            parts.capture,
            dataflow,
        )
    }
}

pub(super) struct Parts {
    pub(super) bindings: BTreeMap<UnitSymbolId, UnitOwnershipBindingDescriptor>,
    pub(super) arguments: Vec<UnitCallArgumentOwnershipContract>,
    pub(super) receivers: Vec<UnitCallReceiverOwnershipContract>,
    pub(super) capture: capture::Analysis,
}

#[test]
fn formed_history_final_gate_preserves_source_identity_and_clears_incomplete_results() {
    let a = format!("package a\n{BODY}");
    let b = format!("package b\n{BODY}");
    let fixture = Fixture::new(&[("a/a.ko", &a), ("b/b.ko", &b)]);
    let baseline = fixture.shared();
    assert!(baseline.diagnostics().is_empty());
    assert!(baseline.deferred().is_empty());
    baseline.clone().validate().unwrap();
    let mut expected = Vec::new();
    for index in 0..2 {
        for name in ["seed", "source", "delivered"] {
            expected.push(fixture.local(index, name));
        }
    }
    expected.sort();
    assert_eq!(baseline.formed_owned_locals(), expected);
    assert!(expected.windows(2).all(|pair| pair[0] < pair[1]));
    for name in ["seed", "source", "delivered"] {
        let left = fixture.local(0, name);
        let right = fixture.local(1, name);
        assert_eq!(left.symbol(), right.symbol(), "same local index fixture");
        assert_ne!(left, right, "source identity must not be deduplicated away");
    }
    for index in 0..2 {
        for name in ["seed", "source"] {
            let symbol = fixture.local(index, name);
            assert!(
                baseline.value_deliveries().iter().any(|fact| {
                    fact.kind() == UnitValueDeliveryKind::Move
                        && fact.place().is_some_and(|place| place.root() == symbol)
                }),
                "history includes actual moved source: {name}"
            );
        }
    }
    let parts = fixture.parts(&baseline);
    assert!(!parts.bindings.is_empty());
    assert_eq!(parts.arguments.len(), 4, "two real own calls per source");
    let raw = fixture.dataflow(&parts);
    assert!(raw.diagnostics.is_empty());
    assert!(raw.deferred.is_empty());
    assert_eq!(raw.formed_owned_locals, expected);
    let published = fixture.publish(parts, raw);
    assert_eq!(published.formed_owned_locals(), expected);
    assert!(published.is_compatible_with(&fixture.typed));

    // Produce a genuine same-source L0133, keeping the successful baseline inputs intact.
    let mut reason_parts = fixture.parts(&baseline);
    let item = fixture.symbol(0, "item");
    let binding = reason_parts.bindings[&item];
    assert_eq!(binding.kind(), OwnershipBindingKind::Owned);
    reason_parts.bindings.insert(
        item,
        UnitOwnershipBindingDescriptor::new(
            item,
            OwnershipBindingKind::Shared,
            binding.declaration_span(),
        ),
    );
    let reason = fixture.dataflow(&reason_parts);
    assert_eq!(reason.diagnostics.len(), 1);
    assert!(reason.deferred.is_empty());
    let diagnostic = reason.diagnostics[0].clone();
    assert_eq!(diagnostic.code().to_string(), "L0133");
    let return_item = fixture.expression(0, "item");
    assert_eq!(
        diagnostic.primary_span(),
        fixture.parsed[0]
            .ast()
            .expressions()
            .get(return_item)
            .unwrap()
            .span()
    );
    let expression =
        UnitExpressionId::new(fixture.source(1), fixture.expression(1, "identity(source)"));
    let deferred =
        UnitOwnershipDeferredFact::new(expression, OwnershipDeferredReason::MemberReceiver);

    for (has_diagnostic, has_deferred) in [(true, false), (false, true), (true, true)] {
        let parts = fixture.parts(&baseline);
        let mut raw = fixture.dataflow(&parts);
        assert!(raw.diagnostics.is_empty());
        assert!(raw.deferred.is_empty());
        assert_eq!(
            raw.formed_owned_locals, expected,
            "non-vacuous final gate input"
        );
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
        raw.diagnostics = diagnostics.clone();
        raw.deferred = deferred.clone();
        let product = fixture.publish(parts, raw);
        assert!(product.formed_owned_locals().is_empty());
        assert_eq!(product.diagnostics(), diagnostics);
        assert_eq!(product.deferred(), deferred);
        assert!(product.is_compatible_with(&fixture.typed));
    }

    // Natural ownership errors independently cover the upstream dataflow/shared gates.
    let erroneous = "package a\nfun identity(own item: String): String = item\nfun bad(item: String): Unit { val prefix = \"formed\"; val rejected = identity(item) }";
    let fixture = Fixture::new(&[("a/error.ko", erroneous)]);
    let shared = fixture.shared();
    assert_eq!(shared.diagnostics().len(), 1);
    assert_eq!(shared.diagnostics()[0].code().to_string(), "L0133");
    assert!(shared.deferred().is_empty());
    let call = fixture.expression(0, "identity(item)");
    let crate::parser::Expression::Call { arguments, .. } = fixture.parsed[0]
        .ast()
        .expressions()
        .get(call)
        .unwrap()
        .payload()
    else {
        panic!("real failing call");
    };
    assert_eq!(arguments.len(), 1);
    let expected_span = fixture.parsed[0]
        .ast()
        .expressions()
        .get(arguments[0].value)
        .unwrap()
        .span();
    assert_eq!(shared.diagnostics()[0].primary_span(), expected_span);
    assert_eq!(fixture.sources.slice(expected_span).unwrap(), "item");
    let parts = fixture.parts(&shared);
    let raw = fixture.dataflow(&parts);
    assert!(raw.formed_owned_locals.is_empty());
    assert_eq!(raw.diagnostics, shared.diagnostics());
    assert!(raw.deferred.is_empty());
    assert!(shared.formed_owned_locals().is_empty());
}
