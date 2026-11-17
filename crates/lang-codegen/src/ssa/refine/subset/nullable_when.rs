//! Nullable When admission consumes the existing same-analysis ownership plan.
use lang_frontend::{
    ast::ExpressionId,
    type_checking::{Copyability, NullableWhenSubjectCategory, ParameterMode, UnitTypeKind},
};

use super::Checker;

impl Checker<'_, '_> {
    /// Admit only an existing pending-drop fact whose real transfer is inside this
    /// owned nullable When and whose active lender encloses that same control.
    pub(super) fn nullable_when_abandons_lender(
        &self,
        call: lang_frontend::type_checking::UnitExpressionId,
        transfer: lang_frontend::type_checking::UnitExpressionId,
    ) -> bool {
        if call.source_unit() != self.inputs.source_unit
            || transfer.source_unit() != self.inputs.source_unit
        {
            return false;
        }
        let ast = self.inputs.parsed.ast();
        let (Ok(lender), Ok(jump)) = (
            ast.expressions().get(call.expression()),
            ast.expressions().get(transfer.expression()),
        ) else {
            return false;
        };
        let encloses = |outer: lang_frontend::source::Span, inner: lang_frontend::source::Span| {
            outer.source_id() == inner.source_id()
                && outer.start() <= inner.start()
                && inner.end() <= outer.end()
        };
        self.inputs.typed.nullable_whens().iter().any(|descriptor| {
            let inner = self.c(descriptor.inner_type());
            descriptor.expression().source_unit() == self.inputs.source_unit
                && (self.class(inner)
                    || self.box_payload(inner).is_some()
                    || self.rc_payload(inner).is_some())
                && super::super::nullable_when::source(self.inputs, descriptor.expression()).is_ok()
                && ast
                    .expressions()
                    .get(descriptor.expression().expression())
                    .is_ok_and(|control| encloses(lender.span(), control.span()))
                && descriptor.entries().iter().any(|entry| {
                    ast.statements()
                        .get(entry.body().statement())
                        .is_ok_and(|body| encloses(body.span(), jump.span()))
                })
        })
    }

    /// A real owned When view does not grant the legacy-forbidden loop crossing.
    pub(super) fn when_proof_crosses_loop(
        &self,
        statement: lang_frontend::ast::StatementId,
    ) -> bool {
        use lang_frontend::parser::Statement;
        let ast = self.inputs.parsed.ast();
        let Ok(node) = ast.statements().get(statement) else {
            return false;
        };
        if !matches!(
            node.payload(),
            Statement::While { .. } | Statement::Loop { .. } | Statement::For { .. }
        ) {
            return false;
        }
        let span = node.span();
        self.inputs.typed.nullable_whens().iter().any(|descriptor| {
            descriptor.expression().source_unit() == self.inputs.source_unit
                && descriptor
                    .stable_symbol()
                    .is_some_and(|root| self.view_symbols.contains(&root))
                && descriptor.entries().get(1).is_some_and(|entry| {
                    ast.statements()
                        .get(entry.body().statement())
                        .is_ok_and(|body| {
                            let scope = body.span();
                            scope.source_id() == span.source_id()
                                && scope.start() <= span.start()
                                && span.end() <= scope.end()
                        })
                })
        })
    }

    /// A When use has its own completed ownership contract, not an if-use adaptation.
    pub(super) fn nullable_when_argument(
        &self,
        call: ExpressionId,
        argument: ExpressionId,
        mode: ParameterMode,
        parameter: lang_frontend::type_checking::UnitTypeId,
    ) -> Option<bool> {
        use lang_frontend::ownership_checking::UnitNullableWhenUseKind;
        let witness = self.inputs.typed.nullable_when_use(self.id(argument))?;
        Some(
            self.inputs
                .owned
                .nullable_when_use(self.id(argument))
                .is_some_and(|fact| {
                    fact.call() == self.id(call)
                        && fact.argument() == self.id(argument)
                        && fact.witness() == witness
                        && self.known.contains(&witness.symbol())
                        && self.view_symbols.contains(&witness.symbol())
                        && self.c(witness.inner_type()) == parameter
                        && matches!(
                            (mode, fact.kind()),
                            (ParameterMode::Borrow, UnitNullableWhenUseKind::Observe)
                                | (ParameterMode::Value, UnitNullableWhenUseKind::Consume)
                        )
                }),
        )
    }

    pub(super) fn nullable_when(&mut self, expression: ExpressionId) -> bool {
        let Ok(source) = super::super::nullable_when::source(self.inputs, self.id(expression))
        else {
            return false;
        };
        let descriptor = source.plan.descriptor();
        // Only the original direct-call observation/consume uses are authorized;
        // a receiver or another value use must retain the existing rejection route.
        if self
            .inputs
            .typed
            .nullable_when_uses()
            .iter()
            .filter(|use_| use_.control() == descriptor.expression())
            .any(|use_| {
                !source.plan.uses().iter().any(|fact| {
                    fact.reference() == use_.reference()
                        && fact.witness().control() == use_.control()
                })
            })
        {
            return false;
        }
        let Some(result_type) = self.expression_type(expression) else {
            return false;
        };
        let nullable = self.c(descriptor.nullable_type());
        let inner = self.c(descriptor.inner_type());
        if self.copyability_of(nullable) != Copyability::MoveOnly
            || self.copyability_of(inner) != Copyability::MoveOnly
            || !(self.class(inner)
                || self.box_payload(inner).is_some()
                || self.rc_payload(inner).is_some())
            || !matches!(self.kind(nullable), Some(UnitTypeKind::Nullable(actual)) if self.c(*actual) == inner)
            || !self.storage_type(nullable)
            || !(self.unit(result_type)
                || self.nothing(result_type)
                || self.copyability_of(result_type) == Copyability::Copyable
                    && self.storage_type(result_type))
        {
            return false;
        }
        if descriptor.source_category() == NullableWhenSubjectCategory::OwnedRoot {
            let Some(root) = descriptor.stable_symbol() else {
                return false;
            };
            if !self.known.contains(&root)
                || self.view_symbols.contains(&root)
                || !matches!(self.parameter_mode(root), None | Some(ParameterMode::Value))
            {
                return false;
            }
        }
        // The source validator already proved every Group transparent and identical in
        // type/category. Admit the actual leaf, not a fabricated non-null expression.
        let Some(leaf) = source.operands.last() else {
            return false;
        };
        if !self.expression(leaf.expression()) {
            return false;
        }
        self.note_nullable(nullable);
        self.migrated = true;
        let outer = self.view_symbols.clone();
        let mut common = outer.clone();
        for (index, entry) in descriptor.entries().iter().enumerate() {
            self.view_symbols.clone_from(&outer);
            if index == 1
                && let Some(root) = descriptor.stable_symbol()
            {
                self.view_symbols.insert(root);
            }
            let admitted = self.statement(entry.body().statement());
            if entry.falls_through() {
                common.retain(|root| self.view_symbols.contains(root));
            }
            self.view_symbols.clone_from(&outer);
            if !admitted {
                return false;
            }
        }
        // A branch-only proof never escapes; retained outer proofs must survive every
        // reached successor, including replacement invalidations in either branch.
        self.view_symbols = common;
        true
    }
}
