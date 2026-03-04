use proc_macro::TokenStream;
use quote::quote;
use syn::{LitStr, parse_macro_input};

/// A macro for cross-platform path string literals in tests.
/// On Windows it replaces `/` with `\\` and adds `C:` to absolute paths.
/// On other platforms, the path is returned unmodified.
#[proc_macro]
pub fn path(input: TokenStream) -> TokenStream {
    let path_str = parse_macro_input!(input as LitStr);
    let path = if cfg!(target_os = "windows") {
        let path = path_str.value().replace('/', "\\");
        if path.starts_with('\\') {
            format!("C:{}", path)
        } else {
            path
        }
    } else {
        path_str.value()
    };

    TokenStream::from(quote! {
        #path
    })
}
