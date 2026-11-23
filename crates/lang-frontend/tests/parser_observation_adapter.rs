//! SPEC-0486 (F0b): Unified four-entry parser observation adapter & contract matrix.
//!
//! Implements the four-way outcome classifier (`Accept`, `SyntaxError`, `ResourceLimit`,
//! `InternalFailure`) across `parse_file`, `parse_expression`, `parse_declaration`, and
//! `parse_block`, enforcing zero reachable `Error`/`Missing` nodes on clean `Accept`,
//! closing the foreign-`SourceMap` `parse_file` witness gap and `parse_file` adversarial
//! determinism/R4 coverage documented in `observation-baseline.md`.

use lang_frontend::{
    ast::{ExpressionId, ItemId, StatementId, TypeRefId},
    diagnostic::Diagnostic,
    lexer::lex,
    parser::{
        Expression, ForBinding, FunctionBody, FunctionForm, Item, NameMarker, ParserInternalError,
        Statement, StringPart, SyntaxAst, TypeRef, WhenCondition, parse_block, parse_declaration,
        parse_expression, parse_file,
    },
    source::{SourceId, SourceMap, Span},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Entry {
    File,
    Expression,
    Declaration,
    Block,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ObservedOutcome {
    Accept {
        roots: usize,
        nodes: usize,
    },
    SyntaxError {
        diag_signatures: Vec<(String, usize, usize)>,
        recovery_nodes: usize,
    },
    ResourceLimit {
        limit: usize,
    },
    InternalFailure(String),
}

#[derive(Default)]
struct AstAudit {
    nodes: usize,
    recovery_nodes: usize,
}

impl AstAudit {
    fn check_span(&self, id: SourceId, len: usize, span: Span) -> Result<(), String> {
        if span.source_id() != id {
            return Err(format!("foreign source_id in span: {span:?}"));
        }
        if span.start() > span.end() || span.end() > len {
            return Err(format!("out-of-bounds span {span:?} for len {len}"));
        }
        Ok(())
    }

    fn check_marker(&mut self, id: SourceId, len: usize, marker: NameMarker) -> Result<(), String> {
        match marker {
            NameMarker::Present(s) => self.check_span(id, len, s),
            NameMarker::Missing(s) | NameMarker::Error(s) => {
                self.recovery_nodes += 1;
                self.check_span(id, len, s)
            }
        }
    }

    fn walk_type(
        &mut self,
        ast: &SyntaxAst,
        id: SourceId,
        len: usize,
        tid: TypeRefId,
    ) -> Result<(), String> {
        let node = ast
            .type_refs()
            .get(tid)
            .map_err(|_| format!("dangling TypeRefId {tid:?}"))?;
        self.nodes += 1;
        self.check_span(id, len, node.span())?;
        match node.payload() {
            TypeRef::Error => {
                self.recovery_nodes += 1;
            }
            TypeRef::Qualified {
                segments,
                nullable_span,
            } => {
                for seg in segments {
                    self.check_span(id, len, seg.name_span)?;
                    if !seg.argument_errors.is_empty() {
                        self.recovery_nodes += seg.argument_errors.len();
                    }
                    for &arg in &seg.arguments {
                        self.walk_type(ast, id, len, arg)?;
                    }
                }
                if let Some(q) = nullable_span {
                    self.check_span(id, len, *q)?;
                }
            }
            TypeRef::Function {
                parameters,
                return_type,
                ..
            } => {
                for p in parameters {
                    self.check_span(id, len, p.span)?;
                    if p.trailing_error.is_some() {
                        self.recovery_nodes += 1;
                    }
                    self.walk_type(ast, id, len, p.type_ref)?;
                }
                self.walk_type(ast, id, len, *return_type)?;
            }
        }
        Ok(())
    }

    fn walk_item(
        &mut self,
        ast: &SyntaxAst,
        id: SourceId,
        len: usize,
        iid: ItemId,
    ) -> Result<(), String> {
        let node = ast
            .items()
            .get(iid)
            .map_err(|_| format!("dangling ItemId {iid:?}"))?;
        self.nodes += 1;
        self.check_span(id, len, node.span())?;
        match node.payload() {
            Item::Error => {
                self.recovery_nodes += 1;
            }
            Item::Modified { declaration, .. } => {
                self.walk_item(ast, id, len, *declaration)?;
            }
            Item::Variable {
                name,
                type_ref,
                initializer,
                ..
            } => {
                self.check_marker(id, len, *name)?;
                if let Some(tr) = type_ref {
                    self.walk_type(ast, id, len, *tr)?;
                }
                self.walk_expr(ast, id, len, *initializer)?;
            }
            Item::Constant {
                val_marker,
                name,
                type_ref,
                initializer,
                ..
            } => {
                self.check_marker(id, len, *val_marker)?;
                self.check_marker(id, len, *name)?;
                if let Some(tr) = type_ref {
                    self.walk_type(ast, id, len, *tr)?;
                }
                self.walk_expr(ast, id, len, *initializer)?;
            }
            Item::Function {
                name,
                extension_receiver,
                type_parameters,
                parameters,
                form,
                ..
            } => {
                self.check_marker(id, len, *name)?;
                if let Some(ext) = extension_receiver {
                    self.walk_type(ast, id, len, ext.type_ref)?;
                }
                for tp in type_parameters {
                    self.check_marker(id, len, tp.name)?;
                    if let Some(b) = tp.bound {
                        self.walk_type(ast, id, len, b)?;
                    }
                }
                for p in parameters {
                    self.check_marker(id, len, p.name)?;
                    self.walk_type(ast, id, len, p.type_ref)?;
                }
                match form {
                    FunctionForm::ImplicitUnitAbsent => {}
                    FunctionForm::ImplicitUnitBlock(body) => {
                        self.walk_stmt(ast, id, len, *body)?;
                    }
                    FunctionForm::Explicit { type_ref, body, .. } => {
                        self.walk_type(ast, id, len, *type_ref)?;
                        match body {
                            FunctionBody::Absent => {}
                            FunctionBody::Expression { expression, .. } => {
                                self.walk_expr(ast, id, len, *expression)?;
                            }
                            FunctionBody::Block(b) => self.walk_stmt(ast, id, len, *b)?,
                        }
                    }
                }
            }
            Item::Classifier(c) => {
                self.check_marker(id, len, c.name)?;
                for tp in &c.type_parameters {
                    self.check_marker(id, len, tp.name)?;
                    if let Some(b) = tp.bound {
                        self.walk_type(ast, id, len, b)?;
                    }
                }
                if let Some(ctor) = &c.primary_constructor {
                    if ctor.right_paren_span.is_none() {
                        self.recovery_nodes += 1;
                    }
                    for f in &ctor.fields {
                        self.check_marker(id, len, f.name)?;
                        self.walk_type(ast, id, len, f.type_ref)?;
                    }
                }
                for sup in &c.supertypes {
                    self.walk_type(ast, id, len, sup.type_ref)?;
                    if sup.trailing_lexical_error.is_some() {
                        self.recovery_nodes += 1;
                    }
                    if let Some(del) = sup.delegation {
                        self.check_marker(id, len, del.target)?;
                    }
                }
                if let Some(body) = &c.body {
                    if body.right_brace_span.is_none() {
                        self.recovery_nodes += 1;
                    }
                    for v in &body.variants {
                        self.check_marker(id, len, v.name)?;
                        for vp in &v.parameters {
                            self.check_marker(id, len, vp.name)?;
                            self.walk_type(ast, id, len, vp.type_ref)?;
                        }
                    }
                    for &m in &body.members {
                        self.walk_item(ast, id, len, m)?;
                    }
                }
            }
            Item::Companion(comp) => {
                if comp.body.right_brace_span.is_none() {
                    self.recovery_nodes += 1;
                }
                for &m in &comp.body.members {
                    self.walk_item(ast, id, len, m)?;
                }
            }
            Item::Deinit {
                right_paren_span,
                body,
                ..
            } => {
                if right_paren_span.is_none() {
                    self.recovery_nodes += 1;
                }
                self.walk_stmt(ast, id, len, *body)?;
            }
        }
        Ok(())
    }

    fn walk_stmt(
        &mut self,
        ast: &SyntaxAst,
        id: SourceId,
        len: usize,
        sid: StatementId,
    ) -> Result<(), String> {
        let node = ast
            .statements()
            .get(sid)
            .map_err(|_| format!("dangling StatementId {sid:?}"))?;
        self.nodes += 1;
        self.check_span(id, len, node.span())?;
        match node.payload() {
            Statement::Error => {
                self.recovery_nodes += 1;
            }
            Statement::Block { elements }
            | Statement::LambdaBody { elements }
            | Statement::ControlBody { elements } => {
                for &child in elements {
                    self.walk_stmt(ast, id, len, child)?;
                }
            }
            Statement::LocalVariable { declaration } => {
                self.walk_item(ast, id, len, *declaration)?;
            }
            Statement::LocalDestructuring {
                bindings,
                right_paren_span,
                equals_span,
                initializer,
                ..
            } => {
                if right_paren_span.is_none() || equals_span.is_none() {
                    self.recovery_nodes += 1;
                }
                for &b in bindings {
                    self.check_marker(id, len, b)?;
                }
                self.walk_expr(ast, id, len, *initializer)?;
            }
            Statement::While {
                condition, body, ..
            } => {
                self.walk_expr(ast, id, len, *condition)?;
                self.walk_stmt(ast, id, len, *body)?;
            }
            Statement::For {
                binding,
                source,
                body,
                ..
            } => {
                match binding {
                    ForBinding::Name(m) => self.check_marker(id, len, *m)?,
                    ForBinding::Destructuring {
                        names,
                        right_paren_span,
                        ..
                    } => {
                        if right_paren_span.is_none() {
                            self.recovery_nodes += 1;
                        }
                        for &n in names {
                            self.check_marker(id, len, n)?;
                        }
                    }
                }
                self.walk_expr(ast, id, len, *source)?;
                self.walk_stmt(ast, id, len, *body)?;
            }
            Statement::Loop { body, .. } => {
                self.walk_stmt(ast, id, len, *body)?;
            }
            Statement::Expression { expression } => {
                self.walk_expr(ast, id, len, *expression)?;
            }
        }
        Ok(())
    }

    fn walk_expr(
        &mut self,
        ast: &SyntaxAst,
        id: SourceId,
        len: usize,
        eid: ExpressionId,
    ) -> Result<(), String> {
        let node = ast
            .expressions()
            .get(eid)
            .map_err(|_| format!("dangling ExpressionId {eid:?}"))?;
        self.nodes += 1;
        self.check_span(id, len, node.span())?;
        match node.payload() {
            Expression::Error => {
                self.recovery_nodes += 1;
            }
            Expression::Name
            | Expression::This
            | Expression::Literal(_)
            | Expression::Break { .. }
            | Expression::Continue { .. } => {}
            Expression::SuperMember { interface, .. } => {
                self.walk_type(ast, id, len, *interface)?;
            }
            Expression::Group { expression } => self.walk_expr(ast, id, len, *expression)?,
            Expression::String { parts } => {
                for part in parts {
                    match part {
                        StringPart::Text(s) => self.check_span(id, len, *s)?,
                        StringPart::Error(s) => {
                            self.recovery_nodes += 1;
                            self.check_span(id, len, *s)?;
                        }
                        StringPart::Interpolation { expression, .. } => {
                            self.walk_expr(ast, id, len, *expression)?;
                        }
                    }
                }
            }
            Expression::Lambda { body, .. } => self.walk_stmt(ast, id, len, *body)?,
            Expression::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                self.walk_expr(ast, id, len, *condition)?;
                self.walk_stmt(ast, id, len, *then_branch)?;
                if let Some(eb) = else_branch {
                    self.walk_stmt(ast, id, len, *eb)?;
                }
            }
            Expression::When {
                subject, entries, ..
            } => {
                if let Some(s) = subject {
                    self.walk_expr(ast, id, len, *s)?;
                }
                for entry in entries {
                    for cond in &entry.conditions {
                        match cond {
                            WhenCondition::Expression(e)
                            | WhenCondition::Contains { expression: e, .. } => {
                                self.walk_expr(ast, id, len, *e)?;
                            }
                            WhenCondition::TypeTest { type_ref, .. } => {
                                self.walk_type(ast, id, len, *type_ref)?;
                            }
                        }
                    }
                    self.walk_stmt(ast, id, len, entry.body)?;
                }
            }
            Expression::Return { value, .. } => {
                if let Some(v) = value {
                    self.walk_expr(ast, id, len, *v)?;
                }
            }
            Expression::Prefix { operand, .. }
            | Expression::NonNullAssert { operand, .. }
            | Expression::Propagate { value: operand, .. } => {
                self.walk_expr(ast, id, len, *operand)?;
            }
            Expression::Cast {
                expression,
                type_ref,
                ..
            }
            | Expression::TypeTest {
                expression,
                type_ref,
                ..
            } => {
                self.walk_expr(ast, id, len, *expression)?;
                self.walk_type(ast, id, len, *type_ref)?;
            }
            Expression::Binary { left, right, .. }
            | Expression::Assignment {
                target: left,
                value: right,
                ..
            }
            | Expression::Index {
                receiver: left,
                index: right,
            } => {
                self.walk_expr(ast, id, len, *left)?;
                self.walk_expr(ast, id, len, *right)?;
            }
            Expression::Member { receiver, .. } => self.walk_expr(ast, id, len, *receiver)?,
            Expression::CallableReference { receiver, .. } => {
                if let Some(r) = receiver {
                    self.walk_expr(ast, id, len, *r)?;
                }
            }
            Expression::Call {
                callee,
                type_arguments,
                arguments,
                ..
            } => {
                self.walk_expr(ast, id, len, *callee)?;
                for &ta in type_arguments {
                    self.walk_type(ast, id, len, ta)?;
                }
                for arg in arguments {
                    self.walk_expr(ast, id, len, arg.value)?;
                }
            }
        }
        Ok(())
    }
}

fn classify_outcome(
    roots: usize,
    audit: AstAudit,
    diagnostics: &[Diagnostic],
) -> Result<ObservedOutcome, String> {
    if diagnostics.is_empty() {
        if audit.recovery_nodes > 0 {
            return Err(format!(
                "contract violation: zero diagnostics but {} reachable recovery/error nodes",
                audit.recovery_nodes
            ));
        }
        Ok(ObservedOutcome::Accept {
            roots,
            nodes: audit.nodes,
        })
    } else {
        let diag_signatures = diagnostics
            .iter()
            .map(|d| {
                (
                    d.code().to_string(),
                    d.primary_span().start(),
                    d.primary_span().end(),
                )
            })
            .collect();
        Ok(ObservedOutcome::SyntaxError {
            diag_signatures,
            recovery_nodes: audit.recovery_nodes,
        })
    }
}

fn classify_internal_error(err: ParserInternalError) -> ObservedOutcome {
    match err {
        ParserInternalError::NestingLimitExceeded { limit } => {
            ObservedOutcome::ResourceLimit { limit }
        }
        other => ObservedOutcome::InternalFailure(other.to_string()),
    }
}

fn observe(entry: Entry, source: &str) -> Result<ObservedOutcome, String> {
    let mut sources = SourceMap::new();
    let id = sources.add_source("obs.ko", source).unwrap();
    let lexed = lex(&sources, id).expect("lex must not fail internally");
    let len = source.len();
    let mut audit = AstAudit::default();

    match entry {
        Entry::File => match parse_file(&sources, &lexed) {
            Err(e) => Ok(classify_internal_error(e)),
            Ok(parsed) => {
                assert_eq!(parsed.ast().source_id(), id);
                for &root in parsed.roots() {
                    audit.walk_item(parsed.ast(), id, len, root)?;
                }
                classify_outcome(parsed.roots().len(), audit, parsed.diagnostics())
            }
        },
        Entry::Expression => match parse_expression(&sources, &lexed) {
            Err(e) => Ok(classify_internal_error(e)),
            Ok(parsed) => {
                assert_eq!(parsed.ast().source_id(), id);
                audit.walk_expr(parsed.ast(), id, len, parsed.root())?;
                classify_outcome(1, audit, parsed.diagnostics())
            }
        },
        Entry::Declaration => match parse_declaration(&sources, &lexed) {
            Err(e) => Ok(classify_internal_error(e)),
            Ok(parsed) => {
                assert_eq!(parsed.ast().source_id(), id);
                audit.walk_item(parsed.ast(), id, len, parsed.root())?;
                classify_outcome(1, audit, parsed.diagnostics())
            }
        },
        Entry::Block => match parse_block(&sources, &lexed) {
            Err(e) => Ok(classify_internal_error(e)),
            Ok(parsed) => {
                assert_eq!(parsed.ast().source_id(), id);
                audit.walk_stmt(parsed.ast(), id, len, parsed.root())?;
                classify_outcome(1, audit, parsed.diagnostics())
            }
        },
    }
}

#[test]
fn fault_controls_detect_undiagnosed_recovery_nodes_and_misclassification() {
    let broken_audit = AstAudit {
        nodes: 3,
        recovery_nodes: 1,
    };
    assert!(classify_outcome(1, broken_audit, &[]).is_err());

    let clean = classify_outcome(
        1,
        AstAudit {
            nodes: 3,
            recovery_nodes: 0,
        },
        &[],
    )
    .unwrap();
    assert_eq!(clean, ObservedOutcome::Accept { roots: 1, nodes: 3 });
    assert_ne!(clean, ObservedOutcome::ResourceLimit { limit: 1024 });
    assert_ne!(
        clean,
        ObservedOutcome::InternalFailure("source mismatch".to_owned())
    );
}

#[test]
fn four_entries_classify_accept_and_syntax_error_deterministically() {
    let cases = [
        (Entry::File, "package demo\nval a = 1\nfun f() {}\n", true),
        (Entry::File, "val a = 1 val b = 2\n", false),
        (Entry::Expression, "a + b * c", true),
        (Entry::Expression, "a b", false),
        (Entry::Declaration, "val x: Int = 1", true),
        (Entry::Declaration, "val x: Int = 1;", false),
        (Entry::Block, "{ block { work() } }", true),
        (Entry::Block, "{ { work() } }", false),
        (Entry::Block, "{ a } b", false),
    ];

    for (entry, src, expect_accept) in cases {
        let first = observe(entry, src).unwrap_or_else(|e| panic!("{entry:?} {src:?}: {e}"));
        let second = observe(entry, src).unwrap_or_else(|e| panic!("{entry:?} {src:?}: {e}"));
        assert_eq!(
            first, second,
            "{entry:?} {src:?}: non-deterministic outcome"
        );
        assert_eq!(
            matches!(first, ObservedOutcome::Accept { .. }),
            expect_accept,
            "{entry:?} {src:?}: got {first:?}"
        );
    }
}

#[test]
fn all_four_entries_reject_foreign_source_map_as_internal_failure() {
    let mut producer_map = SourceMap::new();
    let id = producer_map.add_source("a.ko", "val x = 1").unwrap();
    let lexed = lex(&producer_map, id).unwrap();

    let foreign_map = SourceMap::new();

    let outcomes = [
        (
            Entry::File,
            parse_file(&foreign_map, &lexed)
                .err()
                .map(classify_internal_error),
        ),
        (
            Entry::Expression,
            parse_expression(&foreign_map, &lexed)
                .err()
                .map(classify_internal_error),
        ),
        (
            Entry::Declaration,
            parse_declaration(&foreign_map, &lexed)
                .err()
                .map(classify_internal_error),
        ),
        (
            Entry::Block,
            parse_block(&foreign_map, &lexed)
                .err()
                .map(classify_internal_error),
        ),
    ];

    for (entry, outcome) in outcomes {
        assert!(
            matches!(outcome, Some(ObservedOutcome::InternalFailure(_))),
            "{entry:?}: expected InternalFailure on foreign SourceMap, got {outcome:?}"
        );
    }
}

#[test]
fn all_four_entries_classify_nesting_overflow_as_resource_limit() {
    let deep = format!("{}1{}", "(".repeat(1100), ")".repeat(1100));
    let cases = [
        (Entry::Expression, deep.clone()),
        (Entry::Declaration, format!("val x = {deep}")),
        (Entry::Block, format!("{{ {deep} }}")),
        (Entry::File, format!("val x = {deep}\n")),
    ];

    for (entry, src) in cases {
        let outcome = observe(entry, &src).unwrap();
        assert_eq!(
            outcome,
            ObservedOutcome::ResourceLimit { limit: 1024 },
            "{entry:?}: expected ResourceLimit {{ limit: 1024 }}"
        );
    }
}

#[test]
fn parse_file_adversarial_corpus_satisfies_determinism_integrity_and_r4_r5() {
    let headers = ["", "package p\n", "import a.b\n", ";\n"];
    let bodies = [
        "val x = 1",
        "val x = β",
        "val async = 1",
        "val x = \"unterminated\n",
        "val x = /* unclosed",
        "fun f() { { x() } }",
        "class C(val x: Int",
        "class C : I by β, J {}",
    ];
    let tails = [
        "",
        "\nfun sentinel() {}",
        "\nval next = 2",
        "\n; val tail = 3",
    ];

    let mut total = 0usize;
    for h in headers {
        for b in bodies {
            for t in tails {
                let src = format!("{h}{b}{t}");
                let first = observe(Entry::File, &src)
                    .unwrap_or_else(|e| panic!("contract violation on {src:?}: {e}"));
                let second = observe(Entry::File, &src).unwrap();
                assert_eq!(first, second, "non-deterministic parse_file on {src:?}");

                let mut sources = SourceMap::new();
                let id = sources.add_source("adv.ko", &src).unwrap();
                let lexed = lex(&sources, id).unwrap();
                let parsed = parse_file(&sources, &lexed).unwrap();
                for ldiag in lexed.diagnostics() {
                    let lspan = (ldiag.primary_span().start(), ldiag.primary_span().end());
                    let same_span_count = parsed
                        .diagnostics()
                        .iter()
                        .filter(|d| (d.primary_span().start(), d.primary_span().end()) == lspan)
                        .count();
                    assert_eq!(
                        same_span_count,
                        1,
                        "R4 violation: lexer diagnostic at {lspan:?} duplicated in {src:?}: {:?}",
                        parsed.diagnostics()
                    );
                }
                total += 1;
            }
        }
    }
    assert_eq!(total, 128);
}
