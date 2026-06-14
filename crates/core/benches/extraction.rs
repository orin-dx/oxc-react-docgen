fn main() {
    divan::main();
}

#[divan::bench(args = ["shadcn/button.tsx", "shadcn/input.tsx", "radix/button.d.ts", "mui/Button.d.ts"])]
fn parse_file_bench(bencher: divan::Bencher, fixture: &&str) {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let fixture_path = manifest_dir.join("../../fixtures").join(fixture);
    let source = std::fs::read_to_string(&fixture_path)
        .unwrap_or_else(|_| panic!("fixture not found: {}", fixture_path.display()));
    let path = camino::Utf8Path::new(fixture);
    bencher.bench(|| oxc_react_docgen_core::extractor::parse_file(path, &source));
}
