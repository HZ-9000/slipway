//! Connects the user's system configuration to Slipway's managed folder by
//! adding the one line that brings it in, and nothing else.
//!
//! The user's files are only ever added to: Slipway parses them to find
//! where the line goes, inserts text there, and leaves every other byte as
//! it was. A file it can't place the line in safely is refused, along with
//! what the user can add by hand.

use std::{
    fmt, fs,
    path::{Path, PathBuf},
};

use anyhow::{Context as _, Result, bail};
use rnix::{
    Root, SyntaxKind, SyntaxNode, SyntaxToken, TextSize,
    ast::{self, HasEntry as _},
};
use rowan::ast::AstNode as _;
use similar::TextDiff;

use crate::{
    config::Config,
    system::{ConfigKind, System, SystemConfig},
};

/// The name flake configs know Slipway's folder by, as an input and as an
/// argument of `outputs`.
pub const INPUT_NAME: &str = "slipway";

/// A planned edit to the user's system configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wiring {
    /// The file being edited.
    pub path: PathBuf,
    pub before: String,
    pub after: String,
}

impl Wiring {
    /// The change as a unified diff, for showing the user before it's made.
    #[must_use]
    pub fn diff(&self) -> String {
        let path = self.path.display().to_string();
        let diff = TextDiff::from_lines(&self.before, &self.after);
        let mut unified = diff.unified_diff();
        unified.context_radius(3).header(&path, &path);
        unified.to_string()
    }

    /// Writes the change, replacing the file in one step so it is never left
    /// half-written. Refuses if the file changed since the plan was made.
    pub fn apply(&self) -> Result<()> {
        let current = fs::read_to_string(&self.path)
            .with_context(|| format!("couldn't read {}", self.path.display()))?;
        if current != self.before {
            bail!(
                "{} changed while Slipway was working on it, so nothing was changed; run slipway again to see the new plan",
                self.path.display()
            );
        }

        let dir = self
            .path
            .parent()
            .with_context(|| format!("{} has no parent folder", self.path.display()))?;
        let name = self
            .path
            .file_name()
            .with_context(|| format!("{} has no file name", self.path.display()))?;
        let temp = dir.join(format!(".{}.slipway.tmp", name.to_string_lossy()));
        let permissions = fs::metadata(&self.path)
            .with_context(|| format!("couldn't read {}", self.path.display()))?
            .permissions();

        fs::write(&temp, &self.after)
            .with_context(|| format!("couldn't write {}", temp.display()))?;
        fs::set_permissions(&temp, permissions)
            .with_context(|| format!("couldn't set the permissions of {}", temp.display()))?;
        fs::rename(&temp, &self.path)
            .with_context(|| format!("couldn't replace {}", self.path.display()))?;
        Ok(())
    }
}

/// Returned when a configuration file is too unusual to edit safely.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CantWire {
    /// Why Slipway won't edit the file.
    pub reason: String,
    /// What the user can add themselves instead.
    pub by_hand: String,
}

impl fmt::Display for CantWire {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}, so Slipway won't edit it. To connect it yourself, {}",
            self.reason, self.by_hand
        )
    }
}

impl std::error::Error for CantWire {}

/// Where the managed folder is, for recognising an existing import of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    /// As the configuration names it, such as `./slipway`.
    pub relative: String,
    /// The same folder by its full path, such as `/etc/nixos/slipway`.
    pub absolute: PathBuf,
}

/// Works out the edit that connects `config` to the managed folder at
/// `managed_dir`, or `None` if it's already connected.
pub fn plan(config: &SystemConfig, managed_dir: &Path) -> Result<Option<Wiring>> {
    // `config.dir` has its links followed, so follow them here too: the
    // managed folder may not exist yet, but its parent does.
    let managed_dir = match (managed_dir.parent(), managed_dir.file_name()) {
        (Some(parent), Some(name)) => fs::canonicalize(parent)
            .map_or_else(|_| managed_dir.to_owned(), |parent| parent.join(name)),
        _ => managed_dir.to_owned(),
    };
    let managed_dir = managed_dir.as_path();

    let Ok(inside) = managed_dir.strip_prefix(&config.dir) else {
        return Err(CantWire {
            reason: format!(
                "Slipway keeps its files in {}, which isn't inside the folder your system setup is in ({})",
                managed_dir.display(),
                config.dir.display()
            ),
            by_hand: format!(
                "make {} reachable from {} and import it there",
                managed_dir.display(),
                config.entry.display()
            ),
        })
        .with_context(|| format!("can't connect {}", config.entry.display()));
    };
    let link = Link {
        relative: format!("./{}", inside.display()),
        absolute: managed_dir.to_owned(),
    };

    let before = fs::read_to_string(&config.entry)
        .with_context(|| format!("couldn't read {}", config.entry.display()))?;
    let after = match config.kind {
        ConfigKind::ConfigurationNix => wire_configuration_nix(&before, &link),
        ConfigKind::Flake => wire_flake(&before, &link.relative),
    }
    .with_context(|| format!("can't connect {}", config.entry.display()))?;

    Ok(after.map(|after| Wiring {
        path: config.entry.clone(),
        before,
        after,
    }))
}

/// Writes the managed folder if it has nothing in it yet, then applies
/// `wiring`. The folder comes first so the configuration never points at
/// something missing.
pub fn connect(system: &System, managed_dir: &Path, wiring: Option<&Wiring>) -> Result<()> {
    if !managed_dir.join("default.nix").exists() {
        Config::new(system.channel.clone(), system.output_mode()).write_to(managed_dir)?;
    }
    if let Some(wiring) = wiring {
        wiring.apply()?;
    }
    Ok(())
}

/// Adds `link` to the `imports` of a `configuration.nix`, returning the new
/// text, or `None` if it's already imported.
pub fn wire_configuration_nix(source: &str, link: &Link) -> Result<Option<String>, CantWire> {
    let refuse = |reason: &str| CantWire {
        reason: reason.to_owned(),
        by_hand: format!(
            "add `{}` to the `imports` list in your configuration.nix, for example `imports = [ {} ];`",
            link.relative, link.relative
        ),
    };

    let root = parse(source).ok_or_else(|| refuse("the file has a syntax error"))?;
    let set = root
        .expr()
        .and_then(module_body)
        .ok_or_else(|| refuse("the file doesn't directly return a set of settings"))?;

    let mut edits = Edits::new(source);
    match find_binding(&set, "imports").map_err(|reason| refuse(&reason))? {
        Some(imports) => {
            let ast::Expr::List(list) = imports else {
                return Err(refuse("its `imports` isn't a plain list"));
            };
            if list.items().any(|item| names_link(&item, link)) {
                return Ok(None);
            }
            edits.prepend_to_list(&list, &link.relative);
        }
        None => edits.prepend_to_set(&set, &format!("imports = [ {} ];", link.relative)),
    }
    edits
        .finish()
        .map(Some)
        .ok_or_else(|| refuse("adding the import would break the file"))
}

/// Adds Slipway's folder (at `relative`, such as `./slipway`) to a
/// `flake.nix` as an input and to its system's `modules`, returning the new
/// text, or `None` if both are already there.
pub fn wire_flake(source: &str, relative: &str) -> Result<Option<String>, CantWire> {
    let url = format!("path:{relative}");
    let module = format!("{INPUT_NAME}.nixosModules.default");
    let refuse = |reason: &str| CantWire {
        reason: reason.to_owned(),
        by_hand: format!(
            "add `inputs.{INPUT_NAME}.url = \"{url}\";` to your flake.nix, take `{INPUT_NAME}` as an argument of `outputs`, and add `{module}` to your system's `modules` list"
        ),
    };

    let root = parse(source).ok_or_else(|| refuse("the file has a syntax error"))?;
    let set = root
        .expr()
        .and_then(unparen)
        .and_then(|expr| match expr {
            ast::Expr::AttrSet(set) => Some(set),
            _ => None,
        })
        .ok_or_else(|| refuse("the file isn't a plain set of inputs and outputs"))?;

    let mut edits = Edits::new(source);
    add_input(&mut edits, &set, &url).map_err(|reason| refuse(&reason))?;
    add_module(&mut edits, &set, &module).map_err(|reason| refuse(&reason))?;

    if edits.is_empty() {
        return Ok(None);
    }
    edits
        .finish()
        .map(Some)
        .ok_or_else(|| refuse("adding Slipway would break the file"))
}

fn add_input(edits: &mut Edits, set: &ast::AttrSet, url: &str) -> Result<(), String> {
    let line = format!("{INPUT_NAME}.url = \"{url}\";");

    let mut nested = None;
    let mut dotted = Vec::new();
    for entry in set.entries() {
        match entry {
            ast::Entry::Inherit(inherit) if inherit.attrs().any(|a| attr_name(&a) == "inputs") => {
                return Err("its `inputs` are inherited from elsewhere".to_owned());
            }
            ast::Entry::Inherit(_) => {}
            ast::Entry::AttrpathValue(binding) => {
                let names = attr_names(&binding);
                if names.first().map(String::as_str) != Some("inputs") {
                    continue;
                }
                if names.len() == 1 {
                    if nested.is_some() {
                        return Err("it sets `inputs` more than once".to_owned());
                    }
                    nested = Some(binding);
                } else {
                    dotted.push((names, binding));
                }
            }
        }
    }

    match (nested, dotted.as_slice()) {
        (Some(_), [_, ..]) => Err("it sets `inputs` in two different ways".to_owned()),
        (Some(binding), []) => {
            let Some(ast::Expr::AttrSet(inputs)) = binding.value() else {
                return Err("its `inputs` isn't a plain set".to_owned());
            };
            let present = inputs.entries().any(|entry| match entry {
                ast::Entry::AttrpathValue(input) => {
                    attr_names(&input).first().map(String::as_str) == Some(INPUT_NAME)
                }
                ast::Entry::Inherit(inherit) => {
                    inherit.attrs().any(|attr| attr_name(&attr) == INPUT_NAME)
                }
            });
            if !present {
                edits.prepend_to_set(&inputs, &line);
            }
            Ok(())
        }
        (None, [(_, first), ..]) => {
            let present = dotted
                .iter()
                .any(|(names, _)| names.get(1).map(String::as_str) == Some(INPUT_NAME));
            if !present {
                edits.insert_before(first.syntax(), &format!("inputs.{line}"));
            }
            Ok(())
        }
        (None, []) => {
            edits.prepend_to_set(set, &format!("inputs.{line}"));
            Ok(())
        }
    }
}

fn add_module(edits: &mut Edits, set: &ast::AttrSet, module: &str) -> Result<(), String> {
    let Some(outputs) = find_binding(set, "outputs")? else {
        return Err("it has no `outputs`".to_owned());
    };
    let ast::Expr::Lambda(outputs) = outputs else {
        return Err("its `outputs` isn't a plain function".to_owned());
    };

    let module = match outputs.param() {
        Some(ast::Param::IdentParam(param)) => {
            let name = param.syntax().text().to_string();
            format!("{name}.{module}")
        }
        Some(ast::Param::Pattern(pattern)) => {
            let named = pattern.pat_entries().any(|entry| {
                entry
                    .ident()
                    .is_some_and(|ident| ident.syntax().text() == INPUT_NAME)
            });
            if !named {
                edits.add_to_pattern(&pattern, INPUT_NAME);
            }
            module.to_owned()
        }
        None => return Err("its `outputs` takes no arguments".to_owned()),
    };

    let body = outputs
        .body()
        .ok_or("its `outputs` has no body".to_owned())?;
    let systems: Vec<ast::Apply> = body
        .syntax()
        .descendants()
        .filter_map(ast::Apply::cast)
        .filter(|apply| {
            apply.lambda().is_some_and(|lambda| {
                lambda.syntax().text().to_string().rsplit('.').next() == Some("nixosSystem")
            })
        })
        .collect();
    let system = match systems.as_slice() {
        [system] => system,
        [] => return Err("Slipway couldn't find a `nixosSystem` in its `outputs`".to_owned()),
        [_, _, ..] => {
            return Err(
                "it defines more than one system, and Slipway can't tell which is this computer"
                    .to_owned(),
            );
        }
    };
    let Some(ast::Expr::AttrSet(arguments)) = system.argument().and_then(unparen) else {
        return Err("its `nixosSystem` isn't given a plain set".to_owned());
    };

    match find_binding(&arguments, "modules")? {
        Some(ast::Expr::List(modules)) => {
            let present = modules.items().any(|item| {
                item.syntax()
                    .text()
                    .to_string()
                    .contains(&format!("{INPUT_NAME}.nixosModules"))
            });
            if !present {
                edits.prepend_to_list(&modules, &module);
            }
        }
        Some(_) => return Err("its `modules` isn't a plain list".to_owned()),
        None => edits.prepend_to_set(&arguments, &format!("modules = [ {module} ];")),
    }
    Ok(())
}

fn parse(source: &str) -> Option<Root> {
    Root::parse(source).ok().ok()
}

/// The set of settings a NixOS module returns, looking past the function
/// arguments, `let`, `with` and brackets around it.
fn module_body(expr: ast::Expr) -> Option<ast::AttrSet> {
    match expr {
        ast::Expr::AttrSet(set) => Some(set),
        ast::Expr::Lambda(lambda) => module_body(lambda.body()?),
        ast::Expr::LetIn(let_in) => module_body(let_in.body()?),
        ast::Expr::With(with) => module_body(with.body()?),
        ast::Expr::Assert(assert) => module_body(assert.body()?),
        ast::Expr::Paren(paren) => module_body(paren.expr()?),
        _ => None,
    }
}

fn unparen(expr: ast::Expr) -> Option<ast::Expr> {
    match expr {
        ast::Expr::Paren(paren) => unparen(paren.expr()?),
        expr => Some(expr),
    }
}

/// The value of the one binding named `name` in `set`, refusing names set
/// in a way Slipway can't add to (inherited, split up, or set twice).
fn find_binding(set: &ast::AttrSet, name: &str) -> Result<Option<ast::Expr>, String> {
    let mut found = None;
    for entry in set.entries() {
        match entry {
            ast::Entry::Inherit(inherit) => {
                if inherit.attrs().any(|attr| attr_name(&attr) == name) {
                    return Err(format!("its `{name}` is inherited from elsewhere"));
                }
            }
            ast::Entry::AttrpathValue(binding) => {
                let names = attr_names(&binding);
                if names.first().map(String::as_str) != Some(name) {
                    continue;
                }
                if names.len() > 1 || found.is_some() {
                    return Err(format!("its `{name}` is set in pieces"));
                }
                found = Some(
                    binding
                        .value()
                        .ok_or(format!("its `{name}` has no value"))?,
                );
            }
        }
    }
    Ok(found)
}

fn attr_names(binding: &ast::AttrpathValue) -> Vec<String> {
    binding
        .attrpath()
        .map(|path| path.attrs().map(|attr| attr_name(&attr)).collect())
        .unwrap_or_default()
}

fn attr_name(attr: &ast::Attr) -> String {
    attr.syntax()
        .text()
        .to_string()
        .trim_matches('"')
        .to_owned()
}

/// Whether an `imports` entry is the managed folder, however it's spelled:
/// `./slipway`, `./slipway/default.nix` or its full path.
fn names_link(item: &ast::Expr, link: &Link) -> bool {
    if !matches!(item, ast::Expr::PathRel(_) | ast::Expr::PathAbs(_)) {
        return false;
    }
    let text = item.syntax().text().to_string();
    let text = text.strip_suffix("/default.nix").unwrap_or(&text);
    text == link.relative || Path::new(text) == link.absolute
}

/// Insertions into a source file, applied all at once.
struct Edits<'a> {
    source: &'a str,
    inserts: Vec<(usize, String)>,
}

impl<'a> Edits<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            inserts: Vec::new(),
        }
    }

    fn is_empty(&self) -> bool {
        self.inserts.is_empty()
    }

    fn insert(&mut self, at: TextSize, text: String) {
        self.inserts.push((usize::from(at), text));
    }

    /// The edited text, or `None` if it no longer parses.
    fn finish(mut self) -> Option<String> {
        self.inserts.sort_by_key(|(at, _)| *at);
        let mut out = String::with_capacity(self.source.len() + 128);
        let mut from = 0;
        for (at, text) in &self.inserts {
            out.push_str(&self.source[from..*at]);
            out.push_str(text);
            from = *at;
        }
        out.push_str(&self.source[from..]);
        parse(&out).map(|_| out)
    }

    /// The whitespace a line starts with.
    fn indent_of(&self, at: TextSize) -> &'a str {
        let at = usize::from(at);
        let line_start = self.source[..at].rfind('\n').map_or(0, |i| i + 1);
        let line = &self.source[line_start..];
        &line[..line.len() - line.trim_start_matches([' ', '\t']).len()]
    }

    /// Whether only whitespace comes before `at` on its line.
    fn starts_line(&self, at: TextSize) -> bool {
        let at = usize::from(at);
        let line_start = self.source[..at].rfind('\n').map_or(0, |i| i + 1);
        self.source[line_start..at].trim().is_empty()
    }

    /// Puts `text` just before `node`: on a line of its own if `node` starts
    /// one, or before it on the same line otherwise.
    fn insert_before(&mut self, node: &SyntaxNode, text: &str) {
        let at = node.text_range().start();
        let text = if self.starts_line(at) {
            format!("{text}\n{}", self.indent_of(at))
        } else {
            format!("{text} ")
        };
        self.insert(at, text);
    }

    /// Makes `item` the first item of `list`.
    fn prepend_to_list(&mut self, list: &ast::List, item: &str) {
        if let Some(first) = list.items().next() {
            self.insert_before(first.syntax(), item);
            return;
        }
        let (Some(open), Some(close)) = (list.l_brack_token(), list.r_brack_token()) else {
            return;
        };
        let inside = &self.source
            [usize::from(open.text_range().end())..usize::from(close.text_range().start())];
        if inside.contains('\n') {
            let indent = format!("{}  ", self.indent_of(open.text_range().start()));
            self.insert(open.text_range().end(), format!("\n{indent}{item}"));
        } else if inside.is_empty() {
            self.insert(open.text_range().end(), format!(" {item} "));
        } else {
            self.insert(close.text_range().start(), format!("{item} "));
        }
    }

    /// Makes `binding` the first line inside `set`, keeping any comment that
    /// introduces the set's first entry with that entry.
    fn prepend_to_set(&mut self, set: &ast::AttrSet, binding: &str) {
        let Some(open) = set.l_curly_token() else {
            return;
        };
        let at = open.text_range().end();
        match set.entries().next() {
            Some(first) if self.starts_line(first.syntax().text_range().start()) => {
                let indent = self.indent_of(first.syntax().text_range().start());
                self.insert(at, format!("\n{indent}{binding}"));
            }
            _ => self.insert(at, format!(" {binding}")),
        }
    }

    /// Adds an argument to a function's `{ ... }` pattern, after the last
    /// named one and in the same layout.
    fn add_to_pattern(&mut self, pattern: &ast::Pattern, name: &str) {
        let entries: Vec<ast::PatEntry> = pattern.pat_entries().collect();
        let Some(last) = entries.last() else {
            match pattern.ellipsis_token() {
                Some(ellipsis) => self.insert(ellipsis.text_range().start(), format!("{name}, ")),
                None => {
                    if let Some(open) = child_token(pattern.syntax(), SyntaxKind::TOKEN_L_BRACE) {
                        self.insert(open.text_range().end(), format!(" {name}"));
                    }
                }
            }
            return;
        };

        let after_last = last.syntax().text_range().end();
        if !self.starts_line(entries[0].syntax().text_range().start()) {
            self.insert(after_last, format!(", {name}"));
            return;
        }
        let indent = self.indent_of(last.syntax().text_range().start());
        match next_significant(last.syntax()) {
            Some(comma) if comma.kind() == SyntaxKind::TOKEN_COMMA => {
                self.insert(comma.text_range().end(), format!("\n{indent}{name},"));
            }
            _ => self.insert(after_last, format!(",\n{indent}{name}")),
        }
    }
}

fn child_token(node: &SyntaxNode, kind: SyntaxKind) -> Option<SyntaxToken> {
    node.children_with_tokens()
        .filter_map(|child| child.into_token())
        .find(|token| token.kind() == kind)
}

/// The next token after `node` that isn't whitespace or a comment.
fn next_significant(node: &SyntaxNode) -> Option<SyntaxToken> {
    std::iter::successors(node.next_sibling_or_token(), |element| {
        element.next_sibling_or_token()
    })
    .filter_map(|element| element.into_token())
    .find(|token| {
        !matches!(
            token.kind(),
            SyntaxKind::TOKEN_WHITESPACE | SyntaxKind::TOKEN_COMMENT
        )
    })
}
