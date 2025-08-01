use clap::Parser;

#[derive(Parser, Debug)]
pub struct ProgramParameters {
    #[clap(long, default_value = "main")]
    pub celestial_branch: String,
    #[clap(long, default_value = "main")]
    pub debugger_branch: String,
}