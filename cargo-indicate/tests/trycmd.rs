#[test]
fn cli_test() {
    trycmd::TestCases::new()
        .case("tests/cmd/*.md")
        .case("tests/cmd/*.toml")
        .case("README.md");
}
