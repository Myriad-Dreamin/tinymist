extern crate proc_macro;

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput};

#[proc_macro_attribute]
pub fn toml_model(
    _metadata: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let output = quote! {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(rename_all = "kebab-case")]
        #input
    };
    output.into()
}

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
                    fn global_bounds(&self, var: &Interned<TypeVar>, pol: bool) -> Option<DynTypeBounds> {
                        self.#bind_name.global_bounds(var, pol)
                    }
                    fn local_bind_of(&self, var: &Interned<TypeVar>) -> Option<Ty> {
                        self.#bind_name.local_bind_of(var)
                    }
                }
            }
        }
        _ => panic!("only structs are supported"),
    };

    // Hand the output tokens back to the compiler
    TokenStream::from(expanded)
}

#[proc_macro_derive(DeclEnum)]
pub fn gen_decl_enum(input: TokenStream) -> TokenStream {
    // In form of
    // ```
    // pub enum Decl {
    //   Sub1(X),
    //   Sub2(Y),
    // }
    // ```

    // Parse the input tokens into a list of variants
    let input = parse_macro_input!(input as DeriveInput);

    let variants = match input.data {
        syn::Data::Enum(data) => data.variants,
        _ => panic!("only enums are supported"),
    };

    let names = variants.iter().map(|v| &v.ident).collect::<Vec<_>>();

    let input_name = &input.ident;

    let expanded = quote! {
        impl #input_name {
            pub fn name(&self) -> &Interned<str> {
                match self {
                    #(Self::#names(x) => x.name()),*
                }
            }

            pub fn span(&self) -> Span {
                match self {
                    #(Self::#names(x) => x.span()),*
                }
            }
        }

        impl fmt::Debug for Decl {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                match self {
                    #(Self::#names(x) => write!(f, concat!(stringify!(#names), "({:?})"), x)),*
                }
            }
        }

    };

    TokenStream::from(expanded)
}
