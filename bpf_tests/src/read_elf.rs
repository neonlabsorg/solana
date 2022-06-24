use regex::Regex;
use anyhow::{anyhow, Context};
use std::{
    env,
    ffi::OsStr,
    path::{Path, PathBuf},
    process::{ Command, Stdio},
    fs,

};

// Start a new process running the program and capturing its output.
fn spawn<I, S>(program: &Path, args: I) -> Result<(String, String), anyhow::Error>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
{
    let child = Command::new(program)
        .args(args)
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .with_context(|| format!("Failed to execute {}", program.display()))?;

    let output = child.wait_with_output().expect("failed to wait on child");
    Ok((
        String::from_utf8(output.stdout).context("process stdout is not valid utf8")?,
        String::from_utf8(output.stderr).context("process stderr is not valid utf8")?,
    ))
}

fn extract_sections_list(output: &str) -> Vec<String> {
    let head_re = Regex::new(r"^ +\[[ 0-9]+\] (.bss[^ ]*) .*").unwrap();

    let mut result: Vec<String> = Vec::new();
    for line in output.lines() {
        let line = line.trim_end();
        if let Some(captures) = head_re.captures(line) {
            result.push(captures[1].to_string());
        }
    }

    result
}

fn llvm_home() -> Result<PathBuf, anyhow::Error> {
    if let Ok(home) = env::var("LLVM_HOME") {
        return Ok(home.into());
    }

    let home_dir = PathBuf::from(env::var("HOME").context("Can't get home directory path")?);
    Ok(home_dir
        .join(".cache")
        .join("solana")
        // .join("v1.25")
        .join("v1.19")
        .join("bpf-tools")
        .join("llvm"))
}

fn remove_bss_sections(module: &Path) -> Result<(), anyhow::Error> {
    let module = module.to_string_lossy();
    let llvm_path = llvm_home()?.join("bin");
    let readelf = llvm_path.join("llvm-readelf");
    let mut readelf_args = vec!["--section-headers"];
    readelf_args.push(&module);

    let output = spawn(&readelf, &readelf_args)?.0;
    let sections = extract_sections_list(&output);
    for bss in sections {
        let objcopy = llvm_path.join("llvm-objcopy");
        let mut objcopy_args = vec!["--remove-section"];
        objcopy_args.push(&bss);
        objcopy_args.push(&module);
        spawn(&objcopy, &objcopy_args)?;
    }

    Ok(())
}


pub fn read_so(filename: &str) -> Result<Vec<u8>, anyhow::Error>{
    let mut path =  PathBuf::new();
    path.push(filename);

    if !path.exists() {
        return Err(anyhow!("No such file or directory: {}", path.to_string_lossy()).into());
    }

    remove_bss_sections(&path)?;
    Ok(fs::read(&path)?)
}

pub fn read_bin(filename: &str) -> Result<Vec<u8>, anyhow::Error>{
    let mut path =  PathBuf::new();
    path.push(filename);

    if !path.exists() {
        return Err(anyhow!("No such file or directory: {}", path.to_string_lossy()).into());
    }
    Ok(fs::read(&path)?)
}
