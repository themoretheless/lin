//! `#[derive(LinRow)]` for typed Lin query rows with span-accurate diagnostics,
//! generic lifetime/type parameter support, and custom crate path (`D38`: `LIN-1491`, `LIN-1492`, `LIN-1501`, `LIN-1503`).

use std::collections::{BTreeMap, BTreeSet};

use proc_macro::TokenStream;
use quote::quote;
use syn::{
    Attribute, Data, DeriveInput, Fields, GenericParam, Generics, Ident, LitStr, Path, Type,
    parse_macro_input, parse_quote, spanned::Spanned,
};

/// Derive [`lin::LinRow`] + [`lin::FromRow`] for a struct of named fields.
///
/// Struct attributes:
/// - `#[lin(collection = "docs")]`: default collection for `Db::from_typed`
/// - `#[lin(crate = "::lin")]`: custom path to the `lin` crate
///
/// Field attributes:
/// - `#[lin(rename = "wing")]`: map Rust field to stored column name (duplicate renames rejected)
/// - `#[lin(owned)]` / `#[lin(flatten)]`: decode nested `FromRow` / `LinRow` struct
/// - `#[lin(skip)]`: initialize field via `Default::default()` and omit from projection
#[proc_macro_derive(LinRow, attributes(lin))]
pub fn derive_lin_row(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match expand(input) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

struct StructOpts {
    collection: Option<String>,
    krate: Path,
}

#[derive(Default)]
struct FieldOpts {
    rename: Option<(String, proc_macro2::Span)>,
    owned: Option<proc_macro2::Span>,
    skip: Option<proc_macro2::Span>,
}

fn expand(input: DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let name = &input.ident;
    let struct_opts = parse_struct_attrs(&input.attrs)?;
    let krate = &struct_opts.krate;

    let Data::Struct(data) = &input.data else {
        return Err(syn::Error::new_spanned(
            &input,
            "LinRow can only be derived for structs",
        ));
    };
    let Fields::Named(fields) = &data.fields else {
        return Err(syn::Error::new_spanned(
            &data.fields,
            "LinRow requires a struct with named fields",
        ));
    };

    let mut seen_columns: BTreeMap<String, Ident> = BTreeMap::new();
    let mut col_lits = Vec::new();
    let mut assignments = Vec::new();
    let mut slot_decls = Vec::new();
    let mut batch_assignments = Vec::new();
    let mut col_fields = Vec::new();
    let mut col_inits = Vec::new();

    let mut owned_tys: Vec<Type> = Vec::new();
    let mut skipped_tys: Vec<Type> = Vec::new();
    let mut cell_tys: Vec<Type> = Vec::new();

    for (idx, field) in fields.named.iter().enumerate() {
        let ident = field.ident.as_ref().unwrap();
        let ty = &field.ty;
        let fopts = parse_field_attrs(&field.attrs)?;

        if let Some(skip_span) = fopts.skip {
            if fopts.rename.is_some() {
                return Err(syn::Error::new(
                    skip_span,
                    "conflicting #[lin] attributes: `skip` cannot be combined with `rename`",
                ));
            }
            if fopts.owned.is_some() {
                return Err(syn::Error::new(
                    skip_span,
                    "conflicting #[lin] attributes: `skip` cannot be combined with `owned` or `flatten`",
                ));
            }
            skipped_tys.push(ty.clone());
            assignments.push(quote! {
                #ident: ::core::default::Default::default(),
            });
            batch_assignments.push(quote! {
                #ident: ::core::default::Default::default(),
            });
            continue;
        }

        if let Some(owned_span) = fopts.owned {
            if fopts.rename.is_some() {
                return Err(syn::Error::new(
                    owned_span,
                    "conflicting #[lin] attributes: `owned`/`flatten` cannot be combined with `rename`",
                ));
            }
            owned_tys.push(ty.clone());
            assignments.push(quote! {
                #ident: <#ty as #krate::FromRow>::from_row(__row)?,
            });
            batch_assignments.push(quote! {
                #ident: <#ty as #krate::FromRow>::from_row(__row)?,
            });
            col_fields.push(quote! {
                pub #ident: <#ty as #krate::LinRow>::Cols
            });
            col_inits.push(quote! {
                #ident: <#ty as #krate::LinRow>::cols()
            });
            continue;
        }

        let (col, col_span) = match fopts.rename {
            Some((r, sp)) => (r, sp),
            None => (ident.to_string(), ident.span()),
        };

        if let Some(prev_ident) = seen_columns.get(&col) {
            return Err(syn::Error::new(
                col_span,
                format!(
                    "duplicate column mapping `{col}` (already mapped by field `{prev_ident}`)"
                ),
            ));
        }
        seen_columns.insert(col.clone(), ident.clone());

        cell_tys.push(ty.clone());
        let col_lit = LitStr::new(&col, col_span);
        let slot_ident = quote::format_ident!("__lin_slot_{}", idx);
        col_lits.push(col_lit.clone());
        assignments.push(quote! {
            #ident: #krate::cell_get::<_>(__row, #col_lit)?,
        });
        slot_decls.push(quote! {
            let mut #slot_ident = #krate::row::RowSlot::new(#col_lit);
        });
        batch_assignments.push(quote! {
            #ident: #slot_ident.read::<_>(__row)?,
        });
        col_fields.push(quote! {
            pub #ident: #krate::Col
        });
        col_inits.push(quote! {
            #ident: #krate::Col::new(#col_lit)
        });
    }

    let collection_tokens = match struct_opts.collection {
        Some(c) => {
            let lit = LitStr::new(&c, name.span());
            quote! { ::core::option::Option::Some(#lit) }
        }
        None => quote! { ::core::option::Option::None },
    };

    let cols_name = quote::format_ident!("{}Cols", name);

    let from_row_generics = build_impl_generics(
        &input.generics,
        krate,
        &cell_tys,
        &owned_tys,
        &skipped_tys,
        false,
    );
    let lin_row_generics = build_impl_generics(
        &input.generics,
        krate,
        &cell_tys,
        &owned_tys,
        &skipped_tys,
        true,
    );
    let (from_impl_gen, ty_gen, from_where) = from_row_generics.split_for_impl();
    let (lin_impl_gen, _, lin_where) = lin_row_generics.split_for_impl();

    // Only parameterize `<T>` on `#cols_name` when `owned_tys` actually mention generic type params.
    let cols_generics = build_cols_generics(&input.generics, krate, &owned_tys);
    let (cols_impl_gen, cols_ty_gen, cols_where) = cols_generics.split_for_impl();

    Ok(quote! {
        impl #from_impl_gen #krate::FromRow for #name #ty_gen #from_where {
            fn from_row(__row: &#krate::Row) -> ::core::result::Result<Self, #krate::Error> {
                ::core::result::Result::Ok(Self {
                    #(#assignments)*
                })
            }

            fn from_rows(__rows: &[#krate::Row]) -> ::core::result::Result<::std::vec::Vec<Self>, #krate::Error> {
                #(#slot_decls)*
                let mut __out = ::std::vec::Vec::with_capacity(__rows.len());
                for __row in __rows {
                    __out.push(Self {
                        #(#batch_assignments)*
                    });
                }
                ::core::result::Result::Ok(__out)
            }
        }

        #[derive(Debug, Clone, Copy)]
        pub struct #cols_name #cols_impl_gen #cols_where {
            #(#col_fields,)*
        }

        impl #lin_impl_gen #krate::LinRow for #name #ty_gen #lin_where {
            const COLLECTION: ::core::option::Option<&'static str> = #collection_tokens;
            const COLUMNS: &'static [&'static str] = &[#(#col_lits),*];
            type Cols = #cols_name #cols_ty_gen;
            fn cols() -> Self::Cols {
                #cols_name {
                    #(#col_inits,)*
                }
            }
        }
    })
}

fn type_mentions_ident(ty: &Type, target: &Ident) -> bool {
    match ty {
        Type::Path(tp) => {
            if tp.qself.is_none() && tp.path.is_ident(target) {
                return true;
            }
            for seg in &tp.path.segments {
                if &seg.ident == target {
                    return true;
                }
                if let syn::PathArguments::AngleBracketed(ab) = &seg.arguments {
                    for arg in &ab.args {
                        if let syn::GenericArgument::Type(inner) = arg
                            && type_mentions_ident(inner, target)
                        {
                            return true;
                        }
                    }
                }
            }
            false
        }
        Type::Reference(r) => type_mentions_ident(&r.elem, target),
        Type::Tuple(t) => t.elems.iter().any(|e| type_mentions_ident(e, target)),
        Type::Slice(s) => type_mentions_ident(&s.elem, target),
        Type::Array(a) => type_mentions_ident(&a.elem, target),
        Type::Paren(p) => type_mentions_ident(&p.elem, target),
        Type::Group(g) => type_mentions_ident(&g.elem, target),
        _ => false,
    }
}

fn is_direct_type_param(ty: &Type, target: &Ident) -> bool {
    matches!(ty, Type::Path(tp) if tp.qself.is_none() && tp.path.is_ident(target))
}

fn build_impl_generics(
    generics: &Generics,
    krate: &Path,
    cell_tys: &[Type],
    owned_tys: &[Type],
    skipped_tys: &[Type],
    for_lin_row: bool,
) -> Generics {
    let mut out = generics.clone();
    if out.params.is_empty() {
        return out;
    }
    let type_params: Vec<Ident> = out
        .type_params()
        .map(|tp| tp.ident.clone())
        .collect();
    if type_params.is_empty() {
        return out;
    }
    let where_clause = out.make_where_clause();
    for tp in &type_params {
        let in_owned_direct = owned_tys.iter().any(|t| is_direct_type_param(t, tp));
        let in_cell = cell_tys.iter().any(|t| type_mentions_ident(t, tp));
        let in_skip = skipped_tys.iter().any(|t| type_mentions_ident(t, tp));

        if in_owned_direct {
            if for_lin_row {
                where_clause
                    .predicates
                    .push(parse_quote!(#tp: #krate::LinRow));
            } else {
                where_clause
                    .predicates
                    .push(parse_quote!(#tp: #krate::FromRow));
            }
        } else if in_cell {
            where_clause
                .predicates
                .push(parse_quote!(#tp: #krate::FromCell));
        }
        if in_skip && !in_cell && !in_owned_direct {
            // If used only inside PhantomData<T>, Default is implemented unconditionally;
            // only bound `Default` if the skipped field is directly `T`.
            if skipped_tys.iter().any(|t| is_direct_type_param(t, tp)) {
                where_clause
                    .predicates
                    .push(parse_quote!(#tp: ::core::default::Default));
            }
        }
    }
    for owned_ty in owned_tys {
        if type_params.iter().any(|tp| type_mentions_ident(owned_ty, tp) && !is_direct_type_param(owned_ty, tp)) {
            if for_lin_row {
                where_clause
                    .predicates
                    .push(parse_quote!(#owned_ty: #krate::LinRow));
            } else {
                where_clause
                    .predicates
                    .push(parse_quote!(#owned_ty: #krate::FromRow));
            }
        }
    }
    out
}

fn build_cols_generics(generics: &Generics, krate: &Path, owned_tys: &[Type]) -> Generics {
    if owned_tys.is_empty() || generics.params.is_empty() {
        return Generics::default();
    }
    let mut used_type_params = BTreeSet::new();
    for tp in generics.type_params() {
        if owned_tys.iter().any(|t| type_mentions_ident(t, &tp.ident)) {
            used_type_params.insert(tp.ident.to_string());
        }
    }
    if used_type_params.is_empty() {
        return Generics::default();
    }
    let mut out = Generics::default();
    for param in &generics.params {
        if let GenericParam::Type(tp) = param
            && used_type_params.contains(&tp.ident.to_string())
        {
            let ident = &tp.ident;
            out.params.push(parse_quote!(#ident));
            out.make_where_clause()
                .predicates
                .push(parse_quote!(#ident: #krate::LinRow));
        }
    }
    out
}

fn parse_struct_attrs(attrs: &[Attribute]) -> syn::Result<StructOpts> {
    let mut collection: Option<String> = None;
    let mut krate: Option<Path> = None;

    for attr in attrs {
        if !attr.path().is_ident("lin") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("collection") {
                if collection.is_some() {
                    return Err(meta.error("duplicate #[lin(collection = \"...\")] attribute"));
                }
                let value: LitStr = meta.value()?.parse()?;
                if value.value().is_empty() {
                    return Err(syn::Error::new_spanned(
                        value,
                        "collection name cannot be empty",
                    ));
                }
                collection = Some(value.value());
                Ok(())
            } else if meta.path.is_ident("crate") {
                if krate.is_some() {
                    return Err(meta.error("duplicate #[lin(crate = \"...\")] attribute"));
                }
                let value: LitStr = meta.value()?.parse()?;
                let parsed: Path = value.parse().map_err(|e| {
                    syn::Error::new_spanned(&value, format!("invalid crate path: {e}"))
                })?;
                krate = Some(parsed);
                Ok(())
            } else {
                Err(meta.error(
                    "unsupported #[lin] struct attribute (expected `collection` or `crate`)",
                ))
            }
        })?;
    }

    Ok(StructOpts {
        collection,
        krate: krate.unwrap_or_else(|| parse_quote!(::lin)),
    })
}

fn parse_field_attrs(attrs: &[Attribute]) -> syn::Result<FieldOpts> {
    let mut opts = FieldOpts::default();
    for attr in attrs {
        if !attr.path().is_ident("lin") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("rename") {
                if opts.rename.is_some() {
                    return Err(meta.error("duplicate #[lin(rename = \"...\")] on field"));
                }
                let value: LitStr = meta.value()?.parse()?;
                if value.value().is_empty() {
                    return Err(syn::Error::new_spanned(
                        &value,
                        "renamed column cannot be empty",
                    ));
                }
                opts.rename = Some((value.value(), value.span()));
                Ok(())
            } else if meta.path.is_ident("owned") || meta.path.is_ident("flatten") {
                if opts.owned.is_some() {
                    return Err(meta.error("duplicate `owned`/`flatten` attribute on field"));
                }
                opts.owned = Some(meta.path.span());
                Ok(())
            } else if meta.path.is_ident("skip") {
                if opts.skip.is_some() {
                    return Err(meta.error("duplicate `skip` attribute on field"));
                }
                opts.skip = Some(meta.path.span());
                Ok(())
            } else {
                Err(meta.error(
                    "unsupported #[lin] field attribute (expected `rename`, `owned`, `flatten`, or `skip`)",
                ))
            }
        })?;
    }
    Ok(opts)
}
