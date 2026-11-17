//! Source-qualified observations of the original callable CFG and return fixtures.
use super::*;
use lang_frontend::{
    name_resolution::{
        SourceUnitId, SourceUnitInput, SourceUnitKey, index_compilation_unit,
        resolve_compilation_unit_names,
    },
    ownership_checking::{
        CompilationUnitOwnership, UnitCallableOrigin, check_compilation_unit_ownership,
    },
    type_checking::{
        UnitCallableSignature, UnitCallableTarget, UnitExpressionId, check_compilation_unit_types,
        standard_environments,
    },
};

pub(super) struct Analysis {
    sources: SourceMap,
    parsed: ParsedFile,
    unit: SourceUnitId,
    pub(super) owned: CompilationUnitOwnership,
    signatures: Vec<UnitCallableSignature>,
    typed: lang_frontend::type_checking::CompilationUnitTypes,
    symbols: Vec<(String, SymbolId)>,
}

impl Analysis {
    pub(super) fn assert_factory_preflight(&self) {
        assert_eq!(lambdas(&self.parsed).len(), 1);
        assert!(self.owned.captures().is_empty());
        assert!(self.typed.calls().iter().any(|call| matches!(
            self.typed.types().get(call.return_type()),
            Some(lang_frontend::type_checking::UnitTypeKind::Function { .. })
        )));
    }

    pub(super) fn assert_owned_return_capture_preflight(&self) {
        let lambda = UnitExpressionId::new(self.unit, lambdas(&self.parsed)[0]);
        let captures = self.owned.captures_of(lambda).collect::<Vec<_>>();
        assert_eq!(captures.len(), 1);
        assert_eq!(captures[0].mode(), ClosureCaptureMode::Owned);
    }

    pub(super) fn assert_alias_and_parameter_origins(&self) {
        for value in ["first", "(first)", "alias", "(alias)"] {
            let id = self.expression(value);
            let fact = self
                .owned
                .callable_origin(id)
                .expect("transparent lambda origin");
            assert_eq!(fact.expression(), id);
            assert_eq!(fact.origin(), self.lambda(0));
            assert_eq!(
                fact.span(),
                self.parsed
                    .ast()
                    .expressions()
                    .get(id.expression())
                    .unwrap()
                    .span()
            );
        }
        let signature = self
            .signatures
            .iter()
            .find(|s| s.name() == "relay")
            .unwrap();
        let [parameter] = signature.parameters() else {
            panic!("one relay parameter")
        };
        let parameter = parameter.symbol().expect("actual relay parameter");
        let original = self
            .symbols
            .iter()
            .find(|(name, _)| name == "forwarded")
            .unwrap()
            .1;
        assert_eq!(parameter.source_unit(), self.unit);
        assert_eq!(parameter.symbol(), original);
        for (id, node) in self.parsed.ast().expressions().iter() {
            if self.sources.slice(node.span()) == Ok("forwarded") {
                assert_eq!(
                    self.owned
                        .callable_origin(UnitExpressionId::new(self.unit, id))
                        .unwrap()
                        .origin(),
                    UnitCallableOrigin::Parameter(parameter)
                );
            }
        }
        for fact in self.owned.callable_origins() {
            assert!(matches!(
                self.typed
                    .expression_type(fact.expression())
                    .and_then(|ty| self.typed.types().get(ty)),
                Some(lang_frontend::type_checking::UnitTypeKind::Function { .. })
            ));
        }
    }

    pub(super) fn expression(&self, text: &str) -> UnitExpressionId {
        let expressions = self
            .parsed
            .ast()
            .expressions()
            .iter()
            .filter(|(_, node)| self.sources.slice(node.span()) == Ok(text))
            .map(|(id, _)| id)
            .collect::<Vec<_>>();
        let [expression] = expressions.as_slice() else {
            panic!("one actual expression: {text}");
        };
        UnitExpressionId::new(self.unit, *expression)
    }

    pub(super) fn origin(&self, text: &str) -> Option<UnitCallableOrigin> {
        self.owned
            .callable_origin(self.expression(text))
            .map(|fact| fact.origin())
    }

    pub(super) fn target(&self, name: &str) -> UnitCallableTarget {
        let matches = self
            .signatures
            .iter()
            .filter(|s| s.name() == name)
            .collect::<Vec<_>>();
        let [signature] = matches.as_slice() else {
            panic!("one source callable {name}")
        };
        assert!(matches!(
            signature.target(),
            UnitCallableTarget::Declaration(_)
        ));
        signature.target()
    }

    pub(super) fn assert_value_parameter(&self, callable: &str, parameter: &str) {
        let target = self.target(callable);
        let signature = self
            .signatures
            .iter()
            .find(|s| s.target() == target)
            .unwrap();
        let [actual] = signature.parameters() else {
            panic!("one actual parameter")
        };
        assert_eq!(actual.name(), Some(parameter));
        assert_eq!(
            actual
                .symbol()
                .expect("actual source parameter")
                .source_unit(),
            self.unit
        );
        assert_eq!(actual.mode(), ParameterMode::Value);
    }

    pub(super) fn assert_pointer_lambda(&self, name: &str, operand: Option<&str>, index: usize) {
        use lang_frontend::ownership_checking::UnitPointerCallableReturnOrigin;
        let summary = self
            .owned
            .pointer_callable_return(self.target(name))
            .expect("normal pointer return");
        let UnitCallableOrigin::Lambda(lambda) = self.lambda(index) else {
            unreachable!()
        };
        assert_eq!(
            summary.origin(),
            UnitPointerCallableReturnOrigin::Lambda(lambda)
        );
        if let Some(operand) = operand {
            assert_eq!(summary.return_value(), self.expression(operand));
        }
    }

    pub(super) fn lambda(&self, index: usize) -> UnitCallableOrigin {
        UnitCallableOrigin::Lambda(UnitExpressionId::new(
            self.unit,
            lambdas(&self.parsed)[index],
        ))
    }
}

pub(super) fn preflight(text: &str) -> [Analysis; 2] {
    [false, true].map(|rooted| {
        let mut sources = SourceMap::new();
        let source = sources.add_source("closures.ko", text).expect("source");
        let parsed = parse_file_twice(&sources, source, "closure source");
        assert!(
            parsed.diagnostics().is_empty(),
            "{:?}",
            parsed.diagnostics()
        );
        assert_eq!(sources.source_text(source), Ok(text));
        let input = if rooted {
            SourceUnitInput::new("root", "closures.ko", source, &parsed)
        } else {
            SourceUnitInput::memory_document(source, &parsed)
        };
        let inputs = [input];
        let index = index_compilation_unit(&sources, &inputs).expect("index");
        assert!(index.diagnostics().is_empty(), "{:?}", index.diagnostics());
        let [entry] = index.source_units() else {
            panic!("one actual source");
        };
        assert_eq!(entry.source_id(), source);
        match (rooted, entry.key()) {
            (false, SourceUnitKey::MemoryDocument) => {}
            (true, SourceUnitKey::Rooted { root, logical_path }) => {
                assert_eq!(root.as_str(), "root");
                assert_eq!(logical_path.as_str(), "closures.ko");
            }
            _ => panic!("requested layout"),
        }
        let unit = entry.id();
        let (environment, types) = standard_environments();
        let names =
            resolve_compilation_unit_names(&sources, &inputs, &index, &environment).expect("names");
        assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
        let names = names.validate().expect("valid names");
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).expect("types");
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        let typed = typed.validate().expect("valid types");
        let owned = check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
            .expect("ownership");
        assert!(owned.is_compatible_with(&typed));
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
        let signatures = index
            .declarations()
            .iter()
            .map(|declaration| {
                assert_eq!(declaration.source_unit(), unit);
                let signature = typed
                    .types()
                    .signatures()
                    .declaration(declaration.id())
                    .expect("actual declaration");
                let callable = signature.callable().expect("fixture functions");
                assert_eq!(
                    callable.target(),
                    UnitCallableTarget::Declaration(declaration.id())
                );
                callable.clone()
            })
            .collect();
        let symbols = names.names().source_units()[0]
            .resolution()
            .symbols()
            .iter()
            .map(|symbol| (symbol.name().to_owned(), symbol.id()))
            .collect();
        Analysis {
            sources,
            parsed,
            unit,
            owned,
            signatures,
            typed: typed.into_types(),
            symbols,
        }
    })
}
