use crate::parenthesized::Parenthesized;
use heck::CamelCase;
use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::ToTokens;
use syn::{parse_macro_input, FnArg, Ident, ItemTrait, ReturnType, TraitItem};

/// Implementation for `[salsa::query_group]` decorator.
pub(crate) fn query_group(args: TokenStream, input: TokenStream) -> TokenStream {
    let group_struct: Ident = parse_macro_input!(args as Ident);
    let input: ItemTrait = parse_macro_input!(input as ItemTrait);
    // println!("args: {:#?}", args);
    // println!("input: {:#?}", input);

    let trait_vis = input.vis;
    let trait_name = input.ident;
    let mut associated_type_items = proc_macro2::TokenStream::new();
    let mut assocs = vec![];

    // Decompose the trait into the corresponding queries.
    let mut queries = vec![];
    for item in input.items {
        match item {
            TraitItem::Method(method) => {
                let mut storage = QueryStorage::Memoized;
                let mut invoke = None;
                let mut query_type = Ident::new(
                    &format!("{}Query", method.sig.ident.to_string().to_camel_case()),
                    Span::call_site(),
                );
                let mut num_storages = 0;

                // Extract attributes.
                let mut attrs = vec![];
                for attr in method.attrs {
                    // Leave non-salsa attributes untouched. These are
                    // attributes that don't start with `salsa::` or don't have
                    // exactly two segments in their path.
                    if is_salsa_attr_path(&attr.path) {
                        attrs.push(attr);
                        continue;
                    }

                    // Keep the salsa attributes around.
                    let name = attr.path.segments[1].ident.to_string();
                    let tts = attr.tts.into();
                    match name.as_str() {
                        "memoized" => {
                            storage = QueryStorage::Memoized;
                            num_storages += 1;
                        }
                        "volatile" => {
                            storage = QueryStorage::Volatile;
                            num_storages += 1;
                        }
                        "dependencies" => {
                            storage = QueryStorage::Dependencies;
                            num_storages += 1;
                        }
                        "input" => {
                            storage = QueryStorage::Input;
                            num_storages += 1;
                        }
                        "invoke" => {
                            invoke = Some(parse_macro_input!(tts as Parenthesized<syn::Path>).0);
                        }
                        "query_type" => {
                            query_type = parse_macro_input!(tts as Parenthesized<Ident>).0;
                        }
                        _ => panic!("unknown salsa attribute `{}`", name),
                    }
                }

                // Check attribute combinations.
                if num_storages > 1 {
                    panic!("multiple storage attributes specified");
                }
                if invoke.is_some() && storage == QueryStorage::Input {
                    panic!("#[salsa::invoke] cannot be set on #[salsa::input] queries");
                }

                // Extract keys.
                let mut iter = method.sig.decl.inputs.iter();
                match iter.next() {
                    Some(FnArg::SelfRef(sr)) if sr.mutability.is_none() => (),
                    _ => panic!(
                        "first argument of query `{}` must be `&self` or `&mut self`",
                        method.sig.ident
                    ),
                }
                let mut keys = vec![];
                for arg in iter {
                    match *arg {
                        FnArg::Captured(ref arg) => {
                            keys.push(arg.ty.clone());
                        }
                        ref a => panic!("unsupported argument `{:?}` of `{}`", a, method.sig.ident),
                    }
                }

                // Extract value.
                let value = match method.sig.decl.output {
                    ReturnType::Type(_, ref ty) => ty.as_ref().clone(),
                    ref r => panic!(
                        "unsupported return type `{:?}` of `{}`",
                        r, method.sig.ident
                    ),
                };

                queries.push(Query {
                    query_type,
                    fn_name: method.sig.ident.clone(),
                    attrs,
                    storage,
                    keys,
                    value,
                    invoke,
                });
            }
            TraitItem::Type(assoc) => {
                associated_type_items.extend(quote! { #assoc });
                assocs.push(assoc.clone());
            }
            _ => (),
        }
    }

    let mut assoc_thread = proc_macro2::TokenStream::new();
    let mut assoc_thread_constraints = proc_macro2::TokenStream::new();
    let mut at2 = proc_macro2::TokenStream::new();
    let mut atc2 = proc_macro2::TokenStream::new();
    let mut assoc_links = proc_macro2::TokenStream::new();
    let mut assoc_items = proc_macro2::TokenStream::new();
    let mut assoc_phantoms = proc_macro2::TokenStream::new();
    let mut assoc_phantom_defaults = proc_macro2::TokenStream::new();
    let mut storage_assoc_phantoms = proc_macro2::TokenStream::new();
    let mut storage_assoc_phantom_defaults = proc_macro2::TokenStream::new();
    let mut assoc_equals = proc_macro2::TokenStream::new();
    for assoc in assocs {
        let ident = &assoc.ident;
        let bounds = &assoc.bounds;
        let thread = Ident::new(
            &format!("{}__", assoc.ident.to_string()),
            Span::call_site(),
        );
        let thread2 = Ident::new(
            &format!("{}__InMainImpl", assoc.ident.to_string()),
            Span::call_site(),
        );
        at2.extend(quote! { #thread2, });
        atc2.extend(quote! { #thread2: #bounds, });
        assoc_thread.extend(quote! {#thread, });
        assoc_thread_constraints.extend(quote! {#thread: #bounds, });
        assoc_items.extend(quote! {
            type #ident = #thread2;
        });
        assoc_links.extend(quote! {
            <DB__ as #trait_name>::#ident,
        });
        assoc_phantoms.extend(quote! {
            #ident: std::marker::PhantomData<#thread>,
        });
        assoc_phantom_defaults.extend(quote! {
            #ident: Default::default(),
        });
        storage_assoc_phantoms.extend(quote! {
            #ident: std::marker::PhantomData<<DB__ as #trait_name>::#ident>,
        });
        storage_assoc_phantom_defaults.extend(quote! {
            #ident: Default::default(),
        });
        assoc_equals.extend(quote! {
            #ident = #thread,
        });
    }

    let group_key = Ident::new(
        &format!("{}GroupKey__", trait_name.to_string()),
        Span::call_site(),
    );

    let group_storage = Ident::new(
        &format!("{}GroupStorage__", trait_name.to_string()),
        Span::call_site(),
    );

    let mut query_fn_declarations = proc_macro2::TokenStream::new();
    let mut query_fn_definitions = proc_macro2::TokenStream::new();
    let mut query_descriptor_variants = proc_macro2::TokenStream::new();
    let mut query_descriptor_maybe_change = proc_macro2::TokenStream::new();
    let mut storage_fields = proc_macro2::TokenStream::new();
    let mut storage_defaults = proc_macro2::TokenStream::new();
    for query in &queries {
        let key_names: &Vec<_> = &(0..query.keys.len())
            .map(|i| Ident::new(&format!("key{}", i), Span::call_site()))
            .collect();
        let keys = &query.keys;
        let value = &query.value;
        let fn_name = &query.fn_name;
        let qt = &query.query_type;
        let attrs = &query.attrs;

        query_fn_declarations.extend(quote! {
            #(#attrs)*
            fn #fn_name(&self, #(#key_names: #keys),*) -> #value;
        });

        query_fn_definitions.extend(quote! {
            fn #fn_name(&self, #(#key_names: #keys),*) -> #value {
                <Self as salsa::plumbing::GetQueryTable<#qt<Self>>>::get_query_table(self).get((#(#key_names),*))
            }
        });

        // For input queries, we need `set_foo` etc
        if let QueryStorage::Input = query.storage {
            let set_fn_name = Ident::new(&format!("set_{}", fn_name), fn_name.span());
            let set_constant_fn_name =
                Ident::new(&format!("set_constant_{}", fn_name), fn_name.span());

            query_fn_declarations.extend(quote! {
                /// Set the value of the `#fn_name` input.
                ///
                /// See [`#fn_name()`][] for details.
                ///
                /// *Note:* Setting values will trigger cancellation
                /// of any ongoing queries; this method blocks until
                /// those queries have been cancelled.
                fn #set_fn_name(&mut self, #(#key_names: #keys,)* value__: #value);

                /// Set the value of the `#fn_name` input and promise
                /// that its value will never change again.
                ///
                /// See [`#fn_name()`][] for details.
                ///
                /// *Note:* Setting values will trigger cancellation
                /// of any ongoing queries; this method blocks until
                /// those queries have been cancelled.
                fn #set_constant_fn_name(&mut self, #(#key_names: #keys,)* value__: #value);
            });

            query_fn_definitions.extend(quote! {
                fn #set_fn_name(&mut self, #(#key_names: #keys,)* value__: #value) {
                    <Self as salsa::plumbing::GetQueryTable<#qt<Self>>>::get_query_table_mut(self).set((#(#key_names),*), value__)
                }

                fn #set_constant_fn_name(&mut self, #(#key_names: #keys,)* value__: #value) {
                    <Self as salsa::plumbing::GetQueryTable<#qt<Self>>>::get_query_table_mut(self).set_constant((#(#key_names),*), value__)
                }
            });
        }

        // A variant for the group descriptor below
        query_descriptor_variants.extend(quote! {
            #fn_name((#(#keys),*)),
        });

        // A variant for the group descriptor below
        query_descriptor_maybe_change.extend(quote! {
            #group_key::#fn_name(key) => {
                let group_storage: &#group_storage<DB__> = salsa::plumbing::HasQueryGroup::group_storage(db);
                let storage = &group_storage.#fn_name;

                <_ as salsa::plumbing::QueryStorageOps<DB__, #qt<DB__>>>::maybe_changed_since(
                    storage,
                    db,
                    revision,
                    key,
                    db_descriptor,
                )
            }
        });

        // A field for the storage struct
        //
        // FIXME(#120): the pub should not be necessary once we complete the transition
        storage_fields.extend(quote! {
            pub #fn_name: <#qt<DB__> as salsa::Query<DB__>>::Storage,
        });
        storage_defaults.extend(quote! { #fn_name: Default::default(), });
    }

    // Emit the trait itself.
    let mut output = {
        let attrs = &input.attrs;
        let bounds = &input.supertraits;
        quote! {
            #(#attrs)*
            #trait_vis trait #trait_name : #bounds {
                #associated_type_items
                #query_fn_declarations
            }
        }
    };

    // Emit the query group struct and impl of `QueryGroup`.
    output.extend(quote! {
        /// Representative struct for the query group.
        #trait_vis struct #group_struct<#assoc_thread_constraints> {
            #assoc_phantoms
        }

        impl<#assoc_thread_constraints> Default for #group_struct<#assoc_thread> {
            fn default() -> Self {
                #group_struct {
                    #assoc_phantom_defaults
                }
            }
        }

        impl<DB__, #assoc_thread_constraints> salsa::plumbing::QueryGroup<DB__> for #group_struct<#assoc_thread>
        where DB__: #trait_name<#assoc_equals>
        {
            type GroupStorage = #group_storage<DB__>;
            type GroupKey = #group_key;
        }
    });


    // Emit an impl of the trait
    output.extend({
        let bounds = &input.supertraits;
        quote! {
            impl<T__, #atc2> #trait_name for T__
            where
                T__: #bounds,
                T__: salsa::plumbing::HasQueryGroup<#group_struct<#at2>>
            {
                #assoc_items
                #query_fn_definitions
            }
        }
    });

    // Emit the query types.
    for query in &queries {
        let fn_name = &query.fn_name;
        let qt = &query.query_type;
        let storage = Ident::new(
            match query.storage {
                QueryStorage::Memoized => "MemoizedStorage",
                QueryStorage::Volatile => "VolatileStorage",
                QueryStorage::Dependencies => "DependencyStorage",
                QueryStorage::Input => "InputStorage",
            },
            Span::call_site(),
        );
        let keys = &query.keys;
        let value = &query.value;
        // Self doesn't make sense outside the original trait definition
        let value = replace_self_ident(quote! {#value}, "DB__", Span::call_site());

        // Emit the query struct and implement the Query trait on it.
        // TODO: only add DB__ param if there are associated types
        output.extend(quote! {
            #[derive(Default, Debug)]
            #trait_vis struct #qt<DB__: #trait_name> {
                #storage_assoc_phantoms
            }

            impl<DB__> salsa::Query<DB__> for #qt<DB__>
            where
                DB__: #trait_name,
            {
                type Key = (#(#keys),*);
                type Value = #value;
                type Storage = salsa::plumbing::#storage<DB__, Self>;
                type Group = #group_struct<#assoc_links>;
                type GroupStorage = #group_storage<DB__>;
                type GroupKey = #group_key;

                fn group_storage(group_storage: &Self::GroupStorage) -> &Self::Storage {
                    &group_storage.#fn_name
                }

                fn group_key(key: Self::Key) -> Self::GroupKey {
                    #group_key::#fn_name(key)
                }
            }
        });

        // Implement the QueryFunction trait for all queries except inputs.
        if query.storage != QueryStorage::Input {
            let span = query.fn_name.span();
            let key_names: &Vec<_> = &(0..query.keys.len())
                .map(|i| Ident::new(&format!("key{}", i), Span::call_site()))
                .collect();
            let key_pattern = if query.keys.len() == 1 {
                quote! { #(#key_names),* }
            } else {
                quote! { (#(#key_names),*) }
            };
            let invoke = match &query.invoke {
                Some(i) => i.into_token_stream(),
                None => query.fn_name.clone().into_token_stream(),
            };
            output.extend(quote_spanned! {span=>
                impl<DB__> salsa::plumbing::QueryFunction<DB__> for #qt<DB__>
                where
                    DB__: #trait_name
                {
                    fn execute(db: &DB__, #key_pattern: <Self as salsa::Query<DB__>>::Key)
                        -> <Self as salsa::Query<DB__>>::Value {
                        #invoke(db, #(#key_names),*)
                    }
                }
            });
        }
    }

    // Emit query group descriptor
    output.extend(quote! {
        #[derive(Clone, Debug, PartialEq, Eq, Hash)]
        #trait_vis enum #group_key {
            #query_descriptor_variants
        }

        impl #group_key {
            #trait_vis fn maybe_changed_since<DB__>(
                &self,
                db: &DB__,
                db_descriptor: &<DB__ as salsa::plumbing::DatabaseStorageTypes>::DatabaseKey,
                revision: salsa::plumbing::Revision,
            ) -> bool
            where
                DB__: #trait_name,
                DB__: salsa::plumbing::HasQueryGroup<#group_struct<#assoc_links>>,
            {
                match self {
                    #query_descriptor_maybe_change
                }
            }
        }
    });

    let mut for_each_ops = proc_macro2::TokenStream::new();
    for Query { fn_name, .. } in &queries {
        for_each_ops.extend(quote! {
            op(&self.#fn_name);
        });
    }

    // Emit query group storage struct
    // It would derive Default, but then all database structs would have to implement Default
    // as the derived version includes an unused `+ Default` constraint.
    output.extend(quote! {
        #trait_vis struct #group_storage<DB__: #trait_name> {
            #storage_fields
            #storage_assoc_phantoms
        }

        impl<DB__: #trait_name> Default for #group_storage<DB__> {
            #[inline]
            fn default() -> Self {
                #group_storage {
                    #storage_defaults
                    #storage_assoc_phantom_defaults
                }
            }
        }

        impl<DB__> #group_storage<DB__>
        where
            DB__: #trait_name,
            DB__: salsa::plumbing::HasQueryGroup<#group_struct<#assoc_links>>,
        {
            #trait_vis fn for_each_query(
                &self,
                db: &DB__,
                mut op: &mut dyn FnMut(&dyn salsa::plumbing::QueryStorageMassOps<DB__>),
            ) {
                #for_each_ops
            }
        }
    });

    if std::env::var("SALSA_DUMP").is_ok() {
        println!("~~~ query_group");
        println!("{}", output.to_string());
        println!("~~~ query_group");
    }

    output.into()
}

fn is_salsa_attr_path(path: &syn::Path) -> bool {
    path.segments
        .first()
        .map(|s| s.value().ident != "salsa")
        .unwrap_or(true)
        || path.segments.len() != 2
}

fn replace_self_ident(
    type_tokens: proc_macro2::TokenStream,
    with: &str,
    span: Span,
) -> proc_macro2::TokenStream {
    use proc_macro2::TokenTree;
    type_tokens
        .into_iter()
        .map(|tok| match tok {
            TokenTree::Ident(ref i) => match i.to_string().as_str() {
                "Self" => TokenTree::Ident(Ident::new(with, span)),
                _ => tok.clone(),
            },
            TokenTree::Group(g) => TokenTree::Group(proc_macro2::Group::new(
                g.delimiter(),
                replace_self_ident(g.stream(), with, span),
            )),
            _ => tok.clone(),
        })
        .collect()
}

#[derive(Debug)]
struct Query {
    fn_name: Ident,
    attrs: Vec<syn::Attribute>,
    query_type: Ident,
    storage: QueryStorage,
    keys: Vec<syn::Type>,
    value: syn::Type,
    invoke: Option<syn::Path>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum QueryStorage {
    Memoized,
    Volatile,
    Dependencies,
    Input,
}
