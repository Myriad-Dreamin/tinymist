extern crate proc_macro;

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput};

#[proc_macro_derive(BindTyCtx, attributes(bind))]
pub fn bind_ty_ctx(input: TokenStream) -> TokenStream {
    // Parse the input tokens into a syntax tree
    let input = parse_macro_input!(input as DeriveInput);

    // Build the output, possibly using quasi-quotation
    let expanded = match input.data {
        syn::Data::Struct(..) => {
            let name = &input.ident;
            let bind_name = input
                .attrs
                .iter()
                .find_map(|attr| {
                    if attr.path().is_ident("bind") {
                        Some(attr.parse_args::<syn::Expr>().unwrap())
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| {
                    let t = syn::Ident::new("tyctx", input.ident.span());
                    syn::parse_quote!(#t)
                });
            let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

            quote! {
                impl #impl_generics TyCtx for #name #ty_generics #where_clause {
                    fn var_bounds(&self, var: &Interned<TypeVar>, pol: bool) -> (Option<Ty>, Option<TypeBounds>) {
                        self.#bind_name.var_bounds(var, pol)
                    }
                }
            }
        }
        _ => panic!("only structs are supported"),
    };

    // Hand the output tokens back to the compiler
    TokenStream::from(expanded)
}
