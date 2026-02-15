mod dynamic_spacing;

use proc_macro::TokenStream;

/// Generates the DynamicSpacing enum used for density-aware spacing in the UI.
#[proc_macro]
pub fn derive_dynamic_spacing(input: TokenStream) -> TokenStream {
    dynamic_spacing::derive_spacing(input)
}
