use std::collections::HashSet;

use reflexo::hash::hash128;
use typst::foundations::Repr;

use crate::{adt::interner::Interned, analysis::*, ty::def::*};

impl TypeCheckInfo {
    pub fn describe(&self, ty: &Ty) -> Option<String> {
        let mut worker = TypeDescriber::default();
        worker.describe_root(ty)
    }
}

impl Ty {
    pub fn describe(&self) -> Option<String> {
        let mut worker = TypeDescriber::default();
        worker.describe_root(self)
    }
}

#[derive(Default)]
struct TypeDescriber {
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
                res.push_str(self.describe_root(r).as_deref().unwrap_or(""));
                res.push_str("[]");
            }
            res.push(')');

            if let Some(ret) = &f.ret {
                res.push_str(" => ");
                res.push_str(self.describe_root(ret).as_deref().unwrap_or("any"));
            }

            results.push(res);

            todo!()
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
            Ty::Clause => {}
            Ty::Undef => {}
            Ty::Content => {
                return "content".to_string();
            }
            // Doesn't provide any information, hence we doesn't describe it intermediately here.
            Ty::Any => {}
            Ty::Space => {}
            Ty::None => {
                return "none".to_string();
            }
            Ty::Infer => {}
            Ty::FlowNone => {
                return "none".to_string();
            }
            Ty::Auto => {
                return "auto".to_string();
            }
            Ty::Boolean(None) => {
                return "boolean".to_string();
            }
            Ty::Boolean(Some(b)) => {
                return b.to_string();
            }
            Ty::Builtin(b) => {
                return b.describe().to_string();
            }
            Ty::Value(v) => return v.val.repr().to_string(),
            Ty::Field(..) => {
                return "field".to_string();
            }
            Ty::Args(..) => {
                return "args".to_string();
            }
            Ty::Select(..) => {
                return "any".to_string();
            }
            Ty::Unary(..) => {
                return "any".to_string();
            }
            Ty::Binary(..) => {
                return "any".to_string();
            }
            Ty::If(..) => {
                return "any".to_string();
            }
        }

        String::new()
    }
}
