//! SPEC-0076 的已发布 Parser 诊断公开入口 witness 矩阵。

use std::collections::BTreeSet;

use lang_frontend::{
    diagnostic::codes,
    lexer::{LexedFile, lex},
    parser::{ParserInternalError, parse_block, parse_declaration, parse_expression, parse_file},
    source::{SourceId, SourceMap, Span},
};

#[derive(Clone, Copy, Debug)]
enum Entry {
    Expression,
    Declaration,
    Block,
    File,
}

#[derive(Clone, Copy)]
struct Witness {
    code: &'static str,
    entry: Entry,
    source: &'static str,
}

const WITNESSES: &[Witness] = &[
    Witness {
        code: "L0009",
        entry: Entry::Expression,
        source: "",
    },
    Witness {
        code: "L0010",
        entry: Entry::Expression,
        source: "(",
    },
    Witness {
        code: "L0011",
        entry: Entry::Expression,
        source: "a.",
    },
    Witness {
        code: "L0012",
        entry: Entry::Expression,
        source: "a < b <= c",
    },
    Witness {
        code: "L0013",
        entry: Entry::Expression,
        source: "a b",
    },
    Witness {
        code: "L0014",
        entry: Entry::Expression,
        source: "a as 4",
    },
    Witness {
        code: "L0015",
        entry: Entry::Expression,
        source: "a++b",
    },
    Witness {
        code: "L0017",
        entry: Entry::Declaration,
        source: "",
    },
    Witness {
        code: "L0018",
        entry: Entry::Declaration,
        source: "val = 1",
    },
    Witness {
        code: "L0019",
        entry: Entry::Declaration,
        source: "fun f(: T): R",
    },
    Witness {
        code: "L0020",
        entry: Entry::Declaration,
        source: "val x Int",
    },
    Witness {
        code: "L0021",
        entry: Entry::Declaration,
        source: "fun f() = 1",
    },
    Witness {
        code: "L0022",
        entry: Entry::Declaration,
        source: "const x = 1",
    },
    Witness {
        code: "L0023",
        entry: Entry::Declaration,
        source: "fun f(x T): R",
    },
    Witness {
        code: "L0024",
        entry: Entry::Declaration,
        source: "fun <> f(): R",
    },
    Witness {
        code: "L0025",
        entry: Entry::Declaration,
        source: "fun f(x: T y: U): R",
    },
    Witness {
        code: "L0026",
        entry: Entry::Declaration,
        source: "fun f(x: T,): R",
    },
    Witness {
        code: "L0027",
        entry: Entry::Declaration,
        source: "fun f(x: T = 1): R",
    },
    Witness {
        code: "L0028",
        entry: Entry::Block,
        source: "answer trailing",
    },
    Witness {
        code: "L0029",
        entry: Entry::Block,
        source: "{ ) val y = 2 }",
    },
    Witness {
        code: "L0030",
        entry: Entry::Block,
        source: "{ fun }",
    },
    Witness {
        code: "L0031",
        entry: Entry::Expression,
        source: "{ : }",
    },
    Witness {
        code: "L0032",
        entry: Entry::Expression,
        source: "{ , }",
    },
    Witness {
        code: "L0033",
        entry: Entry::Expression,
        source: "f(name =)",
    },
    Witness {
        code: "L0034",
        entry: Entry::Expression,
        source: "f(a b)",
    },
    Witness {
        code: "L0035",
        entry: Entry::Expression,
        source: "f(,a)",
    },
    Witness {
        code: "L0036",
        entry: Entry::Expression,
        source: "f(a,)",
    },
    Witness {
        code: "L0037",
        entry: Entry::Expression,
        source: "f(borrow name = input)",
    },
    Witness {
        code: "L0038",
        entry: Entry::Expression,
        source: "f(& &x)",
    },
    Witness {
        code: "L0039",
        entry: Entry::Expression,
        source: "input as (inout borrow T, U) -> R",
    },
    Witness {
        code: "L0040",
        entry: Entry::Block,
        source: "{ val () = x }",
    },
    Witness {
        code: "L0041",
        entry: Entry::Block,
        source: "{ val (a b) = x }",
    },
    Witness {
        code: "L0042",
        entry: Entry::Block,
        source: "{ val (_) = x }",
    },
    Witness {
        code: "L0043",
        entry: Entry::Declaration,
        source: "val (a) = x",
    },
    Witness {
        code: "L0044",
        entry: Entry::Block,
        source: "{ val (a,) = x }",
    },
    Witness {
        code: "L0045",
        entry: Entry::Block,
        source: "{ val (a) x }",
    },
    Witness {
        code: "L0046",
        entry: Entry::Block,
        source: "{ val (a) = }",
    },
    Witness {
        code: "L0047",
        entry: Entry::File,
        source: "val answer = 42 fun next() {}",
    },
    Witness {
        code: "L0048",
        entry: Entry::File,
        source: "package\nval x=1",
    },
    Witness {
        code: "L0049",
        entry: Entry::File,
        source: "import\nval x=1",
    },
    Witness {
        code: "L0050",
        entry: Entry::File,
        source: "import a.b as\nval x=1",
    },
    Witness {
        code: "L0051",
        entry: Entry::File,
        source: "package a\npackage b\nval x=1",
    },
    Witness {
        code: "L0052",
        entry: Entry::File,
        source: "val x=1\nimport c",
    },
    Witness {
        code: "L0053",
        entry: Entry::File,
        source: "package a import b\nval x=1",
    },
    Witness {
        code: "L0054",
        entry: Entry::File,
        source: "import a.* as Alias\nval x=1",
    },
    Witness {
        code: "L0055",
        entry: Entry::Expression,
        source: "if () 1 else 2",
    },
    Witness {
        code: "L0056",
        entry: Entry::Expression,
        source: "if (ready) else 2",
    },
    Witness {
        code: "L0057",
        entry: Entry::Expression,
        source: "if (ready) 1",
    },
    Witness {
        code: "L0058",
        entry: Entry::Expression,
        source: "when { -> run() }",
    },
    Witness {
        code: "L0059",
        entry: Entry::Expression,
        source: "when { ready run() }",
    },
    Witness {
        code: "L0060",
        entry: Entry::Block,
        source: "{ while (ready) next }",
    },
    Witness {
        code: "L0061",
        entry: Entry::Block,
        source: "{ for (in values) {} }",
    },
    Witness {
        code: "L0062",
        entry: Entry::Block,
        source: "{ for (item values) {} }",
    },
    Witness {
        code: "L0063",
        entry: Entry::Expression,
        source: "super.log",
    },
    Witness {
        code: "L0064",
        entry: Entry::Expression,
        source: "super<Logger>log",
    },
    Witness {
        code: "L0065",
        entry: Entry::Expression,
        source: "when { ready -> run() other -> stop() }",
    },
    Witness {
        code: "L0066",
        entry: Entry::Declaration,
        source: "value Thing(val x: Int)",
    },
    Witness {
        code: "L0067",
        entry: Entry::Declaration,
        source: "class {}",
    },
    Witness {
        code: "L0068",
        entry: Entry::Declaration,
        source: "value class Empty()",
    },
    Witness {
        code: "L0069",
        entry: Entry::Declaration,
        source: "class Pair(val first: Int val second: Int)",
    },
    Witness {
        code: "L0070",
        entry: Entry::Declaration,
        source: "class Child: {}",
    },
    Witness {
        code: "L0071",
        entry: Entry::Declaration,
        source: "class Broken { @ }",
    },
    Witness {
        code: "L0072",
        entry: Entry::Declaration,
        source: "class C { fun a() {} fun b() {} }",
    },
    Witness {
        code: "L0073",
        entry: Entry::Declaration,
        source: "enum class Empty { }",
    },
    Witness {
        code: "L0074",
        entry: Entry::Declaration,
        source: "enum class E { A B }",
    },
    Witness {
        code: "L0075",
        entry: Entry::Declaration,
        source: "enum class E { A fun f() {} }",
    },
    Witness {
        code: "L0076",
        entry: Entry::Declaration,
        source: "public private class C",
    },
    Witness {
        code: "L0077",
        entry: Entry::Declaration,
        source: "class C(val item: Int = 1)",
    },
    Witness {
        code: "L0078",
        entry: Entry::Declaration,
        source: "class C: I by, J",
    },
];

#[derive(Debug, PartialEq, Eq)]
struct DiagnosticFingerprint {
    code: String,
    message: String,
    start: usize,
    end: usize,
}

#[derive(Debug, PartialEq, Eq)]
struct ParseFingerprint {
    public_output: String,
    diagnostics: Vec<DiagnosticFingerprint>,
}

fn fingerprint_span(source_id: SourceId, source_len: usize, span: Span) -> (usize, usize) {
    assert_eq!(span.source_id(), source_id);
    assert!(span.start() <= span.end());
    assert!(span.end() <= source_len);
    (span.start(), span.end())
}

fn parse_fingerprint(
    entry: Entry,
    sources: &SourceMap,
    lexed: &LexedFile,
) -> Result<ParseFingerprint, ParserInternalError> {
    macro_rules! fingerprint {
        ($parsed:expr) => {{
            let parsed = $parsed?;
            let source_id = parsed.source_id();
            let source_len = sources.source_text(source_id)?.len();
            let diagnostics = parsed
                .diagnostics()
                .iter()
                .map(|diagnostic| {
                    let (start, end) =
                        fingerprint_span(source_id, source_len, diagnostic.primary_span());
                    DiagnosticFingerprint {
                        code: diagnostic.code().to_string(),
                        message: diagnostic.message().to_owned(),
                        start,
                        end,
                    }
                })
                .collect();
            ParseFingerprint {
                public_output: format!("{parsed:?}"),
                diagnostics,
            }
        }};
    }

    Ok(match entry {
        Entry::Expression => fingerprint!(parse_expression(sources, lexed)),
        Entry::Declaration => fingerprint!(parse_declaration(sources, lexed)),
        Entry::Block => fingerprint!(parse_block(sources, lexed)),
        Entry::File => fingerprint!(parse_file(sources, lexed)),
    })
}

#[test]
fn every_current_parser_diagnostic_has_one_public_lexer_clean_witness() {
    assert_eq!(WITNESSES.len(), 69);
    assert!(codes::ALL.contains(&"L0016"));

    let expected_codes = codes::ALL
        .iter()
        .copied()
        .filter(|code| ("L0009"..="L0078").contains(code) && *code != "L0016")
        .collect::<BTreeSet<_>>();
    let witness_codes = WITNESSES
        .iter()
        .map(|witness| witness.code)
        .collect::<BTreeSet<_>>();
    assert_eq!(witness_codes.len(), WITNESSES.len());
    assert_eq!(witness_codes, expected_codes);

    for witness in WITNESSES {
        let mut sources = SourceMap::new();
        let source_id = sources
            .add_source("parser-diagnostic-witness.ko", witness.source)
            .expect("witness source name must be unique");
        let lexed = lex(&sources, source_id).expect("witness must lex internally");
        assert!(
            lexed.diagnostics().is_empty(),
            "{} {:?} is not Lexer-clean: {:?}",
            witness.code,
            witness.source,
            lexed.diagnostics()
        );

        let first = parse_fingerprint(witness.entry, &sources, &lexed).unwrap_or_else(|error| {
            panic!(
                "{} via {:?} failed internally for {:?}: {error}",
                witness.code, witness.entry, witness.source
            )
        });
        let repeated = parse_fingerprint(witness.entry, &sources, &lexed).unwrap_or_else(|error| {
            panic!(
                "repeated {} via {:?} failed internally for {:?}: {error}",
                witness.code, witness.entry, witness.source
            )
        });
        assert_eq!(
            first, repeated,
            "{} via {:?} is not deterministic for {:?}",
            witness.code, witness.entry, witness.source
        );
        assert_eq!(
            first
                .diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.code == witness.code)
                .count(),
            1,
            "{} via {:?} did not produce exactly one witness for {:?}: {:?}",
            witness.code,
            witness.entry,
            witness.source,
            first.diagnostics
        );
        assert!(
            first
                .diagnostics
                .iter()
                .all(|diagnostic| diagnostic.code != "L0016"),
            "retired L0016 was emitted by {:?} for {:?}",
            witness.entry,
            witness.source
        );
    }
}
