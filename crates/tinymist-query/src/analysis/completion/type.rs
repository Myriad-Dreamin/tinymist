//! Completion by types.

use super::*;

pub(crate) struct TypeCompletionWorker<'a, 'b, 'c, 'd> {
    pub base: &'d mut CompletionPair<'a, 'b, 'c>,
    pub filter: &'d dyn Fn(&Ty) -> bool,
}

impl TypeCompletionWorker<'_, '_, '_, '_> {
    fn snippet_completion(&mut self, label: &str, apply: &str, detail: &str) {
        if !(self.filter)(&Ty::Any) {
            return;
        }

        self.base.snippet_completion(label, apply, detail);
    }

    pub fn type_completion(&mut self, infer_type: &Ty, docs: Option<&str>) -> Option<()> {
        // Prevent duplicate completions from appearing.
        if !self.base.worker.seen_types.insert(infer_type.clone()) {
            return Some(());
        }

        crate::log_debug_ct!("type_completion: {infer_type:?}");

        match infer_type {
            Ty::Any => return None,
            Ty::Pattern(_) => return None,
            Ty::Args(_) => return None,
            Ty::Func(_) => return None,
            Ty::With(_) => return None,
            Ty::Select(_) => return None,
            Ty::Var(_) => return None,
            Ty::Unary(_) => return None,
            Ty::Binary(_) => return None,
            Ty::If(_) => return None,
            Ty::Union(u) => {
                for info in u.as_ref() {
                    self.type_completion(info, docs);
                }
            }
            Ty::Let(bounds) => {
                for ut in bounds.ubs.iter() {
                    self.type_completion(ut, docs);
                }
                for lt in bounds.lbs.iter() {
                    self.type_completion(lt, docs);
                }
            }
            Ty::Tuple(..) | Ty::Array(..) => {
                if !(self.filter)(infer_type) {
                    return None;
                }
                self.snippet_completion("()", "(${})", "An array.");
            }
            Ty::Dict(..) => {
                if !(self.filter)(infer_type) {
                    return None;
                }
                self.snippet_completion("()", "(${})", "A dictionary.");
            }
            Ty::Boolean(_b) => {
                if !(self.filter)(infer_type) {
                    return None;
                }
                self.snippet_completion("false", "false", "No / Disabled.");
                self.snippet_completion("true", "true", "Yes / Enabled.");
            }
            Ty::Builtin(v) => {
                if !(self.filter)(infer_type) {
                    return None;
                }
                self.builtin_type_completion(v, docs);
            }
            Ty::Value(v) => {
                if !(self.filter)(infer_type) {
                    return None;
                }
                let docs = v.syntax.as_ref().map(|s| s.doc.as_ref()).or(docs);

                if let Value::Type(ty) = &v.val {
                    self.type_completion(&Ty::Builtin(BuiltinTy::Type(*ty)), docs);
                } else if v.val.ty() == Type::of::<NoneValue>() {
                    self.type_completion(&Ty::Builtin(BuiltinTy::None), docs);
                } else if v.val.ty() == Type::of::<AutoValue>() {
                    self.type_completion(&Ty::Builtin(BuiltinTy::Auto), docs);
                } else {
                    self.base.value_completion(None, &v.val, true, docs);
                }
            }
            Ty::Param(param) => {
                // todo: variadic

                let docs = docs.or_else(|| param.docs.as_deref());
                if param.attrs.positional {
                    self.type_completion(&param.ty, docs);
                }
                if !param.attrs.named {
                    return Some(());
                }

                let field = &param.name;
                if self.base.worker.seen_field(field.clone()) {
                    return Some(());
                }
                if !(self.filter)(infer_type) {
                    return None;
                }

                let mut rev_stream = self.base.cursor.before.chars().rev();
                let ch = rev_stream.find(|ch| !typst::syntax::is_id_continue(*ch));
                // skip label/ref completion.
                // todo: more elegant way
                if matches!(ch, Some('<' | '@')) {
                    return Some(());
                }

                self.base.push_completion(Completion {
                    kind: CompletionKind::Field,
                    label: field.into(),
                    apply: Some(eco_format!("{}: ${{}}", field)),
                    label_details: param.ty.describe(),
                    detail: docs.map(Into::into),
                    command: self
                        .base
                        .worker
                        .ctx
                        .analysis
                        .trigger_on_snippet_with_param_hint(true)
                        .map(From::from),
                    ..Completion::default()
                });
            }
        };

        Some(())
    }

    pub fn builtin_type_completion(&mut self, v: &BuiltinTy, docs: Option<&str>) -> Option<()> {
        match v {
            BuiltinTy::None => self.snippet_completion("none", "none", "Nothing."),
            BuiltinTy::Auto => {
                self.snippet_completion("auto", "auto", "A smart default.");
            }
            BuiltinTy::Clause => return None,
            BuiltinTy::Undef => return None,
            BuiltinTy::Space => return None,
            BuiltinTy::Break => return None,
            BuiltinTy::Continue => return None,
            BuiltinTy::Content => return None,
            BuiltinTy::Infer => return None,
            BuiltinTy::FlowNone => return None,
            BuiltinTy::Tag(..) => return None,
            BuiltinTy::Module(..) => return None,

            BuiltinTy::Path(preference) => {
                let items = self.base.complete_path(preference);
                self.base
                    .worker
                    .completions
                    .extend(items.into_iter().flatten());
            }
            BuiltinTy::Args => return None,
            BuiltinTy::Stroke => {
                self.snippet_completion("stroke()", "stroke(${})", "Stroke type.");
                self.snippet_completion("()", "(${})", "Stroke dictionary.");
                self.type_completion(&Ty::Builtin(BuiltinTy::Color), docs);
                self.type_completion(&Ty::Builtin(BuiltinTy::Length), docs);
            }
            BuiltinTy::Color => {
                self.snippet_completion("luma()", "luma(${v})", "A custom grayscale color.");
                self.snippet_completion(
                    "rgb()",
                    "rgb(${r}, ${g}, ${b}, ${a})",
                    "A custom RGBA color.",
                );
                self.snippet_completion(
                    "cmyk()",
                    "cmyk(${c}, ${m}, ${y}, ${k})",
                    "A custom CMYK color.",
                );
                self.snippet_completion(
                    "oklab()",
                    "oklab(${l}, ${a}, ${b}, ${alpha})",
                    "A custom Oklab color.",
                );
                self.snippet_completion(
                    "oklch()",
                    "oklch(${l}, ${chroma}, ${hue}, ${alpha})",
                    "A custom Oklch color.",
                );
                self.snippet_completion(
                    "color.linear-rgb()",
                    "color.linear-rgb(${r}, ${g}, ${b}, ${a})",
                    "A custom linear RGBA color.",
                );
                self.snippet_completion(
                    "color.hsv()",
                    "color.hsv(${h}, ${s}, ${v}, ${a})",
                    "A custom HSVA color.",
                );
                self.snippet_completion(
                    "color.hsl()",
                    "color.hsl(${h}, ${s}, ${l}, ${a})",
                    "A custom HSLA color.",
                );
            }
            BuiltinTy::TextSize => return None,
            BuiltinTy::TextLang => {
                for (&key, desc) in rust_iso639::ALL_MAP.entries() {
                    let detail = eco_format!("An ISO 639-1/2/3 language code, {}.", desc.name);
                    self.base.push_completion(Completion {
                        kind: CompletionKind::Syntax,
                        label: key.to_lowercase().into(),
                        apply: Some(eco_format!("\"{}\"", key.to_lowercase())),
                        detail: Some(detail),
                        label_details: Some(desc.name.into()),
                        ..Completion::default()
                    });
                }
            }
            BuiltinTy::TextRegion => {
                for (&key, desc) in rust_iso3166::ALPHA2_MAP.entries() {
                    let detail = eco_format!("An ISO 3166-1 alpha-2 region code, {}.", desc.name);
                    self.base.push_completion(Completion {
                        kind: CompletionKind::Syntax,
                        label: key.to_lowercase().into(),
                        apply: Some(eco_format!("\"{}\"", key.to_lowercase())),
                        detail: Some(detail),
                        label_details: Some(desc.name.into()),
                        ..Completion::default()
                    });
                }
            }
            BuiltinTy::Dir => {}
            BuiltinTy::TextFont => {
                self.base.font_completions();
            }
            BuiltinTy::TextFeature => {
                self.base.font_feature_completions();
            }
            BuiltinTy::Margin => {
                self.snippet_completion("()", "(${})", "Margin dictionary.");
                self.type_completion(&Ty::Builtin(BuiltinTy::Length), docs);
            }
            BuiltinTy::Inset => {
                self.snippet_completion("()", "(${})", "Inset dictionary.");
                self.type_completion(&Ty::Builtin(BuiltinTy::Length), docs);
            }
            BuiltinTy::Outset => {
                self.snippet_completion("()", "(${})", "Outset dictionary.");
                self.type_completion(&Ty::Builtin(BuiltinTy::Length), docs);
            }
            BuiltinTy::Radius => {
                self.snippet_completion("()", "(${})", "Radius dictionary.");
                self.type_completion(&Ty::Builtin(BuiltinTy::Length), docs);
            }
            BuiltinTy::Length => {
                self.snippet_completion("pt", "${1}pt", "Point length unit.");
                self.snippet_completion("mm", "${1}mm", "Millimeter length unit.");
                self.snippet_completion("cm", "${1}cm", "Centimeter length unit.");
                self.snippet_completion("in", "${1}in", "Inch length unit.");
                self.snippet_completion("em", "${1}em", "Em length unit.");
                self.type_completion(&Ty::Builtin(BuiltinTy::Auto), docs);
            }
            BuiltinTy::Float => {
                self.snippet_completion(
                    "exponential notation",
                    "${1}e${0}",
                    "Exponential notation",
                );
            }
            BuiltinTy::Label => {
                self.base.label_completions(false);
            }
            BuiltinTy::CiteLabel => {
                self.base.label_completions(true);
            }
            BuiltinTy::RefLabel => {
                self.base.ref_completions();
            }
            BuiltinTy::TypeType(ty) | BuiltinTy::Type(ty) => {
                if *ty == Type::of::<NoneValue>() {
                    let docs = docs.or(Some("Nothing."));
                    self.type_completion(&Ty::Builtin(BuiltinTy::None), docs);
                } else if *ty == Type::of::<AutoValue>() {
                    let docs = docs.or(Some("A smart default."));
                    self.type_completion(&Ty::Builtin(BuiltinTy::Auto), docs);
                } else if *ty == Type::of::<bool>() {
                    self.snippet_completion("false", "false", "No / Disabled.");
                    self.snippet_completion("true", "true", "Yes / Enabled.");
                } else if *ty == Type::of::<Color>() {
                    self.type_completion(&Ty::Builtin(BuiltinTy::Color), docs);
                } else if *ty == Type::of::<Label>() {
                    self.base.label_completions(false)
                } else if *ty == Type::of::<Func>() {
                    self.snippet_completion(
                        "function",
                        "(${params}) => ${output}",
                        "A custom function.",
                    );
                } else if let Ok(cons) = ty.constructor() {
                    let docs = docs.or(cons.docs()).unwrap_or(ty.docs());
                    self.base.value_completion(
                        Some(ty.short_name().into()),
                        &Value::Func(cons),
                        true,
                        Some(docs),
                    );
                } else if ty.scope().iter().any(|(_, b)| {
                    if let Value::Func(f) = b.read() {
                        let pos = f
                            .params()
                            .and_then(|params| params.iter().find(|s| s.required));
                        pos.is_none_or(|pos| pos.name != "self")
                    } else {
                        true
                    }
                }) {
                    let docs = docs.unwrap_or(ty.docs());
                    self.base.push_completion(Completion {
                        kind: CompletionKind::Syntax,
                        label: ty.short_name().into(),
                        apply: Some(eco_format!("${{{ty}}}")),
                        detail: Some(docs.into()),
                        ..Completion::default()
                    });
                }
                // Otherwise, if the type doesn't have constructor and
                // associated scope, we do nothing here. For example,
                // - complete `content` doesn't make much sense.
                // - complete `color` is okay because it has associated
                //   constructors in scope.`
            }
            BuiltinTy::Element(elem) => {
                self.base.value_completion(
                    Some(elem.name().into()),
                    &Value::Func((*elem).into()),
                    true,
                    docs,
                );
            }
        };

        Some(())
    }
}
