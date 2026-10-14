//! 普通源码分析；raw、原 ownership Result 与 final qualification 分开保存。
use super::*;

pub(super) const DIRECT: &str = include_str!("../fixtures/enum-case-borrow-0320/no-when.ko");
pub(super) const P49: &str =
    include_str!("../fixtures/enum-case-borrow-0320/p49_when_enum_no_payload.ko");
pub(super) const Q08: &str =
    include_str!("../fixtures/enum-case-borrow-0320/q08_enum_string_payload.ko");
pub(super) const SHORT: &str = "enum class Flag { On, Off; fun fromShort(): Int = read(On) }\nfun read(value: Flag): Int = 7\n";
pub(super) const MOVE_ONLY: &str = "enum class Msg { Text(text: String), Quit }\nfun read(value: Msg): Int = 7\nfun main(): Unit { if (read(Msg.Quit) == 7) { println(\"empty-borrow\") }; if (read(Msg.Text(\"payload\")) == 7) { println(\"payload-borrow\") } }\n";
pub(super) const INOUT: &str = "enum class Flag { On, Off }\nfun touch(inout value: Flag): Unit {}\nfun main(): Unit { touch(&Flag.On) }\n";
pub(super) const MUTABLE: &str = "enum class Flag { On, Off }\nfun touch(inout value: Flag): Unit {}\nfun main(): Unit { var value = Flag.On; touch(&value) }\n";
pub(super) const PLACES: &str = "enum class Flag { On, Off }\n\
    value class Holder(val current: Flag)\n\
    value class Receiver(val On: Flag)\n\
    fun read(value: Flag): Int = 7\n\
    fun inspect(parameter: Flag, holder: Holder, Flag: Receiver): Int {\n\
        val local = parameter\n\
        read(local)\n\
        read((parameter))\n\
        read(holder.current)\n\
        return read(Flag.On)\n\
    }\n\
    fun element(values: Array<Flag>): Int = read(values[0])\n";
pub(super) const GENERIC: &str = "enum class Maybe<T> { Some(item: T), None }\n\
    fun read(value: Maybe<Int>): Int = 7\n\
    fun empty(): Int = read(Maybe.None)\n\
    fun payload(): Int = read(Maybe.Some<Int>(1))\n";
pub(super) const NAMED: &str = "enum class Flag { On, Off }\n\
    fun pair(first: Flag, second: Flag): Int = 7\n\
    fun entry(): Int = pair(second = Flag.On, first = Flag.Off)\n";

pub(super) const PAYLOADS: &str = "enum class Msg { Text(text: String), Quit }\n\
    enum class Count { Value(number: Int) }\n\
    fun readMsg(value: Msg): Int = 7\n\
    fun readCount(value: Count): Int = 7\n\
    fun moved(own text: String): Int = readMsg(Msg.Text(text))\n\
    fun copied(number: Int): Int = readCount(Count.Value(number))\n";

#[derive(Debug)]
pub(super) enum UnitOwnershipResult {
    Basic(Result<CompilationUnitOwnership, OwnershipCheckingError>),
    Constants(Result<CompilationUnitConstantOwnership, OwnershipCheckingError>),
    // Raw source diagnostics may correctly deny the existing base/constant capability.
    Unqualified,
}

impl UnitOwnershipResult {
    pub(super) fn accepted(&self) -> &CompilationUnitOwnership {
        let owned = match self {
            Self::Basic(Ok(owned)) => owned,
            Self::Constants(Ok(owned)) => owned.ownership(),
            other => panic!("original unit ownership result: {other:?}"),
        };
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
        owned
    }
}

pub(super) struct UnitEvidence {
    pub names: ValidatedCompilationUnitNames,
    pub raw: CompilationUnitTypes,
    pub owned: UnitOwnershipResult,
    pub final_result: Result<(), FinalTypeAdmissionError>,
}

pub(super) struct Evidence {
    pub sources: SourceMap,
    pub file: ParsedFile,
    pub names: NameResolution,
    pub raw: TypedFile,
    pub owned: Result<OwnershipCheckedFile, OwnershipCheckingError>,
    pub final_result: Result<(), FinalTypeAdmissionError>,
    pub unit: UnitEvidence,
}

pub(super) fn parsed(sources: &mut SourceMap, path: &str, text: &str) -> (SourceId, ParsedFile) {
    let source = sources.add_source(path, text).expect("unique source");
    let tokens = lex(sources, source).expect("lex ordinary source");
    assert!(
        tokens.diagnostics().is_empty(),
        "{:?}",
        tokens.diagnostics()
    );
    let file = parse_file(sources, &tokens).expect("parse ordinary source");
    assert!(file.diagnostics().is_empty(), "{:?}", file.diagnostics());
    (source, file)
}

pub(super) fn collect_unit(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'_>],
    constants: bool,
) -> UnitEvidence {
    let (environment, types) = standard_environments();
    let index = index_compilation_unit(sources, inputs).expect("unit source index");
    let names = resolve_compilation_unit_names(sources, inputs, &index, &environment)
        .expect("unit names")
        .validate()
        .expect("valid source names");
    let raw = check_compilation_unit_types(sources, inputs, &names, &types)
        .expect("original unit type checker");
    let owned = if constants {
        match raw.clone().validate_constants() {
            Ok(qualified) => {
                assert!(qualified.types().is_same_analysis(&raw));
                let result = check_compilation_unit_constant_ownership(
                    sources, inputs, &names, &types, &qualified,
                );
                if let Ok(product) = &result {
                    assert!(product.is_compatible_with(&qualified));
                }
                UnitOwnershipResult::Constants(result)
            }
            Err(_) => UnitOwnershipResult::Unqualified,
        }
    } else {
        match raw.clone().validate() {
            Ok(qualified) => {
                assert!(qualified.types().is_same_analysis(&raw));
                let result =
                    check_compilation_unit_ownership(sources, inputs, &names, &types, &qualified);
                if let Ok(product) = &result {
                    assert!(product.is_compatible_with(&qualified));
                }
                UnitOwnershipResult::Basic(result)
            }
            Err(_) => UnitOwnershipResult::Unqualified,
        }
    };
    // This independent gate is deliberately not an ownership precondition here.
    let final_result =
        complete_compilation_unit_types(sources, inputs, &names, &types, &raw).map(|complete| {
            assert!(std::ptr::eq(complete.types(), &raw));
            assert!(complete.is_same_analysis(&raw));
        });
    // Emit the real results before any success/category oracle can stop this test.
    println!(
        "[0320 unit constants={constants}] inputs={:?} raw_diagnostics={:?}",
        inputs
            .iter()
            .map(|input| (
                input.root_identity(),
                input.logical_path(),
                input.source_id(),
                sources.source_text(input.source_id()).map(str::len)
            ))
            .collect::<Vec<_>>(),
        diagnostic_evidence(raw.diagnostics())
    );
    for construction in raw.constructions() {
        println!(
            "[0320 unit] construction={:?} target={:?} root={:?} operands={} category={:?}",
            construction.expression(),
            construction.target(),
            raw.types().get(construction.result_type()),
            construction.arguments().len(),
            raw.expression_category(construction.expression())
        );
    }
    for call in raw.calls() {
        println!(
            "[0320 unit] call={:?} arguments={:?}",
            call.expression(),
            call.arguments()
                .iter()
                .map(|argument| (
                    argument.argument_index(),
                    argument.parameter_index(),
                    argument.mode(),
                    argument.category()
                ))
                .collect::<Vec<_>>()
        );
    }
    let ownership = match &owned {
        UnitOwnershipResult::Basic(Ok(product)) => Some(product),
        UnitOwnershipResult::Constants(Ok(product)) => Some(product.ownership()),
        UnitOwnershipResult::Basic(Err(error)) | UnitOwnershipResult::Constants(Err(error)) => {
            println!("[0320 unit] ownership=Err({error:?})");
            None
        }
        UnitOwnershipResult::Unqualified => {
            println!("[0320 unit] ownership=not-called(existing type qualification rejected)");
            None
        }
    };
    if let Some(product) = ownership {
        println!(
            "[0320 unit] ownership=Ok diagnostics={:?} deferred={:?} loans={:?}",
            diagnostic_evidence(product.diagnostics()),
            product
                .deferred()
                .iter()
                .map(|fact| fact.reason())
                .collect::<Vec<_>>(),
            product
                .loans()
                .iter()
                .map(|loan| (loan.call(), loan.argument(), loan.kind(), loan.target()))
                .collect::<Vec<_>>()
        );
    }
    print_final_evidence("unit", &final_result);
    UnitEvidence {
        names,
        raw,
        owned,
        final_result,
    }
}

pub(super) fn collect(text: &str) -> Evidence {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(&mut sources, "enum-case-borrow.ko", text);
    let (environment, types) = standard_environments();
    let names = resolve_names(&sources, &file, &environment).expect("single names");
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let raw = check_types(&sources, &file, &names, &types).expect("original single type checker");
    let owned = check_ownership(&sources, &file, &names, &raw);
    if let Ok(product) = &owned {
        assert!(product.is_compatible_with(&names, &raw));
    }
    let final_result = complete_file_types(&sources, &file, &names, &types, &raw).map(|complete| {
        assert!(std::ptr::eq(complete.types(), &raw));
        assert!(complete.is_same_analysis(&raw));
    });
    println!(
        "[0320 single] source={source:?} path={:?} bytes={} first_line={:?} raw_diagnostics={:?}",
        sources.source_name(source),
        text.len(),
        text.lines().next(),
        diagnostic_evidence(raw.diagnostics())
    );
    for construction in raw.constructions() {
        println!(
            "[0320 single] construction={:?} target={:?} root={:?} operands={} category={:?}",
            construction.expression(),
            construction.target(),
            raw.types().get(construction.result_type()),
            construction.arguments().len(),
            raw.expression_category(construction.expression())
        );
    }
    for call in raw.calls() {
        println!(
            "[0320 single] call={:?} arguments={:?}",
            call.expression(),
            call.arguments()
                .iter()
                .map(|argument| (
                    argument.argument_index(),
                    argument.parameter_index(),
                    argument.mode(),
                    argument.category()
                ))
                .collect::<Vec<_>>()
        );
    }
    match &owned {
        Ok(product) => println!(
            "[0320 single] ownership=Ok diagnostics={:?} deferred={:?} loans={:?}",
            diagnostic_evidence(product.diagnostics()),
            product
                .deferred()
                .iter()
                .map(|fact| fact.reason())
                .collect::<Vec<_>>(),
            product
                .loans()
                .iter()
                .map(|loan| (loan.call(), loan.argument(), loan.kind(), loan.target()))
                .collect::<Vec<_>>()
        ),
        Err(error) => println!("[0320 single] ownership=Err({error:?})"),
    }
    print_final_evidence("single", &final_result);
    let inputs = [SourceUnitInput::new(
        "root",
        "enum-case-borrow.ko",
        source,
        &file,
    )];
    let unit = collect_unit(&sources, &inputs, false);
    Evidence {
        sources,
        file,
        names,
        raw,
        owned,
        final_result,
        unit,
    }
}

pub(super) fn expression(sources: &SourceMap, file: &ParsedFile, text: &str) -> ExpressionId {
    let matches = file
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| (sources.slice(node.span()) == Ok(text)).then_some(id))
        .collect::<Vec<_>>();
    assert_eq!(matches.len(), 1, "unique source expression: {text}");
    matches[0]
}

pub(super) fn span(file: &ParsedFile, expression: ExpressionId) -> Span {
    file.ast()
        .expressions()
        .get(expression)
        .expect("source expression")
        .span()
}

pub(super) fn source_unit(names: &ValidatedCompilationUnitNames, source: SourceId) -> SourceUnitId {
    names
        .names()
        .index()
        .source_units()
        .iter()
        .find(|unit| unit.source_id() == source)
        .expect("source is selected")
        .id()
}

impl Evidence {
    pub(super) fn expression(&self, text: &str) -> ExpressionId {
        expression(&self.sources, &self.file, text)
    }

    pub(super) fn unit_expression(&self, text: &str) -> UnitExpressionId {
        UnitExpressionId::new(
            source_unit(&self.unit.names, self.file.source_id()),
            self.expression(text),
        )
    }

    pub(super) fn accepted_single(&self) -> &OwnershipCheckedFile {
        let owned = self
            .owned
            .as_ref()
            .unwrap_or_else(|error| panic!("original single ownership: {error:?}"));
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
        owned
    }

    pub(super) fn clean_raw(&self) {
        assert!(
            self.raw.diagnostics().is_empty(),
            "single raw: {:?}",
            self.raw.diagnostics()
        );
        assert!(
            self.unit.raw.diagnostics().is_empty(),
            "unit raw: {:?}",
            self.unit.raw.diagnostics()
        );
    }
}

pub(super) fn parameter_span(sources: &SourceMap, file: &ParsedFile, name: &str) -> Span {
    file.ast()
        .items()
        .iter()
        .find_map(|(_, node)| {
            let Item::Function {
                name: NameMarker::Present(name_span),
                parameters,
                ..
            } = node.payload()
            else {
                return None;
            };
            (sources.slice(*name_span) == Ok(name)).then(|| {
                assert_eq!(parameters.len(), 1);
                parameters[0].span
            })
        })
        .expect("one source parameter declaration")
}

pub(super) fn source_diagnostics(result: &Result<(), FinalTypeAdmissionError>) -> &[Diagnostic] {
    match result {
        Err(FinalTypeAdmissionError::Diagnostics(diagnostics)) => diagnostics,
        other => panic!("expected final source diagnostics, got {other:?}"),
    }
}

// Compact source-backed evidence only; never print the complete type/ownership products.
fn diagnostic_evidence(diagnostics: &[Diagnostic]) -> Vec<(String, Span)> {
    diagnostics
        .iter()
        .map(|diagnostic| (diagnostic.code().to_string(), diagnostic.primary_span()))
        .collect()
}

fn print_final_evidence(entrance: &str, result: &Result<(), FinalTypeAdmissionError>) {
    match result {
        Ok(()) => println!("[0320 {entrance}] final=Ok"),
        Err(FinalTypeAdmissionError::Diagnostics(diagnostics)) => println!(
            "[0320 {entrance}] final=Diagnostics({:?})",
            diagnostic_evidence(diagnostics)
        ),
        Err(FinalTypeAdmissionError::Internal(error)) => {
            println!("[0320 {entrance}] final=Internal({error:?})")
        }
    }
}
