//! SPEC-0255: 两入口只比较已支持的 String 事实，使用逻辑来源而非 arena ID。

use lang_frontend::{
    name_resolution::{
        SourceUnitInput, SymbolKind, index_compilation_unit, resolve_compilation_unit_names,
        resolve_names,
    },
    ownership_checking::{check_compilation_unit_constant_ownership, check_ownership},
    source::{SourceMap, Span},
    type_checking::{check_compilation_unit_types, check_types, standard_environments},
};

use super::{
    LoweringError, LoweringErrorKind, lower_scalar_file_with_entry,
    model::{
        Definition, EntityId, Function, FunctionId, Operation, Program, SsaTypeKind, TerminatorKind,
    },
    unit_lower::{constant::lower_constant_unit_with_entry, lower_scalar_unit_with_entry},
    unit_lower_test_support::{analyze, declaration, parsed},
};

#[derive(Debug, PartialEq, Eq)]
struct SourceKey(String, usize, usize);

fn source_key(sources: &SourceMap, span: Span) -> SourceKey {
    SourceKey(
        sources.source_name(span.source_id()).unwrap().to_owned(),
        span.start(),
        span.end(),
    )
}

#[derive(Debug, PartialEq, Eq)]
enum Event {
    Literal(SourceKey, Vec<u8>),
    Print(SourceKey),
    Concat(SourceKey),
    Borrow(SourceKey),
    EndBorrow(SourceKey),
    Drop(SourceKey),
    Abort(SourceKey),
}

fn events(sources: &SourceMap, program: &Program, entry: FunctionId) -> Vec<Event> {
    let module = &program.modules[0];
    let function = module.function(entry).unwrap();
    let mut events = Vec::new();
    for instruction in &function.instructions {
        let key = source_key(sources, instruction.origin.span());
        let event = match &instruction.operation {
            Operation::StringLiteral { string, bytes } => {
                assert_eq!(module.types[string.index()], SsaTypeKind::StringOwner);
                Event::Literal(key, bytes.clone())
            }
            Operation::PrintString { .. } => Event::Print(key),
            Operation::StringConcat { .. } => Event::Concat(key),
            Operation::BorrowBegin { .. } => Event::Borrow(key),
            Operation::BorrowEnd { .. } => Event::EndBorrow(key),
            Operation::Drop { .. } => Event::Drop(key),
            _ => continue,
        };
        events.push(event);
    }
    for block in &function.blocks {
        let terminator = block.terminator.as_ref().expect("verified terminator");
        if matches!(terminator.kind, TerminatorKind::Abort) {
            events.push(Event::Abort(source_key(sources, terminator.origin.span())));
        }
    }
    crate::llvm::render_verified_program(program).expect("both entry LLVM paths verify");
    events
}

fn paired(text: &str, constants: bool) -> Vec<Event> {
    let first = paired_in_order(text, constants, false);
    assert_eq!(first, paired_in_order(text, constants, true));
    first
}

fn paired_in_order(text: &str, constants: bool, entry_first: bool) -> Vec<Event> {
    paired_in_order_observed(text, constants, entry_first, |_, _, _| {})
}

fn paired_in_order_observed(
    text: &str,
    constants: bool,
    entry_first: bool,
    observe: impl Fn(&SourceMap, &Program, FunctionId),
) -> Vec<Event> {
    let mut sources = SourceMap::new();
    // Identical names in an unrelated logical source must not affect entry facts. Reversed
    // inputs exercise source-qualified lookup without comparing raw source or type IDs.
    let other_text = "package other\nfun entry(): String = \"unreachable\"";
    let ((source, file), (other_source, other)) = if entry_first {
        let entry = parsed(&mut sources, "pair/entry.ko", text);
        let other = parsed(&mut sources, "other/entry.ko", other_text);
        (entry, other)
    } else {
        let other = parsed(&mut sources, "other/entry.ko", other_text);
        let entry = parsed(&mut sources, "pair/entry.ko", text);
        (entry, other)
    };
    let (name_environment, environment) = standard_environments();
    let names = resolve_names(&sources, &file, &name_environment).unwrap();
    let typed = check_types(&sources, &file, &names, &environment).unwrap();
    let owned = check_ownership(&sources, &file, &names, &typed).unwrap();
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let entry = names
        .symbols()
        .iter()
        .find(|symbol| symbol.name() == "entry" && symbol.kind() == SymbolKind::Function)
        .unwrap()
        .id();
    let (single, entry) =
        lower_scalar_file_with_entry(&sources, &file, &names, &typed, &owned, entry).unwrap();
    let expected = events(&sources, &single, entry);
    let expected_relations = string_relations(&sources, &single, entry);
    observe(&sources, &single, entry);
    let inputs = [
        SourceUnitInput::new("root", "other/entry.ko", other_source, &other),
        SourceUnitInput::new("root", "pair/entry.ko", source, &file),
    ];
    for inputs in [inputs, [inputs[1], inputs[0]]] {
        let (unit, entry) = if constants {
            let index = index_compilation_unit(&sources, &inputs).unwrap();
            let names =
                resolve_compilation_unit_names(&sources, &inputs, &index, &name_environment)
                    .unwrap()
                    .validate()
                    .unwrap();
            let recovery =
                check_compilation_unit_types(&sources, &inputs, &names, &environment).unwrap();
            assert!(
                recovery.clone().validate().is_err(),
                "basic capability must reject const input"
            );
            let typed = recovery.validate_constants().unwrap();
            let owned = check_compilation_unit_constant_ownership(
                &sources,
                &inputs,
                &names,
                &environment,
                &typed,
            )
            .unwrap()
            .validate()
            .unwrap();
            lower_constant_unit_with_entry(
                &sources,
                &inputs,
                &names,
                &environment,
                &typed,
                &owned,
                declaration(&names, "pair", "entry"),
            )
            .unwrap()
        } else {
            let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &environment);
            lower_scalar_unit_with_entry(
                &sources,
                &inputs,
                &names,
                &environment,
                &typed,
                &owned,
                declaration(&names, "pair", "entry"),
            )
            .unwrap()
        };
        assert_eq!(events(&sources, &unit, entry), expected, "{text}");
        assert_eq!(
            string_relations(&sources, &unit, entry),
            expected_relations,
            "{text}"
        );
        observe(&sources, &unit, entry);
    }
    expected
}

#[test]
fn paired_literals_preserve_bytes_group_origin_and_string_owner_kind() {
    for (literal, bytes) in [
        (r#""""#, b"".as_slice()),
        (r#""中文🙂\0tail""#, "中文🙂\0tail".as_bytes()),
        (r#"((("\\\'\"\n\r\t\0\$")))"#, b"\\'\"\n\r\t\0$".as_slice()),
    ] {
        let text = format!("package pair\nfun entry(): String = {literal}");
        let events = paired(&text, false);
        let literal_start = text.find('"').unwrap();
        let literal_end = text.rfind('"').unwrap() + 1;
        assert_eq!(
            events,
            vec![Event::Literal(
                SourceKey("pair/entry.ko".to_owned(), literal_start, literal_end),
                bytes.to_vec(),
            )]
        );
    }
}

#[test]
fn paired_print_concat_and_drop_preserve_lifetimes() {
    let events = paired(
        "package pair\nfun entry(): Unit { val printed = println(((\"a\0\")) + \"界\") }",
        false,
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, Event::Literal(..)))
            .count(),
        2
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, Event::Concat(..)))
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, Event::Print(..)))
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, Event::Drop(..)))
            .count(),
        3
    );
}

#[test]
fn paired_statement_sequences_keep_empty_nested_blocks_and_cleanup_order() {
    for constants in [false, true] {
        let declaration = if constants {
            "const val UNUSED = 17\n"
        } else {
            ""
        };
        let text = format!(
            "package pair\n{declaration}fun entry(): Unit {{\n\
             {{}}\nval unused = \"unused\"\nprintln(\"first\")\n\
             {{ {{}}; println(\"nested\"); 7 }}\n\
             println(\"last\")\n}}"
        );
        let events = paired(&text, constants);
        let [Event::Literal(_, unused), Event::Drop(_), remaining @ ..] = events.as_slice() else {
            panic!("unused local must be dropped after its statement: {events:?}");
        };
        assert_eq!(unused.as_slice(), b"unused");
        assert_statement_events(remaining, &[b"first", b"nested", b"last"]);
    }
}

#[test]
fn paired_statement_return_stops_inner_and_outer_sequences() {
    for constants in [false, true] {
        let declaration = if constants {
            "const val UNUSED = 17\n"
        } else {
            ""
        };
        let text = format!(
            "package pair\n{declaration}fun entry(): Unit {{\n\
             println(\"before\")\n\
             {{ return; println(\"inner-unreachable\") }}\n\
             println(\"outer-unreachable\")\n}}"
        );
        assert_statement_events(&paired(&text, constants), &[b"before"]);
    }
}

#[test]
fn paired_control_tail_value_reaches_return_without_early_drop() {
    for constants in [false, true] {
        let declaration = if constants {
            "const val UNUSED = 17\n"
        } else {
            ""
        };
        let text = format!(
            "package pair\n{declaration}fun entry(own flag: Boolean): String = if (flag) {{\n\
             val unused = \"unused\"\nprintln(\"prefix\")\n\
             \"tail-value\"\n}} else {{ return \"early-value\" }}"
        );
        let observe = |sources: &SourceMap, program: &Program, entry| {
            assert_direct_string_returns(sources, program, entry, &["tail-value", "early-value"]);
        };
        let events = paired_in_order_observed(&text, constants, false, observe);
        assert_eq!(
            events,
            paired_in_order_observed(&text, constants, true, observe)
        );
        let [Event::Literal(_, unused), Event::Drop(_), remaining @ ..] = events.as_slice() else {
            panic!("tail prefix must finish the unused owner: {events:?}");
        };
        assert_eq!(unused.as_slice(), b"unused");
        let (prefix, tails) = remaining.split_at(5);
        assert_statement_events(prefix, &[b"prefix"]);
        let [Event::Literal(_, tail), Event::Literal(_, early)] = tails else {
            panic!("both branches must evaluate only their delivered literal: {tails:?}");
        };
        assert_eq!(tail.as_slice(), b"tail-value");
        assert_eq!(early.as_slice(), b"early-value");
    }
}

#[test]
fn paired_control_tail_prefix_and_last_return_stop_evaluation() {
    for constants in [false, true] {
        let declaration = if constants {
            "const val UNUSED = 17\n"
        } else {
            ""
        };
        for (body, has_prefix) in [
            ("return \"early-value\"\n\"unreachable-tail\"", false),
            ("println(\"prefix\")\nreturn \"early-value\"", true),
        ] {
            let text = format!(
                "package pair\n{declaration}fun entry(own flag: Boolean): String = if (flag) {{\n\
                 {body}\n}} else {{ \"tail-value\" }}"
            );
            let observe = |sources: &SourceMap, program: &Program, entry| {
                assert_direct_string_returns(
                    sources,
                    program,
                    entry,
                    &["tail-value", "early-value"],
                );
                assert!(
                    program.modules[0]
                        .function(entry)
                        .unwrap()
                        .instructions
                        .iter()
                        .all(|instruction| !matches!(&instruction.operation,
                            Operation::StringLiteral { bytes, .. }
                                if bytes.as_slice() == b"unreachable-tail"))
                );
            };
            let events = paired_in_order_observed(&text, constants, false, observe);
            assert_eq!(
                events,
                paired_in_order_observed(&text, constants, true, observe)
            );
            let (prefix, tails) = events.split_at(if has_prefix { 5 } else { 0 });
            assert_statement_events(prefix, if has_prefix { &[b"prefix"] } else { &[] });
            let [Event::Literal(_, early), Event::Literal(_, tail)] = tails else {
                panic!("return must stop before an unreachable tail: {tails:?}");
            };
            assert_eq!(early.as_slice(), b"early-value");
            assert_eq!(tail.as_slice(), b"tail-value");
        }
    }
}

fn assert_direct_string_returns(
    sources: &SourceMap,
    program: &Program,
    entry: FunctionId,
    expected: &[&str],
) {
    let function = program.modules[0].function(entry).unwrap();
    let mut expected_owners = Vec::new();
    for text in expected {
        let literals = function
            .instructions
            .iter()
            .filter(|instruction| {
                matches!(&instruction.operation,
                    Operation::StringLiteral { bytes, .. } if bytes.as_slice() == text.as_bytes())
            })
            .collect::<Vec<_>>();
        assert_eq!(literals.len(), 1, "one definition for {text}");
        let literal = literals[0];
        let [EntityId::Value(owner)] = literal.results.as_slice() else {
            panic!("String literal has one owner");
        };
        assert_eq!(
            sources.slice(literal.origin.span()).unwrap(),
            format!("\"{text}\"")
        );
        assert_eq!(
            definition_key(sources, function, EntityId::Value(*owner)),
            DefinitionKey::Instruction(
                source_key(sources, literal.origin.span()),
                std::mem::discriminant(&literal.operation),
                0,
            )
        );
        expected_owners.push(*owner);
    }
    let mut returned = Vec::new();
    for block in &function.blocks {
        if let TerminatorKind::Return { values } = &block.terminator.as_ref().unwrap().kind {
            assert_eq!(values.len(), 1, "each path returns its String owner");
            returned.push(values[0]);
        }
    }
    expected_owners.sort_by_key(|owner| owner.index());
    returned.sort_by_key(|owner| owner.index());
    // Each fixture has one normal branch and one direct-return branch, so the
    // original one-exit merge forwards the literal owner without a phi.
    assert_eq!(
        returned, expected_owners,
        "every exact owner reaches Return once"
    );
    assert!(function.instructions.iter().all(|instruction| {
        !matches!(instruction.operation,
            Operation::Drop { owner } if expected_owners.contains(&owner))
    }));
}

fn assert_statement_events(events: &[Event], bytes: &[&[u8]]) {
    assert_eq!(events.len(), bytes.len() * 5, "{events:?}");
    for (events, expected) in events.chunks_exact(5).zip(bytes) {
        let [
            Event::Literal(_, actual),
            Event::Borrow(_),
            Event::Print(_),
            Event::EndBorrow(_),
            Event::Drop(_),
        ] = events
        else {
            panic!("each statement must finish its borrow and owner before the next: {events:?}");
        };
        assert_eq!(actual.as_slice(), *expected);
    }
}

#[test]
fn paired_abort_preserves_no_unwind_boundary() {
    let events = paired(
        "package pair\nfun entry(): Unit { val stop = error(\"failure\0界\") }",
        false,
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, Event::Abort(..)))
            .count(),
        1
    );
    assert!(!events.iter().any(|event| matches!(event, Event::Drop(..))));
}

#[test]
fn paired_generic_delivery_keeps_literal_contracts() {
    let events = paired(
        "package pair\nfun <T> pass(own value: T): T = value\nfun entry(): String = pass<String>(\"generic界\")",
        false,
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, Event::Literal(..)))
            .count(),
        1
    );
}

#[test]
fn paired_const_string_uses_original_specialized_capability() {
    let events = paired(
        "package pair\nconst val TEXT = \"const\0界\"\nfun entry(): Unit { val printed = println(TEXT) }",
        true,
    );
    assert!(
        events.iter().any(
            |event| matches!(event, Event::Literal(_, bytes) if bytes == "const\0界".as_bytes())
        )
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, Event::Drop(..)))
            .count(),
        1
    );
}

fn error_key(sources: &SourceMap, error: LoweringError) -> (LoweringErrorKind, Option<SourceKey>) {
    (error.kind, error.span.map(|span| source_key(sources, span)))
}

#[test]
fn paired_interpolation_rejection_keeps_exact_unsupported_span() {
    for literal in [r#""${17}""#, r#"(("before${17}after"))"#] {
        let mut sources = SourceMap::new();
        let text = format!("package pair\nfun entry(): String = {literal}");
        let (source, file) = parsed(&mut sources, "pair/entry.ko", &text);
        let (name_environment, environment) = standard_environments();
        let names = resolve_names(&sources, &file, &name_environment).unwrap();
        let typed = check_types(&sources, &file, &names, &environment).unwrap();
        let owned = check_ownership(&sources, &file, &names, &typed).unwrap();
        let entry = names
            .symbols()
            .iter()
            .find(|symbol| symbol.name() == "entry")
            .unwrap()
            .id();
        let single = lower_scalar_file_with_entry(&sources, &file, &names, &typed, &owned, entry)
            .err()
            .expect("lowering must reject");
        let inputs = [SourceUnitInput::new("root", "pair/entry.ko", source, &file)];
        let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &environment);
        let unit = lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &environment,
            &typed,
            &owned,
            declaration(&names, "pair", "entry"),
        )
        .err()
        .expect("lowering must reject");
        let expected = (
            LoweringErrorKind::UnsupportedNode,
            Some(SourceKey(
                "pair/entry.ko".to_owned(),
                text.find('"').unwrap(),
                text.rfind('"').unwrap() + 1,
            )),
        );
        assert_eq!(error_key(&sources, single), expected);
        assert_eq!(error_key(&sources, unit), expected);
    }
}

#[test]
fn recovery_stops_at_each_original_validation_boundary() {
    for (body, code) in [("missing", "L0080"), ("17", "L0084")] {
        let mut sources = SourceMap::new();
        let text = format!("package pair\nfun entry(): String = {body}");
        let (source, file) = parsed(&mut sources, "pair/entry.ko", &text);
        let (name_environment, environment) = standard_environments();
        let names = resolve_names(&sources, &file, &name_environment).unwrap();
        let typed = check_types(&sources, &file, &names, &environment).unwrap();
        let owned = check_ownership(&sources, &file, &names, &typed).unwrap();
        let entry = names
            .symbols()
            .iter()
            .find(|symbol| symbol.name() == "entry")
            .unwrap()
            .id();
        let expected_span = SourceKey(
            "pair/entry.ko".to_owned(),
            text.len() - body.len(),
            text.len(),
        );
        let diagnostic = if body == "missing" {
            &names.diagnostics()[0]
        } else {
            &typed.diagnostics()[0]
        };
        assert_eq!(diagnostic.code().to_string(), code);
        assert_eq!(
            source_key(&sources, diagnostic.primary_span()),
            expected_span
        );
        let error = lower_scalar_file_with_entry(&sources, &file, &names, &typed, &owned, entry)
            .err()
            .expect("single-file diagnostics gate");
        assert_eq!(
            error_key(&sources, error),
            (LoweringErrorKind::FrontendDiagnostics, None)
        );
        // Even when diagnostics are present, mismatched analysis is rejected first.
        let foreign_names = resolve_names(&sources, &file, &name_environment).unwrap();
        let error =
            lower_scalar_file_with_entry(&sources, &file, &foreign_names, &typed, &owned, entry)
                .err()
                .expect("analysis mismatch precedes diagnostics");
        assert_eq!(
            error_key(&sources, error),
            (LoweringErrorKind::MismatchedAnalysis, None)
        );
        let (foreign_source, foreign_file) = parsed(&mut sources, "foreign.ko", &text);
        assert_ne!(source, foreign_source);
        let error = lower_scalar_file_with_entry(
            &sources,
            &foreign_file,
            &foreign_names,
            &typed,
            &owned,
            entry,
        )
        .err()
        .expect("source mismatch precedes analysis and diagnostics");
        assert_eq!(
            error_key(&sources, error),
            (LoweringErrorKind::MismatchedSource, None)
        );
        let inputs = [SourceUnitInput::new("root", "pair/entry.ko", source, &file)];
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names =
            resolve_compilation_unit_names(&sources, &inputs, &index, &name_environment).unwrap();
        if body == "missing" {
            assert_eq!(names.diagnostics()[0].code().to_string(), code);
            assert_eq!(
                source_key(&sources, names.diagnostics()[0].primary_span()),
                expected_span
            );
            assert!(
                names.validate().is_err(),
                "unit cannot create names capability"
            );
        } else {
            let names = names.validate().unwrap();
            let typed =
                check_compilation_unit_types(&sources, &inputs, &names, &environment).unwrap();
            assert_eq!(typed.diagnostics()[0].code().to_string(), code);
            assert_eq!(
                source_key(&sources, typed.diagnostics()[0].primary_span()),
                expected_span
            );
            assert!(
                typed.clone().validate().is_err(),
                "basic cannot create typed capability"
            );
            assert!(
                typed.validate_constants().is_err(),
                "const cannot create typed capability"
            );
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum DefinitionKey {
    Instruction(SourceKey, std::mem::Discriminant<Operation>, usize),
    Parameter(SourceKey, usize),
}

fn definition_key(sources: &SourceMap, function: &Function, entity: EntityId) -> DefinitionKey {
    match function.entity(entity).unwrap().definition {
        Definition::InstructionResult { instruction, index } => {
            let instruction = function.instruction(instruction).unwrap();
            DefinitionKey::Instruction(
                source_key(sources, instruction.origin.span()),
                std::mem::discriminant(&instruction.operation),
                index,
            )
        }
        Definition::BlockParameter { block, index } => DefinitionKey::Parameter(
            source_key(sources, function.block(block).unwrap().origin.span()),
            index,
        ),
    }
}

#[derive(Debug, PartialEq, Eq)]
struct StringRelation {
    origin: SourceKey,
    operation: Option<std::mem::Discriminant<Operation>>,
    operands: Vec<DefinitionKey>,
    results: Vec<DefinitionKey>,
}

fn string_relations(
    sources: &SourceMap,
    program: &Program,
    entry: FunctionId,
) -> Vec<StringRelation> {
    let module = &program.modules[0];
    let function = module.function(entry).unwrap();
    let definitions = |entities: Vec<EntityId>| {
        entities
            .into_iter()
            .filter(|entity| {
                let ty = function.entity(*entity).unwrap().ty.semantic_type();
                module.types[ty.index()] == SsaTypeKind::StringOwner
            })
            .map(|entity| definition_key(sources, function, entity))
            .collect::<Vec<_>>()
    };
    let mut relations = Vec::new();
    for instruction in &function.instructions {
        let operands = definitions(instruction.operation.entities());
        let results = definitions(instruction.results.clone());
        if !operands.is_empty() || !results.is_empty() {
            relations.push(StringRelation {
                origin: source_key(sources, instruction.origin.span()),
                operation: Some(std::mem::discriminant(&instruction.operation)),
                operands,
                results,
            });
        }
    }
    for block in &function.blocks {
        let terminator = block.terminator.as_ref().unwrap();
        let operands = definitions(terminator.kind.entities());
        if !operands.is_empty() {
            relations.push(StringRelation {
                origin: source_key(sources, terminator.origin.span()),
                operation: None,
                operands,
                results: Vec::new(),
            });
        }
    }
    relations
}

#[test]
fn receiver_string_borrow_keeps_existing_single_unit_capability_difference() {
    let text = "package pair\nclass Reader { fun read(text: String): Unit { val printed = println(text) } }\nfun entry(): Unit { val reader = Reader()\nval ignored = reader.read(\"receiver\0\") }";
    let mut sources = SourceMap::new();
    let (source, file) = parsed(&mut sources, "pair/entry.ko", text);
    let (name_environment, environment) = standard_environments();
    let names = resolve_names(&sources, &file, &name_environment).unwrap();
    let typed = check_types(&sources, &file, &names, &environment).unwrap();
    let owned = check_ownership(&sources, &file, &names, &typed).unwrap();
    assert!(typed.diagnostics().is_empty());
    assert!(owned.diagnostics().is_empty());
    let entry = names
        .symbols()
        .iter()
        .find(|symbol| symbol.name() == "entry")
        .unwrap()
        .id();
    let error = lower_scalar_file_with_entry(&sources, &file, &names, &typed, &owned, entry)
        .err()
        .expect("single adapter has not implemented this receiver argument");
    let start = text.find("reader.read").unwrap();
    assert_eq!(
        error_key(&sources, error),
        (
            LoweringErrorKind::UnsupportedNode,
            Some(SourceKey("pair/entry.ko".to_owned(), start, text.len() - 2,))
        )
    );
    let inputs = [SourceUnitInput::new("root", "pair/entry.ko", source, &file)];
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &environment);
    let (unit, entry) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &environment,
        &typed,
        &owned,
        declaration(&names, "pair", "entry"),
    )
    .unwrap();
    let events = events(&sources, &unit, entry);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, Event::Literal(_, bytes) if bytes == b"receiver\0"))
    );
}
