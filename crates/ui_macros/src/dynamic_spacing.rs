use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{
    Error, LitInt, Token, parse::Parse, parse::ParseStream, parse_macro_input,
    punctuated::Punctuated,
};

struct DynamicSpacingInput {
    values: Punctuated<DynamicSpacingValue, Token![,]>,
}

// The input for the derive macro is a list of values.
//
// When a single value is provided, the standard spacing formula is
// used to derive the of spacing values.
//
// When a tuple of three values is provided, the values are used as the
// spacing values directly.
struct DynamicSpacingValue {
    compact: u16,
    base: u16,
    comfortable: u16,
}

struct SpacingNumber {
    value: u16,
    literal: LitInt,
}

impl Parse for DynamicSpacingInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(DynamicSpacingInput {
            values: input.parse_terminated(DynamicSpacingValue::parse, Token![,])?,
        })
    }
}

impl Parse for DynamicSpacingValue {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.peek(syn::token::Paren) {
            let content;
            syn::parenthesized!(content in input);

            let compact = parse_spacing_number(&content)?;
            content.parse::<Token![,]>()?;
            let base = parse_spacing_number(&content)?;
            content.parse::<Token![,]>()?;
            let comfortable = parse_spacing_number(&content)?;
            if !content.is_empty() {
                return Err(content.error("expected exactly three spacing values"));
            }

            if compact.value > base.value {
                return Err(Error::new(
                    compact.literal.span(),
                    "spacing values must be ordered compact <= base <= comfortable",
                ));
            }

            if base.value > comfortable.value {
                return Err(Error::new(
                    comfortable.literal.span(),
                    "spacing values must be ordered compact <= base <= comfortable",
                ));
            }

            Ok(Self {
                compact: compact.value,
                base: base.value,
                comfortable: comfortable.value,
            })
        } else {
            let base = parse_spacing_number(input)?;
            let comfortable = base
                .value
                .checked_add(4)
                .ok_or_else(|| Error::new(base.literal.span(), "spacing value is too large"))?;

            Ok(Self {
                compact: base.value.saturating_sub(4),
                base: base.value,
                comfortable,
            })
        }
    }
}

fn parse_spacing_number(input: ParseStream) -> syn::Result<SpacingNumber> {
    let literal: LitInt = input.parse()?;
    Ok(SpacingNumber {
        value: literal.base10_parse()?,
        literal,
    })
}

/// Derives the spacing method for the `DynamicSpacing` enum.
pub(crate) fn derive_spacing(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DynamicSpacingInput);

    let spacing_ratios: Vec<_> = input
        .values
        .iter()
        .map(|value| {
            let variant = format_ident!("Base{:02}", value.base);
            let compact = f32::from(value.compact);
            let base = f32::from(value.base);
            let comfortable = f32::from(value.comfortable);

            quote! {
                DynamicSpacing::#variant => match ::theme::ThemeSettings::get_global(cx).ui_density {
                    ::theme::UiDensity::Compact => #compact / BASE_REM_SIZE_IN_PX,
                    ::theme::UiDensity::Default => #base / BASE_REM_SIZE_IN_PX,
                    ::theme::UiDensity::Comfortable => #comfortable / BASE_REM_SIZE_IN_PX,
                }
            }
        })
        .collect();

    let (variant_names, doc_strings): (Vec<_>, Vec<_>) = input
        .values
        .iter()
        .map(|value| {
            let variant = format_ident!("Base{:02}", value.base);
            let compact = value.compact;
            let base = value.base;
            let comfortable = value.comfortable;
            let doc_string = format!(
                "`{compact}px`|`{base}px`|`{comfortable}px (@16px/rem)` - Scales with the user's rem size."
            );
            (quote!(#variant), quote!(#doc_string))
        })
        .unzip();

    let expanded = quote! {
        /// A dynamic spacing system that adjusts spacing based on [UiDensity].
        ///
        /// The number following "Base" refers to the base pixel size
        /// at the default rem size and spacing settings.
        ///
        /// When possible, [DynamicSpacing] should be used over manual
        /// or built-in spacing values in places dynamic spacing is needed.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum DynamicSpacing {
            #(
                #[doc = #doc_strings]
                #variant_names,
            )*
        }

        impl DynamicSpacing {
            /// Returns the spacing ratio, should only be used internally.
            fn spacing_ratio(&self, cx: &App) -> f32 {
                const BASE_REM_SIZE_IN_PX: f32 = 16.;
                match self {
                    #(#spacing_ratios,)*
                }
            }

            /// Returns the spacing value in rems.
            pub fn rems(&self, cx: &App) -> Rems {
                ::gpui::rems(self.spacing_ratio(cx))
            }

            /// Returns the spacing value in pixels.
            pub fn px(&self, cx: &App) -> Pixels {
                let ui_font_size_f32: f32 =
                    ::theme::ThemeSettings::get_global(cx).ui_font_size(cx).into();
                ::gpui::px(ui_font_size_f32 * self.spacing_ratio(cx))
            }
        }
    };

    TokenStream::from(expanded)
}
