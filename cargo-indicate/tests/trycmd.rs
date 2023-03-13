#[test]
fn cli_test() {
    trycmd::TestCases::new()
        .case("tests/cmd/*.md")
        .case("README.md");
}
