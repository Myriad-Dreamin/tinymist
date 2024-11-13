use reflexo::hash::hash128;
use typst::foundations::Repr;

use crate::ty::prelude::*;

impl TypeScheme {
    /// Describe the given type with the given type scheme.
    pub fn describe(&self, ty: &Ty) -> Option<String> {
        let mut worker: TypeDescriber = TypeDescriber::default();
        worker.describe_root(ty)
    }
}

impl Ty {
    /// Describe the given type.
    pub fn repr(&self) -> Option<String> {
        let mut worker = TypeDescriber {
            repr: true,
            ..Default::default()
        };
        worker.describe_root(self)
    }

    /// Describe the given type.
    pub fn describe(&self) -> Option<String> {
        let mut worker = TypeDescriber::default();
        worker.describe_root(self)
    }

    // todo: extend this cache idea for all crate?
    // #[allow(clippy::mutable_key_type)]
    // let mut describe_cache = HashMap::<Ty, String>::new();
    // let doc_ty = |ty: Option<&Ty>| {
    //     let ty = ty?;
    //     let short = {
    //         describe_cache
    //             .entry(ty.clone())
    //             .or_insert_with(|| ty.describe().unwrap_or_else(||
    // "unknown".to_string()))             .clone()
    //     };

    //     Some((short, format!("{ty:?}")))
    // };
}

#[derive(Default)]
struct TypeDescriber {
    repr: bool,
    described: HashMap<u128, String>,
    results: HashSet<String>,
    functions: Vec<Interned<SigTy>>,
}

impl TypeDescriber {
    fn describe_root(&mut self, ty: &Ty) -> Option<String> {
        let _ = TypeDescriber::describe_iter;
        // recursive structure
        if let Some(t) = self.described.get(&hash128(ty)) {
            return Some(t.clone());
        }

        let res = self.describe(ty);
        if !res.is_empty() {
            return Some(res);
        }
        self.described.insert(hash128(ty), "$self".to_string());

        let mut results = std::mem::take(&mut self.results)
            .into_iter()
            .collect::<Vec<_>>();
        let functions = std::mem::take(&mut self.functions);
        if !functions.is_empty() {
            // todo: union signature
            // only first function is described
            let f = functions[0].clone();

            let mut res = String::new();
            res.push('(');
            let mut not_first = false;
            for ty in f.positional_params() {
                if not_first {
                    res.push_str(", ");
                } else {
                    not_first = true;
                }
                res.push_str(self.describe_root(ty).as_deref().unwrap_or("any"));
            }
            for (k, ty) in f.named_params() {
                if not_first {
                    res.push_str(", ");
                } else {
                    not_first = true;
                }
                res.push_str(k);
                res.push_str(": ");
                res.push_str(self.describe_root(ty).as_deref().unwrap_or("any"));
            }
            if let Some(r) = f.rest_param() {
                if not_first {
                    res.push_str(", ");
                }
                res.push_str("..: ");
                res.push_str(self.describe_root(r).as_deref().unwrap_or("any"));
            }
            res.push_str(") => ");
            res.push_str(
                f.body
                    .as_ref()
                    .and_then(|ret| self.describe_root(ret))
                    .as_deref()
                    .unwrap_or("any"),
            );

            results.push(res);
        }

        if results.is_empty() {
            self.described.insert(hash128(ty), "any".to_string());
            return None;
        }

        results.sort();
        results.dedup();
        let res = results.join(" | ");
        self.described.insert(hash128(ty), res.clone());
        Some(res)
    }

    fn describe_iter(&mut self, ty: &[Ty]) {
        for ty in ty.iter() {
            let desc = self.describe(ty);
            if !desc.is_empty() {
                self.results.insert(desc);
            }
        }
    }

    fn describe(&mut self, ty: &Ty) -> String {
        match ty {
            Ty::Var(..) => {}
            Ty::Union(tys) => {
                self.describe_iter(tys);
            }
            Ty::Let(lb) => {
                self.describe_iter(&lb.lbs);
                self.describe_iter(&lb.ubs);
            }
            Ty::Func(f) => {
                self.functions.push(f.clone());
            }
            Ty::Dict(..) => {
                return "dict".to_string();
            }
            Ty::Tuple(..) => {
                return "array".to_string();
            }
            Ty::Array(..) => {
                return "array".to_string();
            }
            // todo: sig with
            Ty::With(w) => {
                return self.describe(&w.sig);
            }
            Ty::Builtin(BuiltinTy::Content | BuiltinTy::Space) => {
                return "content".to_string();
            }
            // Doesn't provide any information, hence we doesn't describe it intermediately here.
            Ty::Any | Ty::Builtin(BuiltinTy::Clause | BuiltinTy::Undef | BuiltinTy::Infer) => {}
            Ty::Builtin(BuiltinTy::FlowNone | BuiltinTy::None) => {
                return "none".to_string();
            }
            Ty::Builtin(BuiltinTy::Auto) => {
                return "auto".to_string();
            }
            Ty::Boolean(..) if self.repr => {
                return "bool".to_string();
            }
            Ty::Boolean(None) => {
                return "bool".to_string();
            }
            Ty::Boolean(Some(b)) => {
                return b.to_string();
            }
            Ty::Builtin(b) => {
                return b.describe();
            }
            Ty::Value(v) if self.repr => return v.val.ty().short_name().to_string(),
            Ty::Value(v) => return v.val.repr().to_string(),
            Ty::Field(..) => {
                return "field".to_string();
            }
            Ty::Args(..) => {
                return "arguments".to_string();
            }
            Ty::Pattern(..) => {
                return "pattern".to_string();
            }
            Ty::Select(..) | Ty::Unary(..) | Ty::Binary(..) | Ty::If(..) => {
                return "any".to_string()
            }
        }

        String::new()
    }
}
