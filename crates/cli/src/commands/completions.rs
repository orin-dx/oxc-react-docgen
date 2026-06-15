use miette::Result;

pub fn cmd_completions(args: crate::CompletionsArgs) -> Result<()> {
    use clap::CommandFactory;
    clap_complete::generate(
        args.shell,
        &mut crate::Cli::command(),
        "oxc-react-docgen",
        &mut std::io::stdout(),
    );
    Ok(())
}
