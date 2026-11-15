//! Expression traversal over the existing lexical scopes and name identities.
use super::*;

impl Resolver<'_> {
    pub(super) fn resolve_expression(
        &mut self,
        expression_id: ExpressionId,
        scope: ScopeId,
    ) -> Result<(), NameResolutionError> {
        let node = self.ast().expressions().get(expression_id)?;
        let span = node.span();
        let expression = node.payload().clone();
        match expression {
            Expression::This => self.resolve_extension_this(span, scope),
            Expression::Error
            | Expression::Literal(_)
            | Expression::Break { .. }
            | Expression::Continue { .. } => Ok(()),
            Expression::Name => self.resolve_reference(span, scope, Namespace::Value),
            Expression::Group { expression } => self.resolve_expression(expression, scope),
            Expression::String { parts } => {
                for part in parts {
                    if let StringPart::Interpolation { expression, .. } = part {
                        self.resolve_expression(expression, scope)?;
                    }
                }
                Ok(())
            }
            Expression::Lambda {
                opener_span,
                parameters,
                arrow_span,
                body,
                ..
            } => {
                let lambda = self.add_scope(Some(scope), ScopeKind::Lambda, Some(span));
                if arrow_span.is_none() {
                    self.insert_implicit_it(lambda, opener_span)?;
                }
                for parameter in parameters {
                    self.insert_span(
                        lambda,
                        parameter,
                        Namespace::Value,
                        SymbolKind::LambdaParameter,
                    )?;
                }
                self.resolve_statement(body, lambda)
            }
            Expression::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                self.resolve_expression(condition, scope)?;
                self.resolve_statement(then_branch, scope)?;
                if let Some(branch) = else_branch {
                    self.resolve_statement(branch, scope)?;
                }
                Ok(())
            }
            Expression::When {
                subject, entries, ..
            } => {
                if let Some(subject) = subject {
                    self.resolve_expression(subject, scope)?;
                }
                for entry in entries {
                    for condition in entry.conditions {
                        match condition {
                            WhenCondition::Expression(id)
                            | WhenCondition::Contains { expression: id, .. } => {
                                self.resolve_expression(id, scope)?
                            }
                            WhenCondition::TypeTest { type_ref, .. } => {
                                self.resolve_type(type_ref, scope)?
                            }
                        }
                    }
                    self.resolve_statement(entry.body, scope)?;
                }
                Ok(())
            }
            Expression::Return { value, .. } => {
                if let Some(value) = value {
                    self.resolve_expression(value, scope)?;
                }
                Ok(())
            }
            Expression::SuperMember { interface, .. } => self.resolve_type(interface, scope),
            Expression::Prefix { operand, .. } | Expression::NonNullAssert { operand, .. } => {
                self.resolve_expression(operand, scope)
            }
            Expression::Propagate { value, .. } => self.resolve_expression(value, scope),
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
                self.resolve_expression(expression, scope)?;
                self.resolve_type(type_ref, scope)
            }
            Expression::Binary { left, right, .. } => {
                self.resolve_expression(left, scope)?;
                self.resolve_expression(right, scope)
            }
            Expression::Assignment { target, value, .. } => {
                self.resolve_expression(target, scope)?;
                self.resolve_expression(value, scope)
            }
            Expression::Member {
                receiver,
                name_span,
                ..
            } => {
                self.resolve_expression_with_type_fallback(receiver, scope)?;
                self.record_member_lookup(name_span, scope)?;
                let receiver_node = self.ast().expressions().get(receiver)?;
                if matches!(receiver_node.payload(), Expression::This) {
                    if let Some(candidates) =
                        self.lookup_payload_candidates(scope, self.sources.slice(name_span)?)
                    {
                        self.references.push(NameReference::new(
                            name_span,
                            scope,
                            Namespace::Value,
                            ReferenceTarget::EnumCasePayloadCandidates(candidates),
                        ));
                    }
                } else if matches!(receiver_node.payload(), Expression::Name)
                    && let Some(ReferenceTarget::Symbol(root)) =
                        self.reference_target(receiver_node.span(), Namespace::Type)
                {
                    let name = self.sources.slice(name_span)?.to_owned();
                    if let Some(case) = self.enum_case_by_root_name.get(&(root, name.clone())) {
                        let symbol = self.enum_cases[case.index()].value_symbol();
                        self.references.push(NameReference::new(
                            name_span,
                            scope,
                            Namespace::Value,
                            ReferenceTarget::Symbol(symbol),
                        ));
                    } else if self.enum_cases.iter().any(|case| case.root() == root)
                        && !self.has_companion_constant(root, &name)?
                    {
                        self.diagnostics.push(Diagnostic::new(
                            self.sources,
                            Severity::Error,
                            self.unresolved_code,
                            "unresolved enum case value",
                            name_span,
                        )?);
                        self.references.push(NameReference::new(
                            name_span,
                            scope,
                            Namespace::Value,
                            ReferenceTarget::Unresolved,
                        ));
                    }
                }
                Ok(())
            }
            Expression::Call {
                callee,
                type_arguments,
                arguments,
                ..
            } => {
                self.resolve_expression_with_type_fallback(callee, scope)?;
                for type_ref in type_arguments {
                    self.resolve_type(type_ref, scope)?;
                }
                for argument in arguments {
                    self.resolve_expression(argument.value, scope)?;
                }
                Ok(())
            }
            Expression::Index { receiver, index } => {
                self.resolve_expression(receiver, scope)?;
                self.resolve_expression(index, scope)
            }
            Expression::CallableReference {
                receiver,
                name_span,
                ..
            } => {
                if let Some(receiver) = receiver {
                    self.resolve_expression(receiver, scope)
                } else {
                    self.resolve_reference(name_span, scope, Namespace::Value)
                }
            }
        }
    }
}
