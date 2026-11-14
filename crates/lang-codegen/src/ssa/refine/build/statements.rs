//! Statement dispatch remains mechanical; ownership cleanup stays in the caller.
use super::*;
impl Builder<'_, '_> {
    pub(super) fn statement_inner(
        &mut self,
        statement: StatementId,
    ) -> Result<Lowered, LoweringError> {
        let node = self
            .inputs
            .parsed
            .ast()
            .statements()
            .get(statement)
            .map_err(|_| error(LoweringErrorKind::MissingFact, self.inputs.span))?;
        let span = node.span();
        match node.payload().clone() {
            Statement::Block { elements }
            | Statement::ControlBody { elements }
            | Statement::LambdaBody { elements } => {
                let mut last = Lowered::Unit;
                let mut elements = elements.into_iter();
                while let Some(element) = elements.next() {
                    last = self.statement(element)?;
                    if last == Lowered::Diverged {
                        for skipped in elements {
                            let span = self.statement_span(skipped)?;
                            self.unreachable_drop_scopes.push(span);
                        }
                        return Ok(Lowered::Diverged);
                    }
                }
                Ok(last)
            }
            Statement::Expression { expression } => self.expression(expression),
            Statement::LocalVariable { declaration } => self.local_variable(declaration, span),
            Statement::While {
                condition, body, ..
            } => self.while_loop(statement, condition, body, span),
            Statement::Loop { body, .. } => self.loop_statement(statement, body, span),
            Statement::For { source, body, .. } => {
                self.for_statement(statement, source, body, span)
            }
            Statement::LocalDestructuring { initializer, .. } => {
                self.local_destructuring(statement, initializer, span)
            }
            _ => Err(error(LoweringErrorKind::UnsupportedNode, span)),
        }
    }
}
