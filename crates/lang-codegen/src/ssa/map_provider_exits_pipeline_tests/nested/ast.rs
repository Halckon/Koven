//! 原字节同根双层 For 的父子身份；不沿用单层/Guard/temporary 查询。
use super::super::fixture::{Failure, clean, contains, failure, need};
use lang_frontend::{
    ast::{ExpressionId, ItemId, StatementId},
    lexer::lex,
    parser::{
        Expression, ForBinding, FunctionBody, FunctionForm, Item, NameMarker, ParsedFile,
        Statement, VariableKind, parse_file,
    },
    source::{SourceMap, Span},
};

pub(super) struct Input {
    pub sources: SourceMap,
    pub parsed: ParsedFile,
    pub map: bool,
}

impl Input {
    pub fn new(text: &str, map: bool) -> Result<Self, Failure> {
        let mut sources = SourceMap::new();
        let id = sources
            .add_source("provider.ko", text)
            .map_err(|e| failure("source", e))?;
        let lexed = lex(&sources, id).map_err(|e| failure("lex infrastructure", e))?;
        clean(&sources, "lex diagnostics", lexed.diagnostics())?;
        let parsed =
            parse_file(&sources, &lexed).map_err(|e| failure("parse infrastructure", e))?;
        clean(&sources, "parse diagnostics", parsed.diagnostics())?;
        need(
            parsed.package().is_none(),
            "nested AST default package",
            parsed.package(),
        )?;
        Ok(Self {
            sources,
            parsed,
            map,
        })
    }

    pub fn expression(&self, id: ExpressionId) -> Result<(&Expression, Span), Failure> {
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(id)
            .map_err(|e| failure("nested AST expression", e))?;
        Ok((node.payload(), node.span()))
    }

    fn elements(&self, id: StatementId) -> Result<&[StatementId], Failure> {
        let node = self
            .parsed
            .ast()
            .statements()
            .get(id)
            .map_err(|e| failure("nested AST block", e))?;
        let Statement::Block { elements } = node.payload() else {
            return Err(failure("nested AST actual Block", node));
        };
        Ok(elements)
    }

    fn statement_expression(&self, id: StatementId) -> Result<ExpressionId, Failure> {
        let node = self
            .parsed
            .ast()
            .statements()
            .get(id)
            .map_err(|e| failure("nested AST statement", e))?;
        let Statement::Expression { expression } = node.payload() else {
            return Err(failure("nested AST expression statement", node));
        };
        Ok(*expression)
    }

    fn call(
        &self,
        id: ExpressionId,
        name: &'static str,
        expected: &[&str],
    ) -> Result<Call, Failure> {
        let (expression, span) = self.expression(id)?;
        let Expression::Call {
            callee,
            arguments,
            type_arguments,
            ..
        } = expression
        else {
            return Err(failure("nested AST direct Call", expression));
        };
        let (callee, callee_span) = self.expression(*callee)?;
        need(
            matches!(callee, Expression::Name)
                && self.sources.slice(callee_span) == Ok(name)
                && type_arguments.is_empty()
                && arguments.len() == expected.len(),
            "nested AST exact direct callee/arity",
            (id, expression, name),
        )?;
        let arguments = arguments.iter().map(|a| a.value).collect::<Vec<_>>();
        for (argument, expected) in arguments.iter().zip(expected) {
            self.name(*argument, expected)?;
        }
        Ok(Call {
            expression: id,
            span,
            target: name,
            arguments,
        })
    }

    fn name(&self, id: ExpressionId, expected: &str) -> Result<(), Failure> {
        let (expression, span) = self.expression(id)?;
        need(
            matches!(expression, Expression::Name) && self.sources.slice(span) == Ok(expected),
            "nested AST direct name reference",
            (id, expression, span, expected),
        )
    }

    fn local(&self, id: StatementId, expected: &str) -> Result<(Span, ExpressionId), Failure> {
        let node = self
            .parsed
            .ast()
            .statements()
            .get(id)
            .map_err(|e| failure("nested AST local", e))?;
        let Statement::LocalVariable { declaration } = node.payload() else {
            return Err(failure("nested AST local declaration", node));
        };
        let item = self
            .parsed
            .ast()
            .items()
            .get(*declaration)
            .map_err(|e| failure("nested AST variable", e))?;
        let Item::Variable {
            kind: VariableKind::Val,
            name: NameMarker::Present(name),
            initializer,
            ..
        } = item.payload()
        else {
            return Err(failure("nested AST val declaration", item));
        };
        need(
            self.sources.slice(*name) == Ok(expected),
            "nested AST actual local marker",
            item,
        )?;
        Ok((*name, *initializer))
    }
}

#[derive(Debug)]
pub(super) struct Function {
    pub item: ItemId,
    pub name: &'static str,
    pub name_span: Span,
    pub span: Span,
    pub body: StatementId,
    pub parameters: Vec<Span>,
}

#[derive(Debug)]
pub(super) struct Site {
    pub statement: StatementId,
    pub span: Span,
    pub body: StatementId,
    pub source: ExpressionId,
    pub source_span: Span,
    pub bindings: [Span; 2],
}

#[derive(Debug)]
pub(super) struct Call {
    pub expression: ExpressionId,
    pub span: Span,
    pub target: &'static str,
    pub arguments: Vec<ExpressionId>,
}

#[derive(Debug)]
pub(super) struct Read {
    pub call: Call,
    pub binding_site: usize,
}

#[derive(Debug)]
pub(super) struct Size {
    pub expression: ExpressionId,
    pub receiver: ExpressionId,
    pub root_marker: Span,
    pub span: Span,
}

#[derive(Debug)]
pub(super) struct Anchors {
    pub functions: Vec<Function>,
    pub sites: [Site; 2],
    pub reads: [Read; 4],
    pub scans: [Call; 2],
    pub factory: Call,
    pub locals: [(Span, ExpressionId); 2],
    pub sizes: Vec<Size>,
}

impl Anchors {
    pub fn functions_for_ssa(&self) -> Vec<(&str, Span)> {
        self.functions.iter().map(|f| (f.name, f.span)).collect()
    }
    pub fn function(&self, name: &str) -> Option<&Function> {
        self.functions.iter().find(|f| f.name == name)
    }
    pub fn parameter(&self) -> Option<Span> {
        self.function("scan")
            .and_then(|f| match f.parameters.as_slice() {
                [p] => Some(*p),
                _ => None,
            })
    }
}

pub(super) fn collect(f: &Input) -> Result<Anchors, Failure> {
    let factory_name = if f.map { "makeMap" } else { "makeMutable" };
    let ast = f.parsed.ast();
    need(
        f.parsed.roots().len() == 4,
        "nested AST four function roots",
        f.parsed.roots(),
    )?;
    let mut functions = Vec::new();
    for (expected, expected_parameters) in [
        ("entry", &[][..]),
        ("read", &["key", "value"][..]),
        ("scan", &["source"][..]),
        (factory_name, &[][..]),
    ] {
        let mut found = Vec::new();
        for id in f.parsed.roots() {
            let node = ast
                .items()
                .get(*id)
                .map_err(|e| failure("nested AST function root", e))?;
            if let Item::Function {
                name: NameMarker::Present(name),
                form,
                parameters,
                extension_receiver,
                type_parameters,
                ..
            } = node.payload()
                && f.sources.slice(*name) == Ok(expected)
            {
                let body = match form {
                    FunctionForm::ImplicitUnitBlock(body)
                    | FunctionForm::Explicit {
                        body: FunctionBody::Block(body),
                        ..
                    } => *body,
                    _ => return Err(failure("nested AST function body", node)),
                };
                need(extension_receiver.is_none() && type_parameters.is_empty()
                    && parameters.len() == expected_parameters.len()
                    && parameters.iter().zip(expected_parameters).all(|(p, expected)| p.mode_marker.is_none()
                        && matches!(p.name, NameMarker::Present(span) if f.sources.slice(span) == Ok(*expected)))
                    && node.span().source_id() == f.parsed.source_id()
                    && f.sources.source_name(node.span().source_id()) == Ok("provider.ko"),
                    "nested AST original unmarked Borrow signature", node)?;
                found.push(Function {
                    item: *id,
                    name: expected,
                    name_span: *name,
                    span: node.span(),
                    body,
                    parameters: parameters
                        .iter()
                        .filter_map(|p| match p.name {
                            NameMarker::Present(span) => Some(span),
                            _ => None,
                        })
                        .collect(),
                });
            }
        }
        need(
            found.len() == 1,
            "nested AST unique function",
            (&expected, &found),
        )?;
        functions.extend(found);
    }
    let scan = functions
        .iter()
        .find(|f| f.name == "scan")
        .ok_or_else(|| failure("nested AST scan", &functions))?;
    let [outer, _, _, _] = f.elements(scan.body)? else {
        return Err(failure(
            "nested AST exact scan statements",
            f.elements(scan.body)?,
        ));
    };
    let outer = site(f, *outer, ["outerKey", "outerValue"])?;
    let [_, before_inner, inner, _, after_inner] = f.elements(outer.body)? else {
        return Err(failure(
            "nested AST exact outer body",
            f.elements(outer.body)?,
        ));
    };
    let inner = site(f, *inner, ["innerKey", "innerValue"])?;
    let [_, outer_in_inner, inner_read] = f.elements(inner.body)? else {
        return Err(failure(
            "nested AST exact inner body",
            f.elements(inner.body)?,
        ));
    };
    let reads = [
        Read {
            call: f.call(
                f.statement_expression(*before_inner)?,
                "read",
                &["outerKey", "outerValue"],
            )?,
            binding_site: 0,
        },
        Read {
            call: f.call(
                f.statement_expression(*outer_in_inner)?,
                "read",
                &["outerKey", "outerValue"],
            )?,
            binding_site: 0,
        },
        Read {
            call: f.call(
                f.statement_expression(*inner_read)?,
                "read",
                &["innerKey", "innerValue"],
            )?,
            binding_site: 1,
        },
        Read {
            call: f.call(
                f.statement_expression(*after_inner)?,
                "read",
                &["outerKey", "outerValue"],
            )?,
            binding_site: 0,
        },
    ];
    let all_for = ast
        .statements()
        .iter()
        .filter_map(|(id, n)| matches!(n.payload(), Statement::For { .. }).then_some(id))
        .collect::<Vec<_>>();
    need(
        all_for.len() == 2
            && all_for.contains(&outer.statement)
            && all_for.contains(&inner.statement)
            && outer.statement != inner.statement
            && outer.source != inner.source
            && outer.source_span != inner.source_span
            && contains(outer.span, inner.span)
            && outer.bindings.iter().all(|s| !inner.bindings.contains(s))
            && reads[0].call.span.end() <= inner.span.start()
            && inner.span.end() <= reads[3].call.span.start()
            && contains(inner.span, reads[1].call.span)
            && contains(inner.span, reads[2].call.span),
        "nested AST actual parent/child For and outer read after inner",
        (&all_for, &outer, &inner, &reads),
    )?;
    let entry = functions
        .iter()
        .find(|f| f.name == "entry")
        .ok_or_else(|| failure("nested AST entry", &functions))?;
    let [
        _,
        populated,
        scan_populated,
        populated_size,
        _,
        empty,
        scan_empty,
        empty_size,
    ] = f.elements(entry.body)?
    else {
        return Err(failure(
            "nested AST exact entry statements",
            f.elements(entry.body)?,
        ));
    };
    let locals = [f.local(*populated, "populated")?, f.local(*empty, "empty")?];
    let factory = f.call(locals[0].1, factory_name, &[])?;
    let scans = [
        f.call(
            f.statement_expression(*scan_populated)?,
            "scan",
            &["populated"],
        )?,
        f.call(f.statement_expression(*scan_empty)?, "scan", &["empty"])?,
    ];
    let mut sizes = Vec::new();
    for (id, node) in ast.expressions().iter() {
        let Expression::Member {
            receiver,
            name_span,
            safe: false,
            ..
        } = node.payload()
        else {
            continue;
        };
        if f.sources.slice(*name_span) != Ok("size") {
            continue;
        }
        let (root_marker, expected) = if contains(scan.span, node.span()) {
            need(
                outer.span.end() <= node.span().start(),
                "nested AST scan size after outer",
                node,
            )?;
            (scan.parameters[0], "source")
        } else if contains(entry.span, node.span()) {
            let (_, receiver_span) = f.expression(*receiver)?;
            if f.sources.slice(receiver_span) == Ok("populated") {
                let statement = ast
                    .statements()
                    .get(*populated_size)
                    .map_err(|e| failure("nested AST caller size", e))?;
                need(
                    contains(statement.span(), node.span())
                        && scans[0].span.end() <= node.span().start(),
                    "nested AST populated size after scan",
                    node,
                )?;
                (locals[0].0, "populated")
            } else {
                let statement = ast
                    .statements()
                    .get(*empty_size)
                    .map_err(|e| failure("nested AST caller size", e))?;
                need(
                    contains(statement.span(), node.span())
                        && scans[1].span.end() <= node.span().start(),
                    "nested AST empty size after scan",
                    node,
                )?;
                (locals[1].0, "empty")
            }
        } else {
            return Err(failure("nested AST unexpected size owner", node));
        };
        f.name(*receiver, expected)?;
        sizes.push(Size {
            expression: id,
            receiver: *receiver,
            root_marker,
            span: node.span(),
        });
    }
    need(
        sizes.len() == 4
            && sizes
                .iter()
                .filter(|s| s.root_marker == scan.parameters[0])
                .count()
                == 2
            && locals
                .iter()
                .all(|(marker, _)| sizes.iter().filter(|s| s.root_marker == *marker).count() == 1),
        "nested AST two scan and two caller size references",
        &sizes,
    )?;
    Ok(Anchors {
        functions,
        sites: [outer, inner],
        reads,
        scans,
        factory,
        locals,
        sizes,
    })
}

fn site(f: &Input, statement: StatementId, expected: [&str; 2]) -> Result<Site, Failure> {
    let node = f
        .parsed
        .ast()
        .statements()
        .get(statement)
        .map_err(|e| failure("nested AST For", e))?;
    let Statement::For {
        binding: ForBinding::Destructuring { names, .. },
        source,
        body,
        ..
    } = node.payload()
    else {
        return Err(failure("nested AST destructuring For", node));
    };
    let [NameMarker::Present(key), NameMarker::Present(value)] = names.as_slice() else {
        return Err(failure("nested AST two named markers", names));
    };
    need(
        f.sources.slice(*key) == Ok(expected[0]) && f.sources.slice(*value) == Ok(expected[1]),
        "nested AST actual ordered markers",
        names,
    )?;
    f.name(*source, "source")?;
    let (_, source_span) = f.expression(*source)?;
    Ok(Site {
        statement,
        span: node.span(),
        body: *body,
        source: *source,
        source_span,
        bindings: [*key, *value],
    })
}
