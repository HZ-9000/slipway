#[test]
fn find_a_match() {
    let mut result = Vec::new();
    slipway::find_matches("lorem ipsum\ndolor sit amet", "lorem", &mut result).unwrap();
    assert_eq!(result, b"lorem ipsum\n");
}
