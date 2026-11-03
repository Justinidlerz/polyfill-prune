use crate::{
    compatibility::{Catalog, Targets, normalize_targets},
    model::{Options, PruneResult},
    output::Output,
};
use oxc_allocator::Allocator;
use oxc_ast::ast::*;
use oxc_ast_visit::{Visit, walk};
use oxc_parser::{ParseOptions, Parser};
use oxc_semantic::{Semantic, SemanticBuilder};
use oxc_span::{GetSpan, SourceType, Span};
use oxc_syntax::{
    operator::{BinaryOperator, LogicalOperator, UnaryOperator},
    scope::ScopeFlags,
};
use std::collections::BTreeSet;

pub struct Candidate {
    pub span: Span,
    pub kept: Option<Span>,
    pub dropped: Vec<Span>,
    pub prefix: String,
    pub suffix: String,
    pub features: Vec<String>,
    pub kind: &'static str,
}

#[derive(Clone, PartialEq)]
enum Value {
    Bool(bool),
    String(String),
    Null,
    Undefined,
    Native(String),
}
impl Value {
    fn truthy(&self) -> bool {
        match self {
            Self::Bool(v) => *v,
            Self::String(v) => !v.is_empty(),
            Self::Null | Self::Undefined => false,
            Self::Native(_) => true,
        }
    }
    fn typeof_(&self) -> &str {
        match self {
            Self::Bool(_) => "boolean",
            Self::String(_) => "string",
            Self::Null => "object",
            Self::Undefined => "undefined",
            Self::Native(kind) => kind,
        }
    }
}

struct Analyzer<'s, 'a> {
    semantic: &'s Semantic<'a>,
    catalog: &'s Catalog,
    targets: &'s Targets,
    keep: &'s [String],
    candidates: Vec<Candidate>,
    writes: Vec<(Span, String)>,
    dynamic_scope: bool,
}

impl<'s, 'a> Analyzer<'s, 'a> {
    fn identifier(&self, id: &IdentifierReference<'a>, depth: usize) -> Option<String> {
        if depth > 16 {
            return None;
        }
        let reference = self
            .semantic
            .scoping()
            .get_reference(id.reference_id.get()?);
        reference.symbol_id().is_none().then(|| id.name.to_string())
    }

    fn member(&self, object: &Expression<'a>, property: &str, depth: usize) -> Option<String> {
        let object = self.path(object, depth + 1)?;
        if object == "globalThis" {
            self.catalog
                .supported("globalThis", self.targets, self.keep)?;
            return Some(property.into());
        }
        // window/self are browser globals, never assumed for Node/Deno targets.
        if (object == "window" || object == "self")
            && self
                .targets
                .keys()
                .all(|engine| engine != "nodejs" && engine != "deno")
        {
            return Some(property.into());
        }
        Some(format!("{object}.{property}"))
    }

    fn path(&self, expr: &Expression<'a>, depth: usize) -> Option<String> {
        if depth > 16 {
            return None;
        }
        match expr.get_inner_expression() {
            Expression::Identifier(id) => self.identifier(id, depth + 1),
            Expression::StaticMemberExpression(member) if !member.optional => {
                self.member(&member.object, member.property.name.as_str(), depth + 1)
            }
            Expression::ComputedMemberExpression(member) if !member.optional => {
                if let Expression::StringLiteral(key) = member.expression.get_inner_expression() {
                    self.member(&member.object, key.value.as_str(), depth + 1)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn evaluate(&self, expr: &Expression<'a>, features: &mut BTreeSet<String>) -> Option<Value> {
        if let Some(path) = self.path(expr, 0) {
            if path == "undefined" {
                return Some(Value::Undefined);
            }
            if let Some(feature) = self.catalog.supported(&path, self.targets, self.keep) {
                features.insert(path);
                return Some(Value::Native(feature.kind.clone()));
            }
        }
        match expr.get_inner_expression() {
            Expression::BooleanLiteral(value) => Some(Value::Bool(value.value)),
            Expression::NullLiteral(_) => Some(Value::Null),
            Expression::StringLiteral(value) => Some(Value::String(value.value.to_string())),
            Expression::UnaryExpression(unary) => {
                let value = self.evaluate(&unary.argument, features)?;
                match unary.operator {
                    UnaryOperator::LogicalNot => Some(Value::Bool(!value.truthy())),
                    UnaryOperator::Typeof => Some(Value::String(value.typeof_().into())),
                    UnaryOperator::Void => Some(Value::Undefined),
                    _ => None,
                }
            }
            Expression::BinaryExpression(binary) => {
                let left = self.evaluate(&binary.left, features)?;
                let right = self.evaluate(&binary.right, features)?;
                let strict = matches!(
                    binary.operator,
                    BinaryOperator::StrictEquality | BinaryOperator::StrictInequality
                );
                let equal = match (&left, &right) {
                    (Value::Native(_), Value::Native(_)) => return None,
                    (Value::Native(_), Value::Null | Value::Undefined)
                    | (Value::Null | Value::Undefined, Value::Native(_)) => false,
                    (Value::Native(_), _) | (_, Value::Native(_)) if !strict => return None,
                    (Value::Null, Value::Undefined) | (Value::Undefined, Value::Null) => !strict,
                    _ if strict
                        || std::mem::discriminant(&left) == std::mem::discriminant(&right) =>
                    {
                        left == right
                    }
                    _ => return None,
                };
                match binary.operator {
                    BinaryOperator::Equality | BinaryOperator::StrictEquality => {
                        Some(Value::Bool(equal))
                    }
                    BinaryOperator::Inequality | BinaryOperator::StrictInequality => {
                        Some(Value::Bool(!equal))
                    }
                    _ => None,
                }
            }
            Expression::LogicalExpression(logical) => {
                let left = self.evaluate(&logical.left, features)?;
                let use_left = match logical.operator {
                    LogicalOperator::And => !left.truthy(),
                    LogicalOperator::Or => left.truthy(),
                    LogicalOperator::Coalesce => !matches!(left, Value::Null | Value::Undefined),
                };
                if use_left {
                    Some(left)
                } else {
                    self.evaluate(&logical.right, features)
                }
            }
            _ => None,
        }
    }

    fn record_write(&mut self, span: Span, path: Option<String>) {
        if let Some(path) = path {
            let path = if matches!(path.as_str(), "globalThis" | "window" | "self") {
                String::new()
            } else {
                path
            };
            self.writes.push((span, path));
        }
    }
    fn write_expr(&mut self, expr: &Expression<'a>, span: Span) {
        let path = self
            .path(expr, 0)
            .or_else(|| match expr.get_inner_expression() {
                Expression::ComputedMemberExpression(member) => self.path(&member.object, 0),
                _ => None,
            });
        self.record_write(span, path);
    }
}

#[derive(Default)]
struct Hoisted {
    names: BTreeSet<String>,
    unsafe_declaration: bool,
}
impl<'a> Visit<'a> for Hoisted {
    fn visit_variable_declaration(&mut self, node: &VariableDeclaration<'a>) {
        if node.kind == VariableDeclarationKind::Var {
            for declaration in &node.declarations {
                self.visit_binding_pattern(&declaration.id);
            }
        }
    }
    fn visit_binding_identifier(&mut self, id: &BindingIdentifier<'a>) {
        self.names.insert(id.name.to_string());
    }
    fn visit_function(&mut self, node: &Function<'a>, _: ScopeFlags) {
        if node.is_function_declaration() {
            self.unsafe_declaration = true;
        }
    }
    fn visit_arrow_function_expression(&mut self, _: &ArrowFunctionExpression<'a>) {}
    fn visit_class(&mut self, _: &Class<'a>) {}
}

impl<'a> Visit<'a> for Analyzer<'_, 'a> {
    fn visit_identifier_reference(&mut self, node: &IdentifierReference<'a>) {
        if node.name == "eval" && self.identifier(node, 0).as_deref() == Some("eval") {
            self.dynamic_scope = true;
        }
    }
    fn visit_new_expression(&mut self, node: &NewExpression<'a>) {
        if self.path(&node.callee, 0).as_deref() == Some("Function") {
            self.dynamic_scope = true;
        }
        walk::walk_new_expression(self, node);
    }
    fn visit_variable_declarator(&mut self, node: &VariableDeclarator<'a>) {
        // Local aliases can escape or be read before initialization in hoisted
        // functions. Preserve the aliased API instead of assuming execution order.
        if let Some(init) = &node.init {
            self.write_expr(init, node.span);
        }
        walk::walk_variable_declarator(self, node);
    }
    fn visit_assignment_expression(&mut self, node: &AssignmentExpression<'a>) {
        self.write_expr(&node.right, node.span);
        walk::walk_assignment_expression(self, node);
    }
    fn visit_if_statement(&mut self, node: &IfStatement<'a>) {
        let mut features = BTreeSet::new();
        if let Some(value) = self.evaluate(&node.test, &mut features)
            && !features.is_empty()
        {
            let (kept, dropped) = if value.truthy() {
                (Some(&node.consequent), node.alternate.as_ref())
            } else {
                (node.alternate.as_ref(), Some(&node.consequent))
            };
            let mut hoisted = Hoisted::default();
            if let Some(dropped) = dropped {
                hoisted.visit_statement(dropped);
            }
            if !hoisted.unsafe_declaration {
                let declarations = if hoisted.names.is_empty() {
                    String::new()
                } else {
                    format!(
                        "var {};",
                        hoisted.names.into_iter().collect::<Vec<_>>().join(",")
                    )
                };
                self.candidates.push(Candidate {
                    span: node.span,
                    kept: kept.map(GetSpan::span),
                    dropped: dropped.map(|s| vec![s.span()]).unwrap_or_default(),
                    prefix: format!("{{{declarations}"),
                    suffix: "}".into(),
                    features: features.into_iter().collect(),
                    kind: "if",
                });
            }
        }
        walk::walk_if_statement(self, node);
    }
    fn visit_conditional_expression(&mut self, node: &ConditionalExpression<'a>) {
        let mut features = BTreeSet::new();
        if let Some(value) = self.evaluate(&node.test, &mut features)
            && !features.is_empty()
        {
            let (kept, dropped) = if value.truthy() {
                (&node.consequent, &node.alternate)
            } else {
                (&node.alternate, &node.consequent)
            };
            self.candidates.push(Candidate {
                span: node.span,
                kept: Some(kept.span()),
                dropped: vec![dropped.span()],
                prefix: "(0,".into(),
                suffix: ")".into(),
                features: features.into_iter().collect(),
                kind: "conditional",
            });
        }
        walk::walk_conditional_expression(self, node);
    }
    fn visit_logical_expression(&mut self, node: &LogicalExpression<'a>) {
        let mut features = BTreeSet::new();
        if let Some(left) = self.evaluate(&node.left, &mut features)
            && !features.is_empty()
        {
            let use_left = match node.operator {
                LogicalOperator::And => !left.truthy(),
                LogicalOperator::Or => left.truthy(),
                LogicalOperator::Coalesce => !matches!(left, Value::Null | Value::Undefined),
            };
            let (kept, dropped) = if use_left {
                (&node.left, &node.right)
            } else {
                (&node.right, &node.left)
            };
            self.candidates.push(Candidate {
                span: node.span,
                kept: Some(kept.span()),
                dropped: vec![dropped.span()],
                prefix: "(0,".into(),
                suffix: ")".into(),
                features: features.into_iter().collect(),
                kind: "logical",
            });
        }
        walk::walk_logical_expression(self, node);
    }
    fn visit_simple_assignment_target(&mut self, node: &SimpleAssignmentTarget<'a>) {
        let path = match node {
            SimpleAssignmentTarget::AssignmentTargetIdentifier(id) => self.identifier(id, 0),
            SimpleAssignmentTarget::StaticMemberExpression(member) => {
                self.member(&member.object, member.property.name.as_str(), 0)
            }
            SimpleAssignmentTarget::ComputedMemberExpression(member) => {
                if let Expression::StringLiteral(key) = member.expression.get_inner_expression() {
                    self.member(&member.object, key.value.as_str(), 0)
                } else {
                    self.path(&member.object, 0)
                }
            }
            _ => None,
        };
        self.record_write(node.span(), path);
        walk::walk_simple_assignment_target(self, node);
    }
    fn visit_assignment_target_property_identifier(
        &mut self,
        node: &AssignmentTargetPropertyIdentifier<'a>,
    ) {
        self.record_write(node.span, self.identifier(&node.binding, 0));
        walk::walk_assignment_target_property_identifier(self, node);
    }
    fn visit_unary_expression(&mut self, node: &UnaryExpression<'a>) {
        if node.operator == UnaryOperator::Delete {
            self.write_expr(&node.argument, node.span);
        }
        walk::walk_unary_expression(self, node);
    }
    fn visit_call_expression(&mut self, node: &CallExpression<'a>) {
        let callee = self.path(&node.callee, 0);
        if matches!(callee.as_deref(), Some("eval" | "Function")) {
            self.dynamic_scope = true;
        }
        // Passing a builtin to an unknown function can expose it to writes.
        if callee
            .as_deref()
            .is_none_or(|path| !self.catalog.features.contains_key(path))
        {
            for argument in &node.arguments {
                if let Some(expression) = argument.as_expression() {
                    self.write_expr(expression, node.span);
                }
            }
        }
        if matches!(
            callee.as_deref(),
            Some(
                "Object.defineProperty"
                    | "Object.defineProperties"
                    | "Object.assign"
                    | "Object.setPrototypeOf"
                    | "Reflect.defineProperty"
                    | "Reflect.set"
                    | "Reflect.deleteProperty"
                    | "Reflect.setPrototypeOf"
            )
        ) && let Some(argument) = node.arguments.first().and_then(Argument::as_expression)
        {
            self.write_expr(argument, node.span);
        }
        walk::walk_call_expression(self, node);
    }
    fn visit_with_statement(&mut self, node: &WithStatement<'a>) {
        self.dynamic_scope = true;
        walk::walk_with_statement(self, node);
    }
}

fn contains(outer: Span, inner: Span) -> bool {
    outer.start <= inner.start && inner.end <= outer.end
}
fn conflicts(feature: &str, write: &str) -> bool {
    write.is_empty()
        || feature == write
        || feature
            .strip_prefix(write)
            .is_some_and(|rest| rest.starts_with('.'))
}

pub fn prune(code: &str, options: &Options, catalog: &Catalog) -> Result<PruneResult, String> {
    let targets = normalize_targets(&options.targets)?;
    let source_type =
        SourceType::from_path(&options.filename).unwrap_or_else(|_| SourceType::mjs());
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, code, source_type)
        .with_options(ParseOptions {
            allow_return_outside_function: true,
            ..ParseOptions::default()
        })
        .parse();
    if !parsed.diagnostics.is_empty() {
        return Ok(PruneResult::unchanged(
            code,
            parsed.diagnostics.iter().map(ToString::to_string).collect(),
        ));
    }
    let built = SemanticBuilder::new_compiler()
        .with_build_nodes(true)
        .build(&parsed.program);
    if !built.diagnostics.is_empty() {
        return Ok(PruneResult::unchanged(
            code,
            built.diagnostics.iter().map(ToString::to_string).collect(),
        ));
    }
    let mut analyzer = Analyzer {
        semantic: &built.semantic,
        catalog,
        targets: &targets,
        keep: &options.keep,
        candidates: vec![],
        writes: vec![],
        dynamic_scope: false,
    };
    if !parsed.program.source_type.is_module() {
        // Top-level script declarations may also replace properties accessed
        // through globalThis/window, even when lexical references are shadowed.
        for name in built
            .semantic
            .scoping()
            .get_bindings(built.semantic.scoping().root_scope_id())
            .keys()
        {
            analyzer.writes.push((Span::new(0, 0), name.to_string()));
        }
    }
    analyzer.visit_program(&parsed.program);
    if analyzer.dynamic_scope {
        return Ok(PruneResult::unchanged(
            code,
            vec!["Dynamic code or scope (eval/Function/with): file preserved".into()],
        ));
    }
    let mut candidates = analyzer.candidates;
    // Writes in a fallback are harmless only while that fallback is removable.
    loop {
        let live_writes = analyzer
            .writes
            .iter()
            .filter(|(span, _)| {
                !candidates.iter().any(|candidate| {
                    candidate
                        .dropped
                        .iter()
                        .any(|dropped| contains(*dropped, *span))
                })
            })
            .collect::<Vec<_>>();
        let before = candidates.len();
        candidates.retain(|candidate| {
            !candidate
                .features
                .iter()
                .any(|feature| live_writes.iter().any(|(_, path)| conflicts(feature, path)))
        });
        if candidates.len() == before {
            break;
        }
    }
    if candidates.is_empty() {
        return Ok(PruneResult::unchanged(code, vec![]));
    }
    candidates
        .sort_by_key(|candidate| (candidate.span.start, std::cmp::Reverse(candidate.span.end)));
    let mut output = Output::new(code, &options.filename, options.source_map);
    output.render(Span::new(0, code.len() as u32), &candidates);
    let check_allocator = Allocator::default();
    let checked = Parser::new(&check_allocator, &output.code, source_type)
        .with_options(ParseOptions {
            allow_return_outside_function: true,
            ..ParseOptions::default()
        })
        .parse();
    if !checked.diagnostics.is_empty() {
        return Err("Pruned output failed syntax validation".into());
    }
    let map = output.map()?;
    output.removals.sort_by_key(|removal| removal.start);
    Ok(PruneResult {
        code: output.code,
        map,
        changed: true,
        removals: output.removals,
        diagnostics: vec![],
    })
}
