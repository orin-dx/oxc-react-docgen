#[test]
fn cli_trycmd_tests() {
    std::env::set_var("COLUMNS", "80");
    let t = trycmd::TestCases::new();
    t.case("tests/cmd/*.trycmd");
}
