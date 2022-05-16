use structopt::StructOpt;
use std::{
    path::{PathBuf},
};


#[derive(Debug, StructOpt)]
#[structopt(
name = "bpf-vm-tests",
about = "Test runner for the bpf-vm"
)]
pub struct Opt {
    #[allow(dead_code)]
    #[structopt(long, hidden = true)]
    pub quiet: bool,
    /// RBPF heap size
    #[structopt(long)]
    pub heap_size: Option<usize>,
    #[structopt(parse(from_os_str))]
    pub file: PathBuf,
}


