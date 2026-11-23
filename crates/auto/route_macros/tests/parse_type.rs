#[test]
fn parses_a_generic_response_type() {
    let ty: syn::Type = syn::parse_str("Vec<AccountRow>").unwrap();
    assert_eq!(quote::quote!(#ty).to_string(), "Vec < AccountRow >");
}
