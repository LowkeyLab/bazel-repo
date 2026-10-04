extern crate proc_macro;
#[proc_macro_derive(Demo)]
pub fn demo(input: proc_macro::TokenStream) -> proc_macro::TokenStream { input }
