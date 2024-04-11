use std::collections::BTreeMap;

use ecow::{eco_format, EcoString};
use lsp_types::{CompletionItem, CompletionTextEdit, InsertTextFormat, TextEdit};
use reflexo::path::{unix_slash, PathClean};
use typst::foundations::{AutoValue, Func, Label, NoneValue, Type, Value};
use typst::syntax::ast::AstNode;
use typst::syntax::{ast, SyntaxKind};
use typst::visualize::Color;

use super::{Completion, CompletionContext, CompletionKind};
use crate::analysis::{
    analyze_dyn_signature, analyze_import, resolve_callee, FlowBuiltinType, FlowType,
    PathPreference,
};
use crate::syntax::param_index_at_leaf;
use crate::upstream::plain_docs_sentence;

use crate::{prelude::*, typst_to_lsp::completion_kind, LspCompletion};

impl<'a, 'w> CompletionContext<'a, 'w> {
    pub fn world(&self) -> &'w dyn typst::World {
        self.ctx.world()
    }

    /// Add completions for definitions that are available at the cursor.
    ///
    /// Filters the global/math scope with the given filter.
    pub fn scope_completions_(&mut self, parens: bool, filter: impl Fn(&Value) -> bool) {
        let mut defined = BTreeMap::new();
        let mut try_insert = |name: EcoString, kind: CompletionKind| {
            if name.is_empty() {
                return;
            }

            if let std::collections::btree_map::Entry::Vacant(entry) = defined.entry(name) {
                entry.insert(kind);
            }
        };

        let mut ancestor = Some(self.leaf.clone());
        while let Some(node) = &ancestor {
            let mut sibling = Some(node.clone());
            while let Some(node) = &sibling {
                if let Some(v) = node.cast::<ast::LetBinding>() {
                    let kind = match v.kind() {
                        ast::LetBindingKind::Closure(..) => CompletionKind::Func,
                        ast::LetBindingKind::Normal(..) => CompletionKind::Variable,
                    };
                    for ident in v.kind().bindings() {
                        try_insert(ident.get().clone(), kind.clone());
                    }
                }

                // todo: cache
                if let Some(v) = node.cast::<ast::ModuleImport>() {
                    let imports = v.imports();
                    let anaylyze = node.children().find(|child| child.is::<ast::Expr>());
                    let analyzed = anaylyze
                        .as_ref()
                        .and_then(|source| analyze_import(self.world(), source));
                    if analyzed.is_none() {
                        log::debug!("failed to analyze import: {:?}", anaylyze);
                    }
                    if let Some(value) = analyzed {
                        if imports.is_none() {
                            if let Some(name) = value.name() {
                                try_insert(name.into(), CompletionKind::Module);
                            }
                        } else if let Some(scope) = value.scope() {
                            for (name, v) in scope.iter() {
                                let kind = match v {
                                    Value::Func(..) => CompletionKind::Func,
                                    Value::Module(..) => CompletionKind::Module,
                                    Value::Type(..) => CompletionKind::Type,
                                    _ => CompletionKind::Constant,
                                };
                                try_insert(name.clone(), kind);
                            }
                        }
                    }
                }

                sibling = node.prev_sibling();
            }

            if let Some(parent) = node.parent() {
                if let Some(v) = parent.cast::<ast::ForLoop>() {
                    if node.prev_sibling_kind() != Some(SyntaxKind::In) {
                        let pattern = v.pattern();
                        for ident in pattern.bindings() {
                            try_insert(ident.get().clone(), CompletionKind::Variable);
                        }
                    }
                }
                if let Some(v) = node.cast::<ast::Closure>() {
                    for param in v.params().children() {
                        match param {
                            ast::Param::Pos(pattern) => {
                                for ident in pattern.bindings() {
                                    try_insert(ident.get().clone(), CompletionKind::Variable);
                                }
                            }
                            ast::Param::Named(n) => {
                                try_insert(n.name().get().clone(), CompletionKind::Variable)
                            }
                            ast::Param::Spread(s) => {
                                if let Some(sink_ident) = s.sink_ident() {
                                    try_insert(sink_ident.get().clone(), CompletionKind::Variable)
                                }
                            }
                        }
                    }
                }

                ancestor = Some(parent.clone());
                continue;
            }

            break;
        }

        let in_math = matches!(
            self.leaf.parent_kind(),
            Some(SyntaxKind::Equation)
                | Some(SyntaxKind::Math)
                | Some(SyntaxKind::MathFrac)
                | Some(SyntaxKind::MathAttach)
        );

        let lib = self.world().library();
        let scope = if in_math { &lib.math } else { &lib.global }
            .scope()
            .clone();
        for (name, value) in scope.iter() {
            if filter(value) && !defined.contains_key(name) {
                self.value_completion(Some(name.clone()), value, parens, None);
            }
        }

        for (name, kind) in defined {
            if !name.is_empty() {
                if kind == CompletionKind::Func {
                    // todo: check arguments, if empty, jump to after the parens
                    let apply = eco_format!("{}(${{}})", name);
                    self.completions.push(Completion {
                        kind,
                        label: name,
                        apply: Some(apply),
                        detail: None,
                        // todo: only vscode and neovim (0.9.1) support this
                        command: Some("editor.action.triggerSuggest"),
                    });
                } else {
                    self.completions.push(Completion {
                        kind,
                        label: name,
                        apply: None,
                        detail: None,
                        command: None,
                    });
                }
            }
        }
    }
}

/// Add completions for the parameters of a function.
pub fn param_completions<'a>(
    ctx: &mut CompletionContext<'a, '_>,
    callee: ast::Expr<'a>,
    set: bool,
    args: ast::Args<'a>,
) {
    let Some(func) = ctx
        .root
        .find(callee.span())
        .and_then(|callee| resolve_callee(ctx.ctx, callee))
    else {
        return;
    };

    use typst::foundations::func::Repr;
    let mut func = func;
    while let Repr::With(f) = func.inner() {
        // todo: complete with positional arguments
        // with_args.push(ArgValue::Instance(f.1.clone()));
        func = f.0.clone();
    }

    let pos_index = param_index_at_leaf(&ctx.leaf, &func, args);

    let signature = analyze_dyn_signature(ctx.ctx, func.clone());

    // Exclude named arguments which are already present.
    let exclude: Vec<_> = args
        .items()
        .filter_map(|arg| match arg {
            ast::Arg::Named(named) => Some(named.name()),
            _ => None,
        })
        .collect();

    let primary_sig = signature.primary();

    log::debug!("pos_param_completion: {:?}", pos_index);

    if let Some(pos_index) = pos_index {
        let pos = primary_sig.pos.get(pos_index);
        log::debug!("pos_param_completion_to: {:?}", pos);

        if let Some(pos) = pos {
            if set && !pos.settable {
                return;
            }

            if pos.positional && type_completion(ctx, pos.infer_type.as_ref()).is_none() {
                ctx.cast_completions(&pos.input);
            }
        }
    }

    for (name, param) in &primary_sig.named {
        if exclude.iter().any(|ident| ident.as_str() == name) {
            continue;
        }

        if set && !param.settable {
            continue;
        }

        if param.named {
            ctx.completions.push(Completion {
                kind: CompletionKind::Param,
                label: param.name.clone().into(),
                apply: Some(eco_format!("{}: ${{}}", param.name)),
                detail: Some(plain_docs_sentence(&param.docs)),
                // todo: only vscode and neovim (0.9.1) support this
                //
                // VS Code doesn't do that... Auto triggering suggestion only happens on typing
                // (word starts or trigger characters). However, you can use
                // editor.action.triggerSuggest as command on a suggestion to
                // "manually" retrigger suggest after inserting one
                command: Some("editor.action.triggerSuggest"),
            });
        }

        if param.positional && type_completion(ctx, param.infer_type.as_ref()).is_none() {
            ctx.cast_completions(&param.input);
        }
    }

    if ctx.before.ends_with(',') {
        ctx.enrich(" ", "");
    }
}

fn type_completion(
    ctx: &mut CompletionContext<'_, '_>,
    infer_type: Option<&FlowType>,
) -> Option<()> {
    // Prevent duplicate completions from appearing.
    if !ctx.seen_casts.insert(typst::util::hash128(&infer_type)) {
        return Some(());
    }

    log::debug!("type_completion: {:?}", infer_type);

    match infer_type? {
        FlowType::Clause => return None,
        FlowType::Undef => return None,
        FlowType::Content => return None,
        FlowType::Any => return None,
        FlowType::Array => {
            ctx.snippet_completion("()", "(${v})", "An array.");
        }
        FlowType::Dict => {
            ctx.snippet_completion("()", "(${v})", "A dictionary.");
        }
        FlowType::None => ctx.snippet_completion("none", "none", "Nothing."),
        FlowType::Infer => return None,
        FlowType::FlowNone => return None,
        FlowType::Auto => {
            ctx.snippet_completion("auto", "auto", "A smart default.");
        }
        FlowType::Builtin(v) => match v {
            FlowBuiltinType::Path(p) => {
                let source = ctx.ctx.source_by_id(ctx.root.span().id()?).ok()?;

                log::debug!(
                    "type_path_completion: {:?}",
                    &source.text()[ctx.cursor - 10..ctx.cursor]
                );
                ctx.completions2.extend(
                    complete_path(ctx.ctx, None, &source, ctx.cursor, p)
                        .into_iter()
                        .flatten(),
                );
            }
            FlowBuiltinType::Args => return None,
        },
        FlowType::Args(_) => return None,
        FlowType::Func(_) => return None,
        FlowType::With(_) => return None,
        FlowType::At(_) => return None,
        FlowType::Union(u) => {
            for info in u.as_ref() {
                type_completion(ctx, Some(info));
            }
        }
        FlowType::Let(_) => return None,
        FlowType::Var(_) => return None,
        FlowType::Unary(_) => return None,
        FlowType::Binary(_) => return None,
        FlowType::Value(v) => {
            if let Value::Type(ty) = v.as_ref() {
                if *ty == Type::of::<NoneValue>() {
                    ctx.snippet_completion("none", "none", "Nothing.")
                } else if *ty == Type::of::<AutoValue>() {
                    ctx.snippet_completion("auto", "auto", "A smart default.");
                } else if *ty == Type::of::<bool>() {
                    ctx.snippet_completion("false", "false", "No / Disabled.");
                    ctx.snippet_completion("true", "true", "Yes / Enabled.");
                } else if *ty == Type::of::<Color>() {
                    ctx.snippet_completion("luma()", "luma(${v})", "A custom grayscale color.");
                    ctx.snippet_completion(
                        "rgb()",
                        "rgb(${r}, ${g}, ${b}, ${a})",
                        "A custom RGBA color.",
                    );
                    ctx.snippet_completion(
                        "cmyk()",
                        "cmyk(${c}, ${m}, ${y}, ${k})",
                        "A custom CMYK color.",
                    );
                    ctx.snippet_completion(
                        "oklab()",
                        "oklab(${l}, ${a}, ${b}, ${alpha})",
                        "A custom Oklab color.",
                    );
                    ctx.snippet_completion(
                        "oklch()",
                        "oklch(${l}, ${chroma}, ${hue}, ${alpha})",
                        "A custom Oklch color.",
                    );
                    ctx.snippet_completion(
                        "color.linear-rgb()",
                        "color.linear-rgb(${r}, ${g}, ${b}, ${a})",
                        "A custom linear RGBA color.",
                    );
                    ctx.snippet_completion(
                        "color.hsv()",
                        "color.hsv(${h}, ${s}, ${v}, ${a})",
                        "A custom HSVA color.",
                    );
                    ctx.snippet_completion(
                        "color.hsl()",
                        "color.hsl(${h}, ${s}, ${l}, ${a})",
                        "A custom HSLA color.",
                    );
                    ctx.scope_completions_(false, |value| value.ty() == *ty);
                } else if *ty == Type::of::<Label>() {
                    ctx.label_completions()
                } else if *ty == Type::of::<Func>() {
                    ctx.snippet_completion(
                        "function",
                        "(${params}) => ${output}",
                        "A custom function.",
                    );
                } else {
                    ctx.completions.push(Completion {
                        kind: CompletionKind::Syntax,
                        label: ty.long_name().into(),
                        apply: Some(eco_format!("${{{ty}}}")),
                        detail: Some(eco_format!("A value of type {ty}.")),
                        command: None,
                    });
                    ctx.scope_completions_(false, |value| value.ty() == *ty);
                }
            } else {
                ctx.value_completion(None, v, true, None);
            }
        }
        FlowType::ValueDoc(v) => {
            let (value, docs) = v.as_ref();
            ctx.value_completion(None, value, true, Some(docs));
        }
        FlowType::Element(e) => {
            ctx.value_completion(Some(e.name().into()), &Value::Func((*e).into()), true, None);
        } // CastInfo::Any => {}
    };

    Some(())
}

/// Add completions for the values of a named function parameter.
pub fn named_param_value_completions<'a>(
    ctx: &mut CompletionContext<'a, '_>,
    callee: ast::Expr<'a>,
    name: &str,
) {
    let Some(func) = ctx
        .root
        .find(callee.span())
        .and_then(|callee| resolve_callee(ctx.ctx, callee))
    else {
        return;
    };

    use typst::foundations::func::Repr;
    let mut func = func;
    while let Repr::With(f) = func.inner() {
        // todo: complete with positional arguments
        // with_args.push(ArgValue::Instance(f.1.clone()));
        func = f.0.clone();
    }

    let signature = analyze_dyn_signature(ctx.ctx, func.clone());

    let primary_sig = signature.primary();

    let Some(param) = primary_sig.named.get(name) else {
        return;
    };
    if !param.named {
        return;
    }

    if let Some(expr) = &param.expr {
        ctx.completions.push(Completion {
            kind: CompletionKind::Constant,
            label: expr.clone(),
            apply: None,
            detail: Some(plain_docs_sentence(&param.docs)),
            command: None,
        });
    }

    if type_completion(ctx, param.infer_type.as_ref()).is_none() {
        ctx.cast_completions(&param.input);
    }
    if name == "font" {
        ctx.font_completions();
    }

    if ctx.before.ends_with(':') {
        ctx.enrich(" ", "");
    }
}

pub fn complete_path(
    ctx: &AnalysisContext,
    v: Option<LinkedNode>,
    source: &Source,
    cursor: usize,
    p: &PathPreference,
) -> Option<Vec<CompletionItem>> {
    let id = source.id();
    if id.package().is_some() {
        return None;
    }

    let is_in_text;
    let text;
    let rng;
    if let Some(v) = v {
        let vp = v.cast::<ast::Str>()?;
        // todo: path escape
        let real_content = vp.get();
        let str_content = v.text();
        let unquoted = &str_content[1..str_content.len() - 1];
        if unquoted != real_content {
            return None;
        }

        let vr = v.range();
        let offset = vr.start + 1;
        if cursor < offset || vr.end <= cursor || vr.len() < 2 {
            return None;
        }

        text = &source.text()[offset..cursor];
        rng = offset..vr.end - 1;
        is_in_text = true;
    } else {
        text = "";
        rng = cursor..cursor;
        is_in_text = false;
    }
    let path = Path::new(&text);
    let has_root = path.has_root();

    let src_path = id.vpath();
    let base = src_path.resolve(&ctx.analysis.root)?;
    let dst_path = src_path.join(path);
    let mut compl_path = dst_path.as_rootless_path();
    if !compl_path.is_dir() {
        compl_path = compl_path.parent().unwrap_or(Path::new(""));
    }
    log::debug!("compl_path: {src_path:?} + {path:?} -> {compl_path:?}");

    if compl_path.is_absolute() {
        log::warn!("absolute path completion is not supported for security consideration {path:?}");
        return None;
    }

    let dirs = ctx.analysis.root.join(compl_path);
    log::debug!("compl_dirs: {dirs:?}");
    // find directory or files in the path
    let mut folder_completions = vec![];
    let mut module_completions = vec![];
    // todo: test it correctly
    for entry in dirs.read_dir().ok()? {
        let Ok(entry) = entry else {
            continue;
        };
        let path = entry.path();
        log::debug!("compl_check_path: {path:?}");
        if !path.is_dir() && !path.extension().is_some_and(|ext| p.match_ext(ext)) {
            continue;
        }
        if path
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with('.'))
        {
            continue;
        }

        // diff with root
        let path = dirs.join(path);

        // Skip self smartly
        if path.clean() == base.clean() {
            continue;
        }

        let label = if has_root {
            // diff with root
            let w = path.strip_prefix(&ctx.analysis.root).ok()?;
            eco_format!("/{}", unix_slash(w))
        } else {
            let base = base.parent()?;
            let w = pathdiff::diff_paths(&path, base)?;
            unix_slash(&w).into()
        };
        log::debug!("compl_label: {label:?}");

        if path.is_dir() {
            folder_completions.push(Completion {
                label,
                kind: CompletionKind::Folder,
                apply: None,
                detail: None,
                command: None,
            });
        } else {
            let kind = if label.as_str().ends_with(".typ") {
                CompletionKind::Module
            } else {
                CompletionKind::File
            };
            module_completions.push(Completion {
                label,
                kind,
                apply: None,
                detail: None,
                command: None,
            });
        }
    }

    let replace_range = ctx.to_lsp_range(rng, source);

    module_completions.sort_by(|a, b| a.label.cmp(&b.label));
    folder_completions.sort_by(|a, b| a.label.cmp(&b.label));

    let mut sorter = 0;
    let digits = (module_completions.len() + folder_completions.len())
        .to_string()
        .len();
    let completions = module_completions.into_iter().chain(folder_completions);
    Some(
        completions
            .map(|typst_completion| {
                let lsp_snippet = typst_completion
                    .apply
                    .as_ref()
                    .unwrap_or(&typst_completion.label);
                let text_edit = CompletionTextEdit::Edit(TextEdit::new(
                    replace_range,
                    if is_in_text {
                        lsp_snippet.to_string()
                    } else {
                        format!(r#""{lsp_snippet}""#)
                    },
                ));

                let sort_text = format!("{sorter:0>digits$}");
                sorter += 1;

                let res = LspCompletion {
                    label: typst_completion.label.to_string(),
                    kind: Some(completion_kind(typst_completion.kind.clone())),
                    detail: typst_completion.detail.as_ref().map(String::from),
                    text_edit: Some(text_edit),
                    // don't sort me
                    sort_text: Some(sort_text),
                    filter_text: Some("".to_owned()),
                    insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
                    ..Default::default()
                };

                log::debug!("compl_res: {res:?}");

                res
            })
            .collect_vec(),
    )
}
