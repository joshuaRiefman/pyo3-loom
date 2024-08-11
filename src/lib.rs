extern crate proc_macro;
use proc_macro::TokenStream;
use quote::{quote, format_ident};
use syn::{parse_macro_input, ItemFn, FnArg, LitStr, PatType, Type, Pat, Ident, Token, parse::Parse, parse::ParseStream, punctuated::Punctuated, PathArguments, GenericArgument, TypePath, ReturnType};

fn ident_to_literal(ident: &Ident) -> proc_macro2::TokenStream {
    let ident_string = ident.to_string();
    let literal = LitStr::new(&ident_string, ident.span());

    return quote! { #literal };
}

fn into_wrapper_name(func_name: &Ident) -> Ident {
    return format_ident!("__internal_{}_wrapper", func_name);
}

fn extract_array_dtype(type_path: &TypePath) -> Option<&Ident> {
    let last_segment = type_path.path.segments.last()?;
    let args = match &last_segment.arguments {
        PathArguments::AngleBracketed(pat_args) => &pat_args.args,
        _ => return None,
    };

    for arg in args {
        if let GenericArgument::Type(Type::Path(pat_arg_path)) = arg {
            return Some(&pat_arg_path.path.segments.last()?.ident);
        }
    }

    return None;
}

fn process_return_type(output: &ReturnType) -> Option<proc_macro2::TokenStream> {
    let return_type;

    if let ReturnType::Type(_, boxed_type) = &*output {
        if let Type::Path(output_type) = &**boxed_type {
            let output_ident = &output_type.path.segments.last()?.ident;

            return_type = return match output_ident.to_string().as_str() {
                "Vec" => {
                    let output_array_dtype = extract_array_dtype(output_type)?;
                    Some(quote! { PyArrayDyn<#output_array_dtype> })
                },
                _ => Some(quote! { output })
            };
        }
    }

    return None;
}

#[proc_macro_attribute]
pub fn pyo3_wrapper(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);

    // Extract function parameters
    let name = &input.sig.ident;
    let vis = &input.vis;
    let _block = &input.block;
    let inputs = input.sig.inputs.iter().collect::<Vec<_>>();
    let output = &input.sig.output;

    // Containers for generated wrapper types
    let mut py_args = Vec::new();
    let mut rust_args = Vec::new();
    let mut conversion_code = Vec::new();

    // Loop over input arguments, determining their type and matching them to the corresponding Python type
    for arg in inputs.iter() {
        if let FnArg::Typed(PatType { pat, ty, .. }) = arg {
            if let Type::Path(type_path) = &**ty {
                match type_path.path.segments.last().unwrap().ident.to_string().as_str() {
                    "ArrayViewD" => {
                        if let Pat::Ident(pat_ident) = &**pat {
                            let arg_name = &pat_ident.ident;
                            let array_dtype = extract_array_dtype(type_path).unwrap();
                            let conversion = quote! {
                                let #arg_name = #arg_name.as_array();
                            };

                            conversion_code.push(conversion);
                            py_args.push(quote! { #arg_name: PyReadwriteArrayDyn<'py, #array_dtype> });
                            rust_args.push(quote! { #arg_name });
                        }
                    }
                    _ => {
                        py_args.push(quote!{ #pat });
                        rust_args.push(quote!{ #pat });
                    }
                }
            }
        }
    }

    let return_type = process_return_type(output).unwrap();
    let wrapper_name = into_wrapper_name(name);
    let pyo3_name = ident_to_literal(name);
    let rust_args_names = rust_args.iter().map(|arg| quote! { #arg }).collect::<Vec<_>>();

    let generated = quote! {
        #input

        #[pyo3::prelude::pyfunction]
        #[pyo3(name = #pyo3_name)]
        #vis fn #wrapper_name<'py>(
            py: Python<'py>,
            #(#py_args),*
        ) -> &'py #return_type {
            #(#conversion_code)*
            let result = #name(#(#rust_args_names),*);
            let py_result = PyArray::from_vec(py, result).to_dyn();
            py_result
        }
    };

    println!("{}", generated);

    TokenStream::from(generated)
}

struct FunctionNames {
    module_name: LitStr,
    functions: Punctuated<Ident, Token![,]>,
}

impl Parse for FunctionNames {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let module_name: LitStr = input.parse()?;
        input.parse::<Token![,]>()?;  // consume the comma
        let functions = Punctuated::parse_terminated(input)?;
        Ok(FunctionNames { module_name, functions })
    }
}

#[proc_macro]
pub fn create_pymodule(input: TokenStream) -> TokenStream {
    let FunctionNames { module_name, functions } = parse_macro_input!(input as FunctionNames);

    let wrapped_func_names = functions.iter().map(|name| into_wrapper_name(name));

    let generated = quote! {
        #[pyo3::pymodule]
        #[pyo3(name = #module_name)]
        pub fn lib(py: pyo3::Python, m: &pyo3::types::PyModule) -> pyo3::PyResult<()> {
            use pyo3::wrap_pyfunction;
            #(
                m.add_function(wrap_pyfunction!(#wrapped_func_names, m)?)?;
            )*
            Ok(())
        }
    };

    println!("{}", generated);

    TokenStream::from(generated)
}
