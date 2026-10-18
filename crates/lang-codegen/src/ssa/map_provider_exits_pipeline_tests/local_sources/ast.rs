//! 四个真实 scanner 各两 local/For；A/B 只在 For 后有无 size 读取上不同。
use super::super::fixture::{Failure, clean, failure, need};
use lang_frontend::{
    ast::{ExpressionId, ItemId, StatementId},
    lexer::lex,
    parser::{
        BinaryOperator, Expression, ForBinding, FunctionBody, FunctionForm, Item, NameMarker,
        ParsedFile, Statement, VariableKind, parse_file,
    },
    source::{SourceMap, Span},
};

pub(super) struct Input {
    pub sources: SourceMap,
    pub parsed: ParsedFile,
    pub later: bool,
}

impl Input {
    pub fn new(text: &str, later: bool) -> Result<Self, Failure> {
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
            "local AST default package",
            parsed.package(),
        )?;
        Ok(Self {
            sources,
            parsed,
            later,
        })
    }

    pub fn expression(&self, id: ExpressionId) -> Result<(&Expression, Span), Failure> {
        let n = self
            .parsed
            .ast()
            .expressions()
            .get(id)
            .map_err(|e| failure("local AST expression", e))?;
        Ok((n.payload(), n.span()))
    }

    fn elements(&self, id: StatementId) -> Result<&[StatementId], Failure> {
        let n = self
            .parsed
            .ast()
            .statements()
            .get(id)
            .map_err(|e| failure("local AST block", e))?;
        let Statement::Block { elements } = n.payload() else {
            return Err(failure("local AST actual block", n));
        };
        Ok(elements)
    }

    // parse_if → parse_control_body → parse_control_block 保留独立 ControlBody。
    // 只供冻结 If 的两个花括号分支使用；普通函数/For 仍严格要求 Block。
    fn control_elements(&self, id: StatementId) -> Result<&[StatementId], Failure> {
        let n = self
            .parsed
            .ast()
            .statements()
            .get(id)
            .map_err(|e| failure("local AST control body", e))?;
        let Statement::ControlBody { elements } = n.payload() else {
            return Err(failure("local AST actual ControlBody", (n, n.payload())));
        };
        Ok(elements)
    }

    fn statement_expression(&self, id: StatementId) -> Result<ExpressionId, Failure> {
        let n = self
            .parsed
            .ast()
            .statements()
            .get(id)
            .map_err(|e| failure("local AST statement", e))?;
        let Statement::Expression { expression } = n.payload() else {
            return Err(failure("local AST expression statement", n));
        };
        Ok(*expression)
    }

    fn name(&self, id: ExpressionId, expected: &str) -> Result<(), Failure> {
        let (node, span) = self.expression(id)?;
        need(
            matches!(node, Expression::Name) && self.sources.slice(span) == Ok(expected),
            "local AST exact Name",
            (id, node, span, expected),
        )
    }

    fn call(
        &self,
        id: ExpressionId,
        expected: &'static str,
        arity: usize,
    ) -> Result<Call, Failure> {
        let (node, span) = self.expression(id)?;
        let Expression::Call {
            callee,
            arguments,
            type_arguments,
            ..
        } = node
        else {
            return Err(failure("local AST actual Call", node));
        };
        self.name(*callee, expected)?;
        need(
            type_arguments.is_empty() && arguments.len() == arity,
            "local AST exact ordinary call arity",
            (id, node, arity),
        )?;
        Ok(Call {
            expression: id,
            callee: *callee,
            span,
            target: expected,
            arguments: arguments.iter().map(|a| a.value).collect(),
        })
    }

    fn marker(&self, statement: StatementId, expected: &str) -> Result<(), Failure> {
        let call = self.call(self.statement_expression(statement)?, "println", 1)?;
        let [argument] = call.arguments.as_slice() else {
            return Err(failure("local AST marker arity", call));
        };
        let (node, span) = self.expression(*argument)?;
        need(
            matches!(node, Expression::String { .. }) && self.sources.slice(span) == Ok(expected),
            "local AST exact output marker",
            (node, span, expected),
        )
    }

    fn local(
        &self,
        statement: StatementId,
        name: &str,
        kind: VariableKind,
    ) -> Result<Local, Failure> {
        let n = self
            .parsed
            .ast()
            .statements()
            .get(statement)
            .map_err(|e| failure("local AST declaration", e))?;
        let Statement::LocalVariable { declaration } = n.payload() else {
            return Err(failure("local AST actual LocalVariable", n));
        };
        let item = self
            .parsed
            .ast()
            .items()
            .get(*declaration)
            .map_err(|e| failure("local AST variable", e))?;
        let Item::Variable {
            kind: actual,
            name: NameMarker::Present(marker),
            initializer,
            ..
        } = item.payload()
        else {
            return Err(failure("local AST actual Variable", item));
        };
        need(
            *actual == kind && self.sources.slice(*marker) == Ok(name),
            "local AST val/var and exact marker",
            (item, kind, name),
        )?;
        Ok(Local {
            statement,
            item: *declaration,
            marker: *marker,
            kind,
            initializer: *initializer,
        })
    }
}

#[derive(Debug)]
pub(super) struct Function {
    pub item: ItemId,
    pub name: &'static str,
    pub name_span: Span,
    pub span: Span,
    pub body: StatementId,
}

#[derive(Debug)]
pub(super) struct Call {
    pub expression: ExpressionId,
    pub callee: ExpressionId,
    pub span: Span,
    pub target: &'static str,
    pub arguments: Vec<ExpressionId>,
}

#[derive(Debug)]
pub(super) struct Local {
    pub statement: StatementId,
    pub item: ItemId,
    pub marker: Span,
    pub kind: VariableKind,
    pub initializer: ExpressionId,
}

#[derive(Debug)]
pub(super) struct Size {
    pub expression: ExpressionId,
    pub receiver: ExpressionId,
    pub comparison: ExpressionId,
    pub condition: ExpressionId,
    pub span: Span,
}

#[derive(Debug)]
pub(super) struct Site {
    pub scanner: &'static str,
    pub scanner_span: Span,
    pub scanner_body: StatementId,
    pub map: bool,
    pub local: Local,
    pub factory: Call,
    pub statement: StatementId,
    pub span: Span,
    pub body: StatementId,
    pub source: ExpressionId,
    pub source_span: Span,
    pub bindings: [Span; 2],
    pub read: Call,
    pub size: Option<Size>,
}

#[derive(Debug)]
pub(super) struct Anchors {
    pub functions: Vec<Function>,
    pub scans: Vec<Call>,
    pub sites: Vec<Site>,
}

impl Anchors {
    pub fn functions_for_ssa(&self) -> Vec<(&str, Span)> {
        self.functions.iter().map(|f| (f.name, f.span)).collect()
    }
    pub fn function(&self, name: &str) -> Option<&Function> {
        self.functions.iter().find(|f| f.name == name)
    }
}

const SCANNERS: [(&str, &str, bool, VariableKind); 4] = [
    ("scanMapVal", "map-val", true, VariableKind::Val),
    ("scanMapVar", "map-var", true, VariableKind::Var),
    ("scanMutableVal", "mutable-val", false, VariableKind::Val),
    ("scanMutableVar", "mutable-var", false, VariableKind::Var),
];

pub(super) fn collect(f: &Input) -> Result<Anchors, Failure> {
    let ast = f.parsed.ast();
    need(
        ast.statements()
            .iter()
            .filter(|(_, n)| matches!(n.payload(), Statement::For { .. }))
            .count()
            == 8,
        "local AST exactly eight For nodes",
        f.parsed.source_id(),
    )?;
    need(
        f.parsed.roots().len() == 10,
        "local AST exactly ten roots",
        f.parsed.roots(),
    )?;
    let mut functions = Vec::new();
    for expected in [
        "entry",
        "read",
        "makeMap",
        "makeEmptyMap",
        "makeMutable",
        "makeEmptyMutable",
        "scanMapVal",
        "scanMapVar",
        "scanMutableVal",
        "scanMutableVar",
    ] {
        let mut found = Vec::new();
        for root in f.parsed.roots() {
            let n = ast
                .items()
                .get(*root)
                .map_err(|e| failure("local AST function", e))?;
            if let Item::Function {
                name: NameMarker::Present(name),
                form,
                ..
            } = n.payload()
                && f.sources.slice(*name) == Ok(expected)
            {
                let body = match form {
                    FunctionForm::ImplicitUnitBlock(body)
                    | FunctionForm::Explicit {
                        body: FunctionBody::Block(body),
                        ..
                    } => *body,
                    _ => return Err(failure("local AST function block", n)),
                };
                need(
                    n.span().source_id() == f.parsed.source_id()
                        && f.sources.source_name(n.span().source_id()) == Ok("provider.ko"),
                    "local AST original function source",
                    n,
                )?;
                found.push(Function {
                    item: *root,
                    name: expected,
                    name_span: *name,
                    span: n.span(),
                    body,
                });
            }
        }
        need(
            found.len() == 1,
            "local AST unique function",
            (&expected, &found),
        )?;
        functions.extend(found);
    }
    let entry = functions
        .iter()
        .find(|v| v.name == "entry")
        .ok_or_else(|| failure("local AST entry", &functions))?;
    let entry_body = f.elements(entry.body)?;
    need(
        entry_body.len() == 8,
        "local AST entry four actual calls",
        entry_body,
    )?;
    let mut scans = Vec::new();
    let mut sites = Vec::new();
    for (index, (scanner, label, map, kind)) in SCANNERS.iter().copied().enumerate() {
        f.marker(entry_body[index * 2], &format!("\"{label}\""))?;
        scans.push(f.call(
            f.statement_expression(entry_body[index * 2 + 1])?,
            scanner,
            0,
        )?);
        let function = functions
            .iter()
            .find(|v| v.name == scanner)
            .ok_or_else(|| failure("local AST scanner", scanner))?;
        let elements = f.elements(function.body)?;
        let stride = if f.later { 4 } else { 3 };
        need(
            elements.len() == stride * 2,
            "local AST scanner exact statement count",
            (scanner, elements),
        )?;
        for (part, name) in ["source", "empty"].into_iter().enumerate() {
            let offset = part * stride;
            let local = f.local(elements[offset], name, kind)?;
            let factory = match (map, part) {
                (true, 0) => "makeMap",
                (true, _) => "makeEmptyMap",
                (false, 0) => "makeMutable",
                (false, _) => "makeEmptyMutable",
            };
            let factory = f.call(local.initializer, factory, 0)?;
            let mut actual = site(f, elements[offset + 1], function, map, local, factory, name)?;
            if f.later {
                actual.size = Some(size(f, elements[offset + 2], name, part == 1)?);
            }
            let suffix = if part == 0 { "populated" } else { "empty" };
            f.marker(
                elements[offset + stride - 1],
                &format!("\"{label}-after-{suffix}\""),
            )?;
            sites.push(actual);
        }
    }
    // 仅按实际函数 span 与本次 Statement 身份排序；没有固定裸 ID。
    sites.sort_by_key(|s| (s.scanner_span.start(), s.statement.index()));
    need(
        sites.len() == 8
            && sites.iter().enumerate().all(|(i, s)| {
                sites[..i].iter().all(|prior| {
                    prior.statement != s.statement
                        && prior.source != s.source
                        && prior.local.marker != s.local.marker
                        && prior.local.initializer != s.local.initializer
                        && prior.read.expression != s.read.expression
                        && prior.bindings.iter().all(|b| !s.bindings.contains(b))
                })
            }),
        "local AST all eight sites have distinct identities",
        &sites,
    )?;
    let actual_sizes = ast.expressions().iter().filter(|(_, n)| {
        matches!(n.payload(), Expression::Member { name_span, .. } if f.sources.slice(*name_span) == Ok("size"))
    }).count();
    need(
        actual_sizes == if f.later { 8 } else { 0 },
        "local AST exact eight or zero size members",
        actual_sizes,
    )?;
    Ok(Anchors {
        functions,
        scans,
        sites,
    })
}

fn site(
    f: &Input,
    statement: StatementId,
    scanner: &Function,
    map: bool,
    local: Local,
    factory: Call,
    name: &str,
) -> Result<Site, Failure> {
    let n = f
        .parsed
        .ast()
        .statements()
        .get(statement)
        .map_err(|e| failure("local AST For", e))?;
    let Statement::For {
        binding: ForBinding::Destructuring { names, .. },
        source,
        body,
        ..
    } = n.payload()
    else {
        return Err(failure("local AST destructuring For", n));
    };
    let [NameMarker::Present(key), NameMarker::Present(value)] = names.as_slice() else {
        return Err(failure("local AST two named components", names));
    };
    need(
        f.sources.slice(*key) == Ok("key") && f.sources.slice(*value) == Ok("value"),
        "local AST ordered key/value markers",
        names,
    )?;
    f.name(*source, name)?;
    let source_span = f.expression(*source)?.1;
    let [read_statement] = f.elements(*body)? else {
        return Err(failure("local AST sole body read", f.elements(*body)?));
    };
    let read = f.call(f.statement_expression(*read_statement)?, "read", 2)?;
    for (id, expected) in read.arguments.iter().zip(["key", "value"]) {
        f.name(*id, expected)?;
    }
    need(
        scanner.span.start() <= local.marker.start()
            && local.marker.end() <= factory.span.start()
            && factory.span.end() <= n.span().start()
            && n.span().end() <= scanner.span.end()
            && n.span().start() <= read.span.start()
            && read.span.end() <= n.span().end(),
        "local AST real scanner contains local initializer and For/body read",
        (scanner, &local, &factory, n.span(), &read),
    )?;
    Ok(Site {
        scanner: scanner.name,
        scanner_span: scanner.span,
        scanner_body: scanner.body,
        map,
        local,
        factory,
        statement,
        span: n.span(),
        body: *body,
        source: *source,
        source_span,
        bindings: [*key, *value],
        read,
        size: None,
    })
}

fn size(f: &Input, statement: StatementId, name: &str, empty: bool) -> Result<Size, Failure> {
    let condition = f.statement_expression(statement)?;
    let (node, _) = f.expression(condition)?;
    let Expression::If {
        condition: comparison,
        then_branch,
        else_branch: Some(other),
        ..
    } = node
    else {
        return Err(failure("local AST later-use actual If", node));
    };
    let (node, _) = f.expression(*comparison)?;
    let Expression::Binary {
        left,
        operator: BinaryOperator::Equal,
        right,
        ..
    } = node
    else {
        return Err(failure("local AST size equality", node));
    };
    let (node, span) = f.expression(*left)?;
    let Expression::Member {
        receiver,
        name_span,
        safe: false,
        ..
    } = node
    else {
        return Err(failure("local AST actual size member", node));
    };
    f.name(*receiver, name)?;
    need(
        f.sources.slice(*name_span) == Ok("size")
            && f.sources.slice(f.expression(*right)?.1) == Ok(if empty { "0" } else { "2" }),
        "local AST size member and exact expected integer",
        (node, right),
    )?;
    let [success] = f.control_elements(*then_branch)? else {
        return Err(failure("local AST one success marker", then_branch));
    };
    f.marker(*success, if empty { "\"size-0\"" } else { "\"size-2\"" })?;
    let [failure_statement] = f.control_elements(*other)? else {
        return Err(failure("local AST one failure Call", other));
    };
    let failure_call = f.call(f.statement_expression(*failure_statement)?, "error", 1)?;
    need(
        failure_call.arguments.len() == 1,
        "local AST size failure abort call",
        &failure_call,
    )?;
    Ok(Size {
        expression: *left,
        receiver: *receiver,
        comparison: *comparison,
        condition,
        span,
    })
}
