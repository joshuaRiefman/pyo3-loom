extern crate proc_macro;
use proc_macro::TokenStream;
use quote::{quote, format_ident};
use syn::{parse_macro_input, ItemFn, FnArg, LitStr, PatType, Type, Pat, Ident, Token, parse::Parse, parse::ParseStream, punctuated::Punctuated};

#[proc_macro_attribute]
pub fn pyo3_wrapper(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);

    let name = &input.sig.ident;
    let vis = &input.vis;
    let block = &input.block;

    // Extract function parameters
    let inputs = input.sig.inputs.iter().collect::<Vec<_>>();
    let mut py_args = Vec::new();
    let mut rust_args = Vec::new();
    let mut conversion_code = Vec::new();

    for arg in inputs.iter() {
        if let FnArg::Typed(PatType { pat, ty, .. }) = arg {
            if let Type::Path(type_path) = &**ty {
                if type_path.path.segments.last().unwrap().ident == "ArrayViewD" {
                    if let Pat::Ident(pat_ident) = &**pat {
                        let arg_name = &pat_ident.ident;
                        let py_arg = quote! { #arg_name: PyReadwriteArrayDyn<'py, f64> };
                        py_args.push(py_arg);
                        let conversion = quote! {
                            let #arg_name = #arg_name.as_array();
                        };
                        conversion_code.push(conversion);
                        rust_args.push(quote! { #arg_name });
                    }
                } else {
                    py_args.push(quote!{ #pat });
                    rust_args.push(quote!{ #pat });
                }
            }
        }
    }

    let wrapper_name = format_ident!("{}_wrapper", name);
    let rust_args_names = rust_args.iter().map(|arg| quote! { #arg }).collect::<Vec<_>>();

    let generated = quote! {
        #input

        #[pyo3::prelude::pyfunction]
        #[pyo3(name = #name)]
        #vis fn #wrapper_name<'py>(
            py: Python<'py>,
            #(#py_args),*
        ) -> &'py PyArrayDyn<i64> {
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

    let wrapped_func_names = functions.iter().map(|name| format_ident!("{}_wrapper", name));

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
