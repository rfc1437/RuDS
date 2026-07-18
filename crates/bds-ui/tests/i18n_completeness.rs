use fluent_syntax::ast::{
    CallArguments, Entry, Expression, InlineExpression, Pattern, PatternElement,
};
use fluent_syntax::parser;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use syn::punctuated::Punctuated;
use syn::visit::Visit;
use syn::{Attribute, Expr, ExprCall, ExprMethodCall, ItemFn, ItemMod, Lit, Token};

fn pattern_variables(pattern: &Pattern<&str>, variables: &mut BTreeSet<String>) {
    for element in &pattern.elements {
        if let PatternElement::Placeable { expression } = element {
            expression_variables(expression, variables);
        }
    }
}

fn arguments_variables(arguments: &CallArguments<&str>, variables: &mut BTreeSet<String>) {
    for argument in &arguments.positional {
        inline_variables(argument, variables);
    }
    for argument in &arguments.named {
        inline_variables(&argument.value, variables);
    }
}

fn inline_variables(expression: &InlineExpression<&str>, variables: &mut BTreeSet<String>) {
    match expression {
        InlineExpression::VariableReference { id } => {
            variables.insert(id.name.to_owned());
        }
        InlineExpression::FunctionReference { arguments, .. } => {
            arguments_variables(arguments, variables);
        }
        InlineExpression::TermReference {
            arguments: Some(arguments),
            ..
        } => arguments_variables(arguments, variables),
        InlineExpression::Placeable { expression } => {
            expression_variables(expression, variables);
        }
        InlineExpression::StringLiteral { .. }
        | InlineExpression::NumberLiteral { .. }
        | InlineExpression::MessageReference { .. }
        | InlineExpression::TermReference {
            arguments: None, ..
        } => {}
    }
}

fn expression_variables(expression: &Expression<&str>, variables: &mut BTreeSet<String>) {
    match expression {
        Expression::Inline(expression) => inline_variables(expression, variables),
        Expression::Select { selector, variants } => {
            inline_variables(selector, variables);
            for variant in variants {
                pattern_variables(&variant.value, variables);
            }
        }
    }
}

fn catalog_signature(source: &str) -> BTreeMap<String, BTreeSet<String>> {
    let resource = parser::parse(source).unwrap_or_else(|(_, errors)| {
        panic!("invalid Fluent catalog: {errors:?}");
    });
    let mut signature = BTreeMap::new();

    for entry in resource.body {
        let (prefix, id, value, attributes) = match entry {
            Entry::Message(message) => ("", message.id, message.value, message.attributes),
            Entry::Term(term) => ("-", term.id, Some(term.value), term.attributes),
            Entry::Comment(_)
            | Entry::GroupComment(_)
            | Entry::ResourceComment(_)
            | Entry::Junk { .. } => continue,
        };
        let key = format!("{prefix}{}", id.name);
        let mut variables = BTreeSet::new();
        if let Some(value) = value {
            pattern_variables(&value, &mut variables);
        }
        assert!(
            signature.insert(key.clone(), variables).is_none(),
            "duplicate {key}"
        );

        for attribute in attributes {
            let mut variables = BTreeSet::new();
            pattern_variables(&attribute.value, &mut variables);
            let key = format!("{key}.{}", attribute.id.name);
            assert!(
                signature.insert(key.clone(), variables).is_none(),
                "duplicate {key}"
            );
        }
    }

    signature
}

fn assert_catalog_matches(
    domain: &str,
    locale: &str,
    expected: &BTreeMap<String, BTreeSet<String>>,
    source: &str,
) {
    let actual = catalog_signature(source);
    let missing = expected
        .keys()
        .filter(|key| !actual.contains_key(*key))
        .collect::<Vec<_>>();
    let extra = actual
        .keys()
        .filter(|key| !expected.contains_key(*key))
        .collect::<Vec<_>>();
    let mismatched_variables = expected
        .iter()
        .filter_map(|(key, variables)| {
            actual
                .get(key)
                .filter(|actual| *actual != variables)
                .map(|actual| (key, variables, actual))
        })
        .collect::<Vec<_>>();

    assert!(
        missing.is_empty() && extra.is_empty() && mismatched_variables.is_empty(),
        "{domain}/{locale}.ftl: missing={missing:?}, extra={extra:?}, variable mismatches={mismatched_variables:?}"
    );
}

#[test]
fn every_locale_has_the_same_messages_and_variables_as_english() {
    let ui = catalog_signature(include_str!("../../../locales/ui/en.ftl"));
    for (locale, source) in [
        ("de", include_str!("../../../locales/ui/de.ftl")),
        ("fr", include_str!("../../../locales/ui/fr.ftl")),
        ("it", include_str!("../../../locales/ui/it.ftl")),
        ("es", include_str!("../../../locales/ui/es.ftl")),
    ] {
        assert_catalog_matches("ui", locale, &ui, source);
    }

    let render = catalog_signature(include_str!("../../../locales/render/en.ftl"));
    for (locale, source) in [
        ("de", include_str!("../../../locales/render/de.ftl")),
        ("fr", include_str!("../../../locales/render/fr.ftl")),
        ("it", include_str!("../../../locales/render/it.ftl")),
        ("es", include_str!("../../../locales/render/es.ftl")),
    ] {
        assert_catalog_matches("render", locale, &render, source);
    }
}

fn cfg_test(attributes: &[Attribute]) -> bool {
    attributes.iter().any(|attribute| {
        attribute.path().is_ident("test")
            || attribute.path().is_ident("cfg")
                && attribute
                    .meta
                    .require_list()
                    .is_ok_and(|list| list.tokens.to_string().contains("test"))
    })
}

fn is_user_facing(value: &str) -> bool {
    if matches!(
        value,
        "bDS"
            | "https://"
            | "https://api.example.com/v1"
            | "gpt-4.1-mini"
            | "sk-..."
            | "http://localhost:11434/v1"
            | "llama3.2"
    ) {
        return false;
    }

    let mut outside_placeholder = String::new();
    let mut depth = 0;
    for character in value.chars() {
        match character {
            '{' => depth += 1,
            '}' if depth > 0 => depth -= 1,
            _ if depth == 0 => outside_placeholder.push(character),
            _ => {}
        }
    }

    outside_placeholder
        .split(|character: char| !character.is_alphabetic())
        .filter(|word| !word.is_empty())
        .any(|word| !matches!(word, "B" | "KB" | "MB" | "GB"))
}

#[derive(Default)]
struct LiteralCollector {
    literals: Vec<String>,
}

impl<'ast> Visit<'ast> for LiteralCollector {
    fn visit_expr_call(&mut self, call: &'ast ExprCall) {
        if let Expr::Path(path) = call.func.as_ref()
            && path.path.segments.last().is_some_and(|segment| {
                matches!(
                    segment.ident.to_string().as_str(),
                    "t" | "tw" | "translate" | "translate_with"
                )
            })
        {
            return;
        }
        syn::visit::visit_expr_call(self, call);
    }

    fn visit_expr_lit(&mut self, expression: &'ast syn::ExprLit) {
        if let Lit::Str(literal) = &expression.lit
            && is_user_facing(&literal.value())
        {
            self.literals.push(literal.value());
        }
    }

    fn visit_expr_macro(&mut self, expression: &'ast syn::ExprMacro) {
        let name = expression
            .mac
            .path
            .segments
            .last()
            .map(|segment| segment.ident.to_string());
        if name
            .as_deref()
            .is_some_and(|name| matches!(name, "t" | "tw"))
        {
            return;
        }
        if name
            .as_deref()
            .is_some_and(|name| matches!(name, "format" | "format_args" | "concat"))
            && let Ok(arguments) = expression
                .mac
                .parse_body_with(Punctuated::<Expr, Token![,]>::parse_terminated)
        {
            for argument in &arguments {
                self.visit_expr(argument);
            }
        }
    }
}

fn collect_literals(expression: &Expr) -> Vec<String> {
    let mut collector = LiteralCollector::default();
    collector.visit_expr(expression);
    collector.literals
}

#[derive(Default)]
struct UiLiteralVisitor {
    violations: Vec<String>,
}

impl UiLiteralVisitor {
    fn check(&mut self, sink: &str, expression: &Expr) {
        self.violations.extend(
            collect_literals(expression)
                .into_iter()
                .map(|literal| format!("{sink}: {literal:?}")),
        );
    }
}

impl<'ast> Visit<'ast> for UiLiteralVisitor {
    fn visit_item_mod(&mut self, item: &'ast ItemMod) {
        if !cfg_test(&item.attrs) {
            syn::visit::visit_item_mod(self, item);
        }
    }

    fn visit_item_fn(&mut self, item: &'ast ItemFn) {
        if !cfg_test(&item.attrs) {
            syn::visit::visit_item_fn(self, item);
        }
    }

    fn visit_expr_call(&mut self, call: &'ast ExprCall) {
        if let Expr::Path(path) = call.func.as_ref() {
            let segments = path.path.segments.iter().collect::<Vec<_>>();
            let last = segments.last().map(|segment| segment.ident.to_string());
            let previous = segments
                .get(segments.len().saturating_sub(2))
                .map(|segment| segment.ident.to_string());
            let indices: &[usize] = match (previous.as_deref(), last.as_deref()) {
                (Some("Toast"), Some("new")) => &[1],
                (Some("Submenu" | "MenuItem"), Some("new")) => &[0],
                (
                    _,
                    Some(
                        "text" | "help_text" | "section_header" | "labeled_checkbox" | "date_label"
                        | "pick_folder",
                    ),
                ) => &[0],
                (_, Some("text_input" | "labeled_input" | "pick_media_files")) => &[0, 1],
                _ => &[],
            };
            for index in indices {
                if let Some(argument) = call.args.get(*index) {
                    self.check(last.as_deref().unwrap_or("call"), argument);
                }
            }
        }
        syn::visit::visit_expr_call(self, call);
    }

    fn visit_expr_method_call(&mut self, call: &'ast ExprMethodCall) {
        let method = call.method.to_string();
        let index = match method.as_str() {
            "notify" => Some(1),
            "add_output" | "set_title" | "set_description" | "set_text" | "submit" => Some(0),
            "report_progress" => Some(2),
            _ => None,
        };
        if let Some(index) = index
            && let Some(argument) = call.args.get(index)
        {
            self.check(&method, argument);
        }
        syn::visit::visit_expr_method_call(self, call);
    }
}

fn rust_files(directory: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}

#[test]
fn ui_text_sinks_do_not_receive_untranslated_literals() {
    let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    rust_files(&source_root, &mut files);
    let mut violations = Vec::new();

    for path in files {
        let source = fs::read_to_string(&path).unwrap();
        let syntax = syn::parse_file(&source).unwrap_or_else(|error| {
            panic!("failed to parse {}: {error}", path.display());
        });
        let mut visitor = UiLiteralVisitor::default();
        visitor.visit_file(&syntax);
        violations.extend(
            visitor
                .violations
                .into_iter()
                .map(|violation| format!("{}: {violation}", path.display())),
        );
    }

    assert!(
        violations.is_empty(),
        "user-facing literals must use t()/tw():\n{}",
        violations.join("\n")
    );
}
