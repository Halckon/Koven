//! 两个独立 temporary Call-source 退出 site 的真实 AST 身份；不预测 FE owner/plan。
use super::super::fixture::{Failure, clean, contains, failure, need};
use lang_frontend::{
    ast::{ExpressionId, ItemId, StatementId},
    lexer::lex,
    parser::{
        ClassifierKind, Expression, ForBinding, FunctionBody, FunctionForm, Item, NameMarker,
        ParsedFile, Statement, VariableKind, parse_file,
    },
    source::{SourceMap, Span},
};

#[path = "ast_checks.rs"]
mod checks;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Exit {
    Continue,
    Break,
    Return,
}

pub(super) struct Input {
    pub sources: SourceMap,
    pub parsed: ParsedFile,
    pub map: bool,
    pub exit: Exit,
}

impl Input {
    pub fn new(text: &str, map: bool, exit: Exit) -> Result<Self, Failure> {
        let mut sources = SourceMap::new();
        let id = sources
            .add_source("provider.ko", text)
            .map_err(|e| failure("temporary exits source", e))?;
        let lexed = lex(&sources, id).map_err(|e| failure("lex infrastructure", e))?;
        clean(&sources, "lex diagnostics", lexed.diagnostics())?;
        let parsed =
            parse_file(&sources, &lexed).map_err(|e| failure("parse infrastructure", e))?;
        clean(&sources, "parse diagnostics", parsed.diagnostics())?;
        need(
            parsed.package().is_none(),
            "temporary exits AST default package",
            parsed.package(),
        )?;
        Ok(Self {
            sources,
            parsed,
            map,
            exit,
        })
    }

    pub fn expression(&self, id: ExpressionId) -> Result<(&Expression, Span), Failure> {
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(id)
            .map_err(|e| failure("temporary exits AST expression", e))?;
        Ok((node.payload(), node.span()))
    }

    fn elements(&self, id: StatementId) -> Result<&[StatementId], Failure> {
        let node = self
            .parsed
            .ast()
            .statements()
            .get(id)
            .map_err(|e| failure("temporary exits AST block", e))?;
        let Statement::Block { elements } = node.payload() else {
            return Err(failure("temporary exits AST actual Block", node));
        };
        Ok(elements)
    }

    fn statement_expression(&self, id: StatementId) -> Result<ExpressionId, Failure> {
        let node = self
            .parsed
            .ast()
            .statements()
            .get(id)
            .map_err(|e| failure("temporary exits AST statement", e))?;
        let Statement::Expression { expression } = node.payload() else {
            return Err(failure("temporary exits AST expression statement", node));
        };
        Ok(*expression)
    }

    fn direct_call(
        &self,
        id: ExpressionId,
        name: &'static str,
        arity: usize,
    ) -> Result<Call, Failure> {
        let (expression, span) = self.expression(id)?;
        let Expression::Call {
            callee,
            arguments,
            type_arguments,
            ..
        } = expression
        else {
            return Err(failure("temporary exits AST direct Call", expression));
        };
        self.name(*callee, name)?;
        need(
            type_arguments.is_empty()
                && arguments.len() == arity
                && arguments
                    .iter()
                    .all(|a| a.named_prefix.is_none() && a.mode_marker.is_none()),
            "temporary exits AST unmarked positional call arity",
            (id, expression, name),
        )?;
        Ok(Call {
            expression: id,
            span,
            target: name,
            arguments: arguments.iter().map(|a| a.value).collect(),
        })
    }

    fn call(
        &self,
        id: ExpressionId,
        name: &'static str,
        expected: &[&str],
    ) -> Result<Call, Failure> {
        let call = self.direct_call(id, name, expected.len())?;
        for (argument, expected) in call.arguments.iter().zip(expected) {
            self.name(*argument, expected)?;
        }
        Ok(call)
    }

    fn name(&self, id: ExpressionId, expected: &str) -> Result<(), Failure> {
        let (expression, span) = self.expression(id)?;
        need(
            matches!(expression, Expression::Name) && self.sources.slice(span) == Ok(expected),
            "temporary exits AST direct name reference",
            (id, expression, span, expected),
        )
    }

    fn local(&self, id: StatementId, expected: &str) -> Result<(Span, ExpressionId), Failure> {
        self.local_kind(id, expected, VariableKind::Val)
    }

    fn local_kind(
        &self,
        id: StatementId,
        expected: &str,
        kind: VariableKind,
    ) -> Result<(Span, ExpressionId), Failure> {
        let node = self
            .parsed
            .ast()
            .statements()
            .get(id)
            .map_err(|e| failure("temporary exits AST local", e))?;
        let Statement::LocalVariable { declaration } = node.payload() else {
            return Err(failure("temporary exits AST local declaration", node));
        };
        let item = self
            .parsed
            .ast()
            .items()
            .get(*declaration)
            .map_err(|e| failure("temporary exits AST variable", e))?;
        let Item::Variable {
            kind: actual_kind,
            name: NameMarker::Present(name),
            initializer,
            type_ref: None,
            ..
        } = item.payload()
        else {
            return Err(failure("temporary exits AST val declaration", item));
        };
        need(
            *actual_kind == kind && self.sources.slice(*name) == Ok(expected),
            "temporary exits AST actual local kind and marker",
            item,
        )?;
        Ok((*name, *initializer))
    }

    fn literal(&self, id: ExpressionId, expected: &str, string: bool) -> Result<(), Failure> {
        let (expression, span) = self.expression(id)?;
        need(
            (if string {
                matches!(expression, Expression::String { .. })
            } else {
                matches!(expression, Expression::Literal(_))
            }) && self.sources.slice(span) == Ok(expected),
            "temporary exits AST exact literal",
            (id, expression, span, expected),
        )
    }

    fn marker(&self, id: StatementId, expected: &str) -> Result<(), Failure> {
        let call = self.direct_call(self.statement_expression(id)?, "println", 1)?;
        let [argument] = call.arguments.as_slice() else {
            return Err(failure("temporary exits AST println argument", call));
        };
        self.literal(*argument, expected, true)
    }

    fn scalar_return(&self, id: StatementId, expected: &str) -> Result<(), Failure> {
        self.literal(self.return_value(id)?, expected, false)
    }

    fn return_value(&self, id: StatementId) -> Result<ExpressionId, Failure> {
        let id = self.statement_expression(id)?;
        let (expression, _) = self.expression(id)?;
        let Expression::Return {
            value: Some(value), ..
        } = expression
        else {
            return Err(failure("temporary exits AST scalar return", expression));
        };
        Ok(*value)
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
    pub factory: &'static str,
    pub statement: StatementId,
    pub span: Span,
    pub body: StatementId,
    pub source: ExpressionId,
    pub source_span: Span,
    pub bindings: [Span; 2],
    // R 的 consumer 是 operand；C/B 的 consumer 是 read。
    pub consumer: &'static str,
    pub read: ExpressionId,
    pub read_span: Span,
    pub arguments: [ExpressionId; 2],
    pub transfer: ExpressionId,
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
    pub binding_markers: [Span; 2],
}

#[derive(Debug)]
pub(super) struct Guard {
    pub marker: Span,
    pub initializer: ExpressionId,
    pub span: Span,
    pub class_item: ItemId,
    pub class_marker: Span,
    pub deinit_item: ItemId,
    pub deinit_span: Span,
    pub deinit_body: StatementId,
}

#[derive(Debug)]
pub(super) struct ReturnCheck {
    pub reference: ExpressionId,
    pub marker: Span,
    pub initializer: ExpressionId,
    pub expected: &'static str,
}

#[derive(Debug)]
pub(super) struct Anchors {
    pub functions: Vec<Function>,
    pub sites: [Site; 2],
    pub guards: [Guard; 2],
    // R 唯一 operand 函数内 read；参数身份与各 site 的调用实参分别核对。
    pub operand: Option<Read>,
    pub scans: Vec<Call>,
    pub return_checks: Vec<ReturnCheck>,
}

impl Anchors {
    pub fn functions_for_ssa(&self) -> Vec<(&str, Span)> {
        self.functions
            .iter()
            .map(|f| (f.name, f.span))
            .chain([("Guard.deinit", self.guards[0].deinit_span)])
            .collect()
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
    let functions = functions(f, factories)?;
    checks::factory_bodies(f, &functions, factories)?;
    checks::read_body(f, function(&functions, "read")?)?;
    let entry = function(&functions, "entry")?;
    let (statements, scans, return_checks, operand) = if f.exit == Exit::Return {
        let first = checks::scanner(f, function(&functions, "scanNonempty")?)?;
        let second = checks::scanner(f, function(&functions, "scanEmpty")?)?;
        let caller = checks::return_entry(f, entry)?;
        let operand = checks::operand(f, function(&functions, "operand")?)?;
        (
            [first, second],
            caller.scans,
            caller.return_checks,
            Some(operand),
        )
    } else {
        let [nonempty, first, after_first, empty, second, after_second] = f.elements(entry.body)?
        else {
            return Err(failure(
                "temporary exits AST six entry statements",
                f.elements(entry.body)?,
            ));
        };
        f.marker(*nonempty, "\"nonempty\"")?;
        f.marker(*after_first, "\"after-nonempty\"")?;
        f.marker(*empty, "\"empty\"")?;
        f.marker(*after_second, "\"after-empty\"")?;
        ([*first, *second], Vec::new(), Vec::new(), None)
    };
    let (first, first_guard) = site(f, statements[0], factories[0])?;
    let (second, second_guard) = site(f, statements[1], factories[1])?;
    let sites = [first, second];
    let guards = [first_guard, second_guard];
    checks::sites(f, &functions, &sites, &guards, operand.as_ref())?;
    Ok(Anchors {
        functions,
        sites,
        guards,
        operand,
        scans,
        return_checks,
    })
}

fn functions(f: &Input, factories: [&'static str; 2]) -> Result<Vec<Function>, Failure> {
    let mut expected = vec![
        ("entry", &[][..]),
        ("read", &["key", "value"][..]),
        (factories[0], &[][..]),
        (factories[1], &[][..]),
    ];
    if f.exit == Exit::Return {
        expected.extend([
            ("operand", &["key", "value"][..]),
            ("scanNonempty", &[][..]),
            ("scanEmpty", &[][..]),
        ]);
    }
    need(
        f.parsed.roots().len() == expected.len() + 1,
        "temporary exits AST exact function and one Guard class roots",
        f.parsed.roots(),
    )?;
    let mut functions = Vec::new();
    for (expected, parameters_expected) in expected {
        let mut found = Vec::new();
        for id in f.parsed.roots() {
            let node = f
                .parsed
                .ast()
                .items()
                .get(*id)
                .map_err(|e| failure("temporary exits AST function root", e))?;
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
                    FunctionForm::Explicit {
                        body: FunctionBody::Block(body),
                        result_source: None,
                        ..
                    } => *body,
                    _ => return Err(failure("temporary exits AST actual function block", node)),
                };
                need(
                    extension_receiver.is_none()
                        && type_parameters.is_empty()
                        && parameters.len() == parameters_expected.len()
                        && parameters
                            .iter()
                            .zip(parameters_expected)
                            .all(|(p, expected)| {
                                p.mode_marker.is_none()
                                    && matches!(p.name, NameMarker::Present(span)
                            if f.sources.slice(span) == Ok(*expected))
                            })
                        && node.span().source_id() == f.parsed.source_id()
                        && f.sources.source_name(node.span().source_id()) == Ok("provider.ko"),
                    "temporary exits AST original Borrow signature and provider identity",
                    node,
                )?;
                let FunctionForm::Explicit { type_ref, .. } = form else {
                    return Err(failure(
                        "temporary exits AST explicit return annotation",
                        form,
                    ));
                };
                let result_type = f
                    .parsed
                    .ast()
                    .type_refs()
                    .get(*type_ref)
                    .map_err(|e| failure("temporary exits AST result type", e))?;
                let expected_type = if expected == "entry" || expected == "read" {
                    "Unit"
                } else if factories.contains(&expected) {
                    if f.map {
                        "Map<String, String>"
                    } else {
                        "MutableMap<String, String>"
                    }
                } else {
                    "Int"
                };
                need(
                    f.sources.slice(result_type.span()) == Ok(expected_type),
                    "temporary exits AST actual annotated result",
                    (expected, result_type),
                )?;
                for parameter in parameters {
                    let ty = f
                        .parsed
                        .ast()
                        .type_refs()
                        .get(parameter.type_ref)
                        .map_err(|e| failure("temporary exits AST parameter type", e))?;
                    need(
                        f.sources.slice(ty.span()) == Ok("String"),
                        "temporary exits AST actual String parameter",
                        ty,
                    )?;
                }
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
            "temporary exits AST unique function",
            (expected, &found),
        )?;
        functions.extend(found);
    }
    Ok(functions)
}

fn function<'a>(functions: &'a [Function], name: &str) -> Result<&'a Function, Failure> {
    functions
        .iter()
        .find(|f| f.name == name)
        .ok_or_else(|| failure("temporary exits AST required function", (name, functions)))
}

fn site(
    f: &Input,
    statement: StatementId,
    factory: &'static str,
) -> Result<(Site, Guard), Failure> {
    let node = f
        .parsed
        .ast()
        .statements()
        .get(statement)
        .map_err(|e| failure("temporary exits AST For", e))?;
    let Statement::For {
        binding: ForBinding::Destructuring { names, .. },
        source,
        body,
        ..
    } = node.payload()
    else {
        return Err(failure(
            "temporary exits AST actual destructuring For",
            node,
        ));
    };
    let [NameMarker::Present(key), NameMarker::Present(value)] = names.as_slice() else {
        return Err(failure(
            "temporary exits AST two actual binding markers",
            names,
        ));
    };
    need(
        *key != *value
            && f.sources.slice(*key) == Ok("key")
            && f.sources.slice(*value) == Ok("value"),
        "temporary exits AST ordered distinct binding markers",
        names,
    )?;
    let source_call = f.call(*source, factory, &[])?;
    let elements = f.elements(*body)?;
    let (local, transfer, consumer) = match (f.exit, elements) {
        (Exit::Return, [local, tail]) => {
            let transfer = f.statement_expression(*tail)?;
            let value = f.return_value(*tail)?;
            (
                *local,
                transfer,
                f.call(value, "operand", &["key", "value"])?,
            )
        }
        (Exit::Continue | Exit::Break, [local, read, tail]) => {
            let transfer = f.statement_expression(*tail)?;
            let (expression, _) = f.expression(transfer)?;
            need(
                matches!(
                    (f.exit, expression),
                    (Exit::Continue, Expression::Continue { .. })
                        | (Exit::Break, Expression::Break { .. })
                ),
                "temporary exits AST actual terminal loop transfer",
                (f.exit, expression),
            )?;
            let consumer = f.call(f.statement_expression(*read)?, "read", &["key", "value"])?;
            (*local, transfer, consumer)
        }
        _ => {
            return Err(failure(
                "temporary exits AST exact Guard-consumer-transfer body",
                elements,
            ));
        }
    };
    let [key_argument, value_argument] = consumer.arguments.as_slice() else {
        return Err(failure(
            "temporary exits AST two consumer arguments",
            &consumer,
        ));
    };
    let guard = guard(f, local)?;
    Ok((
        Site {
            factory,
            statement,
            span: node.span(),
            body: *body,
            source: *source,
            source_span: source_call.span,
            bindings: [*key, *value],
            consumer: consumer.target,
            read: consumer.expression,
            read_span: consumer.span,
            arguments: [*key_argument, *value_argument],
            transfer,
        },
        guard,
    ))
}

fn guard(f: &Input, statement: StatementId) -> Result<Guard, Failure> {
    let (marker, initializer) = f.local(statement, "local")?;
    let constructor = f.call(initializer, "Guard", &[])?;
    let mut found = Vec::new();
    for id in f.parsed.roots() {
        let node = f
            .parsed
            .ast()
            .items()
            .get(*id)
            .map_err(|e| failure("temporary exits AST class root", e))?;
        let Item::Classifier(class) = node.payload() else {
            continue;
        };
        let NameMarker::Present(name) = class.name else {
            continue;
        };
        if f.sources.slice(name) != Ok("Guard") {
            continue;
        }
        need(
            matches!(class.kind, ClassifierKind::Class { .. })
                && class.type_parameters.is_empty()
                && class.primary_constructor.is_none()
                && class.supertypes.is_empty(),
            "temporary exits AST plain no-field Guard class",
            class,
        )?;
        let body = class
            .body
            .as_ref()
            .ok_or_else(|| failure("temporary exits AST Guard class body", class))?;
        let [deinit_item] = body.members.as_slice() else {
            return Err(failure(
                "temporary exits AST sole Guard deinit member",
                body,
            ));
        };
        let deinit = f
            .parsed
            .ast()
            .items()
            .get(*deinit_item)
            .map_err(|e| failure("temporary exits AST deinit item", e))?;
        let Item::Deinit {
            body: deinit_body,
            right_paren_span: Some(_),
            ..
        } = deinit.payload()
        else {
            return Err(failure("temporary exits AST actual noarg deinit", deinit));
        };
        let [drop] = f.elements(*deinit_body)? else {
            return Err(failure(
                "temporary exits AST sole Guard deinit marker",
                f.elements(*deinit_body)?,
            ));
        };
        f.marker(*drop, "\"local-drop\"")?;
        need(
            contains(node.span(), deinit.span()) && contains(node.span(), name),
            "temporary exits AST real Guard/deinit source containment",
            (node, deinit),
        )?;
        found.push(Guard {
            marker,
            initializer,
            span: constructor.span,
            class_item: *id,
            class_marker: name,
            deinit_item: *deinit_item,
            deinit_span: deinit.span(),
            deinit_body: *deinit_body,
        });
    }
    need(
        found.len() == 1,
        "temporary exits AST unique actual Guard",
        &found,
    )?;
    found
        .pop()
        .ok_or_else(|| failure("temporary exits AST missing Guard", statement))
}
