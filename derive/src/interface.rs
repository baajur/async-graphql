use crate::args;
use crate::args::{InterfaceField, InterfaceFieldArgument};
use crate::output_type::OutputType;
use crate::utils::{generate_default, get_crate_name, get_rustdoc, GeneratorResult};
use darling::ast::{Data, Style};
use inflector::Inflector;
use proc_macro::TokenStream;
use proc_macro2::{Ident, Span};
use quote::quote;
use std::collections::HashSet;
use syn::{Error, Type};

pub fn generate(interface_args: &args::Interface) -> GeneratorResult<TokenStream> {
    let crate_name = get_crate_name(interface_args.internal);
    let ident = &interface_args.ident;
    let generics = &interface_args.generics;
    let s = match &interface_args.data {
        Data::Enum(s) => s,
        _ => {
            return Err(
                Error::new_spanned(ident, "Interface can only be applied to an enum.").into(),
            )
        }
    };
    let extends = interface_args.extends;
    let mut enum_names = Vec::new();
    let mut enum_items = HashSet::new();
    let mut type_into_impls = Vec::new();
    let gql_typename = interface_args
        .name
        .clone()
        .unwrap_or_else(|| ident.to_string());

    let desc = get_rustdoc(&interface_args.attrs)?
        .map(|s| quote! { Some(#s) })
        .unwrap_or_else(|| quote! {None});

    let mut registry_types = Vec::new();
    let mut possible_types = Vec::new();
    let mut get_introspection_typename = Vec::new();
    let mut collect_all_fields = Vec::new();

    for variant in s {
        let enum_name = &variant.ident;
        let ty = match variant.fields.style {
            Style::Tuple if variant.fields.fields.len() == 1 => &variant.fields.fields[0],
            Style::Tuple => {
                return Err(Error::new_spanned(
                    enum_name,
                    "Only single value variants are supported",
                )
                .into())
            }
            Style::Unit => {
                return Err(
                    Error::new_spanned(enum_name, "Empty variants are not supported").into(),
                )
            }
            Style::Struct => {
                return Err(Error::new_spanned(
                    enum_name,
                    "Variants with named fields are not supported",
                )
                .into())
            }
        };

        if let Type::Path(p) = ty {
            // This validates that the field type wasn't already used
            if !enum_items.insert(p) {
                return Err(
                    Error::new_spanned(ty, "This type already used in another variant").into(),
                );
            }

            type_into_impls.push(quote! {
                #[allow(clippy::all, clippy::pedantic)]
                impl #generics ::std::convert::From<#p> for #ident #generics {
                    fn from(obj: #p) -> Self {
                        #ident::#enum_name(obj)
                    }
                }
            });
            enum_names.push(enum_name);

            registry_types.push(quote! {
                <#p as #crate_name::Type>::create_type_info(registry);
                registry.add_implements(&<#p as #crate_name::Type>::type_name(), #gql_typename);
            });

            possible_types.push(quote! {
                possible_types.insert(<#p as #crate_name::Type>::type_name().to_string());
            });

            get_introspection_typename.push(quote! {
                #ident::#enum_name(obj) => <#p as #crate_name::Type>::type_name()
            });

            collect_all_fields.push(quote! {
                #ident::#enum_name(obj) => obj.collect_all_fields(ctx, fields)
            });
        } else {
            return Err(Error::new_spanned(ty, "Invalid type").into());
        }
    }

    let mut methods = Vec::new();
    let mut schema_fields = Vec::new();
    let mut resolvers = Vec::new();

    for InterfaceField {
        name,
        method,
        desc,
        ty,
        args,
        deprecation,
        external,
        provides,
        requires,
    } in &interface_args.fields
    {
        let (name, method_name) = if let Some(method) = method {
            (name.to_string(), Ident::new(method, Span::call_site()))
        } else {
            let method_name = Ident::new(&name, Span::call_site());
            (name.to_camel_case(), method_name)
        };
        let ty = match syn::parse_str::<syn::Type>(&ty.value()) {
            Ok(ty) => ty,
            Err(_) => return Err(Error::new_spanned(&ty, "Expect type").into()),
        };
        let mut calls = Vec::new();
        let mut use_params = Vec::new();
        let mut decl_params = Vec::new();
        let mut get_params = Vec::new();
        let mut schema_args = Vec::new();
        let requires = match &requires {
            Some(requires) => quote! { Some(#requires) },
            None => quote! { None },
        };
        let provides = match &provides {
            Some(provides) => quote! { Some(#provides) },
            None => quote! { None },
        };

        decl_params.push(quote! { ctx: &'ctx #crate_name::Context<'ctx> });
        use_params.push(quote! { ctx });

        for InterfaceFieldArgument {
            name,
            desc,
            ty,
            default,
            default_with,
        } in args
        {
            let ident = Ident::new(name, Span::call_site());
            let name = name.to_camel_case();
            let ty = match syn::parse_str::<syn::Type>(&ty.value()) {
                Ok(ty) => ty,
                Err(_) => return Err(Error::new_spanned(&ty, "Expect type").into()),
            };
            decl_params.push(quote! { #ident: #ty });
            use_params.push(quote! { #ident });

            let default = generate_default(&default, &default_with)?;
            let get_default = match &default {
                Some(default) => quote! { Some(|| -> #ty { #default }) },
                None => quote! { None },
            };
            get_params.push(quote! {
                let #ident: #ty = ctx.param_value(#name, #get_default)?;
            });

            let desc = desc
                .as_ref()
                .map(|s| quote! {Some(#s)})
                .unwrap_or_else(|| quote! {None});
            let schema_default = default
                .as_ref()
                .map(|value| {
                    quote! {Some( <#ty as #crate_name::InputValueType>::to_value(&#value).to_string() )}
                })
                .unwrap_or_else(|| quote! {None});
            schema_args.push(quote! {
                args.insert(#name, #crate_name::registry::MetaInputValue {
                    name: #name,
                    description: #desc,
                    ty: <#ty as #crate_name::Type>::create_type_info(registry),
                    default_value: #schema_default,
                    validator: None,
                });
            });
        }

        for enum_name in &enum_names {
            calls.push(quote! {
                #ident::#enum_name(obj) => obj.#method_name(#(#use_params),*).await
            });
        }

        let desc = desc
            .as_ref()
            .map(|s| quote! {Some(#s)})
            .unwrap_or_else(|| quote! {None});
        let deprecation = deprecation
            .as_ref()
            .map(|s| quote! {Some(#s)})
            .unwrap_or_else(|| quote! {None});

        let oty = OutputType::parse(&ty)?;
        let ty = match oty {
            OutputType::Value(ty) => ty,
            OutputType::Result(_, ty) => ty,
        };
        let schema_ty = oty.value_type();

        methods.push(quote! {
            #[inline]
            async fn #method_name <'ctx>(&self, #(#decl_params),*) -> #crate_name::FieldResult<#ty> {
                match self {
                    #(#calls,)*
                }
            }
        });

        schema_fields.push(quote! {
            fields.insert(#name.to_string(), #crate_name::registry::MetaField {
                name: #name.to_string(),
                description: #desc,
                args: {
                    let mut args = #crate_name::indexmap::IndexMap::new();
                    #(#schema_args)*
                    args
                },
                ty: <#schema_ty as #crate_name::Type>::create_type_info(registry),
                deprecation: #deprecation,
                cache_control: Default::default(),
                external: #external,
                provides: #provides,
                requires: #requires,
            });
        });

        let resolve_obj = quote! {
            self.#method_name(#(#use_params),*).await.
                map_err(|err| err.into_error_with_path(ctx.item.pos, ctx.path_node.as_ref()))?
        };

        resolvers.push(quote! {
            if ctx.item.node.name.node == #name {
                #(#get_params)*
                let ctx_obj = ctx.with_selection_set(&ctx.item.node.selection_set);
                return #crate_name::OutputValueType::resolve(&#resolve_obj, &ctx_obj, ctx.item).await;
            }
        });
    }

    let introspection_type_name = if get_introspection_typename.is_empty() {
        quote! { unreachable!() }
    } else {
        quote! {
            match self {
            #(#get_introspection_typename),*
            }
        }
    };

    let expanded = quote! {
        #(#type_into_impls)*

        #[allow(clippy::all, clippy::pedantic)]
        impl #generics #ident #generics {
            #(#methods)*
        }

        #[allow(clippy::all, clippy::pedantic)]
        impl #generics #crate_name::Type for #ident #generics {
            fn type_name() -> ::std::borrow::Cow<'static, str> {
                ::std::borrow::Cow::Borrowed(#gql_typename)
            }

            fn introspection_type_name(&self) -> ::std::borrow::Cow<'static, str> {
                #introspection_type_name
            }

            fn create_type_info(registry: &mut #crate_name::registry::Registry) -> String {
                registry.create_type::<Self, _>(|registry| {
                    #(#registry_types)*

                    #crate_name::registry::MetaType::Interface {
                        name: #gql_typename.to_string(),
                        description: #desc,
                        fields: {
                            let mut fields = #crate_name::indexmap::IndexMap::new();
                            #(#schema_fields)*
                            fields
                        },
                        possible_types: {
                            let mut possible_types = #crate_name::indexmap::IndexSet::new();
                            #(#possible_types)*
                            possible_types
                        },
                        extends: #extends,
                        keys: None,
                    }
                })
            }
        }

        #[allow(clippy::all, clippy::pedantic)]
        #[#crate_name::async_trait::async_trait]
        impl #generics #crate_name::resolver_utils::ContainerType for #ident #generics {
            async fn resolve_field(&self, ctx: &#crate_name::Context<'_>) -> #crate_name::Result<#crate_name::serde_json::Value> {
                #(#resolvers)*
                Err(#crate_name::QueryError::FieldNotFound {
                    field_name: ctx.item.node.name.to_string(),
                    object: #gql_typename.to_string(),
                }.into_error(ctx.item.pos))
            }

            fn collect_all_fields<'a>(&'a self, ctx: &#crate_name::ContextSelectionSet<'a>, fields: &mut #crate_name::resolver_utils::Fields<'a>) -> #crate_name::Result<()> {
                match self {
                    #(#collect_all_fields),*
                }
            }
        }

        #[allow(clippy::all, clippy::pedantic)]
        #[#crate_name::async_trait::async_trait]
        impl #generics #crate_name::OutputValueType for #ident #generics {
            async fn resolve(&self, ctx: &#crate_name::ContextSelectionSet<'_>, _field: &#crate_name::Positioned<#crate_name::parser::types::Field>) -> #crate_name::Result<#crate_name::serde_json::Value> {
                #crate_name::resolver_utils::resolve_container(ctx, self).await
            }
        }

        impl #generics #crate_name::InterfaceType for #ident #generics {}
    };
    Ok(expanded.into())
}
