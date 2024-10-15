use std::fmt;
use std::{collections::HashMap, sync::Arc};

use ecow::EcoString;
use serde::{Deserialize, Serialize};
use typst::foundations::Value;

use crate::analysis::analyze_dyn_signature;
use crate::syntax::IdentRef;
use crate::{ty::Ty, AnalysisContext};

type TypeRepr = Option<(/* short */ String, /* long */ String)>;
type ShowTypeRepr<'a> = &'a mut dyn FnMut(Option<&Ty>) -> TypeRepr;

/// Describes a primary function signature.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureDocs {
    /// The positional parameters.
    pub pos: Vec<ParamDocs>,
    /// The named parameters.
    pub named: HashMap<String, ParamDocs>,
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
                name_prints.push((v.name.clone(), ty, v.expr.clone()))
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
    pub docs: String,
    /// Inferred type of the parameter.
    pub cano_type: TypeRepr,
    /// The parameter's default name as value.
    pub expr: Option<EcoString>,
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

type TypeInfo = (Arc<crate::analysis::DefUseInfo>, Arc<crate::ty::TypeScheme>);

pub(crate) fn signature_docs(
    ctx: &mut AnalysisContext,
    type_info: Option<&TypeInfo>,
    def_ident: Option<&IdentRef>,
    runtime_fn: &Value,
    doc_ty: Option<ShowTypeRepr>,
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
    let type_sig = type_info.and_then(|(def_use, ty_chk)| {
        let def_fid = func.span().id()?;
        let (def_id, _) = def_use.get_def(def_fid, def_ident?)?;
        ty_chk.type_of_def(def_id)
    });
    let type_sig = type_sig.and_then(|type_sig| type_sig.sig_repr(true));

    const F: fn(Option<&Ty>) -> TypeRepr = |ty: Option<&Ty>| {
        ty.and_then(|ty| ty.describe())
            .map(|short| (short, format!("{ty:?}")))
    };
    let mut binding = F;
    let doc_ty = doc_ty.unwrap_or(&mut binding);
    let pos_in = sig
        .primary()
        .pos
        .iter()
        .enumerate()
        .map(|(i, pos)| (pos, type_sig.as_ref().and_then(|sig| sig.pos(i))));
    let named_in = sig
        .primary()
        .named
        .iter()
        .map(|x| (x, type_sig.as_ref().and_then(|sig| sig.named(x.0))));
    let rest_in = sig
        .primary()
        .rest
        .as_ref()
        .map(|x| (x, type_sig.as_ref().and_then(|sig| sig.rest_param())));

    let ret_in = type_sig
        .as_ref()
        .and_then(|sig| sig.body.as_ref())
        .or_else(|| sig.primary().ret_ty.as_ref());

    let pos = pos_in
        .map(|(param, ty)| ParamDocs {
            name: param.name.as_ref().to_owned(),
            docs: param.docs.as_ref().to_owned(),
            cano_type: doc_ty(ty.or(Some(&param.base_type))),
            expr: param.expr.clone(),
            positional: param.positional,
            named: param.named,
            variadic: param.variadic,
            settable: param.settable,
        })
        .collect();

    let named = named_in
        .map(|((name, param), ty)| {
            (
                name.as_ref().to_owned(),
                ParamDocs {
                    name: param.name.as_ref().to_owned(),
                    docs: param.docs.as_ref().to_owned(),
                    cano_type: doc_ty(ty.or(Some(&param.base_type))),
                    expr: param.expr.clone(),
                    positional: param.positional,
                    named: param.named,
                    variadic: param.variadic,
                    settable: param.settable,
                },
            )
        })
        .collect();

    let rest = rest_in.map(|(param, ty)| ParamDocs {
        name: param.name.as_ref().to_owned(),
        docs: param.docs.as_ref().to_owned(),
        cano_type: doc_ty(ty.or(Some(&param.base_type))),
        expr: param.expr.clone(),
        positional: param.positional,
        named: param.named,
        variadic: param.variadic,
        settable: param.settable,
    });

    let ret_ty = doc_ty(ret_in);

    Some(SignatureDocs {
        pos,
        named,
        rest,
        ret_ty,
    })
}
