//! Conditional-source AST anchors. No ownership facts or exit-shape oracle are fabricated.
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

    fn elements(&self, id: StatementId, control: bool) -> Result<&[StatementId], Failure> {
        let node = self
            .parsed
            .ast()
            .statements()
            .get(id)
            .map_err(|e| failure("AST block", e))?;
        match (node.payload(), control) {
            (Statement::Block { elements }, false)
            | (Statement::ControlBody { elements }, true) => Ok(elements),
            _ => Err(failure("AST actual block kind", node)),
        }
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

    fn named(&self, id: ExpressionId, name: &str) -> Result<(), Failure> {
        let (expression, span) = self.expression(id)?;
        need(
            matches!(expression, Expression::Name) && self.sources.slice(span) == Ok(name),
            "AST exact Name",
            (expression, span, name),
        )
    }

    fn literal(&self, id: ExpressionId, text: &str, string: bool) -> Result<(), Failure> {
        let (expression, span) = self.expression(id)?;
        need(
            (if string {
                matches!(expression, Expression::String { .. })
            } else {
                matches!(expression, Expression::Literal(_))
            }) && self.sources.slice(span) == Ok(text),
            "AST exact literal",
            (expression, span, text),
        )
    }

    fn call(
        &self,
        id: ExpressionId,
        expected: &str,
        arity: usize,
    ) -> Result<Vec<ExpressionId>, Failure> {
        let (expression, _) = self.expression(id)?;
        let Expression::Call {
            callee, arguments, ..
        } = expression
        else {
            return Err(failure("AST direct call", expression));
        };
        self.named(*callee, expected)?;
        need(arguments.len() == arity, "AST call arity", expression)?;
        Ok(arguments.iter().map(|a| a.value).collect())
    }

    fn local(&self, id: StatementId, expected: &str) -> Result<(Span, ExpressionId), Failure> {
        let node = self
            .parsed
            .ast()
            .statements()
            .get(id)
            .map_err(|e| failure("AST local statement", e))?;
        let Statement::LocalVariable { declaration } = node.payload() else {
            return Err(failure("AST local declaration", node));
        };
        let node = self
            .parsed
            .ast()
            .items()
            .get(*declaration)
            .map_err(|e| failure("AST local item", e))?;
        let Item::Variable {
            kind: VariableKind::Val,
            name: NameMarker::Present(name),
            initializer,
            ..
        } = node.payload()
        else {
            return Err(failure("AST val initializer", node));
        };
        need(
            self.sources.slice(*name) == Ok(expected),
            "AST local binding name",
            node,
        )?;
        Ok((*name, *initializer))
    }

    fn equality(
        &self,
        id: ExpressionId,
        left: &str,
        right: &str,
        string: bool,
    ) -> Result<(ExpressionId, ExpressionId), Failure> {
        let (expression, _) = self.expression(id)?;
        let Expression::Binary {
            left: lhs,
            operator: BinaryOperator::Equal,
            right: rhs,
            ..
        } = expression
        else {
            return Err(failure("AST actual equality", expression));
        };
        self.named(*lhs, left)?;
        self.literal(*rhs, right, string)?;
        Ok((*lhs, *rhs))
    }

    fn println(
        &self,
        statement: StatementId,
        expected: &str,
        control: bool,
    ) -> Result<(), Failure> {
        let statement = if control {
            let [statement] = self.elements(statement, true)? else {
                return Err(failure("AST sole marker statement", statement));
            };
            *statement
        } else {
            statement
        };
        let call = self.statement_expression(statement)?;
        let args = self.call(call, "println", 1)?;
        self.literal(args[0], expected, true)
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
pub(super) struct Branch {
    pub expression: ExpressionId,
    pub condition: ExpressionId,
    pub key: ExpressionId,
    pub literal: ExpressionId,
    pub body: StatementId,
    pub transfer: ExpressionId,
}

#[derive(Debug)]
pub(super) struct EntryCall {
    pub source_binding: Span,
    pub source_initializer: ExpressionId,
    pub result_binding: Span,
    pub call: ExpressionId,
    pub argument: ExpressionId,
    pub comparison: ExpressionId,
    pub result_reference: ExpressionId,
    pub result_literal: ExpressionId,
}

#[derive(Debug)]
pub(super) struct Anchors {
    pub functions: Vec<Function>,
    pub deinit: Span,
    pub guard_name: Span,
    pub statement: StatementId,
    pub body: StatementId,
    pub span: Span,
    pub source: ExpressionId,
    pub bindings: [Span; 2],
    pub local: Span,
    pub guard: ExpressionId,
    pub branches: Vec<Branch>,
    pub entry_calls: Vec<EntryCall>,
    pub operand_read: (ExpressionId, [ExpressionId; 2]),
    pub component_calls: Vec<(&'static str, ExpressionId, [ExpressionId; 2])>,
}

impl Anchors {
    pub fn functions_for_ssa(&self) -> Vec<(&str, Span)> {
        self.functions
            .iter()
            .map(|f| (f.name, f.span))
            .chain([("Guard.deinit", self.deinit)])
            .collect()
    }
}

pub(super) fn collect(f: &Input) -> Result<Anchors, Failure> {
    let ast = f.parsed.ast();
    let factory = if f.map { "makeMap" } else { "makeMutable" };
    need(
        f.parsed.roots().len() == 6,
        "AST exact roots",
        f.parsed.roots(),
    )?;
    let mut functions = Vec::new();
    for expected in ["entry", "scan", "read", "operand", factory] {
        let mut found = Vec::new();
        for id in f.parsed.roots() {
            let node = ast.items().get(*id).map_err(|e| failure("AST root", e))?;
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
                    _ => return Err(failure("AST function body", node)),
                };
                found.push(Function {
                    item: *id,
                    name: expected,
                    name_span: *name,
                    span: node.span(),
                    body,
                });
            }
        }
        need(found.len() == 1, "AST unique function", (&expected, &found))?;
        functions.extend(found);
    }
    let entry = &functions[0];
    let scan = &functions[1];
    let operand = &functions[3];
    let guards = f
        .parsed
        .roots()
        .iter()
        .filter_map(|id| {
            let node = ast.items().get(*id).ok()?;
            let Item::Classifier(class) = node.payload() else {
                return None;
            };
            matches!(class.name, NameMarker::Present(name) if f.sources.slice(name) == Ok("Guard"))
                .then_some(class)
        })
        .collect::<Vec<_>>();
    let [guard_class] = guards.as_slice() else {
        return Err(failure("AST unique Guard", guards));
    };
    let NameMarker::Present(guard_name) = guard_class.name else {
        return Err(failure("AST Guard name", guard_class));
    };
    let members = guard_class
        .body
        .as_ref()
        .map(|b| b.members.as_slice())
        .unwrap_or(&[]);
    let [deinit] = members else {
        return Err(failure("AST sole Guard deinit", members));
    };
    let deinit = ast
        .items()
        .get(*deinit)
        .map_err(|e| failure("AST deinit", e))?;
    need(
        matches!(deinit.payload(), Item::Deinit { .. })
            && f.sources.slice(deinit.span()) == Ok("deinit() { println(\"local-drop\") }"),
        "AST actual deinit",
        deinit,
    )?;
    let scan_elements = f.elements(scan.body, false)?;
    let [statement, after_loop, scan_return] = scan_elements else {
        return Err(failure("AST scan body", scan_elements));
    };
    let node = ast
        .statements()
        .get(*statement)
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
    need(
        ast.statements()
            .iter()
            .filter(|(_, n)| matches!(n.payload(), Statement::For { .. }))
            .count()
            == 1,
        "AST sole For",
        statement,
    )?;
    let [NameMarker::Present(key), NameMarker::Present(value)] = names.as_slice() else {
        return Err(failure("AST two bindings", names));
    };
    need(
        f.sources.slice(*key) == Ok("key") && f.sources.slice(*value) == Ok("value"),
        "AST key/value order",
        names,
    )?;
    f.named(*source, "source")?;
    let elements = f.elements(*body, false)?;
    let [local, if1, if2, if3, read_tail, body_tail] = elements else {
        return Err(failure("AST exact conditional body", elements));
    };
    let (local, guard) = f.local(*local, "local")?;
    f.call(guard, "Guard", 0)?;
    let mut branches = Vec::new();
    let mut component_calls = Vec::new();
    for (i, (statement, literal)) in [(*if1, "\"skip\""), (*if2, "\"stop\""), (*if3, "\"return\"")]
        .into_iter()
        .enumerate()
    {
        let expression = f.statement_expression(statement)?;
        let (node, _) = f.expression(expression)?;
        let Expression::If {
            condition,
            then_branch,
            else_span: None,
            else_branch: None,
            ..
        } = node
        else {
            return Err(failure("AST no-else outer If", node));
        };
        let (key, literal) = f.equality(*condition, "key", literal, true)?;
        let body_elements = f.elements(*then_branch, true)?;
        let expected_len = [2, 3, 1][i];
        need(
            body_elements.len() == expected_len,
            "AST transfer branch body",
            body_elements,
        )?;
        let transfer = f.statement_expression(body_elements[expected_len - 1])?;
        let (jump, _) = f.expression(transfer)?;
        match (i, jump) {
            (0, Expression::Continue { .. }) => {
                f.println(body_elements[0], "\"continue-edge\"", false)?
            }
            (1, Expression::Break { .. }) => {
                f.println(body_elements[0], "\"break-edge\"", false)?;
                let call = f.statement_expression(body_elements[1])?;
                component_calls.push(component_call(f, call, "read")?);
            }
            (
                2,
                Expression::Return {
                    value: Some(call), ..
                },
            ) => component_calls.push(component_call(f, *call, "operand")?),
            _ => return Err(failure("AST actual continue/break/return", jump)),
        }
        branches.push(Branch {
            expression,
            condition: *condition,
            key,
            literal,
            body: *then_branch,
            transfer,
        });
    }
    component_calls.push(component_call(
        f,
        f.statement_expression(*read_tail)?,
        "read",
    )?);
    f.println(*body_tail, "\"body-tail\"", false)?;
    f.println(*after_loop, "\"after-loop\"", false)?;
    scalar_return(f, *scan_return, "0")?;
    let operand_body = f.elements(operand.body, false)?;
    let [operand_marker, operand_read, operand_return] = operand_body else {
        return Err(failure("AST operand body", operand_body));
    };
    f.println(*operand_marker, "\"operand\"", false)?;
    // This read uses operand's Borrow parameters, not the For bindings.
    let (_, operand_read_call, operand_read_arguments) =
        component_call(f, f.statement_expression(*operand_read)?, "read")?;
    scalar_return(f, *operand_return, "7")?;
    let entry_body = f.elements(entry.body, false)?;
    need(
        entry_body.len() == 20,
        "AST entry five exact groups",
        entry_body,
    )?;
    let mut entry_calls = Vec::new();
    for (i, prefix) in ["continue", "break", "return", "fallthrough", "empty"]
        .into_iter()
        .enumerate()
    {
        let group = &entry_body[i * 4..i * 4 + 4];
        f.println(group[0], &format!("\"case-{prefix}\""), false)?;
        let (source_binding, source_initializer) = f.local(group[1], &format!("{prefix}Source"))?;
        if i < 4 {
            let args = f.call(source_initializer, factory, 1)?;
            f.literal(
                args[0],
                ["\"skip\"", "\"stop\"", "\"return\"", "\"pass\""][i],
                true,
            )?;
        } else {
            f.call(
                source_initializer,
                if f.map { "mapOf" } else { "mutableMapOf" },
                0,
            )?;
        }
        let (result_binding, call) = f.local(group[2], &format!("{prefix}Result"))?;
        let args = f.call(call, "scan", 1)?;
        f.named(args[0], &format!("{prefix}Source"))?;
        let comparison = f.statement_expression(group[3])?;
        let (node, _) = f.expression(comparison)?;
        let Expression::If {
            condition,
            then_branch,
            else_span: Some(_),
            else_branch: Some(other),
            ..
        } = node
        else {
            return Err(failure("AST exact result If/else", node));
        };
        let expected = if i == 2 { "7" } else { "0" };
        let (result_reference, result_literal) =
            f.equality(*condition, &format!("{prefix}Result"), expected, false)?;
        f.println(
            *then_branch,
            &format!("\"{prefix}-result-{expected}\""),
            true,
        )?;
        f.println(*other, &format!("\"unexpected-{prefix}-result\""), true)?;
        entry_calls.push(EntryCall {
            source_binding,
            source_initializer,
            result_binding,
            call,
            argument: args[0],
            comparison: *condition,
            result_reference,
            result_literal,
        });
    }
    Ok(Anchors {
        functions,
        deinit: deinit.span(),
        guard_name,
        statement: *statement,
        body: *body,
        span: node.span(),
        source: *source,
        bindings: [*key, *value],
        local,
        guard,
        branches,
        entry_calls,
        component_calls,
        operand_read: (operand_read_call, operand_read_arguments),
    })
}

fn component_call(
    f: &Input,
    call: ExpressionId,
    name: &'static str,
) -> Result<(&'static str, ExpressionId, [ExpressionId; 2]), Failure> {
    let args = f.call(call, name, 2)?;
    f.named(args[0], "key")?;
    f.named(args[1], "value")?;
    Ok((name, call, [args[0], args[1]]))
}

fn scalar_return(f: &Input, statement: StatementId, expected: &str) -> Result<(), Failure> {
    let expression = f.statement_expression(statement)?;
    let (node, _) = f.expression(expression)?;
    let Expression::Return {
        value: Some(value), ..
    } = node
    else {
        return Err(failure("AST scalar Return", node));
    };
    f.literal(*value, expected, false)
}
