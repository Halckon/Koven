//! Concrete local storage demanded by reachable direct construction initializers.

use std::collections::BTreeMap;

use lang_frontend::{
    ast::{ExpressionId, ItemId, StatementId},
    name_resolution::{NameResolution, SymbolId, SymbolKind},
    ownership_checking::OwnershipCheckedFile,
    parser::{Expression, Item, ParsedFile, Statement, StringPart, WhenCondition},
    source::Span,
    type_checking::{TypeId, TypeKind, TypedFile},
};

use super::{
    LoweringError, LoweringErrorKind, error, instances::resolve_concrete_type,
    nominal::NominalTypeMapper, orchestrate::FunctionPlanBody, present_name,
};
use crate::ssa::model::Module;

pub(super) struct Planner<'a> {
    pub(super) module: &'a mut Module,
    pub(super) mapper: &'a mut NominalTypeMapper,
    pub(super) parsed: &'a ParsedFile,
    pub(super) names: &'a NameResolution,
    pub(super) typed: &'a TypedFile,
    pub(super) owned: &'a OwnershipCheckedFile,
    pub(super) substitutions: &'a BTreeMap<SymbolId, TypeId>,
}

impl Planner<'_> {
    pub(super) fn intern_body(&mut self, body: FunctionPlanBody) -> Result<(), LoweringError> {
        match body {
            FunctionPlanBody::Expression(expression) => self.expression(expression),
            FunctionPlanBody::Block(block) => self.statement(block),
        }
    }

    fn statement(&mut self, statement: StatementId) -> Result<(), LoweringError> {
        let parsed = self.parsed;
        let node = parsed
            .ast()
            .statements()
            .get(statement)
            .map_err(|_| LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
        match node.payload() {
            Statement::Block { elements } | Statement::ControlBody { elements } => {
                for &element in elements {
                    self.statement(element)?;
                }
            }
            Statement::LocalVariable { declaration } => {
                self.local_variable(*declaration, node.span())?;
            }
            Statement::LocalDestructuring { initializer, .. }
            | Statement::Expression {
                expression: initializer,
            } => self.expression(*initializer)?,
            Statement::While {
                condition, body, ..
            } => {
                self.expression(*condition)?;
                self.statement(*body)?;
            }
            Statement::For { source, body, .. } => {
                self.expression(*source)?;
                self.statement(*body)?;
            }
            Statement::Loop { body, .. } => self.statement(*body)?,
            // Each callable owns its body and substitutions; never borrow the enclosing plan.
            Statement::LambdaBody { .. } => {}
            Statement::Error => return Err(error(LoweringErrorKind::MissingFact, node.span())),
        }
        Ok(())
    }

    fn expression(&mut self, expression: ExpressionId) -> Result<(), LoweringError> {
        let parsed = self.parsed;
        let node = parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
        match node.payload() {
            Expression::Group { expression }
            | Expression::Cast { expression, .. }
            | Expression::TypeTest { expression, .. }
            | Expression::Prefix {
                operand: expression,
                ..
            }
            | Expression::NonNullAssert {
                operand: expression,
                ..
            }
            | Expression::Propagate {
                value: expression, ..
            }
            | Expression::Member {
                receiver: expression,
                ..
            } => self.expression(*expression)?,
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
                self.expression(*left)?;
                self.expression(*right)?;
            }
            Expression::Call {
                callee, arguments, ..
            } => {
                self.expression(*callee)?;
                for argument in arguments {
                    self.expression(argument.value)?;
                }
            }
            Expression::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                self.expression(*condition)?;
                self.statement(*then_branch)?;
                if let Some(branch) = else_branch {
                    self.statement(*branch)?;
                }
            }
            Expression::When {
                subject, entries, ..
            } => {
                if let Some(subject) = subject {
                    self.expression(*subject)?;
                }
                for entry in entries {
                    for condition in &entry.conditions {
                        match condition {
                            WhenCondition::Expression(expression)
                            | WhenCondition::Contains { expression, .. } => {
                                self.expression(*expression)?;
                            }
                            WhenCondition::TypeTest { .. } => {}
                        }
                    }
                    self.statement(entry.body)?;
                }
            }
            Expression::Return { value, .. }
            | Expression::CallableReference {
                receiver: value, ..
            } => {
                if let Some(value) = value {
                    self.expression(*value)?;
                }
            }
            Expression::String { parts } => {
                for part in parts {
                    if let StringPart::Interpolation { expression, .. } = part {
                        self.expression(*expression)?;
                    }
                }
            }
            Expression::Lambda { .. }
            | Expression::Name
            | Expression::This
            | Expression::Literal(_)
            | Expression::Break { .. }
            | Expression::Continue { .. }
            | Expression::SuperMember { .. } => {}
            Expression::Error => return Err(error(LoweringErrorKind::MissingFact, node.span())),
        }
        Ok(())
    }

    fn local_variable(&mut self, declaration: ItemId, span: Span) -> Result<(), LoweringError> {
        let (item, _) = super::orchestrate::unwrap_modified(self.parsed, declaration)?;
        let Item::Variable {
            name, initializer, ..
        } = item
        else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        // Nested control bodies can contain their own locals even when this initializer
        // is not itself a construction. A lambda remains a separate callable boundary.
        self.expression(initializer)?;
        let mut construction = initializer;
        loop {
            let node = self
                .parsed
                .ast()
                .expressions()
                .get(construction)
                .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
            match node.payload() {
                Expression::Group { expression } => construction = *expression,
                _ => break,
            }
        }
        let Some(descriptor) = self.typed.construction(construction) else {
            return Ok(());
        };
        // Typed facts also describe dead code; only Phase 3 establishes runtime demand.
        let Some(plan) = self.owned.construction_plan(construction) else {
            return Ok(());
        };
        if descriptor.expression() != construction
            || plan.construction() != construction
            || plan.target() != descriptor.target()
            || plan.root_obligation().is_some_and(|root| {
                root.construction() != construction
                    || root.result_type() != descriptor.result_type()
            })
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        if plan.terminating_operand().is_some() {
            return Ok(());
        }
        let name = present_name(name).ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let symbol = self
            .names
            .symbols()
            .iter()
            .find(|symbol| symbol.span() == name && symbol.kind() == SymbolKind::Variable)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, name))?;
        if symbol.name() == "_" {
            return Ok(());
        }
        let declared = self
            .typed
            .symbol_type(symbol.id())
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, name))?;
        let declared = resolve_concrete_type(self.typed, declared, self.substitutions, span)?;
        let actual = resolve_concrete_type(
            self.typed,
            descriptor.result_type(),
            self.substitutions,
            span,
        )?;
        if matches!(self.typed.types().get(declared), Some(TypeKind::Nullable(inner)) if *inner == actual)
        {
            // Keep the canonical mapper's inline-nullable and resource-wrapper guards.
            self.mapper
                .intern(self.module, self.names, self.typed, declared, span)?;
        }
        Ok(())
    }
}
