extern crate proc_macro;
#[proc_macro_attribute]
pub fn demo(_attr: proc_macro::TokenStream, input: proc_macro::TokenStream) -> proc_macro::TokenStream { input }
