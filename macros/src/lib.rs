use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::{format_ident, quote};
use syn::{Error, Ident, LitInt, Result, Token, Visibility, parse::Parse, parse_macro_input};

struct Input {
    vis: Visibility,
    name: Ident,
    bits: LitInt,
}

impl Parse for Input {
    fn parse(input: syn::parse::ParseStream) -> Result<Self> {
        let vis = input.parse()?;
        let name: Ident = input.parse()?;
        let _comma: Token![,] = input.parse()?;
        let bits: LitInt = input.parse()?;
        Ok(Self { vis, name, bits })
    }
}

/// Creates an enum that reprsent all valid numbers for the number of bits in the input
#[proc_macro]
pub fn make_enum(input: TokenStream) -> TokenStream {
    let Input { vis, name, bits } = parse_macro_input!(input as Input);

    let bits_val: u32 = match bits.base10_parse() {
        Ok(v) if v > 0 && v <= 128 => v,
        Err(e) => return Error::new(bits.span(), e).to_compile_error().into(),
        _ => {
            return Error::new(bits.span(), "bits must be between 1 and 128")
                .to_compile_error()
                .into();
        }
    };

    let max_value = 1u128
        .checked_shl(bits_val)
        .map(|v| v - 1)
        .unwrap_or(u128::MAX);

    let repr_ty = match max_value {
        ..256 => quote!(u8),
        ..65_536 => quote!(u16),
        ..4_294_967_296 => quote!(u32),
        ..18_446_744_073_709_551_615 => quote!(u64),
        _ => quote!(u128),
    };

    let variants = (0..=max_value).map(|i| {
        let ident = format_ident!("r{:02X}", i);
        let val = syn::LitInt::new(&i.to_string(), Span::call_site());
        quote!( #ident = #val, )
    });

    let expanded = quote! {
        #[repr(#repr_ty)]
        #[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
        #vis enum #name {
            #(#variants)*
        }
    };

    expanded.into()
}
