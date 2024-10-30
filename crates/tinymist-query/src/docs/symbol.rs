use core::fmt;
use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};

use ecow::{eco_format, EcoString};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tinymist_world::base::{EntryState, ShadowApi, TaskInputs};
use tinymist_world::LspWorld;
use typst::foundations::{Bytes, Value};
use typst::syntax::LinkedNode;
use typst::{
    diag::StrResult,
    syntax::{FileId, VirtualPath},
};

use super::tidy::*;
use crate::analysis::{ParamAttrs, ParamSpec, Signature, ToFunc};
use crate::docs::library;
use crate::ty::{DocSource, Interned};
use crate::upstream::plain_docs_sentence;
use crate::{ty::Ty, AnalysisContext};

type TypeRepr = Option<(/* short */ String, /* long */ String)>;
type ShowTypeRepr<'a> = &'a mut dyn FnMut(Option<&Ty>) -> TypeRepr;

/// Kind of a docstring.
#[derive(Debug, Default, Clone, Copy, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DocStringKind {
    /// A docstring for a any constant.
    #[default]
    Constant,
    /// A docstring for a function.
    Function,
    /// A docstring for a variable.
    Variable,
    /// A docstring for a module.
    Module,
    /// A docstring for a struct.
    Struct,
    /// A docstring for a reference.
    Reference,
}

impl fmt::Display for DocStringKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Constant => write!(f, "constant"),
            Self::Function => write!(f, "function"),
            Self::Variable => write!(f, "variable"),
            Self::Module => write!(f, "module"),
            Self::Struct => write!(f, "struct"),
            Self::Reference => write!(f, "reference"),
        }
    }
}

/// Documentation about a symbol (without type information).
pub type UntypedSymbolDocs = SymbolDocsT<()>;
/// Documentation about a symbol.
pub type SymbolDocs = SymbolDocsT<TypeRepr>;

/// Documentation about a symbol.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum SymbolDocsT<T> {
    /// Documentation about a function.
    #[serde(rename = "func")]
    Function(Box<SignatureDocsT<T>>),
    /// Documentation about a variable.
    #[serde(rename = "var")]
    Variable(VarDocsT<T>),
    /// Documentation about a module.
    #[serde(rename = "module")]
    Module(TidyModuleDocs),
    /// Other kinds of documentation.
    #[serde(rename = "plain")]
    Plain {
        /// The content of the documentation.
        docs: EcoString,
    },
}

impl<T> SymbolDocsT<T> {
    /// Get the markdown representation of the documentation.
    pub fn docs(&self) -> &EcoString {
        match self {
            Self::Function(docs) => &docs.docs,
            Self::Variable(docs) => &docs.docs,
            Self::Module(docs) => &docs.docs,
            Self::Plain { docs } => docs,
        }
    }
}

pub(crate) fn symbol_docs(
    ctx: &mut AnalysisContext,
    kind: DocStringKind,
    sym_value: Option<&Value>,
    docs: Option<&str>,
    doc_ty: Option<ShowTypeRepr>,
) -> Result<SymbolDocs, String> {
    let signature =
        sym_value.and_then(|e| signature_docs(&ctx.signature_dyn(e.to_func()?), doc_ty));
    if let Some(signature) = signature {
        return Ok(SymbolDocs::Function(Box::new(signature)));
    }

    if let Some(docs) = &docs {
        match convert_docs(ctx.world(), docs) {
            Ok(content) => {
                let docs = identify_docs(kind, content.clone())
                    .unwrap_or(SymbolDocs::Plain { docs: content });
                return Ok(docs);
            }
            Err(e) => {
                let err = format!("failed to convert docs: {e}").replace(
                    "-->", "—>", // avoid markdown comment
                );
                log::error!("{err}");
                return Err(err);
            }
        }
    }

    Ok(SymbolDocs::Plain { docs: "".into() })
}

/// Describes a primary function signature.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureDocsT<T> {
    /// Documentation for the function.
    pub docs: EcoString,
    /// The positional parameters.
    pub pos: Vec<ParamDocsT<T>>,
    /// The named parameters.
    pub named: BTreeMap<Interned<str>, ParamDocsT<T>>,
    /// The rest parameter.
    pub rest: Option<ParamDocsT<T>>,
    /// The return type.
    pub ret_ty: T,
    /// The full documentation for the signature.
    #[serde(skip)]
    pub def_docs: OnceLock<String>,
}

impl SignatureDocsT<TypeRepr> {
    /// Get full documentation for the signature.
    pub fn def_docs(&self) -> &String {
        self.def_docs
            .get_or_init(|| plain_docs_sentence(&format!("{}", DefDocs(self))).into())
    }
}

struct DefDocs<'a>(&'a SignatureDocs);

impl fmt::Display for DefDocs<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let docs = self.0;
        let base_docs = docs.docs.trim();

        let has_params_docs = !docs.pos.is_empty() || !docs.named.is_empty() || docs.rest.is_some();

        if !base_docs.is_empty() {
            f.write_str(base_docs)?;

            if has_params_docs {
                f.write_str("\n\n")?;
            }
        }

        if has_params_docs {
            f.write_str("## Parameters")?;

            for p in &docs.pos {
                write!(f, "\n\n@positional `{}`", p.name)?;
                if !p.docs.is_empty() {
                    f.write_str(" — ")?;
                    f.write_str(&p.docs)?;
                }
            }

            for (name, p) in &docs.named {
                write!(f, "\n\n@named `{name}`")?;
                if !p.docs.is_empty() {
                    f.write_str(" — ")?;
                    f.write_str(&p.docs)?;
                }
            }

            if let Some(rest) = &docs.rest {
                write!(f, "\n\n@rest `{}`", rest.name)?;
                if !rest.docs.is_empty() {
                    f.write_str(" — ")?;
                    f.write_str(&rest.docs)?;
                }
            }
        }

        Ok(())
    }
}

/// Documentation about a signature.
pub type UntypedSignatureDocs = SignatureDocsT<()>;
/// Documentation about a signature.
pub type SignatureDocs = SignatureDocsT<TypeRepr>;

impl SignatureDocs {
    /// Get the markdown representation of the documentation.
    pub fn print(&self, f: &mut impl std::fmt::Write) -> fmt::Result {
        let mut is_first = true;
        let mut write_sep = |f: &mut dyn std::fmt::Write| {
            if is_first {
                is_first = false;
                return f.write_str("\n  ");
            }
            f.write_str(",\n  ")
        };

        f.write_char('(')?;
        for p in &self.pos {
            write_sep(f)?;
            f.write_str(&p.name)?;
            if let Some(t) = &p.cano_type {
                write!(f, ": {}", t.0)?;
            }
        }
        if let Some(rest) = &self.rest {
            write_sep(f)?;
            f.write_str("..")?;
            f.write_str(&rest.name)?;
            if let Some(t) = &rest.cano_type {
                write!(f, ": {}", t.0)?;
            }
        }

        if !self.named.is_empty() {
            let mut name_prints = vec![];
            for v in self.named.values() {
                let ty = v.cano_type.as_ref().map(|t| &t.0);
                name_prints.push((v.name.clone(), ty, v.default.clone()))
            }
            name_prints.sort();
            for (k, t, v) in name_prints {
                write_sep(f)?;
                let v = v.as_deref().unwrap_or("any");
                let mut v = v.trim();
                if v.starts_with('{') && v.ends_with('}') && v.len() > 30 {
                    v = "{ .. }"
                }
                if v.starts_with('`') && v.ends_with('`') && v.len() > 30 {
                    v = "raw"
                }
                if v.starts_with('[') && v.ends_with(']') && v.len() > 30 {
                    v = "content"
                }
                f.write_str(&k)?;
                if let Some(t) = t {
                    write!(f, ": {t}")?;
                }
                if v.contains('\n') {
                    write!(f, " = {}", v.replace("\n", "\n  "))?;
                } else {
                    write!(f, " = {v}")?;
                }
            }
        }
        if !is_first {
            f.write_str(",\n")?;
        }
        f.write_char(')')?;

        Ok(())
    }
}

/// Documentation about a parameter (without type information).
pub type TypelessParamDocs = ParamDocsT<()>;
/// Documentation about a parameter.
pub type ParamDocs = ParamDocsT<TypeRepr>;

/// Describes a function parameter.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ParamDocsT<T> {
    /// The parameter's name.
    pub name: Interned<str>,
    /// Documentation for the parameter.
    pub docs: EcoString,
    /// Inferred type of the parameter.
    pub cano_type: T,
    /// The parameter's default name as value.
    pub default: Option<EcoString>,
    /// The attribute of the parameter.
    #[serde(flatten)]
    pub attrs: ParamAttrs,
}

impl ParamDocs {
    fn new(param: &ParamSpec, ty: Option<&Ty>, doc_ty: Option<&mut ShowTypeRepr>) -> Self {
        Self {
            name: param.name.as_ref().into(),
            docs: param.docs.clone().unwrap_or_default(),
            cano_type: format_ty(ty.or(Some(&param.ty)), doc_ty),
            default: param.default.clone(),
            attrs: param.attrs,
        }
    }
}

fn format_ty(ty: Option<&Ty>, doc_ty: Option<&mut ShowTypeRepr>) -> TypeRepr {
    match doc_ty {
        Some(doc_ty) => doc_ty(ty),
        None => ty
            .and_then(|ty| ty.repr())
            .map(|short| (short, format!("{ty:?}"))),
    }
}

pub(crate) fn variable_docs(ctx: &mut AnalysisContext, pos: &LinkedNode) -> Option<VarDocs> {
    let source = ctx.source_by_id(pos.span().id()?).ok()?;
    let type_info = ctx.type_check(&source);
    let ty = type_info.type_of_span(pos.span())?;

    // todo multiple sources
    let mut srcs = ty.sources();
    srcs.sort();
    log::info!("check variable docs of ty: {ty:?} => {srcs:?}");
    let doc_source = srcs.into_iter().next()?;

    let return_ty = ty.describe().map(|short| (short, format!("{ty:?}")));
    match doc_source {
        DocSource::Var(var) => {
            let docs = type_info
                .var_docs
                .get(&var.def)
                .map(|docs| docs.docs().clone());
            Some(VarDocs {
                docs: docs.unwrap_or_default(),
                return_ty,
                def_docs: OnceLock::new(),
            })
        }
        DocSource::Ins(ins) => ins.syntax.as_ref().map(|src| {
            let docs = src.doc.as_ref().into();
            VarDocs {
                docs,
                return_ty,
                def_docs: OnceLock::new(),
            }
        }),
        _ => None,
    }
}

pub(crate) fn signature_docs(
    sig: &Signature,
    mut doc_ty: Option<ShowTypeRepr>,
) -> Option<SignatureDocs> {
    let type_sig = sig.type_sig().clone();

    let pos_in = sig
        .primary()
        .pos()
        .iter()
        .enumerate()
        .map(|(i, pos)| (pos, type_sig.pos(i)));
    let named_in = sig
        .primary()
        .named()
        .iter()
        .map(|x| (x, type_sig.named(&x.name)));
    let rest_in = sig.primary().rest().map(|x| (x, type_sig.rest_param()));

    let ret_in = type_sig.body.as_ref();

    let pos = pos_in
        .map(|(param, ty)| ParamDocs::new(param, ty, doc_ty.as_mut()))
        .collect();
    let named = named_in
        .map(|(param, ty)| {
            (
                param.name.clone(),
                ParamDocs::new(param, ty, doc_ty.as_mut()),
            )
        })
        .collect();
    let rest = rest_in.map(|(param, ty)| ParamDocs::new(param, ty, doc_ty.as_mut()));

    let ret_ty = format_ty(ret_in, doc_ty.as_mut());

    Some(SignatureDocs {
        docs: sig.primary().docs.clone().unwrap_or_default(),
        pos,
        named,
        rest,
        ret_ty,
        def_docs: OnceLock::new(),
    })
}

// Unfortunately, we have only 65536 possible file ids and we cannot revoke
// them. So we share a global file id for all docs conversion.
static DOCS_CONVERT_ID: std::sync::LazyLock<Mutex<FileId>> = std::sync::LazyLock::new(|| {
    Mutex::new(FileId::new(None, VirtualPath::new("__tinymist_docs__.typ")))
});

pub(crate) fn convert_docs(world: &LspWorld, content: &str) -> StrResult<EcoString> {
    static DOCS_LIB: std::sync::LazyLock<Arc<typlite::scopes::Scopes<typlite::value::Value>>> =
        std::sync::LazyLock::new(library::lib);

    let conv_id = DOCS_CONVERT_ID.lock();
    let entry = EntryState::new_rootless(conv_id.vpath().as_rooted_path().into()).unwrap();
    let entry = entry.select_in_workspace(*conv_id);

    let mut w = world.task(TaskInputs {
        entry: Some(entry),
        inputs: None,
    });
    w.map_shadow_by_id(*conv_id, Bytes::from(content.as_bytes().to_owned()))?;
    // todo: bad performance
    w.source_db.take_state();

    let conv = typlite::Typlite::new(Arc::new(w))
        .with_library(DOCS_LIB.clone())
        .annotate_elements(true)
        .convert()
        .map_err(|e| eco_format!("failed to convert to markdown: {e}"))?;

    Ok(conv.replace("```example", "```typ"))
}

pub(crate) fn identify_docs(kind: DocStringKind, docs: EcoString) -> StrResult<SymbolDocs> {
    match kind {
        DocStringKind::Function => Err(eco_format!("must be already handled")),
        DocStringKind::Variable => identify_var_docs(docs).map(SymbolDocs::Variable),
        DocStringKind::Constant => identify_var_docs(docs).map(SymbolDocs::Variable),
        DocStringKind::Module => identify_tidy_module_docs(docs).map(SymbolDocs::Module),
        DocStringKind::Struct => Ok(SymbolDocs::Plain { docs }),
        DocStringKind::Reference => Ok(SymbolDocs::Plain { docs }),
    }
}
