fn main() {
    divan::main();
}

#[divan::bench(args = ["shadcn/button.tsx", "shadcn/input.tsx", "radix/button.d.ts", "mui/Button.d.ts"])]
fn extract_file_bench(bencher: divan::Bencher, fixture: &&str) {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let fixture_dir = manifest_dir
        .join("../../fixtures")
        .join(std::path::Path::new(fixture).parent().unwrap_or(std::path::Path::new(".")));

    let options = oxc_react_docgen_core::pipeline::PipelineOptions {
        src_dirs: vec![camino::Utf8PathBuf::from_path_buf(fixture_dir).expect("valid utf8 path")],
        ..Default::default()
    };

    bencher.bench(|| oxc_react_docgen_core::pipeline::extract(&options));
}
