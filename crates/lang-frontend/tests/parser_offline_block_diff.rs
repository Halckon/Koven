//! SPEC-0478: opt-in differential test against the independent block grammar.

use std::{
    env,
    fs::{self, DirBuilder, File},
    io::Write,
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use lang_frontend::{
    ast::ExpressionId,
    lexer::{LexedFile, LexemeKind, Symbol, TokenKind, TriviaKind, lex},
    parser::{BinaryOperator, Expression, PrefixOperator, Statement, SyntaxAst, parse_block},
    source::{SourceMap, Span},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Event<'a> {
    kind: &'static str,
    spelling: &'a str,
    bytes: (usize, usize),
    owner: (usize, usize, usize),
}

#[derive(Debug, PartialEq, Eq)]
enum Excluded {
    LexicalError,
    OutsideSubset,
}

fn events<'a>(sources: &'a SourceMap, lexed: &LexedFile) -> Result<Vec<Event<'a>>, Excluded> {
    if !lexed.diagnostics().is_empty() {
        return Err(Excluded::LexicalError);
    }
    let mut events = Vec::new();
    let mut end = 0;
    for (index, lexeme) in lexed.lexemes().iter().enumerate() {
        let span = lexeme.span();
        assert_eq!(span.start(), end);
        end = span.end();
        let spelling = sources.slice(span).unwrap();
        let owner = (index, span.start(), end);
        let kind = match lexeme.kind() {
            LexemeKind::Token(TokenKind::Identifier) => "ID",
            LexemeKind::Token(TokenKind::Symbol(Symbol::LeftBrace)) => "_LBRACE",
            LexemeKind::Token(TokenKind::Symbol(Symbol::RightBrace)) => "_RBRACE",
            LexemeKind::Token(TokenKind::Symbol(Symbol::Minus)) => "_MINUS",
            LexemeKind::Token(TokenKind::Symbol(Symbol::Dot)) => "_DOT",
            LexemeKind::Token(TokenKind::Symbol(Symbol::Semicolon)) => "_SEMI",
            LexemeKind::Eof => "_EOF",
            LexemeKind::Trivia(TriviaKind::Newline | TriviaKind::BlockComment) => {
                // Emit just the physical break. The reference grammar decides what it means.
                let raw = spelling.as_bytes();
                for (i, byte) in raw.iter().enumerate() {
                    if *byte == b'\n' {
                        let start = i - usize::from(i > 0 && raw[i - 1] == b'\r');
                        events.push(Event {
                            kind: "_NL",
                            spelling: &spelling[start..=i],
                            bytes: (span.start() + start, span.start() + i + 1),
                            owner,
                        });
                    }
                }
                continue;
            }
            LexemeKind::Trivia(_) => continue,
            LexemeKind::Invalid(_) => panic!("invalid lexeme without lexer diagnostics"),
            LexemeKind::Token(_) => return Err(Excluded::OutsideSubset),
        };
        events.push(Event {
            kind,
            spelling,
            bytes: (span.start(), end),
            owner,
        });
    }
    assert_eq!(events.last().unwrap().kind, "_EOF");
    assert_eq!(
        events.iter().filter(|event| event.kind == "_EOF").count(),
        1
    );
    assert_eq!(events.last().unwrap().spelling, "");
    assert_eq!(end, sources.source_text(lexed.source_id()).unwrap().len());
    Ok(events)
}

fn newline_contract(events: &[Event<'_>], expected: &[Event<'_>]) -> Result<(), &'static str> {
    if events
        .iter()
        .filter(|event| event.kind == "_NL")
        .copied()
        .eq(expected.iter().copied())
    {
        Ok(())
    } else {
        Err("newline event / span / owner mismatch")
    }
}

fn within(parent: Span, child: Span) {
    assert_eq!(parent.source_id(), child.source_id());
    assert!(
        parent.start() <= child.start()
            && child.start() < child.end()
            && child.end() <= parent.end()
    );
}

fn expression(ast: &SyntaxAst, sources: &SourceMap, id: ExpressionId, parent: Span) -> String {
    let node = ast.expressions().get(id).unwrap();
    let span = node.span();
    within(parent, span);
    let literal = |inner| {
        within(span, inner);
        sources.slice(inner).unwrap()
    };
    let child = |id| expression(ast, sources, id, span);
    match node.payload() {
        Expression::Name => format!("(id {})", sources.slice(span).unwrap()),
        Expression::Prefix {
            operator: PrefixOperator::Minus,
            operator_span,
            operand,
        } => {
            assert_eq!(literal(*operator_span), "-");
            let operand_span = ast.expressions().get(*operand).unwrap().span();
            assert!(operator_span.end() <= operand_span.start());
            format!("(neg {})", child(*operand))
        }
        Expression::Binary {
            operator: BinaryOperator::Subtract,
            operator_span,
            left,
            right,
        } => {
            assert_eq!(literal(*operator_span), "-");
            let left_span = ast.expressions().get(*left).unwrap().span();
            let right_span = ast.expressions().get(*right).unwrap().span();
            assert!(left_span.end() <= operator_span.start());
            assert!(operator_span.end() <= right_span.start());
            format!("(sub {} {})", child(*left), child(*right))
        }
        Expression::Member {
            receiver,
            operator_span,
            name_span,
            safe: false,
        } => {
            assert_eq!(literal(*operator_span), ".");
            let receiver_span = ast.expressions().get(*receiver).unwrap().span();
            assert!(receiver_span.end() <= operator_span.start());
            assert!(operator_span.end() <= name_span.start());
            format!("(member {} (id {}))", child(*receiver), literal(*name_span))
        }
        other => panic!("out-of-scope AST expression {other:?}"),
    }
}

fn production(sources: &SourceMap, lexed: &LexedFile, text: &str) -> String {
    let parsed = parse_block(sources, lexed).expect("Parser must not fail internally");
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let ast = parsed.ast();
    assert_eq!(ast.source_id(), lexed.source_id());
    let block = ast.statements().get(parsed.root()).unwrap();
    assert_eq!(block.span().source_id(), lexed.source_id());
    assert_eq!(sources.slice(block.span()).unwrap(), text);
    let Statement::Block { elements } = block.payload() else {
        panic!("not a block")
    };
    let mut shape = String::from("(block");
    for id in elements {
        let node = ast.statements().get(*id).unwrap();
        within(block.span(), node.span());
        let Statement::Expression { expression: id } = node.payload() else {
            panic!("not an expression statement")
        };
        shape.push(' ');
        shape.push_str(&expression(ast, sources, *id, node.span()));
    }
    shape.push(')');
    shape
}

#[derive(Debug, PartialEq, Eq)]
struct DeadlineExceeded;

fn run_before(command: &mut Command, budget: Duration) -> Result<ExitStatus, DeadlineExceeded> {
    let start = Instant::now();
    let mut child = command.spawn().expect("start offline reference");
    loop {
        let remaining = budget.saturating_sub(start.elapsed());
        if remaining.is_zero() {
            // `kill` can race a normal exit; `wait` still reaps the process.
            let _ = child.kill();
            child.wait().expect("reap timed-out reference");
            return Err(DeadlineExceeded);
        }
        if let Some(status) = child.try_wait().expect("check offline reference status") {
            return Ok(status);
        }
        thread::sleep(remaining.min(Duration::from_millis(10)));
    }
}

struct BatchFiles(PathBuf);

impl BatchFiles {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            trusted_batch_root().join(format!("koven-offline-{}-{nonce}", std::process::id()));
        let mut builder = DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder
            .create(&path)
            .expect("create private reference batch directory");
        Self(path)
    }
}

impl Drop for BatchFiles {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[cfg(unix)]
fn trusted_batch_root() -> PathBuf {
    use std::os::unix::fs::MetadataExt;

    // Ignore TMPDIR. Resolve the macOS /tmp symlink before inspecting its ancestors.
    let root = fs::canonicalize("/tmp").expect("resolve system temporary root");
    for (index, path) in root.ancestors().enumerate() {
        let meta = fs::metadata(path).expect("inspect system temporary root");
        let writable = meta.mode() & 0o022 != 0;
        let sticky_root = index == 0 && meta.mode() & 0o1000 != 0;
        assert!(
            meta.is_dir() && meta.uid() == 0 && (!writable || sticky_root),
            "unsafe system temporary root component: {}",
            path.display()
        );
    }
    root
}

#[cfg(not(unix))]
fn trusted_batch_root() -> PathBuf {
    panic!("SPEC-0478 offline reference is supported on macOS and Linux")
}

fn reference_launcher() -> Command {
    // Let the supervised child resolve and exec Python. The fixed shell program
    // never interprets the executable path or arguments as shell source.
    let mut command = Command::new("/bin/sh");
    command.env_remove("ENV").env_remove("BASH_ENV").args([
        "-c",
        "exec \"$@\"",
        "koven-offline-reference",
    ]);
    command
}

#[test]
#[ignore = "CI only: fault injection; see SPEC-0478"]
fn ci_fault_reference_deadline_covers_startup_without_output() {
    let mut command = reference_launcher();
    command.args(["/bin/sleep", "5"]);
    let start = Instant::now();
    assert_eq!(
        run_before(&mut command, Duration::from_millis(50)),
        Err(DeadlineExceeded)
    );
    assert!(
        start.elapsed() < Duration::from_secs(2),
        "deadline did not terminate promptly"
    );
}

fn reference(batches: &[Vec<Event<'_>>]) -> Vec<String> {
    let (status, output) = reference_batch(batches, false);
    assert!(status.success(), "offline reference failed: {status}");
    output
}

fn reference_batch(
    batches: &[Vec<Event<'_>>],
    inject_ambiguity: bool,
) -> (ExitStatus, Vec<String>) {
    let wheel =
        env::var_os("KOVEN_LARK_WHEEL").expect("set KOVEN_LARK_WHEEL to the fixed offline wheel");
    let script =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/offline_block_diff/reference.py");
    let python = env::var_os("KOVEN_OFFLINE_PYTHON").unwrap_or_else(|| "python3".into());
    // File-backed stdio prevents a full or undrained pipe from defeating the deadline.
    let batch = BatchFiles::new();
    let input_path = batch.0.join("events");
    let output_path = batch.0.join("results");
    let mut input = File::create_new(&input_path).unwrap();
    for events in batches {
        for event in events {
            // Only identifiers need spelling in the grammar. No source trivia is retokenized.
            let value = if event.kind == "ID" {
                event.spelling
            } else {
                ""
            };
            writeln!(input, "{}\t{value}", event.kind).unwrap();
        }
        writeln!(input).unwrap();
    }
    drop(input);
    let mut command = reference_launcher();
    command
        .arg(python)
        .args(["-I", "-B", "-S"])
        .arg(script)
        .arg(wheel)
        .arg(&batch.0)
        .stdin(Stdio::from(File::open(input_path).unwrap()))
        .stdout(Stdio::from(File::create_new(&output_path).unwrap()));
    if inject_ambiguity {
        command.arg("--inject-ambiguous-subtraction");
    }
    let status = run_before(&mut command, Duration::from_secs(30))
        .expect("offline reference exceeded the 30-second batch deadline");
    let output = fs::read_to_string(output_path)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect();
    (status, output)
}

fn compare_structure(production: &str, reference: &str) -> Result<(), &'static str> {
    if production == reference {
        Ok(())
    } else {
        Err("production and reference block structures differ")
    }
}

#[test]
#[ignore = "CI only: fault injection; see SPEC-0478"]
fn ci_fault_block_boundary_and_event_projection() {
    let cases = [
        (
            "{a\n-b}",
            Event {
                kind: "_NL",
                spelling: "\n",
                bytes: (2, 3),
                owner: (2, 2, 3),
            },
            "(block (id a) (neg (id b)))",
        ),
        (
            "{a/*雪\r\n*/.b}",
            Event {
                kind: "_NL",
                spelling: "\r\n",
                bytes: (7, 9),
                owner: (2, 2, 11),
            },
            "(block (member (id a) (id b)))",
        ),
    ];
    let mut sources = SourceMap::new();
    let mut batches = Vec::new();
    let mut expected = Vec::new();
    let bare = "{a/*雪\r*/-b}";
    let bare_id = sources.add_source("bare-cr.ko", bare).unwrap();
    let ids = [
        sources.add_source("split.ko", cases[0].0).unwrap(),
        sources.add_source("member.ko", cases[1].0).unwrap(),
    ];
    for (index, ((text, newline, shape), source)) in cases.into_iter().zip(ids).enumerate() {
        let lexed = lex(&sources, source).unwrap();
        assert_eq!(production(&sources, &lexed, text), shape);
        let rows = events(&sources, &lexed).unwrap();
        assert_eq!(newline_contract(&rows, &[newline]), Ok(()), "{text:?}");
        let mut mutant = rows.clone();
        if index == 0 {
            mutant.retain(|event| event.kind != "_NL");
        } else {
            let event = mutant.iter_mut().find(|event| event.kind == "_NL").unwrap();
            event.bytes.1 -= 1;
            event.spelling = "\r";
            assert!(newline_contract(&mutant, &[newline]).is_err());
        }
        batches.extend([rows, mutant]);
        expected.extend([
            shape,
            if index == 0 {
                "(block (sub (id a) (id b)))"
            } else {
                shape
            },
        ]);
    }
    // Dropping the member break is invisible in the tree, but not in the event contract.
    let mut missing_member_break = batches[2].clone();
    missing_member_break.retain(|event| event.kind != "_NL");
    assert!(newline_contract(&missing_member_break, &[cases[1].1]).is_err());
    batches.push(missing_member_break);
    expected.push(cases[1].2);

    // A bare CR in comment content is not a newline.
    let lexed = lex(&sources, bare_id).unwrap();
    let mut injected = events(&sources, &lexed).unwrap();
    assert_eq!(newline_contract(&injected, &[]), Ok(()));
    let original = production(&sources, &lexed, bare);
    assert_eq!(original, "(block (sub (id a) (id b)))");
    injected.insert(
        2,
        Event {
            spelling: "\r",
            bytes: (7, 8),
            owner: (2, 2, 10),
            ..cases[1].1
        },
    );
    assert!(newline_contract(&injected, &[]).is_err());
    batches.push(injected);
    expected.push(cases[0].2);

    let output = reference(&batches);
    assert_eq!(output, expected);
    assert_eq!(compare_structure(cases[0].2, &output[0]), Ok(()));
    assert!(compare_structure(cases[0].2, &output[1]).is_err());
    assert!(compare_structure(&original, &output[5]).is_err());
    assert!(compare_structure("(block (neg (id b)) (id a))", &output[0]).is_err());
    assert!(compare_structure("(block (id a) (neg (id a)))", &output[0]).is_err());
}

#[test]
#[ignore = "CI only: fault injection; see SPEC-0478"]
fn ci_fault_ambiguous_reference_rejects_without_a_tree() {
    let mut sources = SourceMap::new();
    let id = sources.add_source("associativity.ko", "{a-b-c}").unwrap();
    let lexed = lex(&sources, id).unwrap();
    let batches = [events(&sources, &lexed).unwrap()];
    assert_eq!(
        reference(&batches),
        ["(block (sub (sub (id a) (id b)) (id c)))"]
    );
    let (status, output) = reference_batch(&batches, true);
    assert_eq!(
        status.code(),
        Some(42),
        "expected a distinct ambiguity failure"
    );
    assert!(output.is_empty(), "ambiguity published a successful result");
}

#[test]
#[ignore = "CI only: fault injection; see SPEC-0478"]
fn ci_fault_reference_event_limit() {
    let mut sources = SourceMap::new();
    let ids = [61, 62].map(|n| {
        sources
            .add_source(format!("limit-{n}.ko"), format!("{{{}}}", "\n".repeat(n)))
            .unwrap()
    });
    let lexed = ids.map(|id| lex(&sources, id).unwrap());
    let at_limit = events(&sources, &lexed[0]).unwrap();
    let over_limit = events(&sources, &lexed[1]).unwrap();
    assert_eq!((at_limit.len(), over_limit.len()), (64, 65));
    assert_eq!(reference(&[at_limit]), ["(block)"]);
    let (status, output) = reference_batch(&[over_limit], false);
    assert_eq!(status.code(), Some(43), "expected an event limit failure");
    assert!(output.is_empty(), "over-limit input published a result");
}

#[test]
#[ignore = "offline only: requires the verified Lark wheel; see SPEC-0478"]
fn fixed_and_generated_corpus() {
    use Excluded::{LexicalError, OutsideSubset};
    enum Expected {
        Shape(&'static str),
        Syntax(&'static str),
        Exclusion(Excluded),
    }
    use Expected::{Exclusion, Shape, Syntax};
    const SPLIT: &str = "(block (id a) (neg (id b)))";
    const SUB: &str = "(block (sub (id a) (id b)))";
    const MEMBER: &str = "(block (member (id a) (id b)))";
    let fixed = [
        ("{a\n-b}", Shape(SPLIT)),
        ("{a\r\n-b}", Shape(SPLIT)),
        ("{a-\nb}", Shape(SUB)),
        ("{a-\r\nb}", Shape(SUB)),
        ("{a//雪\n-b}", Shape(SPLIT)),
        ("{a/*雪\n*/-b}", Shape(SPLIT)),
        ("{a/*雪\r\n*/.b}", Shape(MEMBER)),
        ("{a/*雪\n\r\n*/.b}", Shape(MEMBER)),
        ("{a/*雪\r*/-b}", Shape(SUB)),
        ("{a;\n.b}", Syntax("L0029")),
        (
            "{a-b.c}",
            Shape("(block (sub (id a) (member (id b) (id c))))"),
        ),
        ("{-a.b}", Shape("(block (neg (member (id a) (id b))))")),
        ("{a}b", Syntax("L0013")),
        ("{a\rb}", Exclusion(LexicalError)),
        ("{a/*雪\n", Exclusion(LexicalError)),
        ("{a+b}", Exclusion(OutsideSubset)),
    ];
    let mut generated = Vec::new();
    for a in ['a', 'b'] {
        for b in ['a', 'b'] {
            for c in ['a', 'b'] {
                generated.extend([
                    (
                        format!("{{{a}-{b}.{c}}}"),
                        format!("(block (sub (id {a}) (member (id {b}) (id {c}))))"),
                    ),
                    (
                        format!("{{-{a}.{b}-{c}}}"),
                        format!("(block (sub (neg (member (id {a}) (id {b}))) (id {c})))"),
                    ),
                    (
                        format!("{{{a}-{b}-{c}}}"),
                        format!("(block (sub (sub (id {a}) (id {b})) (id {c})))"),
                    ),
                    (
                        format!("{{{a}.{b};-{c}}}"),
                        format!("(block (member (id {a}) (id {b})) (neg (id {c})))"),
                    ),
                ]);
            }
        }
    }
    assert_eq!(
        generated
            .iter()
            .map(|(text, _)| text)
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        32
    );
    let mut sources = SourceMap::new();
    let ids: Vec<_> = fixed
        .iter()
        .map(|(text, _)| *text)
        .chain(generated.iter().map(|(text, _)| text.as_str()))
        .chain(["{\n a\n-b}"])
        .enumerate()
        .map(|(n, text)| sources.add_source(format!("case-{n}.ko"), text).unwrap())
        .collect();
    let mut counts = [0; 4];
    let mut batches = Vec::new();
    let mut expected = Vec::new();
    for ((text, want), source) in fixed.into_iter().zip(ids.iter().copied()) {
        let lexed = lex(&sources, source).unwrap();
        let rows = events(&sources, &lexed);
        if let Ok(ref rows) = rows {
            // Hand-recorded positions; do not derive the expected gaps from the projection.
            let nl = |spelling, bytes, owner| Event {
                kind: "_NL",
                spelling,
                bytes,
                owner,
            };
            let breaks = match text {
                "{a\n-b}" => vec![nl("\n", (2, 3), (2, 2, 3))],
                "{a\r\n-b}" => vec![nl("\r\n", (2, 4), (2, 2, 4))],
                "{a-\nb}" => vec![nl("\n", (3, 4), (3, 3, 4))],
                "{a-\r\nb}" => vec![nl("\r\n", (3, 5), (3, 3, 5))],
                "{a//雪\n-b}" => vec![nl("\n", (7, 8), (3, 7, 8))],
                "{a/*雪\n*/-b}" => vec![nl("\n", (7, 8), (2, 2, 10))],
                "{a/*雪\r\n*/.b}" => vec![nl("\r\n", (7, 9), (2, 2, 11))],
                "{a/*雪\n\r\n*/.b}" => vec![
                    nl("\n", (7, 8), (2, 2, 12)),
                    nl("\r\n", (8, 10), (2, 2, 12)),
                ],
                "{a;\n.b}" => vec![nl("\n", (3, 4), (3, 3, 4))],
                _ => vec![],
            };
            assert_eq!(
                newline_contract(rows, &breaks),
                Ok(()),
                "{text:?}: {rows:?}"
            );
        }
        match want {
            Shape(shape) => {
                assert_eq!(production(&sources, &lexed, text), shape, "{text:?}");
                batches.push(rows.unwrap());
                expected.push(shape);
                counts[0] += 1;
            }
            Syntax(code) => {
                let parsed = parse_block(&sources, &lexed).unwrap();
                let diagnostics: Vec<_> = parsed
                    .diagnostics()
                    .iter()
                    .map(|d| d.code().to_string())
                    .collect();
                assert_eq!(diagnostics, [code], "{text:?}");
                batches.push(rows.unwrap());
                expected.push("syntax-error");
                counts[1] += 1;
            }
            Exclusion(reason) => {
                counts[if reason == LexicalError { 2 } else { 3 }] += 1;
                assert_eq!(rows, Err(reason), "{text:?}");
            }
        }
    }
    assert_eq!(counts, [11, 2, 2, 1]);
    for ((text, shape), source) in generated.iter().zip(ids[16..].iter().copied()) {
        let lexed = lex(&sources, source).unwrap();
        assert_eq!(production(&sources, &lexed, text), *shape, "{text:?}");
        batches.push(events(&sources, &lexed).unwrap());
        expected.push(shape);
    }
    let lexed = lex(&sources, *ids.last().unwrap()).unwrap();
    assert_eq!(production(&sources, &lexed, "{\n a\n-b}"), SPLIT);
    let shifted_rows = events(&sources, &lexed).unwrap();
    assert_eq!(
        newline_contract(
            &shifted_rows,
            &[
                Event {
                    kind: "_NL",
                    spelling: "\n",
                    bytes: (1, 2),
                    owner: (1, 1, 2),
                },
                Event {
                    kind: "_NL",
                    spelling: "\n",
                    bytes: (4, 5),
                    owner: (4, 4, 5),
                },
            ],
        ),
        Ok(()),
        "{{\n a\n-b}}: {shifted_rows:?}"
    );
    batches.push(shifted_rows);
    expected.push(SPLIT);
    let output = reference(&batches);
    assert_eq!(output.len(), expected.len());
    for (got, want) in output.iter().zip(expected) {
        assert_eq!(
            compare_structure(want, got),
            Ok(()),
            "expected {want}, got {got}"
        );
    }
}
