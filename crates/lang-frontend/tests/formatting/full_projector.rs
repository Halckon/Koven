//! Exhaustive AST-level normalized projector with per-arm `Span` containment verification
//! for SPEC-0487 (F4c Formatter round-trip expansion).

use lang_frontend::{
    ast::{ExpressionId, ItemId, StatementId, TypeRefId},
    lexer::lex,
    parser::{
        BorrowReturnSource, ClassifierDeclaration, ClassifierKind, DeclarationModifiers,
        Expression, ForBinding, FunctionBody, FunctionForm, FunctionResultSource, Item, NameMarker,
        ParameterModeMarker, Statement, StringPart, SyntaxAst, TypeRef, VariableKind,
        VisibilityModifier, WhenCondition, parse_file,
    },
    source::{SourceId, SourceMap, Span},
};

use super::loaded_source;

pub type Field = (String, String);
pub type Projection = Vec<Field>;
pub type Difference<'a> = (Option<&'a Field>, Option<&'a Field>);

struct Projector<'a> {
    sources: &'a SourceMap,
    source_id: SourceId,
    source_len: usize,
    ast: &'a SyntaxAst,
    fields: Projection,
}

impl<'a> Projector<'a> {
    fn check_span(&self, span: Span, parent: Option<Span>) -> Result<(), String> {
        if span.source_id() != self.source_id {
            return Err(format!("foreign SourceId in span {span:?}"));
        }
        if span.start() > span.end() || span.end() > self.source_len {
            return Err(format!(
                "out-of-bounds span {span:?} for len {}",
                self.source_len
            ));
        }
        self.sources
            .slice(span)
            .map_err(|e| format!("invalid UTF-8 span {span:?}: {e}"))?;
        if let Some(outer) = parent
            && (outer.source_id() != span.source_id()
                || outer.start() > span.start()
                || span.end() > outer.end())
        {
            return Err(format!("child span {span:?} escapes parent {outer:?}"));
        }
        Ok(())
    }

    fn slice(&self, span: Span, parent: Option<Span>) -> Result<&'a str, String> {
        self.check_span(span, parent)?;
        Ok(self.sources.slice(span).expect("validated span"))
    }

    fn marker(&self, marker: NameMarker, parent: Span, path: &str) -> Result<&'a str, String> {
        match marker {
            NameMarker::Present(span) => {
                if span.is_empty() {
                    return Err(format!("empty NameMarker::Present at {path}"));
                }
                self.slice(span, Some(parent))
            }
            other => Err(format!("non-present NameMarker {other:?} at {path}")),
        }
    }

    fn push(&mut self, path: impl Into<String>, value: impl Into<String>) {
        self.fields.push((path.into(), value.into()));
    }

    fn vis(&self, vis: VisibilityModifier, parent: Span) -> Result<&'static str, String> {
        let (span, label) = match vis {
            VisibilityModifier::Public(s) => (s, "public"),
            VisibilityModifier::Internal(s) => (s, "internal"),
            VisibilityModifier::Private(s) => (s, "private"),
        };
        self.check_span(span, Some(parent))?;
        Ok(label)
    }

    fn mode(&self, mode: ParameterModeMarker, parent: Span) -> Result<&'static str, String> {
        let (span, label) = match mode {
            ParameterModeMarker::Own(s) => (s, "own"),
            ParameterModeMarker::Borrow(s) => (s, "borrow"),
            ParameterModeMarker::Inout(s) => (s, "inout"),
        };
        self.check_span(span, Some(parent))?;
        Ok(label)
    }

    fn project_modifiers(
        &mut self,
        mods: DeclarationModifiers,
        parent: Span,
        path: &str,
    ) -> Result<(), String> {
        if let Some(v) = mods.visibility {
            let label = self.vis(v, parent)?;
            self.push(format!("{path}.visibility"), label);
        }
        if let Some(ov) = mods.override_span {
            self.check_span(ov, Some(parent))?;
            self.push(format!("{path}.override"), "true");
        }
        if let Some(rm) = mods.receiver_mode {
            let label = self.mode(rm, parent)?;
            self.push(format!("{path}.receiver_mode"), label);
        }
        Ok(())
    }

    fn project_type(&mut self, id: TypeRefId, parent: Span, path: &str) -> Result<(), String> {
        let node = self
            .ast
            .type_refs()
            .get(id)
            .map_err(|_| format!("dangling TypeRefId at {path}"))?;
        let span = node.span();
        self.check_span(span, Some(parent))?;
        match node.payload() {
            TypeRef::Error => return Err(format!("TypeRef::Error at {path}")),
            TypeRef::Qualified {
                segments,
                nullable_span,
            } => {
                self.push(format!("{path}.kind"), "qualified");
                for (i, seg) in segments.iter().enumerate() {
                    if !seg.argument_errors.is_empty() {
                        return Err(format!("type argument errors at {path}.seg[{i}]"));
                    }
                    let name = self.slice(seg.name_span, Some(span))?;
                    self.push(format!("{path}.seg[{i}].name"), name);
                    for (j, &arg) in seg.arguments.iter().enumerate() {
                        self.project_type(arg, span, &format!("{path}.seg[{i}].arg[{j}]"))?;
                    }
                }
                let nullable = if let Some(q) = nullable_span {
                    self.check_span(*q, Some(span))?;
                    "true"
                } else {
                    "false"
                };
                self.push(format!("{path}.nullable"), nullable);
            }
            TypeRef::Function {
                move_span,
                parameters,
                arrow_span,
                return_type,
            } => {
                self.check_span(*arrow_span, Some(span))?;
                self.push(format!("{path}.kind"), "function");
                let is_move = if let Some(m) = move_span {
                    self.check_span(*m, Some(span))?;
                    "true"
                } else {
                    "false"
                };
                self.push(format!("{path}.move"), is_move);
                for (i, param) in parameters.iter().enumerate() {
                    self.check_span(param.span, Some(span))?;
                    if param.trailing_error.is_some() {
                        return Err(format!("trailing_error in function type param at {path}"));
                    }
                    if let Some(m) = param.mode_marker {
                        let label = self.mode(m, param.span)?;
                        self.push(format!("{path}.param[{i}].mode"), label);
                    }
                    self.project_type(param.type_ref, param.span, &format!("{path}.param[{i}]"))?;
                }
                self.project_type(*return_type, span, &format!("{path}.ret"))?;
            }
        }
        Ok(())
    }

    fn project_item(&mut self, id: ItemId, parent: Option<Span>, path: &str) -> Result<(), String> {
        let node = self
            .ast
            .items()
            .get(id)
            .map_err(|_| format!("dangling ItemId at {path}"))?;
        let span = node.span();
        self.check_span(span, parent)?;
        match node.payload() {
            Item::Error => return Err(format!("Item::Error at {path}")),
            Item::Modified {
                modifiers,
                declaration,
            } => {
                self.push(format!("{path}.kind"), "modified");
                self.project_modifiers(*modifiers, span, &format!("{path}.mods"))?;
                self.project_item(*declaration, Some(span), &format!("{path}.decl"))?;
            }
            Item::Variable {
                kind,
                name,
                colon_span,
                type_ref,
                equals_span,
                initializer,
            } => {
                let kind_label = match kind {
                    VariableKind::Val => "val",
                    VariableKind::Var => "var",
                    VariableKind::BorrowVal(s) => {
                        self.check_span(*s, Some(span))?;
                        "borrow_val"
                    }
                };
                self.check_span(*equals_span, Some(span))?;
                self.push(format!("{path}.kind"), kind_label);
                let n = self.marker(*name, span, &format!("{path}.name"))?;
                self.push(format!("{path}.name"), n);
                if let Some(c) = colon_span {
                    self.check_span(*c, Some(span))?;
                }
                if let Some(tr) = type_ref {
                    self.project_type(*tr, span, &format!("{path}.type"))?;
                }
                self.project_expr(*initializer, span, &format!("{path}.init"))?;
            }
            Item::Constant {
                const_span,
                val_marker,
                name,
                colon_span,
                type_ref,
                equals_span,
                initializer,
            } => {
                self.check_span(*const_span, Some(span))?;
                self.marker(*val_marker, span, &format!("{path}.val"))?;
                self.check_span(*equals_span, Some(span))?;
                self.push(format!("{path}.kind"), "const_val");
                let n = self.marker(*name, span, &format!("{path}.name"))?;
                self.push(format!("{path}.name"), n);
                if let Some(c) = colon_span {
                    self.check_span(*c, Some(span))?;
                }
                if let Some(tr) = type_ref {
                    self.project_type(*tr, span, &format!("{path}.type"))?;
                }
                self.project_expr(*initializer, span, &format!("{path}.init"))?;
            }
            Item::Function {
                name,
                extension_receiver,
                type_parameters,
                type_parameter_list_span,
                parameters,
                form,
            } => {
                self.push(format!("{path}.kind"), "fun");
                let n = self.marker(*name, span, &format!("{path}.name"))?;
                self.push(format!("{path}.name"), n);
                if let Some(ext) = extension_receiver {
                    self.check_span(ext.dot_span, Some(span))?;
                    self.project_type(ext.type_ref, span, &format!("{path}.ext_recv"))?;
                }
                if let Some(tps) = type_parameter_list_span {
                    self.check_span(*tps, Some(span))?;
                }
                for (i, tp) in type_parameters.iter().enumerate() {
                    self.check_span(tp.span, Some(span))?;
                    let tpn = self.marker(tp.name, tp.span, &format!("{path}.tparam[{i}]"))?;
                    self.push(format!("{path}.tparam[{i}].name"), tpn);
                    if let Some(b) = tp.bound {
                        self.project_type(b, tp.span, &format!("{path}.tparam[{i}].bound"))?;
                    }
                }
                for (i, p) in parameters.iter().enumerate() {
                    self.check_span(p.span, Some(span))?;
                    self.check_span(p.colon_span, Some(p.span))?;
                    if let Some(m) = p.mode_marker {
                        let label = self.mode(m, p.span)?;
                        self.push(format!("{path}.param[{i}].mode"), label);
                    }
                    let pn = self.marker(p.name, p.span, &format!("{path}.param[{i}].name"))?;
                    self.push(format!("{path}.param[{i}].name"), pn);
                    self.project_type(p.type_ref, p.span, &format!("{path}.param[{i}].type"))?;
                }
                self.project_function_form(*form, span, &format!("{path}.form"))?;
            }
            Item::Classifier(classifier) => {
                self.project_classifier(classifier, span, path)?;
            }
            Item::Companion(companion) => {
                self.check_span(companion.companion_span, Some(span))?;
                self.check_span(companion.body.left_brace_span, Some(span))?;
                let rbrace = companion
                    .body
                    .right_brace_span
                    .ok_or_else(|| format!("unclosed companion body at {path}"))?;
                self.check_span(rbrace, Some(span))?;
                self.push(format!("{path}.kind"), "companion");
                for (i, &m) in companion.body.members.iter().enumerate() {
                    self.project_item(m, Some(span), &format!("{path}.member[{i}]"))?;
                }
            }
            Item::Deinit {
                deinit_span,
                left_paren_span,
                right_paren_span,
                body,
            } => {
                self.check_span(*deinit_span, Some(span))?;
                self.check_span(*left_paren_span, Some(span))?;
                let rp = right_paren_span.ok_or_else(|| format!("unclosed deinit() at {path}"))?;
                self.check_span(rp, Some(span))?;
                self.push(format!("{path}.kind"), "deinit");
                self.project_stmt(*body, span, &format!("{path}.body"))?;
            }
        }
        Ok(())
    }

    fn project_function_form(
        &mut self,
        form: FunctionForm,
        parent: Span,
        path: &str,
    ) -> Result<(), String> {
        match form {
            FunctionForm::ImplicitUnitAbsent => self.push(path, "implicit-unit-absent"),
            FunctionForm::ImplicitUnitBlock(b) => {
                self.push(path, "implicit-unit-block");
                self.project_stmt(b, parent, &format!("{path}.block"))?;
            }
            FunctionForm::Explicit {
                colon_span,
                type_ref,
                result_source,
                body,
            } => {
                self.check_span(colon_span, Some(parent))?;
                self.push(path, "explicit");
                self.project_type(type_ref, parent, &format!("{path}.ret"))?;
                if let Some(rs) = result_source {
                    let (tag, src) = match rs {
                        FunctionResultSource::Borrow(b) => {
                            self.check_span(b.borrow_span, Some(parent))?;
                            self.check_span(b.from_span, Some(parent))?;
                            ("borrow", b.source)
                        }
                        FunctionResultSource::Carrier(c) => {
                            self.check_span(c.from_span, Some(parent))?;
                            ("carrier", c.source)
                        }
                    };
                    self.push(format!("{path}.source.kind"), tag);
                    match src {
                        BorrowReturnSource::Parameter(m) => {
                            let n = self.marker(m, parent, &format!("{path}.source.param"))?;
                            self.push(format!("{path}.source.target"), format!("param:{n}"));
                        }
                        BorrowReturnSource::Receiver(s) => {
                            self.check_span(s, Some(parent))?;
                            self.push(format!("{path}.source.target"), "this");
                        }
                    }
                }
                match body {
                    FunctionBody::Absent => self.push(format!("{path}.body"), "absent"),
                    FunctionBody::Expression {
                        equals_span,
                        expression,
                    } => {
                        self.check_span(equals_span, Some(parent))?;
                        self.push(format!("{path}.body"), "expr");
                        self.project_expr(expression, parent, &format!("{path}.body.expr"))?;
                    }
                    FunctionBody::Block(b) => {
                        self.push(format!("{path}.body"), "block");
                        self.project_stmt(b, parent, &format!("{path}.body.block"))?;
                    }
                }
            }
        }
        Ok(())
    }

    fn project_classifier(
        &mut self,
        c: &ClassifierDeclaration,
        span: Span,
        path: &str,
    ) -> Result<(), String> {
        let tag = match c.kind {
            ClassifierKind::ValueClass {
                value_span,
                class_span,
            } => {
                self.check_span(value_span, Some(span))?;
                self.check_span(class_span, Some(span))?;
                "value_class"
            }
            ClassifierKind::Class { class_span } => {
                self.check_span(class_span, Some(span))?;
                "class"
            }
            ClassifierKind::Interface { interface_span } => {
                self.check_span(interface_span, Some(span))?;
                "interface"
            }
            ClassifierKind::EnumClass {
                enum_span,
                class_span,
            } => {
                self.check_span(enum_span, Some(span))?;
                self.check_span(class_span, Some(span))?;
                "enum_class"
            }
            ClassifierKind::Object { object_span } => {
                self.check_span(object_span, Some(span))?;
                "object"
            }
        };
        self.push(format!("{path}.kind"), tag);
        let name = self.marker(c.name, span, &format!("{path}.name"))?;
        self.push(format!("{path}.name"), name);
        for (i, tp) in c.type_parameters.iter().enumerate() {
            self.check_span(tp.span, Some(span))?;
            let tpn = self.marker(tp.name, tp.span, &format!("{path}.tparam[{i}]"))?;
            self.push(format!("{path}.tparam[{i}].name"), tpn);
            if let Some(b) = tp.bound {
                self.project_type(b, tp.span, &format!("{path}.tparam[{i}].bound"))?;
            }
        }
        if let Some(ctor) = &c.primary_constructor {
            self.check_span(ctor.left_paren_span, Some(span))?;
            let rp = ctor
                .right_paren_span
                .ok_or_else(|| format!("unclosed primary ctor at {path}"))?;
            self.check_span(rp, Some(span))?;
            for (i, f) in ctor.fields.iter().enumerate() {
                self.check_span(f.span, Some(span))?;
                if let Some(v) = f.visibility {
                    let vl = self.vis(v, f.span)?;
                    self.push(format!("{path}.ctor[{i}].vis"), vl);
                }
                let fk = match f.kind {
                    VariableKind::Val => "val",
                    VariableKind::Var => "var",
                    VariableKind::BorrowVal(_) => {
                        return Err(format!("borrow val field at {path}"));
                    }
                };
                self.push(format!("{path}.ctor[{i}].kind"), fk);
                let fnm = self.marker(f.name, f.span, &format!("{path}.ctor[{i}].name"))?;
                self.push(format!("{path}.ctor[{i}].name"), fnm);
                self.project_type(f.type_ref, f.span, &format!("{path}.ctor[{i}].type"))?;
            }
        }
        for (i, sup) in c.supertypes.iter().enumerate() {
            self.check_span(sup.span, Some(span))?;
            if sup.trailing_lexical_error.is_some() {
                return Err(format!("supertype lexical error at {path}.super[{i}]"));
            }
            self.project_type(sup.type_ref, sup.span, &format!("{path}.super[{i}].type"))?;
            if let Some(del) = sup.delegation {
                self.check_span(del.span, Some(sup.span))?;
                let target = self.marker(del.target, del.span, &format!("{path}.super[{i}].by"))?;
                self.push(format!("{path}.super[{i}].by"), target);
            }
        }
        if let Some(body) = &c.body {
            self.check_span(body.left_brace_span, Some(span))?;
            let rb = body
                .right_brace_span
                .ok_or_else(|| format!("unclosed classifier body at {path}"))?;
            self.check_span(rb, Some(span))?;
            for (i, v) in body.variants.iter().enumerate() {
                self.check_span(v.span, Some(span))?;
                let vn = self.marker(v.name, v.span, &format!("{path}.variant[{i}].name"))?;
                self.push(format!("{path}.variant[{i}].name"), vn);
                for (j, vp) in v.parameters.iter().enumerate() {
                    self.check_span(vp.span, Some(v.span))?;
                    let vpn = self.marker(
                        vp.name,
                        vp.span,
                        &format!("{path}.variant[{i}].param[{j}].name"),
                    )?;
                    self.push(format!("{path}.variant[{i}].param[{j}].name"), vpn);
                    self.project_type(
                        vp.type_ref,
                        vp.span,
                        &format!("{path}.variant[{i}].param[{j}].type"),
                    )?;
                }
            }
            for (i, &m) in body.members.iter().enumerate() {
                self.project_item(m, Some(span), &format!("{path}.member[{i}]"))?;
            }
        }
        Ok(())
    }

    fn project_stmt(&mut self, id: StatementId, parent: Span, path: &str) -> Result<(), String> {
        let node = self
            .ast
            .statements()
            .get(id)
            .map_err(|_| format!("dangling StatementId at {path}"))?;
        let span = node.span();
        self.check_span(span, Some(parent))?;
        match node.payload() {
            Statement::Error => return Err(format!("Statement::Error at {path}")),
            Statement::Block { elements } => {
                self.push(format!("{path}.kind"), "block");
                for (i, &el) in elements.iter().enumerate() {
                    self.project_stmt(el, span, &format!("{path}.el[{i}]"))?;
                }
            }
            Statement::LambdaBody { elements } => {
                self.push(format!("{path}.kind"), "lambda_body");
                for (i, &el) in elements.iter().enumerate() {
                    self.project_stmt(el, span, &format!("{path}.el[{i}]"))?;
                }
            }
            Statement::ControlBody { elements } => {
                self.push(format!("{path}.kind"), "control_body");
                for (i, &el) in elements.iter().enumerate() {
                    self.project_stmt(el, span, &format!("{path}.el[{i}]"))?;
                }
            }
            Statement::LocalVariable { declaration } => {
                self.push(format!("{path}.kind"), "local_var");
                self.project_item(*declaration, Some(span), &format!("{path}.decl"))?;
            }
            Statement::LocalDestructuring {
                val_span,
                left_paren_span,
                bindings,
                right_paren_span,
                equals_span,
                initializer,
            } => {
                self.check_span(*val_span, Some(span))?;
                self.check_span(*left_paren_span, Some(span))?;
                let rp =
                    right_paren_span.ok_or_else(|| format!("unclosed destructuring at {path}"))?;
                let eq = equals_span.ok_or_else(|| format!("missing = in destructuring {path}"))?;
                self.check_span(rp, Some(span))?;
                self.check_span(eq, Some(span))?;
                self.push(format!("{path}.kind"), "destructuring");
                for (i, &b) in bindings.iter().enumerate() {
                    let bn = self.marker(b, span, &format!("{path}.bind[{i}]"))?;
                    self.push(format!("{path}.bind[{i}]"), bn);
                }
                self.project_expr(*initializer, span, &format!("{path}.init"))?;
            }
            Statement::While {
                keyword_span,
                condition,
                body,
            } => {
                self.check_span(*keyword_span, Some(span))?;
                self.push(format!("{path}.kind"), "while");
                self.project_expr(*condition, span, &format!("{path}.cond"))?;
                self.project_stmt(*body, span, &format!("{path}.body"))?;
            }
            Statement::For {
                keyword_span,
                binding,
                in_span,
                source,
                body,
            } => {
                self.check_span(*keyword_span, Some(span))?;
                self.check_span(*in_span, Some(span))?;
                self.push(format!("{path}.kind"), "for");
                match binding {
                    ForBinding::Name(m) => {
                        let n = self.marker(*m, span, &format!("{path}.bind"))?;
                        self.push(format!("{path}.bind"), n);
                    }
                    ForBinding::Destructuring {
                        left_paren_span,
                        names,
                        right_paren_span,
                    } => {
                        self.check_span(*left_paren_span, Some(span))?;
                        let rp = right_paren_span
                            .ok_or_else(|| format!("unclosed for destructuring at {path}"))?;
                        self.check_span(rp, Some(span))?;
                        for (i, &m) in names.iter().enumerate() {
                            let n = self.marker(m, span, &format!("{path}.bind[{i}]"))?;
                            self.push(format!("{path}.bind[{i}]"), n);
                        }
                    }
                }
                self.project_expr(*source, span, &format!("{path}.source"))?;
                self.project_stmt(*body, span, &format!("{path}.body"))?;
            }
            Statement::Loop { keyword_span, body } => {
                self.check_span(*keyword_span, Some(span))?;
                self.push(format!("{path}.kind"), "loop");
                self.project_stmt(*body, span, &format!("{path}.body"))?;
            }
            Statement::Expression { expression } => {
                self.push(format!("{path}.kind"), "expr_stmt");
                self.project_expr(*expression, span, &format!("{path}.expr"))?;
            }
        }
        Ok(())
    }

    fn project_expr(&mut self, id: ExpressionId, parent: Span, path: &str) -> Result<(), String> {
        let node = self
            .ast
            .expressions()
            .get(id)
            .map_err(|_| format!("dangling ExpressionId at {path}"))?;
        let span = node.span();
        self.check_span(span, Some(parent))?;
        match node.payload() {
            Expression::Error => return Err(format!("Expression::Error at {path}")),
            Expression::Name => {
                let s = self.slice(span, Some(parent))?;
                self.push(format!("{path}.name"), s);
            }
            Expression::This => self.push(format!("{path}.kind"), "this"),
            Expression::Literal(lit) => {
                let s = self.slice(span, Some(parent))?;
                self.push(format!("{path}.lit"), format!("{lit:?}:{s}"));
            }
            Expression::Group { expression } => {
                self.push(format!("{path}.kind"), "group");
                self.project_expr(*expression, span, &format!("{path}.inner"))?;
            }
            Expression::String { parts } => {
                self.push(format!("{path}.kind"), "string");
                for (i, part) in parts.iter().enumerate() {
                    match part {
                        StringPart::Text(s) => {
                            let txt = self.slice(*s, Some(span))?;
                            self.push(format!("{path}.part[{i}].text"), txt);
                        }
                        StringPart::Interpolation {
                            span: ispan,
                            expression,
                        } => {
                            self.check_span(*ispan, Some(span))?;
                            self.project_expr(
                                *expression,
                                *ispan,
                                &format!("{path}.part[{i}].interp"),
                            )?;
                        }
                        StringPart::Error(s) => {
                            return Err(format!("StringPart::Error at {path} ({s:?})"));
                        }
                    }
                }
            }
            Expression::Lambda {
                move_span,
                opener_span,
                parameters,
                arrow_span,
                body,
            } => {
                self.check_span(*opener_span, Some(span))?;
                self.push(format!("{path}.kind"), "lambda");
                let is_move = if let Some(m) = move_span {
                    self.check_span(*m, Some(span))?;
                    "true"
                } else {
                    "false"
                };
                self.push(format!("{path}.move"), is_move);
                for (i, &p) in parameters.iter().enumerate() {
                    let pn = self.slice(p, Some(span))?;
                    self.push(format!("{path}.param[{i}]"), pn);
                }
                let has_arrow = if let Some(a) = arrow_span {
                    self.check_span(*a, Some(span))?;
                    "true"
                } else {
                    "false"
                };
                self.push(format!("{path}.arrow"), has_arrow);
                self.project_stmt(*body, span, &format!("{path}.body"))?;
            }
            Expression::If {
                keyword_span,
                condition,
                then_branch,
                else_span,
                else_branch,
            } => {
                self.check_span(*keyword_span, Some(span))?;
                self.push(format!("{path}.kind"), "if");
                self.project_expr(*condition, span, &format!("{path}.cond"))?;
                self.project_stmt(*then_branch, span, &format!("{path}.then"))?;
                if let Some(es) = else_span {
                    self.check_span(*es, Some(span))?;
                }
                if let Some(eb) = else_branch {
                    self.project_stmt(*eb, span, &format!("{path}.else"))?;
                }
            }
            Expression::When {
                keyword_span,
                subject,
                entries,
            } => {
                self.check_span(*keyword_span, Some(span))?;
                self.push(format!("{path}.kind"), "when");
                if let Some(sub) = subject {
                    self.project_expr(*sub, span, &format!("{path}.subject"))?;
                }
                for (i, entry) in entries.iter().enumerate() {
                    self.check_span(entry.span, Some(span))?;
                    self.check_span(entry.arrow_span, Some(entry.span))?;
                    if let Some(es) = entry.else_span {
                        self.check_span(es, Some(entry.span))?;
                        self.push(format!("{path}.entry[{i}].else"), "true");
                    }
                    for (j, cond) in entry.conditions.iter().enumerate() {
                        let cpath = format!("{path}.entry[{i}].cond[{j}]");
                        match cond {
                            WhenCondition::Expression(e) => {
                                self.project_expr(*e, entry.span, &cpath)?;
                            }
                            WhenCondition::TypeTest {
                                operator_span,
                                negated,
                                type_ref,
                            } => {
                                self.check_span(*operator_span, Some(entry.span))?;
                                self.push(
                                    format!("{cpath}.op"),
                                    if *negated { "!is" } else { "is" },
                                );
                                self.project_type(*type_ref, entry.span, &format!("{cpath}.type"))?;
                            }
                            WhenCondition::Contains {
                                operator_span,
                                negated,
                                expression,
                            } => {
                                self.check_span(*operator_span, Some(entry.span))?;
                                self.push(
                                    format!("{cpath}.op"),
                                    if *negated { "!in" } else { "in" },
                                );
                                self.project_expr(
                                    *expression,
                                    entry.span,
                                    &format!("{cpath}.expr"),
                                )?;
                            }
                        }
                    }
                    self.project_stmt(entry.body, entry.span, &format!("{path}.entry[{i}].body"))?;
                }
            }
            Expression::Return {
                keyword_span,
                value,
            } => {
                self.check_span(*keyword_span, Some(span))?;
                self.push(format!("{path}.kind"), "return");
                if let Some(v) = value {
                    self.project_expr(*v, span, &format!("{path}.value"))?;
                }
            }
            Expression::Break { keyword_span } => {
                self.check_span(*keyword_span, Some(span))?;
                self.push(format!("{path}.kind"), "break");
            }
            Expression::Continue { keyword_span } => {
                self.check_span(*keyword_span, Some(span))?;
                self.push(format!("{path}.kind"), "continue");
            }
            Expression::SuperMember {
                keyword_span,
                interface,
                dot_span,
                name_span,
            } => {
                self.check_span(*keyword_span, Some(span))?;
                self.check_span(*dot_span, Some(span))?;
                let name = self.slice(*name_span, Some(span))?;
                self.push(format!("{path}.kind"), "super_member");
                self.project_type(*interface, span, &format!("{path}.interface"))?;
                self.push(format!("{path}.name"), name);
            }
            Expression::Prefix {
                operator,
                operator_span,
                operand,
            } => {
                self.check_span(*operator_span, Some(span))?;
                self.push(format!("{path}.prefix"), format!("{operator:?}"));
                self.project_expr(*operand, span, &format!("{path}.operand"))?;
            }
            Expression::Cast {
                expression,
                operator,
                operator_span,
                type_ref,
            } => {
                self.check_span(*operator_span, Some(span))?;
                self.push(format!("{path}.cast"), format!("{operator:?}"));
                self.project_expr(*expression, span, &format!("{path}.expr"))?;
                self.project_type(*type_ref, span, &format!("{path}.type"))?;
            }
            Expression::Binary {
                left,
                operator,
                operator_span,
                right,
            } => {
                self.check_span(*operator_span, Some(span))?;
                self.push(format!("{path}.bin"), format!("{operator:?}"));
                self.project_expr(*left, span, &format!("{path}.lhs"))?;
                self.project_expr(*right, span, &format!("{path}.rhs"))?;
            }
            Expression::TypeTest {
                expression,
                negated,
                operator_span,
                type_ref,
            } => {
                self.check_span(*operator_span, Some(span))?;
                self.push(
                    format!("{path}.typetest"),
                    if *negated { "!is" } else { "is" },
                );
                self.project_expr(*expression, span, &format!("{path}.expr"))?;
                self.project_type(*type_ref, span, &format!("{path}.type"))?;
            }
            Expression::Assignment {
                target,
                operator,
                operator_span,
                value,
            } => {
                self.check_span(*operator_span, Some(span))?;
                self.push(format!("{path}.assign"), format!("{operator:?}"));
                self.project_expr(*target, span, &format!("{path}.target"))?;
                self.project_expr(*value, span, &format!("{path}.value"))?;
            }
            Expression::Member {
                receiver,
                operator_span,
                name_span,
                safe,
            } => {
                self.check_span(*operator_span, Some(span))?;
                let name = self.slice(*name_span, Some(span))?;
                self.push(format!("{path}.member"), if *safe { "?." } else { "." });
                self.project_expr(*receiver, span, &format!("{path}.recv"))?;
                self.push(format!("{path}.name"), name);
            }
            Expression::Call {
                callee,
                type_arguments,
                type_arguments_span,
                arguments,
            } => {
                self.push(format!("{path}.kind"), "call");
                self.project_expr(*callee, span, &format!("{path}.callee"))?;
                if let Some(tas) = type_arguments_span {
                    self.check_span(*tas, Some(span))?;
                }
                for (i, &ta) in type_arguments.iter().enumerate() {
                    self.project_type(ta, span, &format!("{path}.targ[{i}]"))?;
                }
                for (i, arg) in arguments.iter().enumerate() {
                    self.check_span(arg.span, Some(span))?;
                    if let Some(np) = arg.named_prefix {
                        self.check_span(np.equals_span, Some(arg.span))?;
                        let nn = self.slice(np.name_span, Some(arg.span))?;
                        self.push(format!("{path}.arg[{i}].named"), nn);
                    }
                    if let Some(m) = arg.mode_marker {
                        let ml = self.mode(m, arg.span)?;
                        self.push(format!("{path}.arg[{i}].mode"), ml);
                    }
                    self.project_expr(arg.value, arg.span, &format!("{path}.arg[{i}].val"))?;
                }
            }
            Expression::Index { receiver, index } => {
                self.push(format!("{path}.kind"), "index");
                self.project_expr(*receiver, span, &format!("{path}.recv"))?;
                self.project_expr(*index, span, &format!("{path}.idx"))?;
            }
            Expression::NonNullAssert {
                operand,
                operator_span,
            } => {
                self.check_span(*operator_span, Some(span))?;
                self.push(format!("{path}.kind"), "nonnull");
                self.project_expr(*operand, span, &format!("{path}.operand"))?;
            }
            Expression::Propagate {
                value,
                question_span,
            } => {
                self.check_span(*question_span, Some(span))?;
                self.push(format!("{path}.kind"), "propagate");
                self.project_expr(*value, span, &format!("{path}.val"))?;
            }
            Expression::CallableReference {
                receiver,
                operator_span,
                name_span,
            } => {
                self.check_span(*operator_span, Some(span))?;
                let name = self.slice(*name_span, Some(span))?;
                self.push(format!("{path}.kind"), "callable_ref");
                if let Some(r) = receiver {
                    self.project_expr(*r, span, &format!("{path}.recv"))?;
                }
                self.push(format!("{path}.name"), name);
            }
        }
        Ok(())
    }
}

pub fn project_full(source: &str) -> Result<Projection, String> {
    let (sources, id) = loaded_source("full-roundtrip.ko", source);
    let lexed = lex(&sources, id).expect("source can be lexed");
    if !lexed.diagnostics().is_empty() {
        return Err(format!("lexer diagnostics: {:?}", lexed.diagnostics()));
    }
    let parsed = parse_file(&sources, &lexed).expect("source can be parsed");
    if !parsed.diagnostics().is_empty() {
        return Err(format!("parser diagnostics: {:?}", parsed.diagnostics()));
    }
    assert_eq!(parsed.source_id(), id);
    let mut projector = Projector {
        sources: &sources,
        source_id: id,
        source_len: source.len(),
        ast: parsed.ast(),
        fields: Vec::new(),
    };
    if let Some(pkg) = parsed.package() {
        projector.check_span(pkg.span, None)?;
        projector.check_span(pkg.keyword_span, Some(pkg.span))?;
        for (i, seg) in pkg.segments.iter().enumerate() {
            let name = projector.slice(seg.span, Some(pkg.span))?;
            projector.push(format!("package.seg[{i}]"), name);
        }
    }
    for (i, imp) in parsed.imports().iter().enumerate() {
        projector.check_span(imp.span, None)?;
        projector.check_span(imp.keyword_span, Some(imp.span))?;
        for (j, seg) in imp.segments.iter().enumerate() {
            let name = projector.slice(seg.span, Some(imp.span))?;
            projector.push(format!("import[{i}].seg[{j}]"), name);
        }
        if let Some(w) = imp.wildcard_span {
            projector.check_span(w, Some(imp.span))?;
            projector.push(format!("import[{i}].wildcard"), "true");
        }
        if let Some(a) = imp.alias {
            projector.check_span(a.as_span, Some(imp.span))?;
            let an = projector.slice(a.name_span, Some(imp.span))?;
            projector.push(format!("import[{i}].alias"), an);
        }
    }
    for (i, &root) in parsed.roots().iter().enumerate() {
        projector.project_item(root, None, &format!("root[{i}]"))?;
    }
    Ok(projector.fields)
}

pub fn first_difference<'a>(
    original: &'a [Field],
    formatted: &'a [Field],
) -> Option<Difference<'a>> {
    (0..original.len().max(formatted.len()))
        .map(|i| (original.get(i), formatted.get(i)))
        .find(|(a, b)| a != b)
}
