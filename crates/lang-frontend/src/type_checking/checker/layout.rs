use std::collections::{BTreeMap, BTreeSet};

use crate::diagnostic::{Diagnostic, Severity};

use super::*;

#[derive(Clone)]
struct InlineTarget {
    nominal: NominalId,
    substitutions: BTreeMap<SymbolId, TypeId>,
}

#[derive(Clone, Copy)]
struct InlineEdge {
    from: NominalId,
    to: NominalId,
    span: Span,
}

enum LayoutWalk {
    Finite,
    ReachesInvalid,
    Cycle {
        edges: Vec<InlineEdge>,
        affected: Vec<NominalId>,
    },
}

impl Checker<'_> {
    pub(super) fn check_inline_layouts(&mut self) -> Result<(), TypeCheckingError> {
        let roots = self
            .nominals
            .iter()
            .filter(|descriptor| {
                matches!(
                    descriptor.kind(),
                    NominalKind::ValueClass | NominalKind::EnumClass
                )
            })
            .map(NominalDescriptor::id)
            .collect::<Vec<_>>();
        for root in roots {
            if self.invalid_inline_nominals.contains(&root) {
                continue;
            }
            let substitutions = self.identity_substitutions(root);
            match self.walk_inline_layout(root, &substitutions, &mut Vec::new(), &mut Vec::new())? {
                LayoutWalk::Finite => {}
                LayoutWalk::ReachesInvalid => {
                    self.invalid_inline_nominals.insert(root);
                }
                LayoutWalk::Cycle { edges, affected } => {
                    self.invalid_inline_nominals.extend(affected);
                    self.emit_inline_cycle(&edges)?;
                }
            }
        }
        Ok(())
    }

    fn identity_substitutions(&self, nominal: NominalId) -> BTreeMap<SymbolId, TypeId> {
        self.nominals
            .iter()
            .find(|descriptor| descriptor.id() == nominal)
            .into_iter()
            .flat_map(|descriptor| descriptor.type_parameters())
            .filter_map(|parameter| self.symbol_type(*parameter).map(|ty| (*parameter, ty)))
            .collect()
    }

    fn walk_inline_layout(
        &self,
        nominal: NominalId,
        substitutions: &BTreeMap<SymbolId, TypeId>,
        path: &mut Vec<NominalId>,
        edges: &mut Vec<InlineEdge>,
    ) -> Result<LayoutWalk, TypeCheckingError> {
        path.push(nominal);
        for (component, span) in self.inline_components(nominal)? {
            let Some(target) = self.inline_target(component, substitutions) else {
                continue;
            };
            let edge = InlineEdge {
                from: nominal,
                to: target.nominal,
                span,
            };
            if let Some(position) = path
                .iter()
                .position(|candidate| *candidate == target.nominal)
            {
                let mut cycle_edges = edges[position..].to_vec();
                cycle_edges.push(edge);
                let affected = path.clone();
                path.pop();
                return Ok(LayoutWalk::Cycle {
                    edges: cycle_edges,
                    affected,
                });
            }
            if self.invalid_inline_nominals.contains(&target.nominal) {
                path.pop();
                return Ok(LayoutWalk::ReachesInvalid);
            }
            edges.push(edge);
            let result =
                self.walk_inline_layout(target.nominal, &target.substitutions, path, edges)?;
            edges.pop();
            if !matches!(result, LayoutWalk::Finite) {
                path.pop();
                return Ok(result);
            }
        }
        path.pop();
        Ok(LayoutWalk::Finite)
    }

    fn inline_components(
        &self,
        nominal: NominalId,
    ) -> Result<Vec<(TypeId, Span)>, TypeCheckingError> {
        let descriptor = self
            .nominals
            .iter()
            .find(|descriptor| descriptor.id() == nominal)
            .ok_or(TypeCheckingError::InvalidExternalBinding)?;
        let symbols = if descriptor.kind() == NominalKind::ValueClass {
            descriptor.fields().to_vec()
        } else {
            self.enum_cases
                .iter()
                .filter(|case| case.root() == nominal)
                .flat_map(|case| case.payloads().iter().map(|(symbol, _)| *symbol))
                .collect()
        };
        symbols
            .into_iter()
            .map(|symbol| {
                let ty = self
                    .symbol_type(symbol)
                    .ok_or(TypeCheckingError::InvalidExternalBinding)?;
                let span = self.component_type_spans[symbol.index()]
                    .ok_or(TypeCheckingError::InvalidExternalBinding)?;
                Ok((ty, span))
            })
            .collect()
    }

    fn inline_target(
        &self,
        mut ty: TypeId,
        substitutions: &BTreeMap<SymbolId, TypeId>,
    ) -> Option<InlineTarget> {
        let mut seen_parameters = BTreeSet::new();
        loop {
            match self.kind(ty) {
                TypeKind::TypeParameter(symbol) => {
                    let actual = substitutions.get(symbol).copied()?;
                    if actual == ty || !seen_parameters.insert(*symbol) {
                        return None;
                    }
                    ty = actual;
                }
                TypeKind::Nullable(inner) => ty = *inner,
                TypeKind::Nominal { nominal, arguments } => {
                    let descriptor = self
                        .nominals
                        .iter()
                        .find(|descriptor| descriptor.id() == *nominal)?;
                    if !matches!(
                        descriptor.kind(),
                        NominalKind::ValueClass | NominalKind::EnumClass
                    ) {
                        return None;
                    }
                    let mut child = substitutions.clone();
                    child.extend(
                        descriptor
                            .type_parameters()
                            .iter()
                            .copied()
                            .zip(arguments.iter().copied()),
                    );
                    return Some(InlineTarget {
                        nominal: *nominal,
                        substitutions: child,
                    });
                }
                TypeKind::Builtin(_)
                | TypeKind::Function { .. }
                | TypeKind::Intrinsic { .. }
                | TypeKind::EnumCase { .. }
                | TypeKind::StaticSelf(_)
                | TypeKind::Capability(_)
                | TypeKind::IntegerLiteral(_)
                | TypeKind::Error
                | TypeKind::Deferred(_) => return None,
            }
        }
    }

    fn emit_inline_cycle(&mut self, edges: &[InlineEdge]) -> Result<(), TypeCheckingError> {
        let Some((closing, preceding)) = edges.split_last() else {
            return Err(TypeCheckingError::InvalidExternalBinding);
        };
        let mut diagnostic = Diagnostic::new(
            self.sources,
            Severity::Error,
            self.infinite_inline_layout_code,
            "inline value layout forms an infinite recursive cycle",
            closing.span,
        )?;
        for edge in preceding {
            diagnostic.add_label(
                self.sources,
                edge.span,
                format!(
                    "inline edge nominal#{} -> nominal#{}",
                    edge.from.symbol().index(),
                    edge.to.symbol().index()
                ),
            )?;
        }
        self.diagnostics.push(diagnostic);
        Ok(())
    }
}
