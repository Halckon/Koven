//! SPEC-0481 (F4b): Complete-file differential tests comparing Rust `parse_file`
//! against the locked `editors/tree-sitter` parser across the documented common
//! subset, SPEC-0479 constructs, shared rejections, and attributed drift witnesses.

use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use lang_frontend::{
    ast::{ExpressionId, ItemId, StatementId},
    lexer::lex,
    parser::{
        ClassifierKind, Expression, FunctionBody, FunctionForm, Item, NameMarker, ParsedFile,
        Statement, StringPart, VariableKind, WhenCondition, parse_file,
    },
    source::{SourceMap, Span},
};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct StructuralCounts {
    nested_blocks: usize,
    control_blocks: usize,
    lambdas: usize,
    calls: usize,
    call_lambdas: usize,
    when_entries: usize,
    while_loops: usize,
    for_loops: usize,
    loops: usize,
    prefixes: usize,
    binaries: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ImportSummary {
    segments: Vec<String>,
    wildcard: bool,
    alias: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FileSummary {
    package: Vec<String>,
    imports: Vec<ImportSummary>,
    items: Vec<(&'static str, String)>,
    counts: StructuralCounts,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ParseOutcome {
    Accepted(FileSummary),
    Rejected,
}

fn slice(sources: &SourceMap, span: Span) -> &str {
    sources.slice(span).expect("valid UTF-8 span")
}

fn marker_name(sources: &SourceMap, marker: NameMarker) -> String {
    match marker {
        NameMarker::Present(span) => slice(sources, span).to_owned(),
        other => panic!("unexpected non-present name marker in clean parse: {other:?}"),
    }
}

fn project_rust(source: &str) -> ParseOutcome {
    let mut sources = SourceMap::new();
    let id = sources.add_source("diff.ko", source).expect("add source");
    let lexed = lex(&sources, id).expect("lex must not fail internally");
    if !lexed.diagnostics().is_empty() {
        return ParseOutcome::Rejected;
    }
    let parsed = parse_file(&sources, &lexed).expect("parse_file must not fail internally");
    if !parsed.diagnostics().is_empty() {
        return ParseOutcome::Rejected;
    }
    ParseOutcome::Accepted(summarize_rust(&sources, &parsed))
}

fn summarize_rust(sources: &SourceMap, parsed: &ParsedFile) -> FileSummary {
    let package = parsed
        .package()
        .map(|header| {
            header
                .segments
                .iter()
                .map(|seg| slice(sources, seg.span).to_owned())
                .collect()
        })
        .unwrap_or_default();
    let imports = parsed
        .imports()
        .iter()
        .map(|import| ImportSummary {
            segments: import
                .segments
                .iter()
                .map(|seg| slice(sources, seg.span).to_owned())
                .collect(),
            wildcard: import.wildcard_span.is_some(),
            alias: import
                .alias
                .map(|alias| slice(sources, alias.name_span).to_owned()),
        })
        .collect();
    let mut items = Vec::new();
    let mut counts = StructuralCounts::default();
    for &root in parsed.roots() {
        items.push(top_level_item_header(sources, parsed, root));
        visit_item(parsed, root, &mut counts);
    }
    FileSummary {
        package,
        imports,
        items,
        counts,
    }
}

fn top_level_item_header(
    sources: &SourceMap,
    parsed: &ParsedFile,
    item_id: ItemId,
) -> (&'static str, String) {
    match parsed.ast().items().get(item_id).unwrap().payload() {
        Item::Modified { declaration, .. } => top_level_item_header(sources, parsed, *declaration),
        Item::Variable { kind, name, .. } => {
            let label = match kind {
                VariableKind::Val => "val",
                VariableKind::Var => "var",
                VariableKind::BorrowVal(_) => "borrow_val",
            };
            (label, marker_name(sources, *name))
        }
        Item::Constant { name, .. } => ("const_val", marker_name(sources, *name)),
        Item::Function { name, .. } => ("fun", marker_name(sources, *name)),
        Item::Classifier(classifier) => {
            let label = match classifier.kind {
                ClassifierKind::ValueClass { .. } => "value_class",
                ClassifierKind::Class { .. } => "class",
                ClassifierKind::Interface { .. } => "interface",
                ClassifierKind::EnumClass { .. } => "enum_class",
                ClassifierKind::Object { .. } => "object",
            };
            (label, marker_name(sources, classifier.name))
        }
        other => panic!("unexpected top-level item in clean parse: {other:?}"),
    }
}

fn visit_item(parsed: &ParsedFile, item_id: ItemId, counts: &mut StructuralCounts) {
    match parsed.ast().items().get(item_id).unwrap().payload() {
        Item::Modified { declaration, .. } => visit_item(parsed, *declaration, counts),
        Item::Variable { initializer, .. } | Item::Constant { initializer, .. } => {
            visit_expr(parsed, *initializer, counts);
        }
        Item::Function { form, .. } => match form {
            FunctionForm::ImplicitUnitAbsent => {}
            FunctionForm::ImplicitUnitBlock(body) => visit_stmt(parsed, *body, false, counts),
            FunctionForm::Explicit { body, .. } => match body {
                FunctionBody::Absent => {}
                FunctionBody::Expression { expression, .. } => {
                    visit_expr(parsed, *expression, counts);
                }
                FunctionBody::Block(body) => visit_stmt(parsed, *body, false, counts),
            },
        },
        Item::Classifier(classifier) => {
            if let Some(body) = &classifier.body {
                for &member in &body.members {
                    visit_item(parsed, member, counts);
                }
            }
        }
        Item::Companion(companion) => {
            for &member in &companion.body.members {
                visit_item(parsed, member, counts);
            }
        }
        Item::Deinit { body, .. } => visit_stmt(parsed, *body, false, counts),
        Item::Error => {}
    }
}

fn visit_stmt(
    parsed: &ParsedFile,
    stmt_id: StatementId,
    is_nested_block: bool,
    counts: &mut StructuralCounts,
) {
    match parsed.ast().statements().get(stmt_id).unwrap().payload() {
        Statement::Block { elements } => {
            if is_nested_block {
                counts.nested_blocks += 1;
            }
            for &child in elements {
                visit_stmt(parsed, child, true, counts);
            }
        }
        Statement::LambdaBody { elements } => {
            for &child in elements {
                visit_stmt(parsed, child, true, counts);
            }
        }
        Statement::ControlBody { elements } => {
            counts.control_blocks += 1;
            for &child in elements {
                visit_stmt(parsed, child, true, counts);
            }
        }
        Statement::LocalVariable { declaration } => visit_item(parsed, *declaration, counts),
        Statement::LocalDestructuring { initializer, .. } => {
            visit_expr(parsed, *initializer, counts);
        }
        Statement::While {
            condition, body, ..
        } => {
            counts.while_loops += 1;
            visit_expr(parsed, *condition, counts);
            visit_stmt(parsed, *body, false, counts);
        }
        Statement::For { source, body, .. } => {
            counts.for_loops += 1;
            visit_expr(parsed, *source, counts);
            visit_stmt(parsed, *body, false, counts);
        }
        Statement::Loop { body, .. } => {
            counts.loops += 1;
            visit_stmt(parsed, *body, false, counts);
        }
        Statement::Expression { expression } => visit_expr(parsed, *expression, counts),
        Statement::Error => {}
    }
}

fn visit_expr(parsed: &ParsedFile, expr_id: ExpressionId, counts: &mut StructuralCounts) {
    match parsed.ast().expressions().get(expr_id).unwrap().payload() {
        Expression::Error
        | Expression::Name
        | Expression::This
        | Expression::Literal(_)
        | Expression::Break { .. }
        | Expression::Continue { .. }
        | Expression::SuperMember { .. } => {}
        Expression::Group { expression } => visit_expr(parsed, *expression, counts),
        Expression::String { parts } => {
            for part in parts {
                if let StringPart::Interpolation { expression, .. } = part {
                    visit_expr(parsed, *expression, counts);
                }
            }
        }
        Expression::Lambda { body, .. } => {
            counts.lambdas += 1;
            visit_stmt(parsed, *body, false, counts);
        }
        Expression::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            visit_expr(parsed, *condition, counts);
            visit_stmt(parsed, *then_branch, false, counts);
            if let Some(branch) = else_branch {
                visit_stmt(parsed, *branch, false, counts);
            }
        }
        Expression::When {
            subject, entries, ..
        } => {
            if let Some(subject) = subject {
                visit_expr(parsed, *subject, counts);
            }
            counts.when_entries += entries.len();
            for entry in entries {
                for cond in &entry.conditions {
                    match cond {
                        WhenCondition::Expression(expr)
                        | WhenCondition::Contains {
                            expression: expr, ..
                        } => visit_expr(parsed, *expr, counts),
                        WhenCondition::TypeTest { .. } => {}
                    }
                }
                visit_stmt(parsed, entry.body, false, counts);
            }
        }
        Expression::Return { value, .. } => {
            if let Some(value) = value {
                visit_expr(parsed, *value, counts);
            }
        }
        Expression::Prefix { operand, .. } => {
            counts.prefixes += 1;
            visit_expr(parsed, *operand, counts);
        }
        Expression::NonNullAssert { operand, .. }
        | Expression::Cast {
            expression: operand,
            ..
        }
        | Expression::TypeTest {
            expression: operand,
            ..
        }
        | Expression::Propagate { value: operand, .. } => {
            visit_expr(parsed, *operand, counts);
        }
        Expression::Binary { left, right, .. } => {
            counts.binaries += 1;
            visit_expr(parsed, *left, counts);
            visit_expr(parsed, *right, counts);
        }
        Expression::Assignment {
            target: left,
            value: right,
            ..
        }
        | Expression::Index {
            receiver: left,
            index: right,
        } => {
            visit_expr(parsed, *left, counts);
            visit_expr(parsed, *right, counts);
        }
        Expression::Member { receiver, .. } => visit_expr(parsed, *receiver, counts),
        Expression::CallableReference { receiver, .. } => {
            if let Some(receiver) = receiver {
                visit_expr(parsed, *receiver, counts);
            }
        }
        Expression::Call {
            callee, arguments, ..
        } => {
            counts.calls += 1;
            visit_expr(parsed, *callee, counts);
            for arg in arguments {
                if matches!(
                    parsed.ast().expressions().get(arg.value).unwrap().payload(),
                    Expression::Lambda { .. }
                ) {
                    counts.call_lambdas += 1;
                }
                visit_expr(parsed, arg.value, counts);
            }
        }
    }
}

#[derive(Debug)]
struct XmlNode {
    tag: String,
    field: Option<String>,
    text: String,
    children: Vec<XmlNode>,
}

fn parse_xml_element(input: &str) -> Result<XmlNode, String> {
    let start = input
        .find("<source_file")
        .or_else(|| input.find("<ERROR"))
        .ok_or_else(|| "missing <source_file> or <ERROR> in tree-sitter XML".to_owned())?;
    let bytes = &input.as_bytes()[start..];
    let mut pos = 0usize;
    let mut stack: Vec<XmlNode> = Vec::new();
    while pos < bytes.len() {
        if bytes[pos] == b'<' {
            let close = bytes[pos..]
                .iter()
                .position(|&b| b == b'>')
                .ok_or_else(|| "unterminated XML tag".to_owned())?
                + pos;
            let raw = std::str::from_utf8(&bytes[pos + 1..close]).unwrap().trim();
            pos = close + 1;
            if let Some(end_tag) = raw.strip_prefix('/') {
                let finished = stack
                    .pop()
                    .ok_or_else(|| format!("unexpected closing tag </{end_tag}>"))?;
                if finished.tag != end_tag.trim() {
                    return Err(format!(
                        "mismatched XML tag </{end_tag}> for <{}>",
                        finished.tag
                    ));
                }
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(finished);
                } else {
                    return Ok(finished);
                }
            } else {
                let self_closing = raw.ends_with('/');
                let header = raw.trim_end_matches('/').trim();
                let mut parts = header.split_whitespace();
                let tag = parts.next().unwrap().to_owned();
                let mut field = None;
                for attr in parts {
                    if let Some(rest) = attr.strip_prefix("field=\"") {
                        field = rest.strip_suffix('"').map(str::to_owned);
                    }
                }
                let node = XmlNode {
                    tag,
                    field,
                    text: String::new(),
                    children: Vec::new(),
                };
                if self_closing {
                    if let Some(parent) = stack.last_mut() {
                        parent.children.push(node);
                    } else {
                        return Ok(node);
                    }
                } else {
                    stack.push(node);
                }
            }
        } else {
            let next_lt = bytes[pos..]
                .iter()
                .position(|&b| b == b'<')
                .map_or(bytes.len(), |off| pos + off);
            if let Some(top) = stack.last_mut() {
                let chunk = std::str::from_utf8(&bytes[pos..next_lt]).unwrap();
                top.text.push_str(chunk);
            }
            pos = next_lt;
        }
    }
    Err("unclosed <source_file> in tree-sitter XML".to_owned())
}

fn has_xml_tag(node: &XmlNode, name: &str) -> bool {
    node.tag == name || node.children.iter().any(|child| has_xml_tag(child, name))
}

fn collect_xml_identifiers(node: &XmlNode, out: &mut Vec<String>) {
    if node.tag == "identifier" {
        out.push(node.text.trim().to_owned());
        return;
    }
    for child in &node.children {
        collect_xml_identifiers(child, out);
    }
}

fn child_by_field<'a>(node: &'a XmlNode, field: &str) -> Option<&'a XmlNode> {
    node.children
        .iter()
        .find(|child| child.field.as_deref() == Some(field))
}

fn summarize_tree_sitter_xml(xml: &str) -> Result<ParseOutcome, String> {
    let root = parse_xml_element(xml)?;
    if has_xml_tag(&root, "ERROR") || has_xml_tag(&root, "MISSING") {
        return Ok(ParseOutcome::Rejected);
    }
    let mut package = Vec::new();
    let mut imports = Vec::new();
    let mut items = Vec::new();
    for child in &root.children {
        match child.tag.as_str() {
            "package_directive" => collect_xml_identifiers(child, &mut package),
            "import_directive" => {
                let target = child_by_field(child, "target").expect("import target");
                let mut segments = Vec::new();
                collect_xml_identifiers(target, &mut segments);
                let wildcard = target.text.contains(".*");
                let alias = child_by_field(child, "alias").map(|n| n.text.trim().to_owned());
                imports.push(ImportSummary {
                    segments,
                    wildcard,
                    alias,
                });
            }
            "variable_declaration" => {
                let kind = if child.text.contains("var") {
                    "var"
                } else {
                    "val"
                };
                let name = child_by_field(child, "name")
                    .expect("var name")
                    .text
                    .trim()
                    .to_owned();
                items.push((kind, name));
            }
            "constant_declaration" => {
                let name = child_by_field(child, "name")
                    .expect("const name")
                    .text
                    .trim()
                    .to_owned();
                items.push(("const_val", name));
            }
            "function_declaration" => {
                let name = child_by_field(child, "name")
                    .expect("fun name")
                    .text
                    .trim()
                    .to_owned();
                items.push(("fun", name));
            }
            "classifier_declaration" => {
                let inner = child
                    .children
                    .iter()
                    .find(|n| n.tag.ends_with("_declaration"))
                    .expect("classifier inner");
                let kind = match inner.tag.as_str() {
                    "value_class_declaration" => "value_class",
                    "class_declaration" => "class",
                    "interface_declaration" => "interface",
                    "enum_class_declaration" => "enum_class",
                    "object_declaration" => "object",
                    other => panic!("unexpected classifier tag: {other}"),
                };
                let name = child_by_field(inner, "name")
                    .expect("classifier name")
                    .text
                    .trim()
                    .to_owned();
                items.push((kind, name));
            }
            _ => {}
        }
    }
    let mut counts = StructuralCounts::default();
    walk_xml_counts(&root, &mut counts);
    Ok(ParseOutcome::Accepted(FileSummary {
        package,
        imports,
        items,
        counts,
    }))
}

fn walk_xml_counts(node: &XmlNode, counts: &mut StructuralCounts) {
    match node.tag.as_str() {
        "nested_block_statement" => counts.nested_blocks += 1,
        "control_block" => counts.control_blocks += 1,
        "lambda_expression" => counts.lambdas += 1,
        "call_expression" => {
            counts.calls += 1;
            for child in &node.children {
                if child.tag == "lambda_expression" && child.field.as_deref() != Some("function") {
                    counts.call_lambdas += 1;
                } else if child.tag == "argument_list" {
                    for arg in &child.children {
                        if arg.tag == "call_argument"
                            && arg.children.iter().any(|c| {
                                c.tag == "lambda_expression"
                                    || (c.tag == "expression"
                                        && c.children.iter().any(|g| g.tag == "lambda_expression"))
                            })
                        {
                            counts.call_lambdas += 1;
                        }
                    }
                }
            }
        }
        "when_entry" => counts.when_entries += 1,
        "while_statement" => counts.while_loops += 1,
        "for_statement" => counts.for_loops += 1,
        "loop_statement" => counts.loops += 1,
        "prefix_expression" => counts.prefixes += 1,
        "binary_expression" => counts.binaries += 1,
        _ => {}
    }
    for child in &node.children {
        walk_xml_counts(child, counts);
    }
}

fn tree_sitter_cli() -> PathBuf {
    if let Some(path) = env::var_os("KOVEN_TREE_SITTER").or_else(|| env::var_os("TREE_SITTER")) {
        return PathBuf::from(path);
    }
    let grammar = grammar_dir();
    let local = grammar.join("node_modules/.bin/tree-sitter");
    if local.exists() {
        return local;
    }
    panic!("set KOVEN_TREE_SITTER to the locked tree-sitter CLI (see SPEC-0481)");
}

fn grammar_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../editors/tree-sitter")
        .canonicalize()
        .expect("resolve editors/tree-sitter directory")
}

static TREE_SITTER_LOCK: Mutex<()> = Mutex::new(());

fn project_tree_sitter(cli: &Path, grammar: &Path, source: &str) -> ParseOutcome {
    let _guard = TREE_SITTER_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = env::temp_dir().join(format!("koven-f4b-{}-{nonce}", std::process::id()));
    let cache = env::temp_dir().join("koven-f4b-ts-cache");
    fs::create_dir_all(&dir).expect("create temp dir");
    fs::create_dir_all(cache.join("tree-sitter/lock")).expect("create tree-sitter lock dir");
    fs::create_dir_all(cache.join("tree-sitter/lib")).expect("create tree-sitter lib dir");
    let file = dir.join("probe.ko");
    fs::write(&file, source).expect("write probe file");
    let output = Command::new(cli)
        .args(["parse", "--xml"])
        .arg(&file)
        .env("XDG_CACHE_HOME", &cache)
        .current_dir(grammar)
        .output()
        .expect("execute locked tree-sitter CLI");
    let _ = fs::remove_dir_all(&dir);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let outcome = summarize_tree_sitter_xml(&stdout).unwrap_or_else(|err| {
        panic!(
            "failed to parse tree-sitter XML: {err}\nstdout: {stdout}\nstderr: {}",
            String::from_utf8_lossy(&output.stderr)
        )
    });
    if output.status.success() {
        assert!(
            matches!(outcome, ParseOutcome::Accepted(_)),
            "tree-sitter exited 0 but XML contained ERROR/MISSING"
        );
    } else {
        assert_eq!(
            outcome,
            ParseOutcome::Rejected,
            "tree-sitter exited non-zero without ERROR/MISSING in XML"
        );
    }
    outcome
}

fn compare_outcomes(left: &ParseOutcome, right: &ParseOutcome) -> Result<(), String> {
    if left == right {
        Ok(())
    } else {
        Err(format!(
            "differential mismatch:\n  rust:        {left:?}\n  tree-sitter: {right:?}"
        ))
    }
}

#[test]
fn fault_controls_detect_structural_and_acceptance_drift() {
    let sample = "\
package demo.app
import std.io.println as emit
const val LIMIT = 10
fun run(flag: Boolean): Unit {
    if (flag) {
        block { emit(LIMIT) }
    }
    val b = f()
    { x() }
}
";
    let ParseOutcome::Accepted(baseline) = project_rust(sample) else {
        panic!("sample must be accepted by parse_file");
    };
    assert_eq!(baseline.package, ["demo", "app"]);
    assert_eq!(baseline.imports.len(), 1);
    assert_eq!(baseline.imports[0].alias.as_deref(), Some("emit"));
    assert_eq!(
        baseline.items,
        [("const_val", "LIMIT".to_owned()), ("fun", "run".to_owned())]
    );
    assert_eq!(baseline.counts.nested_blocks, 1);
    assert_eq!(baseline.counts.control_blocks, 1);
    assert_eq!(baseline.counts.lambdas, 1);
    assert_eq!(baseline.counts.calls, 3);
    assert_eq!(baseline.counts.call_lambdas, 1);

    let base_out = ParseOutcome::Accepted(baseline.clone());
    let assert_drift = |mutate: fn(&mut FileSummary)| {
        let mut m = baseline.clone();
        mutate(&mut m);
        assert!(compare_outcomes(&base_out, &ParseOutcome::Accepted(m)).is_err());
    };
    // Acceptance mismatch and structural mutations (including detached trailing lambda) must fail.
    assert!(compare_outcomes(&base_out, &ParseOutcome::Rejected).is_err());
    assert_drift(|m| m.counts.call_lambdas = 0);
    assert_drift(|m| m.package.push("extra".to_owned()));
    assert_drift(|m| {
        m.counts.control_blocks = 0;
        m.counts.lambdas += 1;
    });
    assert_drift(|m| m.counts.nested_blocks = 0);
    assert_drift(|m| m.counts.when_entries = 1);

    // Bare block must be rejected by Rust parse_file.
    assert_eq!(
        project_rust("fun a() { { x() } }\n"),
        ParseOutcome::Rejected
    );
    assert_eq!(
        project_rust("val f = { { x() } }\n"),
        ParseOutcome::Rejected
    );
}

#[test]
#[ignore = "requires locked tree-sitter CLI; see SPEC-0481"]
fn common_subset_and_spec_0479_differential() {
    let cli = tree_sitter_cli();
    let grammar = grammar_dir();

    let accepted_cases = [
        (
            "headers-and-top-level-decls",
            "\
package acme.core
import std.io.println
import std.math.*
import std.text.format as fmt
const val VERSION = 1
val immutable = 42
var mutable = 0
fun add(own a: Int, borrow b: Int, inout c: Int): Int = a + b
fun empty() {}
",
        ),
        (
            "class-family-and-delegation",
            "\
interface Shape {
    fun area(): Int
}
value class Point(val x: Int, val y: Int)
class Circle(val r: Int) : Shape {
    override fun area(): Int = r * r
    companion {
        const val PI = 3
    }
}
class Adapter(val inner: Shape) : Shape by inner
enum class Color {
    Red,
    Rgb(r: Int, g: Int, b: Int)
}
object Registry {
    const val MAX = 8
    fun count(): Int = MAX
}
",
        ),
        (
            "expressions-and-operators",
            "\
fun exprs(a: Int, b: Int, flag: Boolean, opt: Int?) {
    val pair = a to b
    val fallback = opt ?: 0
    val logic = flag && !flag || flag
    val cast = a as Int
    val safe = opt as? Int
    val arith = -a + b * 2 - 1 / 1 % 2
    val call = compute(name = a, &mutable, list[0].field?.next!!, ::helper)
    val text = \"val=${a}\"
}
",
        ),
        (
            "control-flow-and-soft-keywords",
            "\
fun control(ok: Boolean, ready: Boolean, items: List<Int>) {
    if (ok) {
        while (ready) {
            break
        }
    } else {
        for (item in items) {
            continue
        }
    }
    loop {
        return
    }
}
",
        ),
        (
            "explicit-block-and-ordinary-block-name",
            "\
fun blocks() {
    block {
        val inner = 1
        block {
            work(inner)
        }
    }
    val block = 1
    block(block)
    obj.block()
    obj.block { render() }
}
",
        ),
        (
            "same-line-and-crossline-trailing-lambdas",
            "\
fun lambdas() {
    html { render() }
    build<Int> { render() }
    val b = f()
    { x() }
    val c = f() // reason
    { work() }
    val d = f() /* reason */
    { work() }
    f(1)
    { item -> use(item) }
}
",
        ),
        (
            "when-compound-conditions-and-attached-lambda",
            "\
fun branching(flag: Boolean, other: Boolean) {
    when {
        flag -> f()
        { -> true }() -> g()
    }
    when {
        flag -> f()
        { -> true }() && flag -> g()
    }
    when {
        flag -> f()
        { -> true }(), other -> g()
    }
    when {
        flag -> f()
        { work() }
        other -> g()
    }
}
",
        ),
        (
            "grouped-and-move-lambdas-at-statement-start",
            "\
fun closures() {
    ({ x() })
    move { x() }
    val fnRef: move (own Int) -> Int = move { n -> n }
}
",
        ),
        (
            "spec-0488-closed-drifts-ts03-ts05-ts07",
            "\
fun closedDrifts(a: Int, b: Int, c: Int, move: Int) {
    val bits = a shl 1 shr 2 ushr 3 and b xor c or 4
    val cont = a shl
      1
    val prec = a or b and c
    a(); b()
    val f = move
    { 1 }
    val f2 = move
    // comment
    { 1 }
    val ft: move
    (own Int) -> Int = move { n -> n }
    val div = move / { 2 }()
    val g = { a(); b() }
}
",
        ),
    ];

    for (name, source) in accepted_cases {
        let rust = project_rust(source);
        assert!(
            matches!(rust, ParseOutcome::Accepted(_)),
            "{name}: Rust parse_file unexpectedly rejected common-subset case"
        );
        let ts = project_tree_sitter(&cli, &grammar, source);
        compare_outcomes(&rust, &ts).unwrap_or_else(|err| panic!("{name}: {err}"));
    }

    let rejected_cases = [
        ("bare-block-in-fn", "fun a() { { x() } }\n"),
        ("bare-block-in-lambda", "val f = { { x() } }\n"),
        ("reserved-word-binding", "val async = 1\n"),
        ("own-at-call-site", "fun f() { consume(own x) }\n"),
        ("wildcard-destructuring", "fun f() { val (a, _) = pair }\n"),
        ("TS-01-bare-cr", "val x =\r 1\n"),
        ("TS-01-form-feed", "val x =\x0c 1\n"),
        ("TS-04-chained-range", "val x = a..b..<c\n"),
        ("TS-04-chained-membership", "val x = a in b in c\n"),
        ("TS-04-chained-comparison", "val x = a < b <= c\n"),
        ("TS-04-chained-equality", "val x = a == b != c\n"),
        ("TS-05-same-line-unseparated-block", "fun f() { 1 2 }\n"),
        (
            "TS-05-same-line-unseparated-control",
            "fun f(ok: Boolean) { if (ok) { a() b() } }\n",
        ),
        (
            "TS-05-same-line-unseparated-lambda",
            "val g = { x() y() }\n",
        ),
    ];

    for (name, source) in rejected_cases {
        let rust = project_rust(source);
        let ts = project_tree_sitter(&cli, &grammar, source);
        assert_eq!(rust, ParseOutcome::Rejected, "{name}: Rust must reject");
        assert_eq!(
            ts,
            ParseOutcome::Rejected,
            "{name}: Tree-sitter must reject"
        );
    }
}

#[test]
#[ignore = "requires locked tree-sitter CLI; see SPEC-0481"]
fn spec_0491_closed_drifts_ts01_ts02_ts04_match_rust() {
    let cli = tree_sitter_cli();
    let grammar = grammar_dir();

    // All 7 documented F4b drifts (TS-01..TS-07) are now closed across SPEC-0479, 0488, 0491, and 0497.
    let accepted_checks = [
        ("TS-01-valid-crlf", "val x = 1\r\nval y = 2\r\n"),
        (
            "TS-02-multiline-block-comment",
            "fun f() {\n    a/*\n*/-b\n}\n",
        ),
        (
            "TS-02-inline-block-comment",
            "fun f() {\n    a/* inline */-b\n}\n",
        ),
        (
            "TS-02-operator-continuation-and-trailing-lambda",
            "fun f(a: Int, b: Int) {\n    val x = a +/*\n*/b\n    val y = g()/*\n*/{ 1 }\n}\n",
        ),
        (
            "TS-04-parenthesized-and-cross-group",
            "fun f(a: Int, b: Int, c: Int, items: List<Int>) {\n    val r1 = (a < b) < c\n    val r2 = a in 1..10\n    val r3 = a < b == true\n    val r4 = a ?: b to c\n}\n",
        ),
    ];
    for (id, source) in accepted_checks {
        let rust = project_rust(source);
        assert!(
            matches!(rust, ParseOutcome::Accepted(_)),
            "{id}: Rust must accept"
        );
        let ts = project_tree_sitter(&cli, &grammar, source);
        compare_outcomes(&rust, &ts).unwrap_or_else(|err| panic!("{id}: {err}"));
    }

    let ParseOutcome::Accepted(ts02_multi) =
        project_tree_sitter(&cli, &grammar, "fun f() {\n    a/*\n*/-b\n}\n")
    else {
        panic!("TS-02 multiline must be accepted");
    };
    assert_eq!(
        (ts02_multi.counts.prefixes, ts02_multi.counts.binaries),
        (1, 0)
    );
    let ParseOutcome::Accepted(ts02_inline) =
        project_tree_sitter(&cli, &grammar, "fun f() {\n    a/* inline */-b\n}\n")
    else {
        panic!("TS-02 inline must be accepted");
    };
    assert_eq!(
        (ts02_inline.counts.prefixes, ts02_inline.counts.binaries),
        (0, 1)
    );
}
