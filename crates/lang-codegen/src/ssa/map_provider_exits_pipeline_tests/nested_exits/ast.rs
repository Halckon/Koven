//! 同根双层 For 内层退出的真实 AST 身份；独立于正常耗尽和单层退出查询。
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
            .map_err(|e| failure("nested exits source", e))?;
        let lexed = lex(&sources, id).map_err(|e| failure("lex infrastructure", e))?;
        clean(&sources, "lex diagnostics", lexed.diagnostics())?;
        let parsed =
            parse_file(&sources, &lexed).map_err(|e| failure("parse infrastructure", e))?;
        clean(&sources, "parse diagnostics", parsed.diagnostics())?;
        need(
            parsed.package().is_none(),
            "nested exits AST default package",
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
            .map_err(|e| failure("nested exits AST expression", e))?;
        Ok((node.payload(), node.span()))
    }

    fn elements(&self, id: StatementId) -> Result<&[StatementId], Failure> {
        let node = self
            .parsed
            .ast()
            .statements()
            .get(id)
            .map_err(|e| failure("nested exits AST block", e))?;
        let Statement::Block { elements } = node.payload() else {
            return Err(failure("nested exits AST actual Block", node));
        };
        Ok(elements)
    }

    fn statement_expression(&self, id: StatementId) -> Result<ExpressionId, Failure> {
        let node = self
            .parsed
            .ast()
            .statements()
            .get(id)
            .map_err(|e| failure("nested exits AST statement", e))?;
        let Statement::Expression { expression } = node.payload() else {
            return Err(failure("nested exits AST expression statement", node));
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
            return Err(failure("nested exits AST direct Call", expression));
        };
        self.name(*callee, name)?;
        need(
            type_arguments.is_empty()
                && arguments.len() == arity
                && arguments
                    .iter()
                    .all(|a| a.named_prefix.is_none() && a.mode_marker.is_none()),
            "nested exits AST unmarked positional call arity",
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
            "nested exits AST direct name reference",
            (id, expression, span, expected),
        )
    }

    fn local(&self, id: StatementId, expected: &str) -> Result<(Span, ExpressionId), Failure> {
        let node = self
            .parsed
            .ast()
            .statements()
            .get(id)
            .map_err(|e| failure("nested exits AST local", e))?;
        let Statement::LocalVariable { declaration } = node.payload() else {
            return Err(failure("nested exits AST local declaration", node));
        };
        let item = self
            .parsed
            .ast()
            .items()
            .get(*declaration)
            .map_err(|e| failure("nested exits AST variable", e))?;
        let Item::Variable {
            kind: VariableKind::Val,
            name: NameMarker::Present(name),
            initializer,
            ..
        } = item.payload()
        else {
            return Err(failure("nested exits AST val declaration", item));
        };
        need(
            self.sources.slice(*name) == Ok(expected),
            "nested exits AST actual local marker",
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
            "nested exits AST exact literal",
            (id, expression, span, expected),
        )
    }

    fn marker(&self, id: StatementId, expected: &str) -> Result<(), Failure> {
        let call = self.direct_call(self.statement_expression(id)?, "println", 1)?;
        let [argument] = call.arguments.as_slice() else {
            return Err(failure("nested exits AST println argument", call));
        };
        self.literal(*argument, expected, true)
    }

    fn scalar_return(&self, id: StatementId, expected: &str) -> Result<(), Failure> {
        let id = self.statement_expression(id)?;
        let (expression, _) = self.expression(id)?;
        let Expression::Return {
            value: Some(value), ..
        } = expression
        else {
            return Err(failure("nested exits AST scalar return", expression));
        };
        self.literal(*value, expected, false)
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
    pub binding_markers: Vec<Span>,
}

#[derive(Debug)]
pub(super) struct Size {
    pub expression: ExpressionId,
    pub receiver: ExpressionId,
    pub root_marker: Span,
    pub span: Span,
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
    pub reads: Vec<Read>,
    pub operand: Option<Call>,
    pub scans: [Call; 2],
    pub factory: Call,
    pub locals: [(Span, ExpressionId); 2],
    pub sizes: Vec<Size>,
    pub guards: [Guard; 2],
    pub transfer: ExpressionId,
    pub return_checks: Vec<ReturnCheck>,
}

impl Anchors {
    pub fn functions_for_ssa(&self) -> Vec<(&str, Span)> {
        self.functions
            .iter()
            .map(|f| (f.name, f.span))
            .chain([
                ("OuterGuard.deinit", self.guards[0].deinit_span),
                ("InnerGuard.deinit", self.guards[1].deinit_span),
            ])
            .collect()
    }

    pub fn function(&self, name: &str) -> Option<&Function> {
        self.functions.iter().find(|f| f.name == name)
    }

    pub fn parameter(&self) -> Option<Span> {
        self.function("scan")
            .and_then(|f| match f.parameters.as_slice() {
                [parameter] => Some(*parameter),
                _ => None,
            })
    }
}

pub(super) fn collect(f: &Input) -> Result<Anchors, Failure> {
    let functions = functions(f)?;
    let scan = function(&functions, "scan")?;
    let scan_elements = f.elements(scan.body)?;
    let (outer, scan_sizes) = match (f.exit, scan_elements) {
        (Exit::Return, [outer, exhausted, tail]) => {
            f.marker(*exhausted, "\"exhausted\"")?;
            f.scalar_return(*tail, "0")?;
            (*outer, Vec::new())
        }
        (Exit::Continue | Exit::Break, [outer, exhausted, size_two, size_zero]) => {
            f.marker(*exhausted, "\"exhausted\"")?;
            (*outer, vec![*size_two, *size_zero])
        }
        _ => return Err(failure("nested exits AST exact scan body", scan_elements)),
    };
    let outer = site(f, outer, ["outerKey", "outerValue"])?;
    let outer_elements = f.elements(outer.body)?;
    let (outer_guard, outer_read, inner, after_inner) = match (f.exit, outer_elements) {
        (Exit::Return, [guard, marker, read, inner]) => {
            f.marker(*marker, "\"outer\"")?;
            (*guard, *read, *inner, None)
        }
        (Exit::Continue | Exit::Break, [guard, marker, read, inner, after, read_after]) => {
            f.marker(*marker, "\"outer\"")?;
            f.marker(*after, "\"after-inner\"")?;
            (*guard, *read, *inner, Some(*read_after))
        }
        _ => return Err(failure("nested exits AST exact outer body", outer_elements)),
    };
    let inner = site(f, inner, ["innerKey", "innerValue"])?;
    let inner_elements = f.elements(inner.body)?;
    let mut reads = vec![read(f, outer_read, &outer.bindings)?];
    let (inner_guard, transfer, operand) = match (f.exit, inner_elements) {
        (Exit::Return, [guard, marker, tail]) => {
            f.marker(*marker, "\"pair\"")?;
            let transfer = f.statement_expression(*tail)?;
            let (expression, _) = f.expression(transfer)?;
            let Expression::Return {
                value: Some(value), ..
            } = expression
            else {
                return Err(failure("nested exits AST actual inner return", expression));
            };
            let call = f.call(
                *value,
                "operand",
                &["outerKey", "outerValue", "innerKey", "innerValue"],
            )?;
            let operand = function(&functions, "operand")?;
            let [marker, outer_read, inner_read, tail] = f.elements(operand.body)? else {
                return Err(failure(
                    "nested exits AST exact operand body",
                    f.elements(operand.body)?,
                ));
            };
            f.marker(*marker, "\"operand\"")?;
            f.scalar_return(*tail, "7")?;
            let [outer_key, outer_value, inner_key, inner_value] = operand.parameters.as_slice()
            else {
                return Err(failure(
                    "nested exits AST four operand roots",
                    &operand.parameters,
                ));
            };
            // operand 内 read 的根是本函数参数；调用实参仍是 scan 四个 For binding。
            reads.push(read(f, *outer_read, &[*outer_key, *outer_value])?);
            reads.push(read(f, *inner_read, &[*inner_key, *inner_value])?);
            (*guard, transfer, Some(call))
        }
        (Exit::Continue | Exit::Break, [guard, marker, outer_read, inner_read, tail]) => {
            f.marker(*marker, "\"pair\"")?;
            reads.push(read(f, *outer_read, &outer.bindings)?);
            reads.push(read(f, *inner_read, &inner.bindings)?);
            let transfer = f.statement_expression(*tail)?;
            let (expression, _) = f.expression(transfer)?;
            need(
                matches!(
                    (f.exit, expression),
                    (Exit::Continue, Expression::Continue { .. })
                        | (Exit::Break, Expression::Break { .. })
                ),
                "nested exits AST actual inner terminal transfer",
                (f.exit, expression),
            )?;
            let after = after_inner.ok_or_else(|| {
                failure("nested exits AST missing after-inner read", outer_elements)
            })?;
            reads.push(read(f, after, &outer.bindings)?);
            (*guard, transfer, None)
        }
        _ => return Err(failure("nested exits AST exact inner body", inner_elements)),
    };
    let guards = [
        guard(f, outer_guard, "outerGuard", "OuterGuard", "\"outer-drop\"")?,
        guard(f, inner_guard, "innerGuard", "InnerGuard", "\"inner-drop\"")?,
    ];
    check_sites(f, &outer, &inner, &reads, transfer, operand.as_ref())?;
    need(
        guards[0].class_item != guards[1].class_item
            && guards[0].deinit_item != guards[1].deinit_item
            && guards[0].marker != guards[1].marker
            && guards[0].initializer != guards[1].initializer,
        "nested exits AST distinct Guard classes/deinits/locals",
        &guards,
    )?;
    let entry = checks::entry(f, &functions, &outer, &scan_sizes)?;
    Ok(Anchors {
        functions,
        sites: [outer, inner],
        reads,
        operand,
        scans: entry.scans,
        factory: entry.factory,
        locals: entry.locals,
        sizes: entry.sizes,
        guards,
        transfer,
        return_checks: entry.return_checks,
    })
}

fn functions(f: &Input) -> Result<Vec<Function>, Failure> {
    let factory = if f.map { "makeMap" } else { "makeMutable" };
    let mut expected = vec![
        ("entry", &[][..]),
        ("read", &["key", "value"][..]),
        ("scan", &["source"][..]),
        (factory, &[][..]),
    ];
    if f.exit == Exit::Return {
        expected.push((
            "operand",
            &["outerKey", "outerValue", "innerKey", "innerValue"][..],
        ));
    }
    need(
        f.parsed.roots().len() == expected.len() + 2,
        "nested exits AST exact function and two class roots",
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
                .map_err(|e| failure("nested exits AST function root", e))?;
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
                    _ => return Err(failure("nested exits AST actual function block", node)),
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
                    "nested exits AST original Borrow signature and provider identity",
                    node,
                )?;
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
            "nested exits AST unique function",
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
        .ok_or_else(|| failure("nested exits AST required function", (name, functions)))
}

fn site(f: &Input, statement: StatementId, expected: [&str; 2]) -> Result<Site, Failure> {
    let node = f
        .parsed
        .ast()
        .statements()
        .get(statement)
        .map_err(|e| failure("nested exits AST For", e))?;
    let Statement::For {
        binding: ForBinding::Destructuring { names, .. },
        source,
        body,
        ..
    } = node.payload()
    else {
        return Err(failure("nested exits AST actual destructuring For", node));
    };
    let [NameMarker::Present(key), NameMarker::Present(value)] = names.as_slice() else {
        return Err(failure(
            "nested exits AST two actual binding markers",
            names,
        ));
    };
    need(
        *key != *value
            && f.sources.slice(*key) == Ok(expected[0])
            && f.sources.slice(*value) == Ok(expected[1]),
        "nested exits AST ordered distinct binding markers",
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

fn read(f: &Input, statement: StatementId, markers: &[Span; 2]) -> Result<Read, Failure> {
    let expected = markers
        .iter()
        .map(|marker| {
            f.sources
                .slice(*marker)
                .map_err(|e| failure("nested exits AST read root spelling", e))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let call = f.call(f.statement_expression(statement)?, "read", &expected)?;
    Ok(Read {
        call,
        binding_markers: markers.to_vec(),
    })
}

fn guard(
    f: &Input,
    statement: StatementId,
    local_name: &str,
    class_name: &'static str,
    drop_marker: &str,
) -> Result<Guard, Failure> {
    let (marker, initializer) = f.local(statement, local_name)?;
    let constructor = f.call(initializer, class_name, &[])?;
    let mut found = Vec::new();
    for id in f.parsed.roots() {
        let node = f
            .parsed
            .ast()
            .items()
            .get(*id)
            .map_err(|e| failure("nested exits AST class root", e))?;
        let Item::Classifier(class) = node.payload() else {
            continue;
        };
        let NameMarker::Present(name) = class.name else {
            continue;
        };
        if f.sources.slice(name) != Ok(class_name) {
            continue;
        }
        need(
            matches!(class.kind, ClassifierKind::Class { .. })
                && class.type_parameters.is_empty()
                && class.primary_constructor.is_none()
                && class.supertypes.is_empty(),
            "nested exits AST plain no-field Guard class",
            class,
        )?;
        let body = class
            .body
            .as_ref()
            .ok_or_else(|| failure("nested exits AST Guard class body", class))?;
        let [deinit_item] = body.members.as_slice() else {
            return Err(failure("nested exits AST unique Guard deinit member", body));
        };
        let deinit = f
            .parsed
            .ast()
            .items()
            .get(*deinit_item)
            .map_err(|e| failure("nested exits AST deinit item", e))?;
        let Item::Deinit {
            body: deinit_body,
            right_paren_span: Some(_),
            ..
        } = deinit.payload()
        else {
            return Err(failure("nested exits AST actual noarg deinit", deinit));
        };
        let [drop] = f.elements(*deinit_body)? else {
            return Err(failure(
                "nested exits AST sole Guard deinit marker",
                f.elements(*deinit_body)?,
            ));
        };
        f.marker(*drop, drop_marker)?;
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
        "nested exits AST unique actual Guard",
        (class_name, &found),
    )?;
    found
        .pop()
        .ok_or_else(|| failure("nested exits AST missing Guard", class_name))
}

fn check_sites(
    f: &Input,
    outer: &Site,
    inner: &Site,
    reads: &[Read],
    transfer: ExpressionId,
    operand: Option<&Call>,
) -> Result<(), Failure> {
    let all_for = f
        .parsed
        .ast()
        .statements()
        .iter()
        .filter_map(|(id, n)| matches!(n.payload(), Statement::For { .. }).then_some(id))
        .collect::<Vec<_>>();
    let (_, transfer_span) = f.expression(transfer)?;
    need(
        all_for.len() == 2
            && all_for.contains(&outer.statement)
            && all_for.contains(&inner.statement)
            && outer.statement != inner.statement
            && outer.source != inner.source
            && outer.source_span != inner.source_span
            && contains(outer.span, inner.span)
            && contains(inner.span, transfer_span)
            && outer.bindings.iter().all(|s| !inner.bindings.contains(s))
            && reads
                .first()
                .is_some_and(|r| r.call.span.end() <= inner.span.start()),
        "nested exits AST parent/child For, two source expressions and four bindings",
        (&all_for, outer, inner, reads, transfer),
    )?;
    let read_ids = f
        .parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| {
            let Expression::Call { callee, .. } = node.payload() else {
                return None;
            };
            let (target, span) = f.expression(*callee).ok()?;
            (matches!(target, Expression::Name) && f.sources.slice(span) == Ok("read"))
                .then_some(id)
        })
        .collect::<Vec<_>>();
    need(
        read_ids.len() == reads.len()
            && reads
                .iter()
                .all(|read| read_ids.contains(&read.call.expression)),
        "nested exits AST all actual read calls accounted",
        (&read_ids, reads),
    )?;
    match (f.exit, reads, operand) {
        (Exit::Continue | Exit::Break, [_, outer_read, inner_read, after], None) => need(
            contains(inner.span, outer_read.call.span)
                && contains(inner.span, inner_read.call.span)
                && inner_read.call.span.end() <= transfer_span.start()
                && inner.span.end() <= after.call.span.start(),
            "nested exits AST two inner reads before transfer and outer read after inner",
            reads,
        ),
        (Exit::Return, [scan_read, outer_read, inner_read], Some(call)) => need(
            contains(outer.span, scan_read.call.span)
                && contains(transfer_span, call.span)
                && call.arguments.len() == 4
                && !contains(outer.span, outer_read.call.span)
                && !contains(outer.span, inner_read.call.span),
            "nested exits AST scan read and four-argument returned operand",
            (reads, call),
        ),
        _ => Err(failure(
            "nested exits AST exact read and operand counts",
            (f.exit, reads, operand),
        )),
    }
}
