//! `#[derive(LinRow)]` for typed Lin query rows.

use proc_macro::TokenStream;
use quote::quote;
use syn::{Attribute, Data, DeriveInput, Fields, LitStr, parse_macro_input, spanned::Spanned};

/// Derive [`lin::LinRow`] + [`lin::FromRow`] for a struct of named fields.
///
/// ```ignore
/// #[derive(LinRow)]
/// #[lin(collection = "docs")]
/// struct Doc {
///     id: String,
///     title: String,
/// }
/// ```
///
/// Optional per-field `#[lin(rename = "wing")]` maps a Rust field to a column name.
#[proc_macro_derive(LinRow, attributes(lin))]
pub fn derive_lin_row(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match expand(input) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

fn expand(input: DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let name = &input.ident;
    let collection = parse_collection(&input.attrs)?;

    let Data::Struct(data) = &input.data else {
        return Err(syn::Error::new(
            input.span(),
            "LinRow can only be derived for structs",
        ));
    };
    let Fields::Named(fields) = &data.fields else {
        return Err(syn::Error::new(
            input.span(),
            "LinRow requires a struct with named fields",
        ));
    };

    let mut col_lits = Vec::new();
    let mut assignments = Vec::new();

    for field in &fields.named {
        let ident = field.ident.as_ref().unwrap();
        let col = parse_rename(&field.attrs)?.unwrap_or_else(|| ident.to_string());
        let col_lit = LitStr::new(&col, ident.span());
        col_lits.push(col_lit.clone());
        assignments.push(quote! {
            #ident: ::lin::cell_get::<_>(__row, #col_lit)?,
        });
    }

    let collection_tokens = match collection {
        Some(c) => {
            let lit = LitStr::new(&c, name.span());
            quote! { ::core::option::Option::Some(#lit) }
        }
        None => quote! { ::core::option::Option::None },
    };

    Ok(quote! {
        impl ::lin::FromRow for #name {
            fn from_row(__row: &::lin::Row) -> ::core::result::Result<Self, ::lin::Error> {
                ::core::result::Result::Ok(Self {
                    #(#assignments)*
                })
            }
        }

        impl ::lin::LinRow for #name {
            const COLLECTION: ::core::option::Option<&'static str> = #collection_tokens;
            const COLUMNS: &'static [&'static str] = &[#(#col_lits),*];
        }
    })
}

fn parse_collection(attrs: &[Attribute]) -> syn::Result<Option<String>> {
    let mut out = None;
    for attr in attrs {
        if !attr.path().is_ident("lin") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("collection") {
                let value: LitStr = meta.value()?.parse()?;
                out = Some(value.value());
                Ok(())
            } else {
                Err(meta.error("unsupported lin attribute on struct (expected collection)"))
            }
        })?;
    }
    Ok(out)
}

fn parse_rename(attrs: &[Attribute]) -> syn::Result<Option<String>> {
    let mut out = None;
    for attr in attrs {
        if !attr.path().is_ident("lin") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("rename") {
                let value: LitStr = meta.value()?.parse()?;
                out = Some(value.value());
                Ok(())
            } else {
                Err(meta.error("unsupported lin attribute on field (expected rename)"))
            }
        })?;
    }
    Ok(out)
}
