use core::fmt;
use std::collections::BTreeMap;
use std::sync::Arc;

use ecow::{eco_format, EcoString};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tinymist_world::base::{EntryState, ShadowApi, TaskInputs};
use tinymist_world::LspWorld;
use typst::foundations::{Bytes, Value};
use typst::{
    diag::StrResult,
    syntax::{FileId, VirtualPath},
};

use super::tidy::*;
use crate::analysis::{analyze_dyn_signature, ParamSpec};
use crate::docs::library;
use crate::syntax::IdentRef;
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

/// Documentation about a symbol.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum SymbolDocs {
    /// Documentation about a function.
    #[serde(rename = "func")]
    Function(Box<SignatureDocs>),
    /// Documentation about a variable.
    #[serde(rename = "var")]
    Variable(TidyVarDocs),
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

impl SymbolDocs {
    /// Get the markdown representation of the documentation.
    pub fn docs(&self) -> &str {
        match self {
            Self::Function(docs) => docs.docs.as_str(),
            Self::Variable(docs) => docs.docs.as_str(),
            Self::Module(docs) => docs.docs.as_str(),
            Self::Plain { docs } => docs.as_str(),
        }
    }
}

pub(crate) fn symbol_docs(
    ctx: &mut AnalysisContext,
    type_info: Option<&TypeInfo>,
    kind: DocStringKind,
    def_ident: Option<&IdentRef>,
    sym_value: Option<&Value>,
    docs: Option<&str>,
    doc_ty: Option<ShowTypeRepr>,
) -> Result<SymbolDocs, String> {
    let signature = sym_value.and_then(|e| signature_docs(ctx, type_info, def_ident, e, doc_ty));
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
pub struct SignatureDocs {
    /// Documentation for the function.
    pub docs: EcoString,
    // pub return_ty: Option<EcoString>,
    // pub params: Vec<TidyParamDocs>,
    /// The positional parameters.
    pub pos: Vec<ParamDocs>,
    /// The named parameters.
    pub named: BTreeMap<String, ParamDocs>,
    /// The rest parameter.
    pub rest: Option<ParamDocs>,
    /// The return type.
    pub ret_ty: TypeRepr,
}

impl fmt::Display for SignatureDocs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut is_first = true;
        let mut write_sep = |f: &mut fmt::Formatter<'_>| {
            if is_first {
                is_first = false;
                return Ok(());
            }
            f.write_str(", ")
        };

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
                    v = "{ ... }"
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
                write!(f, " = {v}")?;
            }
        }

        Ok(())
    }
}

/// Describes a function parameter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamDocs {
    /// The parameter's name.
    pub name: String,
    /// Documentation for the parameter.
    pub docs: EcoString,
    /// Inferred type of the parameter.
    pub cano_type: TypeRepr,
    /// The parameter's default name as value.
    pub default: Option<EcoString>,
    /// Is the parameter positional?
    pub positional: bool,
    /// Is the parameter named?
    ///
    /// Can be true even if `positional` is true if the parameter can be given
    /// in both variants.
    pub named: bool,
    /// Can the parameter be given any number of times?
    pub variadic: bool,
    /// Is the parameter settable with a set rule?
    pub settable: bool,
}

impl ParamDocs {
    fn new(param: &ParamSpec, ty: Option<&Ty>, doc_ty: Option<&mut ShowTypeRepr>) -> Self {
        Self {
            name: param.name.as_ref().to_owned(),
            docs: param.docs.clone().unwrap_or_default(),
            cano_type: format_ty(ty.or(Some(&param.ty)), doc_ty),
            default: param.default.clone(),
            positional: param.positional,
            named: param.named,
            variadic: param.variadic,
            settable: param.settable,
        }
    }
}

type TypeInfo = (Arc<crate::analysis::DefUseInfo>, Arc<crate::ty::TypeScheme>);

fn format_ty(ty: Option<&Ty>, doc_ty: Option<&mut ShowTypeRepr>) -> TypeRepr {
    match doc_ty {
        Some(doc_ty) => doc_ty(ty),
        None => ty
            .and_then(|ty| ty.describe())
            .map(|short| (short, format!("{ty:?}"))),
    }
}

pub(crate) fn signature_docs(
    ctx: &mut AnalysisContext,
    type_info: Option<&TypeInfo>,
    def_ident: Option<&IdentRef>,
    runtime_fn: &Value,
    mut doc_ty: Option<ShowTypeRepr>,
) -> Option<SignatureDocs> {
    let func = match runtime_fn {
        Value::Func(f) => f,
        _ => return None,
    };

    // todo: documenting with bindings
    use typst::foundations::func::Repr;
    let mut func = func;
    loop {
        match func.inner() {
            Repr::Element(..) | Repr::Native(..) => {
                break;
            }
            Repr::With(w) => {
                func = &w.0;
            }
            Repr::Closure(..) => {
                break;
            }
        }
    }

    let sig = analyze_dyn_signature(ctx, func.clone());
    let def_id = type_info.and_then(|(def_use, _)| {
        let def_fid = func.span().id()?;
        let (def_id, _) = def_use.get_def(def_fid, def_ident?)?;
        Some(def_id)
    });
    let docstring = type_info.and_then(|(_, ty_chk)| ty_chk.var_docs.get(&def_id?));
    let type_sig = type_info.and_then(|(_, ty_chk)| ty_chk.type_of_def(def_id?));
    let type_sig = type_sig.and_then(|type_sig| type_sig.sig_repr(true));

    let pos_in = sig
        .primary()
        .pos()
        .iter()
        .enumerate()
        .map(|(i, pos)| (pos, type_sig.as_ref().and_then(|sig| sig.pos(i))));
    let named_in = sig
        .primary()
        .named()
        .iter()
        .map(|x| (x, type_sig.as_ref().and_then(|sig| sig.named(&x.name))));
    let rest_in = sig
        .primary()
        .rest()
        .map(|x| (x, type_sig.as_ref().and_then(|sig| sig.rest_param())));

    let ret_in = type_sig
        .as_ref()
        .and_then(|sig| sig.body.as_ref())
        .or_else(|| sig.primary().sig_ty.body.as_ref());

    let pos = pos_in
        .map(|(param, ty)| ParamDocs::new(param, ty, doc_ty.as_mut()))
        .collect();
    let named = named_in
        .map(|(param, ty)| {
            (
                param.name.as_ref().to_owned(),
                ParamDocs::new(param, ty, doc_ty.as_mut()),
            )
        })
        .collect();
    let rest = rest_in.map(|(param, ty)| ParamDocs::new(param, ty, doc_ty.as_mut()));

    let ret_ty = format_ty(ret_in, doc_ty.as_mut());

    Some(SignatureDocs {
        docs: docstring.and_then(|x| x.docs.clone()).unwrap_or_default(),
        pos,
        named,
        rest,
        ret_ty,
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

    Ok(conv)
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
