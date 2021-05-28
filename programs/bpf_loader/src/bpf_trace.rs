use log::{trace, warn};
use solana_sdk::pubkey::Pubkey;

/// Controls the trace if the environment variable exists.
pub fn control(header: &str, trace: &str, program_id: &Pubkey) {
    trace!("BPF Program: {}", &program_id);

    let trace_control = std::env::var("SOLANA_BPF_TRACE_CONTROL").unwrap_or_default();
    if trace_control.is_empty() {
        trace!("{}\n{}", header, trace);
        return;
    }

    let cfg = match parse_bpf_trace_control_file(&trace_control) {
        Ok(cfg) => cfg,
        Err(err) => {
            warn!("{}", err);
            trace!("{}\n{}", header, trace);
            return;
        }
    };

    if !cfg.enable {
        trace!("BPF Trace is disabled (see {})", &trace_control);
        return;
    }

    let program_id = program_id.to_string();
    if !cfg.passes_program(&program_id) {
        trace!(
            "BPF Trace is disabled for program {} (see {})",
            &program_id,
            &trace_control
        );
        return;
    }

    if cfg.trace_file.is_empty() {
        trace!("{}\n{}", header, trace);
        return;
    }

    let filename = cfg.generate_filename();
    if let Err(err) = write_bpf_trace(&filename, &program_id, header, trace) {
        warn!("{}", err);
        trace!("{}\n{}", header, trace);
        return;
    }
    trace!("BPF Trace is written to file {}", &filename);
}

/// Represents parameters to control BPF tracing.
#[derive(Default, Debug)]
struct BpfTraceConfig {
    enable: bool,
    filter: String,
    trace_file: String,
    new_trace_file_each_execution: bool,
}

impl BpfTraceConfig {
    /// Checks if the program id is in the filter. Example:
    /// evm_loader:3CMCRJieHS3sWWeovyFyH4iRyX4rHf3u2zbC5RCFrRex
    /// Empty filter passes everything.
    fn passes_program(&self, id: &str) -> bool {
        if self.filter.is_empty() {
            return true;
        }
        let pro: Vec<&str> = self.filter.split(":").collect();
        pro.len() == 2 && pro[1] == id
    }

    /// Creates new output filename.
    fn generate_filename(&self) -> String {
        assert!(!self.trace_file.is_empty());
        let mut result = self.trace_file.clone();
        if !self.filter.is_empty() {
            let pro: Vec<&str> = self.filter.split(":").collect();
            if pro.len() == 2 {
                result += "_";
                result += pro[0];
            }
        }
        if self.new_trace_file_each_execution {
            let now = std::time::SystemTime::now();
            let epoch = now.duration_since(std::time::SystemTime::UNIX_EPOCH);
            if let Ok(epoch) = epoch {
                result += "_";
                result += &epoch.as_nanos().to_string();
            }
        }
        result
    }
}

/// Parses a BPF tracing control file. For example:
/// ```conf
/// enable=true
/// filter=evm_loader:3CMCRJieHS3sWWeovyFyH4iRyX4rHf3u2zbC5RCFrRex
/// trace-file=/tmp/trace
/// new-trace-file-each-execution=true
/// ```
fn parse_bpf_trace_control_file(filename: &str) -> std::io::Result<BpfTraceConfig> {
    use std::io::BufRead;
    use std::io::{Error, ErrorKind};

    let mut cfg = BpfTraceConfig::default();
    let file = std::fs::File::open(filename)
        .map_err(|e| Error::new(e.kind(), format!("{}: '{}'", e, filename)))?;
    let reader = std::io::BufReader::new(file);

    const COMMENT: &str = "#";
    for line in reader.lines() {
        let mut line = line?;
        line.retain(|c| !c.is_whitespace());
        if line.starts_with(COMMENT) {
            continue;
        }
        let kv: Vec<&str> = line.split("=").collect();
        if kv.len() != 2 {
            return Err(Error::new(
                ErrorKind::Other,
                format!("Invalid format of BPF trace control parameter '{}'", &line),
            ));
        }
        match kv[0] {
            "enable" => cfg.enable = !(kv[1] == "0" || kv[1] == "false"),
            "filter" => cfg.filter = kv[1].into(),
            "trace-file" => cfg.trace_file = kv[1].into(),
            "new-trace-file-each-execution" => {
                cfg.new_trace_file_each_execution = !(kv[1] == "0" || kv[1] == "false")
            }
            _ => {
                return Err(Error::new(
                    ErrorKind::Other,
                    format!("Unsupported BPF trace control parameter '{}'", &line),
                ));
            }
        }
    }

    Ok(cfg)
}

/// Writes a BPF trace into file.
fn write_bpf_trace(
    filename: &str,
    program_id: &str,
    header: &str,
    trace: &str,
) -> std::io::Result<()> {
    use std::io::{BufWriter, Error, Write};
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(filename)
        .map_err(|e| Error::new(e.kind(), format!("{}: '{}'", e, filename)))?;
    let mut output = BufWriter::new(file);
    let timestamp = std::time::SystemTime::now();
    write!(
        output,
        "[{:?} TRACE solana_bpf_loader_program] BPF Program: {}",
        &timestamp, program_id
    )?;
    write!(
        output,
        "[{:?} TRACE solana_bpf_loader_program] {}\n{}",
        &timestamp, header, trace
    )?;
    output.flush()
}
