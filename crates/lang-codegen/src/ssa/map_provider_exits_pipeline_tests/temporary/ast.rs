//! 两个同层 For 的真实 AST 身份；不复用单 For/Name 参数 fixture 假设。
use super::super::fixture::{Failure, clean, failure, need};
use lang_frontend::{
    ast::{ExpressionId, ItemId, StatementId},
    lexer::lex,
    parser::{
        Expression, ForBinding, FunctionBody, FunctionForm, Item, NameMarker, ParsedFile,
        Statement, parse_file,
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
            "AST default package",
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
            .map_err(|e| failure("AST expression", e))?;
        Ok((node.payload(), node.span()))
    }

    fn elements(&self, id: StatementId) -> Result<&[StatementId], Failure> {
        let node = self
            .parsed
            .ast()
            .statements()
            .get(id)
            .map_err(|e| failure("AST block", e))?;
        let Statement::Block { elements } = node.payload() else {
            return Err(failure("AST actual Block", node));
        };
        Ok(elements)
    }

    fn statement_expression(&self, id: StatementId) -> Result<ExpressionId, Failure> {
        let node = self
            .parsed
            .ast()
            .statements()
            .get(id)
            .map_err(|e| failure("AST statement", e))?;
        let Statement::Expression { expression } = node.payload() else {
            return Err(failure("AST expression statement", node));
        };
        Ok(*expression)
    }

    fn call(
        &self,
        id: ExpressionId,
        name: &str,
        arity: usize,
    ) -> Result<Vec<ExpressionId>, Failure> {
        let (expression, _) = self.expression(id)?;
        let Expression::Call {
            callee,
            arguments,
            type_arguments,
            ..
        } = expression
        else {
            return Err(failure("AST actual direct Call", expression));
        };
        let (callee, span) = self.expression(*callee)?;
        need(
            matches!(callee, Expression::Name)
                && self.sources.slice(span) == Ok(name)
                && type_arguments.is_empty()
                && arguments.len() == arity,
            "AST exact direct callee and arity",
            (id, expression, name),
        )?;
        Ok(arguments.iter().map(|a| a.value).collect())
    }

    fn marker(&self, statement: StatementId, expected: &str) -> Result<(), Failure> {
        let id = self.statement_expression(statement)?;
        let arguments = self.call(id, "println", 1)?;
        let [argument] = arguments.as_slice() else {
            return Err(failure("AST marker argument", arguments));
        };
        let (literal, span) = self.expression(*argument)?;
        need(
            matches!(literal, Expression::String { .. })
                && self.sources.slice(span) == Ok(expected),
            "AST exact entry marker",
            (id, literal, span),
        )
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
pub(super) struct Site {
    pub factory: &'static str,
    pub statement: StatementId,
    pub span: Span,
    pub body: StatementId,
    pub source: ExpressionId,
    pub source_span: Span,
    pub bindings: [Span; 2],
    pub read: ExpressionId,
    pub arguments: [ExpressionId; 2],
}

#[derive(Debug)]
pub(super) struct Anchors {
    pub functions: Vec<Function>,
    pub sites: [Site; 2],
}

impl Anchors {
    pub fn functions_for_ssa(&self) -> Vec<(&str, Span)> {
        self.functions.iter().map(|f| (f.name, f.span)).collect()
    }

    pub fn function(&self, name: &str) -> Option<&Function> {
        self.functions.iter().find(|f| f.name == name)
    }
}

pub(super) fn collect(f: &Input) -> Result<Anchors, Failure> {
    let factories = if f.map {
        ["makeMap", "makeEmptyMap"]
    } else {
        ["makeMutable", "makeEmptyMutable"]
    };
    let ast = f.parsed.ast();
    need(
        f.parsed.roots().len() == 4,
        "AST exact four function roots",
        f.parsed.roots(),
    )?;
    let mut functions = Vec::new();
    for expected in ["entry", "read", factories[0], factories[1]] {
        let mut found = Vec::new();
        for id in f.parsed.roots() {
            let node = ast
                .items()
                .get(*id)
                .map_err(|e| failure("AST function root", e))?;
            if let Item::Function {
                name: NameMarker::Present(name),
                form,
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
                    _ => return Err(failure("AST actual function body", node)),
                };
                need(
                    node.span().source_id() == f.parsed.source_id()
                        && f.sources.source_name(node.span().source_id()) == Ok("provider.ko"),
                    "AST original source function identity",
                    node,
                )?;
                found.push(Function {
                    item: *id,
                    name: expected,
                    name_span: *name,
                    span: node.span(),
                    body,
                });
            }
        }
        need(
            found.len() == 1,
            "AST unique actual function",
            (&expected, &found),
        )?;
        functions.extend(found);
    }
    let entry = functions
        .first()
        .ok_or_else(|| failure("AST entry", &functions))?;
    let [nonempty, first, first_done, empty, second, second_done] = f.elements(entry.body)? else {
        return Err(failure(
            "AST six actual entry statements",
            f.elements(entry.body)?,
        ));
    };
    f.marker(*nonempty, "\"nonempty\"")?;
    f.marker(*first_done, "\"nonempty-exhausted\"")?;
    f.marker(*empty, "\"empty\"")?;
    f.marker(*second_done, "\"empty-exhausted\"")?;
    let sites = [
        site(f, *first, factories[0])?,
        site(f, *second, factories[1])?,
    ];
    let all_for = ast
        .statements()
        .iter()
        .filter_map(|(id, node)| matches!(node.payload(), Statement::For { .. }).then_some(id))
        .collect::<Vec<_>>();
    need(
        all_for.len() == 2
            && sites.iter().all(|site| all_for.contains(&site.statement))
            && sites[0].statement != sites[1].statement
            && sites[0].source != sites[1].source
            && sites[0].read != sites[1].read
            && sites[0].span.end() <= sites[1].span.start()
            && sites[0]
                .bindings
                .iter()
                .all(|span| !sites[1].bindings.contains(span)),
        "AST exactly two independent same-level For sites",
        (&all_for, &sites),
    )?;
    Ok(Anchors { functions, sites })
}

fn site(f: &Input, statement: StatementId, factory: &'static str) -> Result<Site, Failure> {
    let node = f
        .parsed
        .ast()
        .statements()
        .get(statement)
        .map_err(|e| failure("AST For", e))?;
    let Statement::For {
        binding: ForBinding::Destructuring { names, .. },
        source,
        body,
        ..
    } = node.payload()
    else {
        return Err(failure("AST actual destructuring For", node));
    };
    let [NameMarker::Present(key), NameMarker::Present(value)] = names.as_slice() else {
        return Err(failure("AST two real binding markers", names));
    };
    need(
        f.sources.slice(*key) == Ok("key") && f.sources.slice(*value) == Ok("value"),
        "AST ordered key/value markers",
        names,
    )?;
    f.call(*source, factory, 0)?;
    let (_, source_span) = f.expression(*source)?;
    let [read_statement] = f.elements(*body)? else {
        return Err(failure("AST sole actual body read", f.elements(*body)?));
    };
    let read = f.statement_expression(*read_statement)?;
    let arguments = f.call(read, "read", 2)?;
    let [key_argument, value_argument] = arguments.as_slice() else {
        return Err(failure("AST two actual read arguments", arguments));
    };
    for (argument, expected) in [(*key_argument, "key"), (*value_argument, "value")] {
        let (expression, span) = f.expression(argument)?;
        need(
            matches!(expression, Expression::Name) && f.sources.slice(span) == Ok(expected),
            "AST actual read binding reference",
            (argument, span),
        )?;
    }
    Ok(Site {
        factory,
        statement,
        span: node.span(),
        body: *body,
        source: *source,
        source_span,
        bindings: [*key, *value],
        read,
        arguments: [*key_argument, *value_argument],
    })
}
