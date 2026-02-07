mod derive_register_component;
mod dynamic_spacing;

use proc_macro::TokenStream;

/// Generates the DynamicSpacing enum used for density-aware spacing in the UI.
#[proc_macro]
pub fn derive_dynamic_spacing(input: TokenStream) -> TokenStream {
    dynamic_spacing::derive_spacing(input)
}

/// Registers components that implement the `Component` trait.
#[proc_macro_derive(RegisterComponent)]
pub fn derive_register_component(input: TokenStream) -> TokenStream {
    derive_register_component::derive_register_component(input)
}
