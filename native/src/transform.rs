use crate::{
    compatibility::{Catalog, Targets, normalize_targets},
    model::{Options, PruneResult},
    output::Output,
};
use oxc_allocator::{Allocator, Vec as ArenaVec};
use oxc_ast::ast::*;
use oxc_ast_visit::{Visit, walk};
use oxc_parser::{ParseOptions, Parser};
use oxc_semantic::{Semantic, SemanticBuilder};
use oxc_span::{GetSpan, SourceType, Span};
use oxc_syntax::{
    operator::{BinaryOperator, LogicalOperator, UnaryOperator},
    scope::ScopeFlags,
    symbol::SymbolId,
};
use std::collections::{BTreeMap, BTreeSet};

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

#[derive(Clone, PartialEq, Eq)]
struct Alias {
    path: String,
    owner: u32,
    available: u32,
    readable: bool,
    regions: Vec<u32>,
}

struct Analyzer<'s, 'a> {
    semantic: &'s Semantic<'a>,
    catalog: &'s Catalog,
    targets: &'s Targets,
    keep: &'s [String],
    candidates: Vec<Candidate>,
    writes: Vec<(Span, String)>,
    dynamic_scope: bool,
    aliases: BTreeMap<SymbolId, Alias>,
    owner: u32,
    collect_aliases: bool,
    resolving_write: bool,
    regions: Vec<u32>,
    browser: bool,
}

impl<'s, 'a> Analyzer<'s, 'a> {
    fn supported(&self, path: &str) -> Option<&crate::compatibility::Feature> {
        let feature = self.catalog.supported(path, self.targets, self.keep)?;
        if feature.web
            && (!self.browser
                || self
                    .targets
                    .keys()
                    .any(|engine| matches!(engine.as_str(), "nodejs" | "deno")))
        {
            return None;
        }
        // A member's introduction date alone does not establish its receiver.
        let mut parent = path;
        while let Some((prefix, _)) = parent.rsplit_once('.') {
            parent = prefix;
            if parent.ends_with(".prototype") {
                continue;
            }
            self.catalog.supported(parent, self.targets, self.keep)?;
        }
        Some(feature)
    }

    fn identifier(&self, id: &IdentifierReference<'a>, depth: usize) -> Option<String> {
        if depth > 16 {
            return None;
        }
        let reference = self
            .semantic
            .scoping()
            .get_reference(id.reference_id.get()?);
        match reference.symbol_id() {
            None => Some(id.name.to_string()),
            Some(symbol) => {
                let alias = self.aliases.get(&symbol)?;
                (self.resolving_write
                    || (alias.readable
                        && alias.owner == self.owner
                        && self.regions.starts_with(&alias.regions)
                        && id.span.start >= alias.available))
                    .then(|| alias.path.clone())
            }
        }
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
            if let Some(feature) = self.supported(&path)
                && matches!(feature.kind.as_str(), "function" | "object")
            {
                features.insert(path);
                return Some(Value::Native(feature.kind.clone()));
            }
        }
        match expr.get_inner_expression() {
            Expression::BooleanLiteral(value) => Some(Value::Bool(value.value)),
            Expression::NullLiteral(_) => Some(Value::Null),
            Expression::StringLiteral(value) => Some(Value::String(value.value.to_string())),
            Expression::TemplateLiteral(value) if value.expressions.is_empty() => Some(
                Value::String(value.quasis.first()?.value.cooked.as_ref()?.to_string()),
            ),
            Expression::UnaryExpression(unary) => {
                let value = self.evaluate(&unary.argument, features)?;
                match unary.operator {
                    UnaryOperator::LogicalNot => Some(Value::Bool(!value.truthy())),
                    UnaryOperator::Typeof => Some(Value::String(value.typeof_().into())),
                    UnaryOperator::Void => Some(Value::Undefined),
                    _ => None,
                }
            }
            Expression::BinaryExpression(binary) if binary.operator == BinaryOperator::In => {
                let Value::String(key) = self.evaluate(&binary.left, features)? else {
                    return None;
                };
                let path = self.member(&binary.right, &key, 0)?;
                self.supported(&path)?;
                features.insert(path);
                Some(Value::Bool(true))
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

    /// Truth can be known even when a short-circuit operand must still run.
    /// Callers using this proof preserve the original test expression.
    fn truth(&self, expr: &Expression<'a>, features: &mut BTreeSet<String>) -> Option<bool> {
        if let Some(value) = self.evaluate(expr, features) {
            return Some(value.truthy());
        }
        match expr.get_inner_expression() {
            Expression::UnaryExpression(unary) if unary.operator == UnaryOperator::LogicalNot => {
                self.truth(&unary.argument, features).map(|value| !value)
            }
            Expression::LogicalExpression(logical)
                if logical.operator != LogicalOperator::Coalesce =>
            {
                let left = self.truth(&logical.left, features);
                let right = self.truth(&logical.right, features);
                match logical.operator {
                    LogicalOperator::And if left == Some(false) || right == Some(false) => {
                        Some(false)
                    }
                    LogicalOperator::Or if left == Some(true) || right == Some(true) => Some(true),
                    LogicalOperator::And if left == Some(true) && right == Some(true) => Some(true),
                    LogicalOperator::Or if left == Some(false) && right == Some(false) => {
                        Some(false)
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn bind_alias(
        &mut self,
        pattern: &BindingPattern<'a>,
        path: &str,
        available: u32,
        readable: bool,
    ) -> bool {
        match pattern {
            BindingPattern::BindingIdentifier(id) => {
                let Some(symbol) = id.symbol_id.get() else {
                    return false;
                };
                if self
                    .semantic
                    .scoping()
                    .get_resolved_references(symbol)
                    .any(|reference| reference.is_write())
                {
                    return false;
                }
                if self.collect_aliases {
                    self.aliases.insert(
                        symbol,
                        Alias {
                            path: path.into(),
                            owner: self.owner,
                            available,
                            readable,
                            regions: self.regions.clone(),
                        },
                    );
                }
                self.aliases.contains_key(&symbol)
            }
            BindingPattern::ObjectPattern(object) if object.rest.is_none() => {
                let mut complete = true;
                for property in &object.properties {
                    if property.computed {
                        complete = false;
                        continue;
                    }
                    let Some(key) = property.key.static_name() else {
                        complete = false;
                        continue;
                    };
                    let member = if (path == "globalThis" && self.supported("globalThis").is_some())
                        || (matches!(path, "window" | "self")
                            && self
                                .targets
                                .keys()
                                .all(|engine| !matches!(engine.as_str(), "nodejs" | "deno")))
                    {
                        key.to_string()
                    } else {
                        format!("{path}.{key}")
                    };
                    complete &= self.bind_alias(&property.value, &member, available, readable);
                }
                complete
            }
            _ => false,
        }
    }

    fn dead_body(&mut self, body: &Statement<'a>, features: &BTreeSet<String>, kind: &'static str) {
        let mut hoisted = Hoisted::default();
        hoisted.visit_statement(body);
        if !hoisted.unsafe_declaration {
            self.candidates.push(Candidate {
                span: body.span(),
                kept: None,
                dropped: vec![body.span()],
                prefix: format!("{{{}}}", hoisted.declarations()),
                suffix: String::new(),
                features: features.iter().cloned().collect(),
                kind,
            });
        }
    }

    fn region_statement(&mut self, statement: &Statement<'a>) {
        self.regions.push(statement.span().start + 1);
        self.visit_statement(statement);
        self.regions.pop();
    }

    fn bind_parameters(
        &mut self,
        params: &FormalParameters<'a>,
        args: &[Argument<'a>],
        owner: u32,
        available: u32,
    ) -> bool {
        if params.rest.is_some()
            || params.items.len() != args.len()
            || params.items.iter().any(|p| p.initializer.is_some())
        {
            return false;
        }
        let paths = args
            .iter()
            .map(|argument| {
                let expression = argument.as_expression()?;
                let readable = self.path(expression, 0).is_some();
                self.write_path(expression).map(|path| (path, readable))
            })
            .collect::<Vec<_>>();
        let previous = std::mem::replace(&mut self.owner, owner);
        // The callee's body is not entered yet; caller regions remain ancestors.
        let mut complete = true;
        for (param, path) in params.items.iter().zip(paths) {
            complete &= path.is_some_and(|(path, readable)| {
                self.bind_alias(&param.pattern, &path, available, readable)
            });
        }
        self.owner = previous;
        complete
    }

    fn write_path(&mut self, expr: &Expression<'a>) -> Option<String> {
        let previous = std::mem::replace(&mut self.resolving_write, true);
        let path = self
            .path(expr, 0)
            .or_else(|| match expr.get_inner_expression() {
                Expression::ComputedMemberExpression(member) => self.path(&member.object, 0),
                _ => None,
            });
        self.resolving_write = previous;
        path
    }

    fn record_write(&mut self, span: Span, path: Option<String>) {
        if self.collect_aliases {
            return;
        }
        if let Some(path) = path {
            let path = if matches!(path.as_str(), "globalThis" | "window" | "self") {
                String::new()
            } else {
                path
            };
            self.writes.push((span, path));
        }
    }
    fn record_escape(&mut self, span: Span, path: Option<String>) {
        // A receiver can expose its members, but passing a function/object value
        // cannot replace the property from which that value was read.
        let path = path.map(|path| {
            if matches!(path.as_str(), "window" | "self" | "globalThis") {
                path
            } else {
                format!("{path}.")
            }
        });
        self.record_write(span, path);
    }
    fn write_expr(&mut self, expr: &Expression<'a>, span: Span) {
        let path = self.write_path(expr);
        self.record_write(span, path);
        self.escape_expr(expr, span);
    }
    fn escape_expr(&mut self, expr: &Expression<'a>, span: Span) {
        let path = self.write_path(expr);
        self.record_escape(span, path);
        match expr.get_inner_expression() {
            Expression::ConditionalExpression(node) => {
                self.escape_expr(&node.consequent, span);
                self.escape_expr(&node.alternate, span);
            }
            Expression::LogicalExpression(node) => {
                self.escape_expr(&node.left, span);
                self.escape_expr(&node.right, span);
            }
            Expression::SequenceExpression(node) => {
                if let Some(last) = node.expressions.last() {
                    self.escape_expr(last, span);
                }
            }
            Expression::AwaitExpression(node) => self.escape_expr(&node.argument, span),
            _ => {}
        }
    }
}

#[derive(Default)]
struct ParameterHazards {
    arguments: bool,
}
impl<'a> Visit<'a> for ParameterHazards {
    fn visit_identifier_reference(&mut self, node: &IdentifierReference<'a>) {
        self.arguments |= node.name == "arguments";
    }
}

#[derive(Default)]
struct Hoisted {
    names: BTreeSet<String>,
    unsafe_declaration: bool,
}
impl Hoisted {
    fn declarations(&self) -> String {
        if self.names.is_empty() {
            String::new()
        } else {
            format!(
                "var {};",
                self.names.iter().cloned().collect::<Vec<_>>().join(",")
            )
        }
    }
}

#[derive(PartialEq)]
enum Flow {
    Continue,
    Return,
    Unknown,
}

fn statement_flow(statement: &Statement<'_>, depth: usize) -> Flow {
    if depth > 64 {
        return Flow::Unknown;
    }
    match statement {
        Statement::ReturnStatement(_) => Flow::Return,
        Statement::BlockStatement(block) => {
            for statement in &block.body {
                let flow = statement_flow(statement, depth + 1);
                if flow != Flow::Continue {
                    return flow;
                }
            }
            Flow::Continue
        }
        Statement::IfStatement(node) => {
            let consequent = statement_flow(&node.consequent, depth + 1);
            let alternate = node
                .alternate
                .as_ref()
                .map_or(Flow::Continue, |node| statement_flow(node, depth + 1));
            match (consequent, alternate) {
                (Flow::Return, Flow::Return) => Flow::Return,
                (Flow::Unknown, _) | (_, Flow::Unknown) => Flow::Unknown,
                _ => Flow::Continue,
            }
        }
        Statement::ExpressionStatement(_)
        | Statement::VariableDeclaration(_)
        | Statement::FunctionDeclaration(_)
        | Statement::ClassDeclaration(_)
        | Statement::EmptyStatement(_) => Flow::Continue,
        _ => Flow::Unknown,
    }
}

fn definitely_returns(statement: &Statement<'_>) -> bool {
    statement_flow(statement, 0) == Flow::Return
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
    fn visit_function(&mut self, node: &Function<'a>, flags: ScopeFlags) {
        let owner = std::mem::replace(&mut self.owner, node.span.start + 1);
        walk::walk_function(self, node, flags);
        self.owner = owner;
    }
    fn visit_arrow_function_expression(&mut self, node: &ArrowFunctionExpression<'a>) {
        let owner = std::mem::replace(&mut self.owner, node.span.start + 1);
        if let Some(expression) = node.body.as_expression() {
            self.escape_expr(expression, expression.span());
        }
        walk::walk_arrow_function_expression(self, node);
        self.owner = owner;
    }
    fn visit_class(&mut self, node: &Class<'a>) {
        let owner = std::mem::replace(&mut self.owner, node.span.start + 1);
        walk::walk_class(self, node);
        self.owner = owner;
    }
    fn visit_block_statement(&mut self, node: &BlockStatement<'a>) {
        self.regions.push(node.span.start + 1);
        walk::walk_block_statement(self, node);
        self.regions.pop();
    }
    fn visit_switch_case(&mut self, node: &SwitchCase<'a>) {
        self.regions.push(node.span.start + 1);
        walk::walk_switch_case(self, node);
        self.regions.pop();
    }
    fn visit_statements(&mut self, statements: &ArenaVec<'a, Statement<'a>>) {
        if !self.collect_aliases {
            for (index, statement) in statements.iter().enumerate() {
                let Statement::IfStatement(node) = statement else {
                    continue;
                };
                let tail = &statements[index + 1..];
                if tail.is_empty() {
                    continue;
                }
                let mut features = BTreeSet::new();
                let Some(truth) = self.truth(&node.test, &mut features) else {
                    continue;
                };
                let selected = if truth {
                    Some(&node.consequent)
                } else {
                    node.alternate.as_ref()
                };
                if features.is_empty() || !selected.is_some_and(definitely_returns) {
                    continue;
                }
                // Keep lexical declarations that can be observed before their TDZ ends.
                if tail.iter().any(|s| matches!(s, Statement::ClassDeclaration(_)) || matches!(s, Statement::VariableDeclaration(v) if v.kind != VariableDeclarationKind::Var)) { continue; }
                let mut hoisted = Hoisted::default();
                for statement in tail {
                    hoisted.visit_statement(statement);
                }
                if hoisted.unsafe_declaration {
                    continue;
                }
                let span = Span::new(tail[0].span().start, tail.last().unwrap().span().end);
                self.candidates.push(Candidate {
                    span,
                    kept: None,
                    dropped: vec![span],
                    prefix: hoisted.declarations(),
                    suffix: String::new(),
                    features: features.into_iter().collect(),
                    kind: "early-return",
                });
            }
        }
        walk::walk_statements(self, statements);
    }
    fn visit_while_statement(&mut self, node: &WhileStatement<'a>) {
        let mut features = BTreeSet::new();
        if !self.collect_aliases
            && self.truth(&node.test, &mut features) == Some(false)
            && !features.is_empty()
        {
            self.dead_body(&node.body, &features, "while");
        }
        self.regions.push(node.span.start + 1);
        walk::walk_while_statement(self, node);
        self.regions.pop();
    }
    fn visit_for_statement(&mut self, node: &ForStatement<'a>) {
        let mut features = BTreeSet::new();
        if !self.collect_aliases
            && node
                .test
                .as_ref()
                .and_then(|test| self.truth(test, &mut features))
                == Some(false)
            && !features.is_empty()
        {
            self.dead_body(&node.body, &features, "for");
            if let Some(update) = &node.update {
                self.candidates.push(Candidate {
                    span: update.span(),
                    kept: None,
                    dropped: vec![update.span()],
                    prefix: String::new(),
                    suffix: String::new(),
                    features: features.into_iter().collect(),
                    kind: "for",
                });
            }
        }
        self.regions.push(node.span.start + 1);
        walk::walk_for_statement(self, node);
        self.regions.pop();
    }
    fn visit_for_in_statement(&mut self, node: &ForInStatement<'a>) {
        self.regions.push(node.span.start + 1);
        walk::walk_for_in_statement(self, node);
        self.regions.pop();
    }
    fn visit_for_of_statement(&mut self, node: &ForOfStatement<'a>) {
        self.regions.push(node.span.start + 1);
        walk::walk_for_of_statement(self, node);
        self.regions.pop();
    }
    fn visit_do_while_statement(&mut self, node: &DoWhileStatement<'a>) {
        self.regions.push(node.span.start + 1);
        walk::walk_do_while_statement(self, node);
        self.regions.pop();
    }
    fn visit_export_declaration(&mut self, node: &ExportDeclaration<'a>) {
        if let Declaration::VariableDeclaration(declaration) = &node.declaration {
            for declarator in &declaration.declarations {
                if let Some(init) = &declarator.init {
                    self.escape_expr(init, node.span);
                }
            }
        }
        walk::walk_export_declaration(self, node);
    }
    fn visit_export_named_declaration(&mut self, node: &ExportNamedDeclaration<'a>) {
        for specifier in &node.specifiers {
            if let ModuleExportName::IdentifierReference(id) = &specifier.local {
                let previous = std::mem::replace(&mut self.resolving_write, true);
                let path = self.identifier(id, 0);
                self.resolving_write = previous;
                self.record_escape(node.span, path);
            }
        }
        walk::walk_export_named_declaration(self, node);
    }
    fn visit_export_default_declaration(&mut self, node: &ExportDefaultDeclaration<'a>) {
        if let Some(expression) = node.declaration.as_expression() {
            self.escape_expr(expression, node.span);
        }
        walk::walk_export_default_declaration(self, node);
    }
    fn visit_throw_statement(&mut self, node: &ThrowStatement<'a>) {
        self.escape_expr(&node.argument, node.span);
        walk::walk_throw_statement(self, node);
    }
    fn visit_jsx_expression_container(&mut self, node: &JSXExpressionContainer<'a>) {
        if let Some(expression) = node.expression.as_expression() {
            self.escape_expr(expression, node.span);
        }
        walk::walk_jsx_expression_container(self, node);
    }
    fn visit_yield_expression(&mut self, node: &YieldExpression<'a>) {
        if let Some(argument) = &node.argument {
            self.escape_expr(argument, node.span);
        }
        walk::walk_yield_expression(self, node);
    }
    fn visit_return_statement(&mut self, node: &ReturnStatement<'a>) {
        if let Some(argument) = &node.argument {
            self.escape_expr(argument, node.span);
        }
        walk::walk_return_statement(self, node);
    }
    fn visit_object_property(&mut self, node: &ObjectProperty<'a>) {
        self.escape_expr(&node.value, node.span);
        walk::walk_object_property(self, node);
    }
    fn visit_array_expression(&mut self, node: &ArrayExpression<'a>) {
        for element in &node.elements {
            if let Some(expression) = element.as_expression() {
                self.escape_expr(expression, node.span);
            }
        }
        walk::walk_array_expression(self, node);
    }
    fn visit_spread_element(&mut self, node: &SpreadElement<'a>) {
        self.escape_expr(&node.argument, node.span);
        walk::walk_spread_element(self, node);
    }

    fn visit_identifier_reference(&mut self, node: &IdentifierReference<'a>) {
        if node.name == "eval" && self.identifier(node, 0).as_deref() == Some("eval") {
            self.dynamic_scope = true;
        }
    }
    fn visit_new_expression(&mut self, node: &NewExpression<'a>) {
        if self.path(&node.callee, 0).as_deref() == Some("Function") {
            self.dynamic_scope = true;
        }
        for argument in &node.arguments {
            if let Some(expression) = argument.as_expression() {
                self.escape_expr(expression, node.span);
            }
        }
        walk::walk_new_expression(self, node);
    }
    fn visit_variable_declarator(&mut self, node: &VariableDeclarator<'a>) {
        if let Some(init) = &node.init {
            let readable = self.path(init, 0).is_some();
            let path = self.write_path(init);
            let bound = path
                .as_deref()
                .is_some_and(|path| self.bind_alias(&node.id, path, node.span.end, readable));
            if !bound {
                self.escape_expr(init, node.span);
            }
        }
        walk::walk_variable_declarator(self, node);
    }
    fn visit_assignment_expression(&mut self, node: &AssignmentExpression<'a>) {
        self.escape_expr(&node.right, node.span);
        walk::walk_assignment_expression(self, node);
    }
    fn visit_if_statement(&mut self, node: &IfStatement<'a>) {
        let mut features = BTreeSet::new();
        if !self.collect_aliases
            && let Some(value) = self.evaluate(&node.test, &mut features)
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
        if !self.collect_aliases && self.evaluate(&node.test, &mut BTreeSet::new()).is_none() {
            let mut features = BTreeSet::new();
            if let Some(truth) = self.truth(&node.test, &mut features)
                && !features.is_empty()
            {
                let dropped = if truth {
                    node.alternate.as_ref()
                } else {
                    Some(&node.consequent)
                };
                if let Some(dropped) = dropped {
                    self.dead_body(dropped, &features, "if");
                }
            }
        }
        self.visit_expression(&node.test);
        self.region_statement(&node.consequent);
        if let Some(alternate) = &node.alternate {
            self.region_statement(alternate);
        }
    }
    fn visit_conditional_expression(&mut self, node: &ConditionalExpression<'a>) {
        let mut features = BTreeSet::new();
        if !self.collect_aliases
            && let Some(value) = self.evaluate(&node.test, &mut features)
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
        if !self.collect_aliases
            && let Some(left) = self.evaluate(&node.left, &mut features)
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
        let previous = std::mem::replace(&mut self.resolving_write, true);
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
        self.resolving_write = previous;
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
        let bound_iife = if !node.optional {
            match node.callee.get_inner_expression() {
                Expression::FunctionExpression(function)
                    if !function.r#async && !function.generator && function.id.is_none() =>
                {
                    function.body.as_ref().is_some_and(|body| {
                        let mut hazards = ParameterHazards::default();
                        hazards.visit_function_body(body);
                        if hazards.arguments {
                            return false;
                        }
                        self.bind_parameters(
                            &function.params,
                            &node.arguments,
                            function.span.start + 1,
                            body.span.start,
                        )
                    })
                }
                Expression::ArrowFunctionExpression(function) if !function.r#async => {
                    let mut hazards = ParameterHazards::default();
                    hazards.visit_arrow_function_expression(function);
                    !hazards.arguments
                        && self.bind_parameters(
                            &function.params,
                            &node.arguments,
                            function.span.start + 1,
                            function.body.span().start,
                        )
                }
                _ => false,
            }
        } else {
            false
        };
        // Passing a builtin to an unknown function can expose it to writes.
        if !bound_iife
            && callee
                .as_deref()
                .is_none_or(|path| !self.catalog.features.contains_key(path))
        {
            for argument in &node.arguments {
                if let Some(expression) = argument.as_expression() {
                    self.escape_expr(expression, node.span);
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
        || (write.ends_with('.') && feature.starts_with(write))
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
        aliases: BTreeMap::new(),
        owner: 0,
        collect_aliases: true,
        resolving_write: false,
        regions: vec![],
        browser: options.environment == crate::model::Environment::Browser,
    };
    let mut converged = false;
    for _ in 0..32 {
        let previous = analyzer.aliases.clone();
        analyzer.visit_program(&parsed.program);
        if analyzer.aliases == previous {
            converged = true;
            break;
        }
    }
    if !converged {
        return Ok(PruneResult::unchanged(
            code,
            vec!["Alias resolution limit exceeded: file preserved".into()],
        ));
    }
    analyzer.collect_aliases = false;
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
