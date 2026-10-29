//! SPEC-0331：普通源码 Map 借用迭代的 rooted/MemoryDocument Unit Phase 2 成功目标。
//! 只消费真实 AST/name/raw/complete 产物；不假造 provider、Pair、field 或 ownership facts。

use lang_frontend::{
    ast::ExpressionId,
    diagnostic::Diagnostic,
    lexer::{LexedFile, lex},
    name_resolution::{
        NameResolution, SourceUnitInput, SourceUnitKey, Symbol, SymbolKind, UnitReferenceTarget,
        UnitSymbolId, ValidatedCompilationUnitNames, index_compilation_unit,
        resolve_compilation_unit_names,
    },
    parser::{Expression, ForBinding, Item, NameMarker, ParsedFile, Statement, parse_file},
    source::{SourceMap, Span},
    type_checking::{
        BuiltinType, CompilationUnitTypes, FinalTypeAdmissionError, IntrinsicTypeConstructor,
        TypeEnvironment, UnitExpressionId, UnitTypeKind, check_compilation_unit_types,
        complete_compilation_unit_types, standard_environments,
    },
};

const LOGICAL_PATH: &str = "iteration.ko";

#[derive(Clone, Copy, Debug)]
enum Entry {
    Unit,
    MemoryUnit,
}

/// 前置失败单列，不能冒充预期的语言能力 RED。
#[derive(Debug)]
struct Blocked {
    stage: &'static str,
    detail: String,
}

fn blocked(stage: &'static str, detail: impl std::fmt::Debug) -> Blocked {
    Blocked {
        stage,
        detail: format!("{detail:?}"),
    }
}

fn diagnostics(sources: &SourceMap, stage: &str, items: &[Diagnostic]) -> Vec<String> {
    items
        .iter()
        .map(|item| {
            let span = item.primary_span();
            format!(
                "{stage}: {} {}..{} slice={:?} message={:?}",
                item.code(),
                span.start(),
                span.end(),
                sources.slice(span),
                item.message(),
            )
        })
        .collect()
}

fn require_clean(
    sources: &SourceMap,
    stage: &'static str,
    items: &[Diagnostic],
) -> Result<(), Blocked> {
    if items.is_empty() {
        Ok(())
    } else {
        Err(blocked(stage, diagnostics(sources, stage, items)))
    }
}

struct Fixture {
    sources: SourceMap,
    lexed: LexedFile,
    file: ParsedFile,
    types: TypeEnvironment,
}

struct UnitAnalysis {
    names: ValidatedCompilationUnitNames,
    raw: CompilationUnitTypes,
    complete: Result<bool, FinalTypeAdmissionError>,
}

struct Observed {
    fixture: Fixture,
    analysis: UnitAnalysis,
}

struct Observation {
    constructor: IntrinsicTypeConstructor,
    entry: Entry,
    text: String,
    result: Result<Observed, Blocked>,
}

fn observe(constructor: IntrinsicTypeConstructor, spelling: &str, entry: Entry) -> Observation {
    let text = format!(
        "fun scan(m: {spelling}<Int, String>): Unit {{ for ((key, value) in m) {{ val copied: Int = key; println(value) }} }}"
    );
    let result = analyze(&text, entry);
    Observation {
        constructor,
        entry,
        text,
        result,
    }
}

fn analyze(text: &str, entry: Entry) -> Result<Observed, Blocked> {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source(LOGICAL_PATH, text)
        .map_err(|error| blocked("source infrastructure", error))?;
    let lexed = lex(&sources, source).map_err(|error| blocked("lex infrastructure", error))?;
    require_clean(&sources, "lex", lexed.diagnostics())?;
    let file =
        parse_file(&sources, &lexed).map_err(|error| blocked("parse infrastructure", error))?;
    require_clean(&sources, "parse", file.diagnostics())?;
    let (environment, types) = standard_environments();
    let fixture = Fixture {
        sources,
        lexed,
        file,
        types,
    };
    let sources = &fixture.sources;
    let file = &fixture.file;
    let types = &fixture.types;
    let analysis = {
        // rooted 路径仍与无 package 源匹配；MemoryDocument 不伪造路径身份。
        let inputs = [if matches!(entry, Entry::MemoryUnit) {
            SourceUnitInput::memory_document(file.source_id(), file)
        } else {
            SourceUnitInput::new("root", LOGICAL_PATH, source, file)
        }];
        let index = index_compilation_unit(sources, &inputs)
            .map_err(|error| blocked("unit index infrastructure", error))?;
        require_clean(sources, "unit index", index.diagnostics())?;
        let names = resolve_compilation_unit_names(sources, &inputs, &index, &environment)
            .map_err(|error| blocked("unit names infrastructure", error))?;
        require_clean(sources, "unit names", names.diagnostics())?;
        let names = names
            .validate()
            .map_err(|error| blocked("unit names qualification", error.diagnostics()))?;
        let [indexed] = names.names().index().source_units() else {
            return Err(blocked("unit identity", "one actual indexed source"));
        };
        let key_matches = match (entry, indexed.key()) {
            (Entry::Unit, SourceUnitKey::Rooted { root, logical_path }) => {
                root.as_str() == "root" && logical_path.as_str() == LOGICAL_PATH
            }
            (Entry::MemoryUnit, SourceUnitKey::MemoryDocument) => true,
            _ => false,
        };
        if indexed.source_id() != source || !key_matches {
            return Err(blocked("unit identity", indexed));
        }

        let raw = check_compilation_unit_types(sources, &inputs, &names, types)
            .map_err(|error| blocked("unit raw types infrastructure", error))?;
        let complete = complete_compilation_unit_types(sources, &inputs, &names, types, &raw)
            .map(|final_types| final_types.is_same_analysis(&raw));
        UnitAnalysis {
            names,
            raw,
            complete,
        }
    };
    Ok(Observed { fixture, analysis })
}

#[derive(Debug)]
struct Sites {
    source: ExpressionId,
    source_span: Span,
    binding_spans: [Span; 2],
    reads: [ExpressionId; 2],
    read_spans: [Span; 2],
    print: ExpressionId,
}

fn sites(fixture: &Fixture) -> Result<Sites, Blocked> {
    let ast = fixture.file.ast();
    let loops = ast
        .statements()
        .iter()
        .filter_map(|(_, node)| match node.payload() {
            Statement::For {
                binding,
                source,
                body,
                ..
            } => Some((binding, *source, *body)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(binding, source, body)] = loops.as_slice() else {
        return Err(blocked("AST fixture", "expected exactly one for statement"));
    };
    let ForBinding::Destructuring { names, .. } = binding else {
        return Err(blocked("AST fixture", "expected destructuring binding"));
    };
    let [NameMarker::Present(key), NameMarker::Present(value)] = names.as_slice() else {
        return Err(blocked(
            "AST fixture",
            "expected two real binding identifiers",
        ));
    };
    let body = ast
        .statements()
        .get(*body)
        .map_err(|error| blocked("AST fixture", error))?;
    let Statement::Block { elements } = body.payload() else {
        return Err(blocked("AST fixture", "expected ordinary loop body"));
    };
    let [copy_statement, print_statement] = elements.as_slice() else {
        return Err(blocked("AST fixture", "expected copy then print"));
    };
    let copy_statement = ast
        .statements()
        .get(*copy_statement)
        .map_err(|error| blocked("AST fixture", error))?;
    let Statement::LocalVariable { declaration } = copy_statement.payload() else {
        return Err(blocked("AST fixture", "expected copied local declaration"));
    };
    let declaration = ast
        .items()
        .get(*declaration)
        .map_err(|error| blocked("AST fixture", error))?;
    let Item::Variable {
        name: NameMarker::Present(copied),
        initializer: key_read,
        ..
    } = declaration.payload()
    else {
        return Err(blocked("AST fixture", "expected real copied initializer"));
    };
    let print_statement = ast
        .statements()
        .get(*print_statement)
        .map_err(|error| blocked("AST fixture", error))?;
    let Statement::Expression { expression: print } = print_statement.payload() else {
        return Err(blocked(
            "AST fixture",
            "expected println expression statement",
        ));
    };
    let print_node = ast
        .expressions()
        .get(*print)
        .map_err(|error| blocked("AST fixture", error))?;
    let Expression::Call { arguments, .. } = print_node.payload() else {
        return Err(blocked("AST fixture", "expected println call"));
    };
    let [argument] = arguments.as_slice() else {
        return Err(blocked("AST fixture", "expected one value argument"));
    };
    let span = |id| {
        ast.expressions()
            .get(id)
            .map(|node| node.span())
            .map_err(|error| blocked("AST fixture", error))
    };
    let source_span = span(*source)?;
    let read_spans = [span(*key_read)?, span(argument.value)?];
    for (span, expected) in [
        (source_span, "m"),
        (*key, "key"),
        (*value, "value"),
        (*copied, "copied"),
        (read_spans[0], "key"),
        (read_spans[1], "value"),
        (print_node.span(), "println(value)"),
    ] {
        if fixture.sources.slice(span) != Ok(expected) {
            return Err(blocked("AST fixture", (span, expected)));
        }
    }
    Ok(Sites {
        source: *source,
        source_span,
        binding_spans: [*key, *value],
        reads: [*key_read, argument.value],
        read_spans,
        print: *print,
    })
}

/// 先按 AST binding Span 取得声明，再核对 body 读取实际解析到这个 symbol。
fn binding(names: &NameResolution, span: Span) -> Option<&Symbol> {
    let mut found = names.symbols().iter().filter(|symbol| {
        symbol.span() == span && symbol.kind() == SymbolKind::ForBinding && !symbol.is_synthetic()
    });
    let symbol = found.next()?;
    found.next().is_none().then_some(symbol)
}

fn unit_read_target(names: &ValidatedCompilationUnitNames, span: Span) -> Option<UnitSymbolId> {
    let mut found = names.names().references().iter().filter_map(|reference| {
        if reference.span() == span
            && let UnitReferenceTarget::Symbol(symbol) = reference.target()
        {
            Some(*symbol)
        } else {
            None
        }
    });
    let symbol = found.next()?;
    found.next().is_none().then_some(symbol)
}

#[derive(Debug)]
struct Facts {
    source_kind: String,
    source_kv: Vec<String>,
    source_matches: bool,
    binding_symbols: Vec<String>,
    binding_types: Vec<String>,
    read_types: Vec<String>,
    binding_matches: [bool; 2],
    print_selected: bool,
    existing_sequential_plan_count: usize,
}

fn unit_facts(
    names: &ValidatedCompilationUnitNames,
    raw: &CompilationUnitTypes,
    sites: &Sites,
    constructor: IntrinsicTypeConstructor,
) -> Result<Facts, Blocked> {
    let [source_names] = names.names().source_units() else {
        return Err(blocked("unit identity", "expected one source unit"));
    };
    let unit = source_names.source_unit();
    let [indexed] = names.names().index().source_units() else {
        return Err(blocked("unit identity", "one indexed source"));
    };
    if indexed.id() != unit
        || indexed.source_id() != sites.source_span.source_id()
        || source_names.resolution().source_id() != sites.source_span.source_id()
    {
        return Err(blocked("unit identity", indexed));
    }
    let expression = |id| UnitExpressionId::new(unit, id);
    let source_kind = raw
        .expression_type(expression(sites.source))
        .and_then(|ty| raw.types().get(ty));
    let source_matches = match source_kind {
        Some(UnitTypeKind::Intrinsic {
            constructor: actual,
            arguments,
        }) => {
            *actual == constructor
                && arguments.len() == 2
                && raw.types().get(arguments[0]) == Some(&UnitTypeKind::Builtin(BuiltinType::Int))
                && raw.types().get(arguments[1])
                    == Some(&UnitTypeKind::Builtin(BuiltinType::String))
        }
        _ => false,
    };
    let mut facts = Facts {
        source_kind: format!("{source_kind:?}"),
        source_kv: match source_kind {
            Some(UnitTypeKind::Intrinsic { arguments, .. }) => arguments
                .iter()
                .map(|ty| format!("{:?}", raw.types().get(*ty)))
                .collect(),
            _ => Vec::new(),
        },
        source_matches,
        binding_symbols: Vec::new(),
        binding_types: Vec::new(),
        read_types: Vec::new(),
        binding_matches: [false; 2],
        print_selected: raw
            .call(expression(sites.print))
            .is_some_and(|call| call.prints_line())
            && raw
                .expression_type(expression(sites.print))
                .and_then(|ty| raw.types().get(ty))
                == Some(&UnitTypeKind::Builtin(BuiltinType::Unit)),
        existing_sequential_plan_count: raw.sequential_iterations().len(),
    };
    for (index, builtin) in [BuiltinType::Int, BuiltinType::String]
        .into_iter()
        .enumerate()
    {
        let local = binding(source_names.resolution(), sites.binding_spans[index]);
        // UnitSymbolId 构造器并非公共 API；必须读取名称阶段发布的真实 unit target。
        let symbol = unit_read_target(names, sites.read_spans[index]);
        let symbol_type = symbol.and_then(|symbol| raw.symbol_type(symbol));
        let read_type = raw.expression_type(expression(sites.reads[index]));
        facts.binding_matches[index] = local.zip(symbol).is_some_and(|(local, symbol)| {
            symbol.source_unit() == unit && symbol.symbol() == local.id()
        }) && symbol_type == read_type
            && symbol_type.and_then(|ty| raw.types().get(ty))
                == Some(&UnitTypeKind::Builtin(builtin));
        facts
            .binding_symbols
            .push(format!("local={local:?}, unit={symbol:?}"));
        facts.binding_types.push(format!(
            "{:?}",
            symbol_type.and_then(|ty| raw.types().get(ty))
        ));
        facts.read_types.push(format!(
            "{:?}",
            read_type.and_then(|ty| raw.types().get(ty))
        ));
    }
    Ok(facts)
}

fn assess(observation: &Observation) -> (bool, String) {
    let label = format!("{:?}/{:?}", observation.constructor, observation.entry);
    let observed = match &observation.result {
        Ok(observed) => observed,
        Err(error) => {
            return (
                false,
                format!("{label}: BLOCKED {}: {}", error.stage, error.detail),
            );
        }
    };
    let fixture = &observed.fixture;
    let sites = match sites(fixture) {
        Ok(sites) => sites,
        Err(error) => {
            return (
                false,
                format!("{label}: BLOCKED {}: {}", error.stage, error.detail),
            );
        }
    };
    let analysis = &observed.analysis;
    let (facts, raw_diagnostics, complete) = (
        unit_facts(
            &analysis.names,
            &analysis.raw,
            &sites,
            observation.constructor,
        ),
        analysis.raw.diagnostics(),
        &analysis.complete,
    );
    let facts = match facts {
        Ok(facts) => facts,
        Err(error) => {
            return (
                false,
                format!("{label}: BLOCKED {}: {}", error.stage, error.detail),
            );
        }
    };
    let mut messages = diagnostics(&fixture.sources, "raw types", raw_diagnostics);
    match complete {
        Err(FinalTypeAdmissionError::Diagnostics(items)) => {
            messages.extend(diagnostics(&fixture.sources, "complete types", items));
        }
        Err(FinalTypeAdmissionError::Internal(error)) => {
            messages.push(format!("complete types INTERNAL: {error:?}"));
        }
        Ok(same_analysis) => messages.push(format!(
            "complete types accepted; same_analysis={same_analysis}"
        )),
    }
    // 所有 Unit 观察完成之后才检查成功目标；现有 plan 数只观察，不硬定新 Map 的表示。
    let accepted = raw_diagnostics.is_empty()
        && matches!(complete, Ok(true))
        && facts.source_matches
        && facts.binding_matches == [true, true]
        && facts.print_selected;
    let retained = fixture.lexed.source_id() == fixture.file.source_id();
    let outcome = match complete {
        Err(FinalTypeAdmissionError::Internal(_)) => "INTERNAL",
        _ if !raw_diagnostics.is_empty() || complete.is_err() => "PHASE_2_REJECTED",
        _ if accepted && retained => "ACCEPTED",
        _ => "CONTRACT_FACTS_FAILED",
    };
    let report = format!(
        "{label}: {outcome} accepted={accepted} lex=parse=names=pass retained_lex_parse={retained}\nSOURCE {:?}\nSOURCE_SPAN {:?}\nSOURCE_KIND {} SOURCE_KV {:?}\nBINDINGS {:?}\nBINDING_TYPES {:?}\nREAD_TYPES {:?}\nSOURCE_MATCH {} BINDING_MATCH {:?} PRINT_SELECTED {} EXISTING_SEQUENTIAL_PLAN_COUNT {}\nDIAGNOSTICS {messages:#?}",
        observation.text,
        sites.source_span,
        facts.source_kind,
        facts.source_kv,
        facts.binding_symbols,
        facts.binding_types,
        facts.read_types,
        facts.source_matches,
        facts.binding_matches,
        facts.print_selected,
        facts.existing_sequential_plan_count,
    );
    (accepted && retained, report)
}

#[test]
fn map_and_mutable_map_named_borrowed_iteration_complete_in_both_entries() {
    let mut observations = Vec::new();
    for (constructor, spelling) in [
        (IntrinsicTypeConstructor::Map, "Map"),
        (IntrinsicTypeConstructor::MutableMap, "MutableMap"),
    ] {
        for entry in [Entry::Unit, Entry::MemoryUnit] {
            observations.push(observe(constructor, spelling, entry));
        }
    }
    let assessments = observations.iter().map(assess).collect::<Vec<_>>();
    // 先完整输出四次 Unit 分析，再统一断言；首个 RED 不会掩盖其余结果。
    for (_, report) in &assessments {
        println!("{report}");
    }
    assert_eq!(
        assessments.len(),
        4,
        "Map/MutableMap × rooted Unit/MemoryDocument Unit"
    );
    assert!(
        assessments.iter().all(|(accepted, _)| *accepted),
        "SPEC-0331 Phase 2 success target: all four ordinary analyses must accept, retain intrinsic K/V and real body binding types, and complete with original identity. BLOCKED/INTERNAL is not feature RED.\n{}",
        assessments
            .iter()
            .map(|(_, report)| report.as_str())
            .collect::<Vec<_>>()
            .join("\n"),
    );
}
