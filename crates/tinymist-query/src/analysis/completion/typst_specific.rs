//! Completion by typst specific semantics, like `font`, `package`, `label`, or
//! `typst::foundations::Value`.

use super::*;
impl CompletionPair<'_, '_, '_> {
    /// Add completions for all font families.
    pub fn font_completions(&mut self) {
        let equation = self.cursor.before_window(25).contains("equation");
        for (family, iter) in self.worker.world().clone().book().families() {
            let detail = summarize_font_family(iter);
            if !equation || family.contains("Math") {
                self.value_completion(
                    None,
                    &Value::Str(family.into()),
                    false,
                    Some(detail.as_str()),
                );
            }
        }
    }

    /// Add completions for current font features.
    pub fn font_feature_completions(&mut self) {
        // todo: add me
    }

    /// Add completions for all available packages.
    pub fn package_completions(&mut self, all_versions: bool) {
        let w = self.worker.world().clone();
        let mut packages: Vec<_> = w
            .packages()
            .iter()
            .map(|(spec, desc)| (spec, desc.clone()))
            .collect();
        // local_packages to references and add them to the packages
        let local_packages_refs = self.worker.ctx.local_packages();
        packages.extend(
            local_packages_refs
                .iter()
                .map(|spec| (spec, Some(eco_format!("{} v{}", spec.name, spec.version)))),
        );

        packages.sort_by_key(|(spec, _)| (&spec.namespace, &spec.name, Reverse(spec.version)));
        if !all_versions {
            packages.dedup_by_key(|(spec, _)| (&spec.namespace, &spec.name));
        }
        for (package, description) in packages {
            self.value_completion(
                None,
                &Value::Str(format_str!("{package}")),
                false,
                description.as_deref(),
            );
        }
    }

    /// Add completions for raw block tags.
    pub fn raw_completions(&mut self) {
        for (name, mut tags) in RawElem::languages() {
            let lower = name.to_lowercase();
            if !tags.contains(&lower.as_str()) {
                tags.push(lower.as_str());
            }

            tags.retain(|tag| is_ident(tag));
            if tags.is_empty() {
                continue;
            }

            self.push_completion(Completion {
                kind: CompletionKind::Constant,
                label: name.into(),
                apply: Some(tags[0].into()),
                detail: Some(repr::separated_list(&tags, " or ").into()),
                ..Completion::default()
            });
        }
    }

    /// Add completions for labels and references.
    pub fn ref_completions(&mut self) {
        self.label_completions_(false, true);
    }

    /// Add completions for labels and references.
    pub fn label_completions(&mut self, only_citation: bool) {
        self.label_completions_(only_citation, false);
    }

    /// Add completions for labels and references.
    pub fn label_completions_(&mut self, only_citation: bool, ref_label: bool) {
        let Some(document) = self.worker.document else {
            return;
        };
        let (labels, split) = analyze_labels(&self.worker.ctx.shared, document);

        let head = &self.cursor.text[..self.cursor.from];
        let at = head.ends_with('@');
        let open = !at && !head.ends_with('<');
        let close = !at && !self.cursor.after.starts_with('>');
        let citation = !at && only_citation;

        let (skip, take) = if at || ref_label {
            (0, usize::MAX)
        } else if citation {
            (split, usize::MAX)
        } else {
            (0, split)
        };

        for DynLabel {
            label,
            label_desc,
            detail,
            bib_title,
        } in labels.into_iter().skip(skip).take(take)
        {
            if !self.worker.seen_casts.insert(hash128(&label)) {
                continue;
            }
            let label: EcoString = label.resolve().as_str().into();
            let completion = Completion {
                kind: CompletionKind::Reference,
                apply: Some(eco_format!(
                    "{}{}{}",
                    if open { "<" } else { "" },
                    label.as_str(),
                    if close { ">" } else { "" }
                )),
                label: label.clone(),
                label_details: label_desc.clone(),
                filter_text: Some(label.clone()),
                detail: detail.clone(),
                ..Completion::default()
            };

            if let Some(bib_title) = bib_title {
                // Note that this completion re-uses the above `apply` field to
                // alter the `bib_title` to the corresponding label.
                self.push_completion(Completion {
                    kind: CompletionKind::Constant,
                    label: bib_title.clone(),
                    label_details: Some(label),
                    filter_text: Some(bib_title),
                    detail,
                    ..completion.clone()
                });
            }

            self.push_completion(completion);
        }
    }

    /// Add a completion for a specific value.
    pub fn value_completion(
        &mut self,
        label: Option<EcoString>,
        value: &Value,
        parens: bool,
        docs: Option<&str>,
    ) {
        self.value_completion_(
            value,
            ValueCompletionInfo {
                label,
                parens,
                label_details: None,
                docs,
                bound_self: false,
            },
        );
    }

    /// Add a completion for a specific value.
    pub fn value_completion_(&mut self, value: &Value, extras: ValueCompletionInfo) {
        let ValueCompletionInfo {
            label,
            parens,
            label_details,
            docs,
            bound_self,
        } = extras;

        // Prevent duplicate completions from appearing.
        if !self.worker.seen_casts.insert(hash128(&(&label, &value))) {
            return;
        }

        let at = label.as_deref().is_some_and(|field| !is_ident(field));
        let label = label.unwrap_or_else(|| value.repr());

        let detail = docs.map(Into::into).or_else(|| match value {
            Value::Symbol(symbol) => Some(symbol_detail(symbol.get())),
            Value::Func(func) => func.docs().map(plain_docs_sentence),
            Value::Type(ty) => Some(plain_docs_sentence(ty.docs())),
            v => {
                let repr = v.repr();
                (repr.as_str() != label).then_some(repr)
            }
        });
        let label_details = label_details.or_else(|| match value {
            Value::Symbol(s) => Some(symbol_label_detail(s.get())),
            _ => None,
        });

        let mut apply = None;
        if parens && matches!(value, Value::Func(_)) {
            let mode = self.cursor.leaf_mode();
            let kind_checker = CompletionKindChecker {
                symbols: HashSet::default(),
                functions: HashSet::from_iter([Ty::Value(InsTy::new(value.clone()))]),
            };
            let mut fn_feat = FnCompletionFeat::default();
            // todo: unify bound self checking
            fn_feat.bound_self = bound_self;
            let fn_feat = fn_feat.check(kind_checker.functions.iter());
            self.func_completion(mode, fn_feat, label, label_details, detail, parens);
            return;
        } else if at {
            apply = Some(eco_format!("at(\"{label}\")"));
        } else {
            let apply_label = &mut label.as_str();
            if apply_label.ends_with('"') && self.cursor.after.starts_with('"') {
                if let Some(trimmed) = apply_label.strip_suffix('"') {
                    *apply_label = trimmed;
                }
            }
            let from_before = slice_at(self.cursor.text, 0..self.cursor.from);
            if apply_label.starts_with('"') && from_before.ends_with('"') {
                if let Some(trimmed) = apply_label.strip_prefix('"') {
                    *apply_label = trimmed;
                }
            }

            if apply_label.len() != label.len() {
                apply = Some((*apply_label).into());
            }
        }

        self.push_completion(Completion {
            kind: value_to_completion_kind(value),
            label,
            apply,
            detail,
            label_details,
            ..Completion::default()
        });
    }
}

#[derive(Debug, Clone, Default)]
pub struct ValueCompletionInfo<'a> {
    pub label: Option<EcoString>,
    pub parens: bool,
    pub label_details: Option<EcoString>,
    pub docs: Option<&'a str>,
    pub bound_self: bool,
}
