//! SPEC-0484 (F3e -> F4a): Independent reference parser differential tests for
//! Guide v0.47 Chapters 02 / 05 / 08 (`parse_file` and `parse_declaration`).
//!
//! Covers all five F3e rule families (`D-FILE`, `D-VAL`, `D-FUN`, `D-CLASS`, `D-MEMBER`)
//! with recursive `Span` containment verification across every item, classifier, member,
//! parameter, and `TypeRef` node.

#[path = "f3e_file_diff/reference_parser.rs"]
mod reference_parser;

use lang_frontend::{
    ast::{ExpressionId, ItemId, StatementId, TypeRefId},
    lexer::lex,
    parser::{
        BinaryOperator, ClassifierKind, Expression, FunctionBody, FunctionForm, Item, LiteralKind,
        NameMarker, ParameterModeMarker, Statement, SyntaxAst, TypeRef, VariableKind,
        VisibilityModifier, parse_declaration, parse_file,
    },
    source::{SourceMap, Span},
};
use reference_parser::{Outcome, run_declaration_reference, run_file_reference};

fn within(parent: Span, child: Span) {
    assert_eq!(parent.source_id(), child.source_id());
    assert!(
        parent.start() <= child.start()
            && child.start() <= child.end()
            && child.end() <= parent.end(),
        "child span {child:?} escapes parent {parent:?}"
    );
}

fn name_of(sources: &SourceMap, parent: Span, marker: NameMarker) -> String {
    let NameMarker::Present(span) = marker else {
        panic!("unexpected non-present name marker in clean parse: {marker:?}");
    };
    within(parent, span);
    sources.slice(span).unwrap().to_owned()
}

fn vis_str(vis: VisibilityModifier, parent: Span) -> &'static str {
    match vis {
        VisibilityModifier::Public(s) => {
            within(parent, s);
            "public"
        }
        VisibilityModifier::Internal(s) => {
            within(parent, s);
            "internal"
        }
        VisibilityModifier::Private(s) => {
            within(parent, s);
            "private"
        }
    }
}

fn mode_str(mode: ParameterModeMarker, parent: Span) -> &'static str {
    match mode {
        ParameterModeMarker::Own(s) => {
            within(parent, s);
            "own"
        }
        ParameterModeMarker::Borrow(s) => {
            within(parent, s);
            "borrow"
        }
        ParameterModeMarker::Inout(s) => {
            within(parent, s);
            "inout"
        }
    }
}

fn prod_type(ast: &SyntaxAst, sources: &SourceMap, id: TypeRefId, parent: Span) -> String {
    let node = ast.type_refs().get(id).unwrap();
    let span = node.span();
    within(parent, span);
    match node.payload() {
        TypeRef::Qualified {
            segments,
            nullable_span,
        } => {
            assert_eq!(segments.len(), 1);
            let seg = &segments[0];
            within(span, seg.name_span);
            let mut out = sources.slice(seg.name_span).unwrap().to_owned();
            if !seg.arguments.is_empty() {
                let inner: Vec<_> = seg
                    .arguments
                    .iter()
                    .map(|&arg| prod_type(ast, sources, arg, span))
                    .collect();
                out = format!("{out}<{}>", inner.join(","));
            }
            if let Some(q) = nullable_span {
                within(span, *q);
                out.push('?');
            }
            out
        }
        other => panic!("unexpected TypeRef in F3e slice: {other:?}"),
    }
}

fn prod_expr(ast: &SyntaxAst, sources: &SourceMap, id: ExpressionId, parent: Span) -> String {
    let node = ast.expressions().get(id).unwrap();
    let span = node.span();
    within(parent, span);
    match node.payload() {
        Expression::Name => format!("(id {})", sources.slice(span).unwrap()),
        Expression::Literal(LiteralKind::Integer(_)) => {
            format!("(int {})", sources.slice(span).unwrap())
        }
        Expression::Binary {
            left,
            operator: BinaryOperator::Add,
            right,
            ..
        } => format!(
            "(add {} {})",
            prod_expr(ast, sources, *left, span),
            prod_expr(ast, sources, *right, span)
        ),
        other => panic!("unexpected Expression in F3e slice: {other:?}"),
    }
}

fn prod_block(ast: &SyntaxAst, sources: &SourceMap, id: StatementId, parent: Span) -> String {
    let node = ast.statements().get(id).unwrap();
    within(parent, node.span());
    let Statement::Block { elements } = node.payload() else {
        panic!("expected Statement::Block, got {:?}", node.payload());
    };
    if elements.is_empty() {
        return "(block)".to_owned();
    }
    let parts: Vec<_> = elements
        .iter()
        .map(|&s| {
            let child = ast.statements().get(s).unwrap();
            within(node.span(), child.span());
            let Statement::Expression { expression } = child.payload() else {
                panic!("unexpected block element: {:?}", child.payload());
            };
            prod_expr(ast, sources, *expression, child.span())
        })
        .collect();
    format!("(block {})", parts.join(" "))
}

fn prod_item(ast: &SyntaxAst, sources: &SourceMap, id: ItemId, parent: Span) -> String {
    let node = ast.items().get(id).unwrap();
    let span = node.span();
    within(parent, span);
    match node.payload() {
        Item::Modified {
            modifiers,
            declaration,
        } => {
            let mut mods = Vec::new();
            if let Some(v) = modifiers.visibility {
                mods.push(vis_str(v, span));
            }
            if let Some(ov) = modifiers.override_span {
                within(span, ov);
                mods.push("override");
            }
            if let Some(rm) = modifiers.receiver_mode {
                mods.push(mode_str(rm, span));
            }
            let inner = prod_item(ast, sources, *declaration, span);
            format!("(mod [{}] {inner})", mods.join(" "))
        }
        Item::Constant {
            name,
            type_ref,
            initializer,
            ..
        } => {
            let n = name_of(sources, span, *name);
            let init = prod_expr(ast, sources, *initializer, span);
            match type_ref {
                Some(t) => format!(
                    "(const-val {n} : {} = {init})",
                    prod_type(ast, sources, *t, span)
                ),
                None => format!("(const-val {n} = {init})"),
            }
        }
        Item::Variable {
            kind,
            name,
            type_ref,
            initializer,
            ..
        } => {
            let k = match kind {
                VariableKind::Val => "val",
                VariableKind::Var => "var",
                VariableKind::BorrowVal(_) => panic!("unexpected borrow val"),
            };
            let n = name_of(sources, span, *name);
            let init = prod_expr(ast, sources, *initializer, span);
            match type_ref {
                Some(t) => format!("({k} {n} : {} = {init})", prod_type(ast, sources, *t, span)),
                None => format!("({k} {n} = {init})"),
            }
        }
        Item::Function {
            name,
            type_parameters,
            parameters,
            form,
            ..
        } => {
            let n = name_of(sources, span, *name);
            let mut head = format!("fun {n}");
            if !type_parameters.is_empty() {
                let tps: Vec<_> = type_parameters
                    .iter()
                    .map(|tp| {
                        within(span, tp.span);
                        let tpn = name_of(sources, tp.span, tp.name);
                        match tp.bound {
                            Some(b) => format!("{tpn}:{}", prod_type(ast, sources, b, tp.span)),
                            None => tpn,
                        }
                    })
                    .collect();
                head.push_str(&format!("<{}>", tps.join(",")));
            }
            let ps: Vec<_> = parameters
                .iter()
                .map(|p| {
                    within(span, p.span);
                    let pn = name_of(sources, p.span, p.name);
                    let pt = prod_type(ast, sources, p.type_ref, p.span);
                    match p.mode_marker {
                        Some(m) => format!("{} {pn}:{pt}", mode_str(m, p.span)),
                        None => format!("{pn}:{pt}"),
                    }
                })
                .collect();
            head.push_str(&format!("({})", ps.join(", ")));
            let body = match form {
                FunctionForm::ImplicitUnitAbsent => "absent".to_owned(),
                FunctionForm::ImplicitUnitBlock(b) => prod_block(ast, sources, *b, span),
                FunctionForm::Explicit { type_ref, body, .. } => {
                    head.push_str(&format!(" : {}", prod_type(ast, sources, *type_ref, span)));
                    match body {
                        FunctionBody::Absent => "absent".to_owned(),
                        FunctionBody::Expression { expression, .. } => {
                            format!("= {}", prod_expr(ast, sources, *expression, span))
                        }
                        FunctionBody::Block(b) => prod_block(ast, sources, *b, span),
                    }
                }
            };
            format!("({head} {body})")
        }
        Item::Deinit { body, .. } => format!("(deinit {})", prod_block(ast, sources, *body, span)),
        Item::Companion(companion) => {
            let members: Vec<_> = companion
                .body
                .members
                .iter()
                .map(|&m| prod_item(ast, sources, m, span))
                .collect();
            let body = if members.is_empty() {
                "(body)".to_owned()
            } else {
                format!("(body {})", members.join(" "))
            };
            format!("(companion {body})")
        }
        Item::Classifier(classifier) => {
            let tag = match classifier.kind {
                ClassifierKind::ValueClass { .. } => "value-class",
                ClassifierKind::Class { .. } => "class",
                ClassifierKind::Interface { .. } => "interface",
                ClassifierKind::EnumClass { .. } => "enum-class",
                ClassifierKind::Object { .. } => "object",
            };
            let n = name_of(sources, span, classifier.name);
            let mut parts = vec![format!("{tag} {n}")];
            if let Some(ctor) = &classifier.primary_constructor {
                let fs: Vec<_> = ctor
                    .fields
                    .iter()
                    .map(|f| {
                        within(span, f.span);
                        let fk = match f.kind {
                            VariableKind::Val => "val",
                            VariableKind::Var => "var",
                            VariableKind::BorrowVal(_) => panic!("unexpected borrow val"),
                        };
                        let fnm = name_of(sources, f.span, f.name);
                        let fty = prod_type(ast, sources, f.type_ref, f.span);
                        match f.visibility {
                            Some(v) => format!("{} {fk} {fnm}:{fty}", vis_str(v, f.span)),
                            None => format!("{fk} {fnm}:{fty}"),
                        }
                    })
                    .collect();
                parts.push(format!("(ctor {})", fs.join(", ")));
            }
            if !classifier.supertypes.is_empty() {
                let ss: Vec<_> = classifier
                    .supertypes
                    .iter()
                    .map(|s| {
                        within(span, s.span);
                        let ty = prod_type(ast, sources, s.type_ref, s.span);
                        match s.delegation {
                            Some(d) => {
                                within(s.span, d.span);
                                format!("{ty} by {}", name_of(sources, d.span, d.target))
                            }
                            None => ty,
                        }
                    })
                    .collect();
                parts.push(format!("(supers {})", ss.join(", ")));
            }
            if let Some(body) = &classifier.body {
                if matches!(classifier.kind, ClassifierKind::EnumClass { .. }) {
                    let vs: Vec<_> = body
                        .variants
                        .iter()
                        .map(|v| {
                            within(span, v.span);
                            let vn = name_of(sources, v.span, v.name);
                            if v.parameters.is_empty() {
                                vn
                            } else {
                                let vps: Vec<_> = v
                                    .parameters
                                    .iter()
                                    .map(|p| {
                                        within(v.span, p.span);
                                        format!(
                                            "{}:{}",
                                            name_of(sources, p.span, p.name),
                                            prod_type(ast, sources, p.type_ref, p.span)
                                        )
                                    })
                                    .collect();
                                format!("{vn}({})", vps.join(", "))
                            }
                        })
                        .collect();
                    let ms: Vec<_> = body
                        .members
                        .iter()
                        .map(|&m| prod_item(ast, sources, m, span))
                        .collect();
                    let mut eb = format!("(enum-body (variants {})", vs.join(", "));
                    if !ms.is_empty() {
                        eb.push_str(&format!(" (members {})", ms.join(" ")));
                    }
                    eb.push(')');
                    parts.push(eb);
                } else {
                    let ms: Vec<_> = body
                        .members
                        .iter()
                        .map(|&m| prod_item(ast, sources, m, span))
                        .collect();
                    if ms.is_empty() {
                        parts.push("(body)".to_owned());
                    } else {
                        parts.push(format!("(body {})", ms.join(" ")));
                    }
                }
            }
            format!("({})", parts.join(" "))
        }
        Item::Error => panic!("unexpected Item::Error in clean parse"),
    }
}

fn run_file_production(source: &str) -> (Outcome, Vec<String>) {
    let mut sources = SourceMap::new();
    let id = sources.add_source("prod.ko", source).unwrap();
    let lexed = lex(&sources, id).expect("lex must not fail internally");
    if !lexed.diagnostics().is_empty() {
        return (
            Outcome::Rejected,
            lexed
                .diagnostics()
                .iter()
                .map(|d| d.code().to_string())
                .collect(),
        );
    }
    let parsed = parse_file(&sources, &lexed).expect("parse_file must not fail internally");
    if !parsed.diagnostics().is_empty() {
        return (
            Outcome::Rejected,
            parsed
                .diagnostics()
                .iter()
                .map(|d| d.code().to_string())
                .collect(),
        );
    }
    let check_in_file = |span: Span| {
        assert_eq!(span.source_id(), id);
        assert!(span.start() <= span.end() && span.end() <= source.len());
    };
    let mut parts = Vec::new();
    if let Some(pkg) = parsed.package() {
        let segs: Vec<_> = pkg
            .segments
            .iter()
            .map(|s| {
                check_in_file(s.span);
                sources.slice(s.span).unwrap()
            })
            .collect();
        parts.push(format!("(package {})", segs.join(".")));
    }
    for imp in parsed.imports() {
        let segs: Vec<_> = imp
            .segments
            .iter()
            .map(|s| {
                check_in_file(s.span);
                sources.slice(s.span).unwrap()
            })
            .collect();
        let mut s = format!("(import {}", segs.join("."));
        if let Some(w) = imp.wildcard_span {
            check_in_file(w);
            s.push_str(".*");
        }
        if let Some(a) = imp.alias {
            check_in_file(a.name_span);
            s.push_str(&format!(" as {}", sources.slice(a.name_span).unwrap()));
        }
        s.push(')');
        parts.push(s);
    }
    for &root in parsed.roots() {
        let root_span = parsed.ast().items().get(root).unwrap().span();
        check_in_file(root_span);
        parts.push(prod_item(parsed.ast(), &sources, root, root_span));
    }
    let out = if parts.is_empty() {
        "(file)".to_owned()
    } else {
        format!("(file {})", parts.join(" "))
    };
    (Outcome::Accepted(out), Vec::new())
}

fn run_declaration_production(source: &str) -> (Outcome, Vec<String>) {
    let mut sources = SourceMap::new();
    let id = sources.add_source("prod-decl.ko", source).unwrap();
    let lexed = lex(&sources, id).expect("lex must not fail internally");
    if !lexed.diagnostics().is_empty() {
        return (
            Outcome::Rejected,
            lexed
                .diagnostics()
                .iter()
                .map(|d| d.code().to_string())
                .collect(),
        );
    }
    let parsed =
        parse_declaration(&sources, &lexed).expect("parse_declaration must not fail internally");
    if !parsed.diagnostics().is_empty() {
        return (
            Outcome::Rejected,
            parsed
                .diagnostics()
                .iter()
                .map(|d| d.code().to_string())
                .collect(),
        );
    }
    let root_span = parsed.ast().items().get(parsed.root()).unwrap().span();
    assert_eq!(root_span.source_id(), id);
    assert!(root_span.start() <= root_span.end() && root_span.end() <= source.len());
    let out = prod_item(parsed.ast(), &sources, parsed.root(), root_span);
    (Outcome::Accepted(out), Vec::new())
}

fn compare(prod: &Outcome, reference: &Outcome) -> Result<(), String> {
    if prod == reference {
        Ok(())
    } else {
        Err(format!(
            "F3e differential mismatch:\n  production: {prod:?}\n  reference:  {reference:?}"
        ))
    }
}

fn assert_file_accepted(label: &str, source: &str) {
    let (prod, codes) = run_file_production(source);
    assert!(
        matches!(prod, Outcome::Accepted(_)),
        "{label}: production unexpectedly rejected with {codes:?}"
    );
    let reference = run_file_reference(source);
    compare(&prod, &reference).unwrap_or_else(|err| panic!("{label}:\n{source}\n{err}"));
}

fn assert_file_rejected(label: &str, source: &str) {
    let (prod, _) = run_file_production(source);
    assert_eq!(prod, Outcome::Rejected, "{label}: production must reject");
    let reference = run_file_reference(source);
    assert_eq!(
        reference,
        Outcome::Rejected,
        "{label}: reference must reject"
    );
}

#[test]
fn fault_controls_detect_file_header_modifier_and_classifier_drift() {
    let sample = "\
package acme.core
import std.io.println as emit
public class Box(val inner: Shape) : Shape by inner {
    public override inout fun draw(): Unit {}
    deinit() {}
}
";
    let (prod, _) = run_file_production(sample);
    let reference = run_file_reference(sample);
    assert_eq!(compare(&prod, &reference), Ok(()));

    let Outcome::Accepted(baseline) = &reference else {
        panic!("baseline must accept");
    };
    assert!(compare(&prod, &Outcome::Rejected).is_err());
    assert!(
        compare(
            &prod,
            &Outcome::Accepted(baseline.replace("acme.core", "acme.other"))
        )
        .is_err()
    );
    assert!(
        compare(
            &prod,
            &Outcome::Accepted(baseline.replace("public override inout", "public inout"))
        )
        .is_err()
    );
    assert!(
        compare(
            &prod,
            &Outcome::Accepted(baseline.replace("Shape by inner", "Shape"))
        )
        .is_err()
    );
    assert!(
        compare(
            &prod,
            &Outcome::Accepted(baseline.replace("(deinit (block))", ""))
        )
        .is_err()
    );
}

#[test]
fn f3e_twelve_positive_witnesses_match_reference() {
    let cases = [
        ("empty-file", ""),
        ("newlines-only", "\n\n"),
        (
            "headers-and-trailing-semi",
            "package a.b\nimport std.io.println as emit\nimport std.math.*;\n",
        ),
        (
            "val-var-const-val",
            "public val a: Int = 1\ninternal var b = 2\nprivate const val C: Int = 3;\n",
        ),
        (
            "fun-three-bodies",
            "fun a() {}\nfun b(own x: Int, borrow y: Int, inout z: Int): Int = x + y\ninterface I { public fun c(): Unit }\n",
        ),
        ("value-class", "value class Point(val x: Int, val y: Int)\n"),
        (
            "class-with-delegation-deinit-companion",
            "\
class Adapter(val inner: Shape) : Shape by inner {
    public override inout fun tick(): Unit {}
    deinit() {}
    companion {
        const val VERSION = 1
        fun make(): Int = 1
    }
}
",
        ),
        (
            "enum-class-variants-and-member",
            "enum class Color {\n    Red,\n    Rgb(r: Int, g: Int, b: Int);\n    fun code(): Int = 1\n}\n",
        ),
        (
            "object-const-and-fun",
            "object Registry {\n    const val MAX = 8\n    fun count(): Int = MAX\n}\n",
        ),
        (
            "generic-fun-with-bound",
            "fun <T: Copyable> id(x: T?): T? = x\n",
        ),
        (
            "semicolon-separated-top-level",
            "val a = 1; var b = 2; const val C = 3;",
        ),
        (
            "interface-default-method",
            "interface Greeter {\n    fun hi()\n    public fun hello(): Int = 1\n}\n",
        ),
        ("private-companion", "class C { private companion {} }\n"),
        ("object-borrow-method", "object O { borrow fun f() {} }\n"),
        ("class-bodyless-method", "class C { fun f() }\n"),
        ("enum-with-supertype", "enum class E : I { A }\n"),
        ("supertype-followed-by-decl", "class C : I\nval x = 1\n"),
    ];

    for (label, src) in cases {
        assert_file_accepted(label, src);
    }
}

#[test]
fn f3e_fourteen_negative_witnesses_match_reference() {
    let rejected = [
        ("wildcard-import-alias", "import a.b.* as c\n"),
        ("package-after-decl", "val x = 1\npackage a.b\n"),
        ("import-after-decl", "val x = 1\nimport a.b\n"),
        ("same-line-unseparated-decls", "val a = 1 val b = 2\n"),
        ("expr-fun-without-return-type", "fun f() = 1\n"),
        ("empty-value-class-ctor", "value class Empty()\n"),
        (
            "override-before-visibility",
            "class C { override public fun f() {} }\n",
        ),
        (
            "receiver-mode-before-override",
            "class C { own override fun f() {} }\n",
        ),
        ("visible-deinit", "class C { public deinit() {} }\n"),
        (
            "private-interface-method",
            "interface I { private fun f() }\n",
        ),
        ("object-val-member", "object O { val x = 1 }\n"),
        ("enum-semicolon-without-member", "enum class E { A; }\n"),
        ("enum-trailing-comma", "enum class E { A, }\n"),
        ("leading-semicolon", ";\n"),
        ("double-semicolon", "val a = 1;; val b = 2\n"),
    ];

    for (label, src) in rejected {
        assert_file_rejected(label, src);
    }
}

#[test]
fn parse_declaration_standalone_entry_matches_reference() {
    let accepted = [
        ("decl-val", "public val count: Int = 1 + 2"),
        ("decl-fun", "fun <T> wrap(own x: T): T = x"),
        (
            "decl-class",
            "class Box(val v: Int) : Base by v { deinit() {} }",
        ),
    ];
    for (label, src) in accepted {
        let (prod, codes) = run_declaration_production(src);
        assert!(
            matches!(prod, Outcome::Accepted(_)),
            "{label}: production rejected with {codes:?}"
        );
        let reference = run_declaration_reference(src);
        compare(&prod, &reference).unwrap_or_else(|err| panic!("{label}:\n{src}\n{err}"));
    }

    for (label, src) in [
        ("decl-trailing-semi", "val a = 1;"),
        ("decl-two-items", "val a = 1\nval b = 2"),
    ] {
        let (prod, _) = run_declaration_production(src);
        assert_eq!(prod, Outcome::Rejected, "{label}: production must reject");
        assert_eq!(
            run_declaration_reference(src),
            Outcome::Rejected,
            "{label}: reference must reject"
        );
    }
}

#[test]
fn combinatorial_f3e_matrix_matches_reference() {
    let visibilities = ["", "public ", "internal ", "private "];
    let overrides = ["", "override "];
    let modes = ["", "own ", "borrow ", "inout "];
    let bodies = ["{}", ": Int = 1"];

    let mut total = 0usize;
    for (vi, vis) in visibilities.iter().enumerate() {
        for (oi, ov) in overrides.iter().enumerate() {
            for (mi, mode) in modes.iter().enumerate() {
                for (bi, body) in bodies.iter().enumerate() {
                    let src = format!("class Host {{\n    {vis}{ov}{mode}fun m() {body}\n}}\n");
                    assert_file_accepted(&format!("f3e-mod-{vi}-{oi}-{mi}-{bi}"), &src);
                    total += 1;
                }
            }
        }
    }
    assert_eq!(total, 64);
}
