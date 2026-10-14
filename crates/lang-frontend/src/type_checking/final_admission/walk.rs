//! AST 消费角色和选中事实共同决定 roots；仅在这些 roots 上闭合类型图。
use super::{
    FinalTypeAdmissionInternalError as Internal,
    adapters::{Facts, Shape},
};
use crate::{
    ast::{ExpressionId, ItemId, StatementId, TypeRefId},
    diagnostic::{Diagnostic, Severity, codes},
    parser::{
        Expression, ForBinding, FunctionBody, FunctionForm, Item, NameMarker, ParsedFile,
        Statement, StringPart, TypeRef, WhenCondition,
    },
    source::{SourceMap, Span},
    type_checking::{
        BuiltinType, DeferredReason, ExpressionUse, TypeCheckingError, collect_expression_uses,
    },
};
use std::{collections::BTreeSet, thread};

type Result<T> = std::result::Result<T, Internal>;

pub(super) fn check<F: Facts + Sync>(
    sources: &SourceMap,
    files: &[(&ParsedFile, F)],
    definition_order: &[(usize, ItemId)],
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<()> {
    // 同一显式工作栈覆盖全部 source-set，跨文件依赖不引入宿主栈递归。
    thread::scope(|scope| {
        thread::Builder::new()
            .name("koven-final-type-admission".to_owned())
            .stack_size(32 * 1024 * 1024)
            .spawn_scoped(scope, || {
                check_roots(sources, files, definition_order, diagnostics)
            })
            .map_err(|error| TypeCheckingError::CheckerThread(error.kind()))?
            .join()
            .map_err(|_| TypeCheckingError::CheckerThreadPanicked)?
    })
}

fn check_roots<F: Facts>(
    sources: &SourceMap,
    files: &[(&ParsedFile, F)],
    definition_order: &[(usize, ItemId)],
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<()> {
    let mut states = files
        .iter()
        .map(|(parsed, _)| FileState {
            uses: collect_expression_uses(parsed),
            expressions: vec![None; parsed.ast().expressions().len()],
            items: vec![None; parsed.ast().items().len()],
        })
        .collect::<Vec<_>>();
    let mut poisoned = BTreeSet::new();
    let mut first_incomplete = None;
    let mut check_items = |file: usize, items: &[ItemId]| -> Result<()> {
        let (parsed, facts) = files
            .get(file)
            .ok_or(Internal::MismatchedAnalysisIdentity)?;
        let state = states
            .get_mut(file)
            .ok_or(Internal::MismatchedAnalysisIdentity)?;
        let mut checker = Checker {
            sources,
            parsed,
            facts,
            diagnostics,
            poisoned: &mut poisoned,
            uses: &state.uses,
            expressions: &mut state.expressions,
            items: &mut state.items,
        };
        for &item in items {
            if !checker.item(item)? && first_incomplete.is_none() {
                first_incomplete = Some(
                    parsed
                        .ast()
                        .items()
                        .get(item)
                        .map_err(TypeCheckingError::from)?
                        .span(),
                );
            }
        }
        Ok(())
    };
    for &(file, item) in definition_order {
        check_items(file, &[item])?;
    }
    for (file, (parsed, _)) in files.iter().enumerate() {
        check_items(file, parsed.roots())?;
    }
    if let Some(span) = first_incomplete
        && !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity() == Severity::Error)
    {
        return Err(Internal::MissingRequiredFact {
            span,
            kind: "root failure diagnostic",
        });
    }
    Ok(())
}
struct FileState {
    uses: Vec<ExpressionUse>,
    expressions: Vec<Option<bool>>,
    items: Vec<Option<bool>>,
}
struct Checker<'a, F: Facts> {
    sources: &'a SourceMap,
    parsed: &'a ParsedFile,
    facts: &'a F,
    diagnostics: &'a mut Vec<Diagnostic>,
    uses: &'a [ExpressionUse],
    poisoned: &'a mut BTreeSet<F::Symbol>,
    expressions: &'a mut [Option<bool>],
    items: &'a mut [Option<bool>],
}
impl<F: Facts> Checker<'_, F> {
    fn missing(&self, span: Span, kind: &'static str) -> Internal {
        Internal::MissingRequiredFact { span, kind }
    }
    fn existing_error(&self, span: Span) -> bool {
        self.diagnostics.iter().any(|diagnostic| {
            let root = diagnostic.primary_span();
            diagnostic.severity() == Severity::Error
                && root.source_id() == span.source_id()
                && root.start() >= span.start()
                && root.end() <= span.end()
        })
    }
    fn emit(&mut self, code: &str, message: String, span: Span) -> Result<bool> {
        let code = codes::catalog()
            .and_then(|catalog| catalog.resolve(code))
            .map_err(TypeCheckingError::from)?;
        self.diagnostics.push(
            Diagnostic::new(self.sources, Severity::Error, code, message, span)
                .map_err(TypeCheckingError::from)?,
        );
        Ok(false)
    }
    fn type_root(&mut self, ty: usize, span: Span, bound: bool) -> Result<bool> {
        self.type_graph(ty, span, bound, &mut BTreeSet::new())
    }
    fn type_graph(
        &mut self,
        ty: usize,
        span: Span,
        bound: bool,
        visited: &mut BTreeSet<usize>,
    ) -> Result<bool> {
        if !visited.insert(ty) {
            return Ok(true);
        }
        match self
            .facts
            .shape(ty)
            .ok_or_else(|| self.missing(span, "canonical type"))?
        {
            Shape::Complete | Shape::Builtin(_) => Ok(true),
            Shape::Nullable(inner) => self.type_graph(inner, span, bound, visited),
            Shape::Components(children) => {
                let mut complete = true;
                for child in children {
                    complete &= self.type_graph(child, span, bound, visited)?;
                }
                Ok(complete)
            }
            Shape::Deferred(DeferredReason::AnyValueRepresentation) if bound => Ok(true),
            Shape::Deferred(reason) => self.emit(
                codes::FINAL_TYPE_ANALYSIS_INCOMPLETE,
                format!("final type analysis is incomplete: {reason:?}"),
                span,
            ),
            Shape::Error if self.existing_error(span) => Ok(false),
            Shape::Error => Err(self.missing(span, "original diagnostic for required Error type")),
            Shape::UnselectedInteger => Err(self.missing(span, "selected integer type")),
        }
    }
    fn type_ref(&mut self, id: TypeRefId, bound: bool) -> Result<bool> {
        let parsed = self.parsed;
        let node = parsed
            .ast()
            .type_refs()
            .get(id)
            .map_err(TypeCheckingError::from)?;
        let mut complete = !self.existing_error(node.span());
        match node.payload() {
            TypeRef::Qualified { segments, .. } => {
                for segment in segments {
                    for argument in &segment.arguments {
                        complete &= self.type_ref(*argument, false)?;
                    }
                }
            }
            TypeRef::Function {
                parameters,
                return_type,
                ..
            } => {
                for parameter in parameters {
                    complete &= self.type_ref(parameter.type_ref, false)?;
                }
                complete &= self.type_ref(*return_type, false)?;
            }
            TypeRef::Error => return Ok(false),
        }
        if !complete {
            return Ok(false);
        }
        let ty = self
            .facts
            .type_ref_type(id)
            .ok_or_else(|| self.missing(node.span(), "type-reference type"))?;
        self.type_root(ty, node.span(), bound)
    }
    fn binding(&mut self, marker: NameMarker, complete: bool) -> Result<bool> {
        let NameMarker::Present(span) = marker else {
            return Ok(false);
        };
        // 与既有 destructuring/iteration checker 相同：真 discard 没有 symbol/value root。
        if self.sources.slice(span).map_err(TypeCheckingError::from)? == "_" {
            return Ok(complete);
        }
        let Some((symbol, ty)) = self.facts.declaration(span) else {
            return if !complete || self.existing_error(span) {
                Ok(false)
            } else {
                Err(self.missing(span, "declaration type"))
            };
        };
        if !complete {
            self.poisoned.insert(symbol);
            return Ok(false);
        }
        let complete = self.type_root(ty, span, false)?;
        if !complete {
            self.poisoned.insert(symbol);
        }
        Ok(complete)
    }
    fn item(&mut self, id: ItemId) -> Result<bool> {
        if let Some(complete) = self.items[id.index()] {
            return Ok(complete);
        }
        let parsed = self.parsed;
        let node = parsed
            .ast()
            .items()
            .get(id)
            .map_err(TypeCheckingError::from)?;
        let mut complete = true;
        match node.payload() {
            Item::Error => complete = false,
            Item::Modified { declaration, .. } => complete &= self.item(*declaration)?,
            Item::Variable {
                name,
                type_ref,
                initializer,
                ..
            }
            | Item::Constant {
                name,
                type_ref,
                initializer,
                ..
            } => {
                complete &= self.expression(*initializer, false)?;
                if let Some(type_ref) = type_ref {
                    complete &= self.type_ref(*type_ref, false)?;
                }
                complete &= self.binding(*name, complete)?;
            }
            Item::Function {
                type_parameters,
                extension_receiver,
                parameters,
                form,
                ..
            } => {
                for parameter in type_parameters {
                    if let Some(bound) = parameter.bound {
                        complete &= self.type_ref(bound, true)?;
                    }
                }
                if let Some(receiver) = extension_receiver {
                    complete &= self.type_ref(receiver.type_ref, false)?;
                }
                for parameter in parameters {
                    complete &= self.type_ref(parameter.type_ref, false)?;
                }
                match form {
                    FunctionForm::ImplicitUnitAbsent => {}
                    FunctionForm::ImplicitUnitBlock(body) => complete &= self.statement(*body)?,
                    FunctionForm::Explicit { type_ref, body, .. } => {
                        complete &= self.type_ref(*type_ref, false)?;
                        complete &= match body {
                            FunctionBody::Absent => true,
                            FunctionBody::Block(body) => self.statement(*body)?,
                            FunctionBody::Expression { expression, .. } => {
                                self.expression(*expression, false)?
                            }
                        };
                    }
                }
            }
            Item::Classifier(classifier) => {
                for parameter in &classifier.type_parameters {
                    if let Some(bound) = parameter.bound {
                        complete &= self.type_ref(bound, true)?;
                    }
                }
                if let Some(constructor) = &classifier.primary_constructor {
                    for field in &constructor.fields {
                        complete &= self.type_ref(field.type_ref, false)?;
                    }
                }
                for supertype in &classifier.supertypes {
                    complete &= self.type_ref(supertype.type_ref, true)?;
                }
                if let Some(body) = &classifier.body {
                    for variant in &body.variants {
                        for parameter in &variant.parameters {
                            complete &= self.type_ref(parameter.type_ref, false)?;
                        }
                    }
                    for member in &body.members {
                        complete &= self.item(*member)?;
                    }
                }
            }
            Item::Companion(companion) => {
                for member in &companion.body.members {
                    complete &= self.item(*member)?;
                }
            }
            Item::Deinit { body, .. } => complete &= self.statement(*body)?,
        }
        self.items[id.index()] = Some(complete);
        Ok(complete)
    }
    fn statement(&mut self, id: StatementId) -> Result<bool> {
        let parsed = self.parsed;
        let node = parsed
            .ast()
            .statements()
            .get(id)
            .map_err(TypeCheckingError::from)?;
        let mut complete = true;
        match node.payload() {
            Statement::Error => complete = false,
            Statement::Block { elements }
            | Statement::LambdaBody { elements }
            | Statement::ControlBody { elements } => {
                for element in elements {
                    complete &= self.statement(*element)?;
                }
            }
            Statement::LocalVariable { declaration } => complete &= self.item(*declaration)?,
            Statement::LocalDestructuring {
                initializer,
                bindings,
                ..
            } => {
                complete &= self.expression(*initializer, false)?;
                for binding in bindings {
                    complete &= self.binding(*binding, complete)?;
                }
            }
            Statement::Expression { expression } => {
                complete &= self.expression(*expression, false)?
            }
            Statement::While {
                condition, body, ..
            } => {
                complete &= self.expression(*condition, false)?;
                complete &= self.statement(*body)?;
            }
            Statement::For {
                source,
                binding,
                body,
                ..
            } => {
                complete &= self.expression(*source, false)?;
                match binding {
                    ForBinding::Name(name) => complete &= self.binding(*name, complete)?,
                    ForBinding::Destructuring { names, .. } => {
                        let source_complete = complete;
                        for name in names {
                            complete &= self.binding(*name, source_complete)?;
                        }
                    }
                }
                complete &= self.statement(*body)?;
            }
            Statement::Loop { body, .. } => complete &= self.statement(*body)?,
        }
        Ok(complete)
    }
    // 只有真实选中事实才能调用 selector。豁免壳，不豁免运行时 receiver/子表达式。
    fn selector(&mut self, id: ExpressionId) -> Result<bool> {
        let parsed = self.parsed;
        let node = parsed
            .ast()
            .expressions()
            .get(id)
            .map_err(TypeCheckingError::from)?;
        match node.payload() {
            Expression::Name => Ok(true),
            Expression::Group { expression } => self.selector(*expression),
            Expression::Member { receiver, .. } => {
                let receiver_node = parsed
                    .ast()
                    .expressions()
                    .get(*receiver)
                    .map_err(TypeCheckingError::from)?;
                let span = match receiver_node.payload() {
                    Expression::Member { name_span, .. } => *name_span,
                    _ => receiver_node.span(),
                };
                if self.facts.static_qualifier(span) {
                    self.selector(*receiver)
                } else {
                    self.expression(*receiver, false)
                }
            }
            Expression::SuperMember { interface, .. } => self.type_ref(*interface, true),
            _ => self.expression(id, false),
        }
    }
    fn rejected_type_arguments(&self, call: ExpressionId, callee: ExpressionId) -> Result<bool> {
        if self.facts.selection(call).is_some()
            || !matches!(
                self.facts
                    .expression_type(call)
                    .and_then(|ty| self.facts.shape(ty)),
                Some(Shape::Error)
            )
        {
            return Ok(false);
        }
        let callee = self
            .parsed
            .ast()
            .expressions()
            .get(callee)
            .map_err(TypeCheckingError::from)?;
        let primary = match callee.payload() {
            Expression::Name => callee.span(),
            Expression::Member { name_span, .. } => *name_span,
            _ => return Ok(false),
        };
        let code = codes::catalog()
            .and_then(|catalog| catalog.resolve(codes::TYPE_ARGUMENT_ARITY))
            .map_err(TypeCheckingError::from)?;
        Ok(self.diagnostics.iter().any(|diagnostic| {
            diagnostic.severity() == Severity::Error
                && diagnostic.code() == code
                && diagnostic.primary_span() == primary
        }))
    }
    fn call_type_ref(&mut self, id: TypeRefId, rejected: bool) -> Result<bool> {
        // 只有本 call 的原 L0091 及 Error 产物证明早拒，才允许尚未遍历的类型实参缺值。
        // 已有真实 Deferred/完整事实仍正常闭合；运行时实参从不走这里。
        if rejected
            && matches!(
                self.facts
                    .type_ref_type(id)
                    .and_then(|ty| self.facts.shape(ty)),
                None | Some(Shape::Error)
            )
        {
            return Ok(false);
        }
        self.type_ref(id, false)
    }
    fn static_candidate(&self, id: ExpressionId) -> Result<bool> {
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(id)
            .map_err(TypeCheckingError::from)?;
        match node.payload() {
            Expression::Name => Ok(self.facts.static_candidate(node.span())),
            Expression::Group { expression } => self.static_candidate(*expression),
            _ => Ok(false),
        }
    }
    // recovery 仅由失败 Call 传入其 callee 壳；不会传播到 receiver 或其他运行时子节点。
    fn expression(&mut self, id: ExpressionId, recovery: bool) -> Result<bool> {
        if let Some(complete) = self.expressions[id.index()] {
            return Ok(complete);
        }
        let parsed = self.parsed;
        let node = parsed
            .ast()
            .expressions()
            .get(id)
            .map_err(TypeCheckingError::from)?;
        let span = node.span();
        let selection = self.facts.selection(id);
        let has_selection = selection.is_some();
        let mut complete = !self.existing_error(span);
        match node.payload() {
            Expression::Error => complete = false,
            Expression::Name => {
                if self
                    .facts
                    .referenced_symbol(span)
                    .is_some_and(|symbol| self.poisoned.contains(&symbol))
                {
                    complete = false;
                }
            }
            Expression::This
            | Expression::Literal(_)
            | Expression::Break { .. }
            | Expression::Continue { .. } => {}
            Expression::Group { expression } => {
                complete &= self.expression(*expression, recovery)?
            }
            Expression::String { parts } => {
                for part in parts {
                    if let StringPart::Interpolation { expression, .. } = part {
                        complete &= self.expression(*expression, false)?;
                    }
                }
            }
            Expression::Lambda { body, .. } => complete &= self.statement(*body)?,
            Expression::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                complete &= self.expression(*condition, false)?;
                complete &= self.statement(*then_branch)?;
                if let Some(branch) = else_branch {
                    complete &= self.statement(*branch)?;
                }
            }
            Expression::When {
                subject, entries, ..
            } => {
                if let Some(subject) = subject {
                    complete &= self.expression(*subject, false)?;
                }
                for entry in entries {
                    for condition in &entry.conditions {
                        complete &= match condition {
                            WhenCondition::Expression(expression)
                            | WhenCondition::Contains { expression, .. } => {
                                self.expression(*expression, false)?
                            }
                            WhenCondition::TypeTest { type_ref, .. } => {
                                self.type_ref(*type_ref, true)?
                            }
                        };
                    }
                    complete &= self.statement(entry.body)?;
                }
            }
            Expression::Return { value, .. } => {
                if let Some(value) = value {
                    complete &= self.expression(*value, false)?;
                }
            }
            Expression::Prefix { operand, .. } | Expression::NonNullAssert { operand, .. } => {
                complete &= self.expression(*operand, false)?
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
                complete &= self.expression(*expression, false)?;
                complete &= self.type_ref(*type_ref, true)?;
            }
            Expression::Binary { left, right, .. } => {
                complete &= self.expression(*left, false)?;
                complete &= self.expression(*right, false)?;
            }
            Expression::Assignment { target, value, .. } => {
                complete &= self.expression(*target, false)?;
                complete &= self.expression(*value, false)?;
            }
            Expression::Index {
                receiver, index, ..
            } => {
                complete &= self.expression(*receiver, false)?;
                complete &= self.expression(*index, false)?;
            }
            Expression::Member {
                receiver,
                name_span,
                ..
            } => {
                let receiver_node = parsed
                    .ast()
                    .expressions()
                    .get(*receiver)
                    .map_err(TypeCheckingError::from)?;
                if selection.as_ref().is_some_and(|s| s.qualifier)
                    && self.facts.static_qualifier(receiver_node.span())
                {
                    complete &= self.selector(*receiver)?;
                } else {
                    complete &= self.expression(*receiver, false)?;
                }
                if complete && selection.is_none() {
                    let receiver_kind = self
                        .facts
                        .expression_type(*receiver)
                        .and_then(|ty| self.facts.shape(ty));
                    let name = self
                        .sources
                        .slice(*name_span)
                        .map_err(TypeCheckingError::from)?;
                    let closed_missing =
                        matches!(receiver_kind, Some(Shape::Builtin(BuiltinType::Int)))
                            && name != "inv"
                            || matches!(receiver_kind, Some(Shape::Builtin(BuiltinType::String)))
                                && name != "clone";
                    if closed_missing {
                        complete = self.emit(
                            codes::UNRESOLVED_NAME,
                            "receiver has no such member".to_owned(),
                            *name_span,
                        )?;
                    }
                }
            }
            Expression::Call {
                callee,
                arguments,
                type_arguments,
                ..
            } => {
                for argument in arguments {
                    complete &= self.expression(argument.value, false)?;
                }
                let rejected_type_arguments = self.rejected_type_arguments(id, *callee)?;
                for type_ref in type_arguments {
                    complete &= self.call_type_ref(*type_ref, rejected_type_arguments)?;
                }
                if selection.as_ref().is_some_and(|s| s.static_callee) {
                    complete &= self.selector(*callee)?;
                } else {
                    let deferred_call = matches!(
                        self.facts
                            .expression_type(id)
                            .and_then(|ty| self.facts.shape(ty)),
                        Some(Shape::Deferred(_))
                    );
                    // 已选 FunctionValue callee 必须具有自己的值事实。失败候选的缺值
                    // 只在已有 child 根因，或具名静态候选对应真实 Deferred 时抑制级联。
                    let failed_callee = selection.is_none()
                        && (!complete || (deferred_call && self.static_candidate(*callee)?));
                    complete &= self.expression(*callee, failed_callee)?;
                }
            }
            Expression::Propagate {
                value,
                question_span,
            } => {
                complete &= self.expression(*value, false)?;
                let proven_non_result = match self
                    .facts
                    .expression_type(*value)
                    .and_then(|ty| self.facts.shape(ty))
                {
                    // Nothing 的 bottom/传播组合不在本片非法用法闭合范围内。
                    Some(Shape::Builtin(BuiltinType::Nothing)) => false,
                    Some(Shape::Builtin(_) | Shape::Nullable(_)) => true,
                    _ => false,
                };
                if complete && proven_non_result {
                    complete = self.emit(
                        codes::INVALID_OPERAND_TYPES,
                        "error propagation requires a Result operand".to_owned(),
                        *question_span,
                    )?;
                }
            }
            Expression::CallableReference { receiver, .. } => {
                if let Some(receiver) = receiver {
                    complete &= self.expression(*receiver, false)?;
                }
            }
            Expression::SuperMember { interface, .. } => {
                complete &= self.type_ref(*interface, true)?
            }
        }
        if let Some(selection) = selection {
            for receiver in selection.receivers {
                complete &= self.expression(receiver, false)?;
            }
            if complete {
                for ty in selection.types {
                    complete &= self.type_root(ty, span, false)?;
                }
            }
        }
        if complete {
            let ty = self.facts.expression_type(id);
            let shape = ty.and_then(|ty| self.facts.shape(ty));
            if matches!(node.payload(), Expression::Call { .. })
                && !has_selection
                && !matches!(shape, Some(Shape::Deferred(_) | Shape::Error) | None)
            {
                return Err(
                    self.missing(span, "selected call, construction or intrinsic descriptor")
                );
            }
            let statement_assignment = self.facts.statement_assignment()
                && self.uses[id.index()] == ExpressionUse::Statement
                && matches!(node.payload(), Expression::Assignment { .. })
                && matches!(shape, Some(Shape::Deferred(DeferredReason::Assignment)));
            if !statement_assignment {
                if recovery && matches!(shape, None | Some(Shape::Error)) {
                    return Ok(true);
                }
                let ty = ty.ok_or_else(|| self.missing(span, "expression type"))?;
                complete &= self.type_root(ty, span, false)?;
            }
        }
        self.expressions[id.index()] = Some(complete);
        Ok(complete)
    }
}
