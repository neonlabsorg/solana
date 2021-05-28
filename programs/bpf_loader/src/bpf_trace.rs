//! mod bpf_trace.

use lazy_static::lazy_static;
use log::{trace, warn};
use solana_sdk::pubkey::Pubkey;
use std::io::{Error, Result, Write};
use std::sync::Mutex;

/// Controls the trace if the environment variable exists.
pub fn control(header: &str, trace: &str, program_id: &Pubkey) {
    trace!("BPF Program: {}", &program_id);

    let port = std::env::var("SOLANA_BPF_TRACE_CONTROL").unwrap_or_default();
    if port.is_empty() {
        trace!("{}\n{}", header, trace);
        return;
    }

    if let Err(err) = ensure_start_tcp_server(&port) {
        warn!("{}", err);
        trace!("{}\n{}", header, trace);
        return;
    }

    let cfg = config();

    if !cfg.enable {
        trace!("BPF Trace is disabled");
        return;
    }

    if !cfg.passes_program(&program_id) {
        trace!("BPF Trace is disabled for program {})", &program_id);
        return;
    }

    if cfg.output.is_empty() {
        trace!("{}\n{}", header, trace);
        return;
    }

    let filename = cfg.generate_filename();
    if let Err(err) = write_bpf_trace(&filename, &program_id.to_string(), header, trace) {
        warn!("{}", err);
        trace!("{}\n{}", header, trace);
        return;
    }
    trace!("BPF Trace is written to file {}", &filename);
}

lazy_static! {
    static ref CONFIG: Mutex<BpfTraceConfig> = Mutex::new(BpfTraceConfig::default());
}

fn config() -> BpfTraceConfig {
    CONFIG.lock().unwrap().clone()
}

fn set_enable(enable: bool) {
    CONFIG.lock().unwrap().enable = enable;
}

/// Represents parameters to control BPF tracing.
#[derive(Default, Clone)]
struct BpfTraceConfig {
    enable: bool,
    filter: String,
    output: String,
    multiple_output: bool,
}

impl BpfTraceConfig {
    /// Checks if the program id is in the filter. Example:
    /// evm_loader:3CMCRJieHS3sWWeovyFyH4iRyX4rHf3u2zbC5RCFrRex
    /// Empty filter passes everything.
    fn passes_program(&self, id: &Pubkey) -> bool {
        if self.filter.is_empty() {
            return true;
        }
        let pro: Vec<&str> = self.filter.split(":").collect();
        pro.len() == 2 && pro[1] == id.to_string()
    }

    /// Creates new output filename.
    fn generate_filename(&self) -> String {
        assert!(!self.output.is_empty());
        let mut result = self.output.clone();
        if !self.filter.is_empty() {
            let pro: Vec<&str> = self.filter.split(":").collect();
            if pro.len() == 2 {
                result += "_";
                result += pro[0];
            }
        }
        if self.multiple_output {
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

/// Writes a BPF trace into file.
fn write_bpf_trace(filename: &str, program_id: &str, header: &str, trace: &str) -> Result<()> {
    use std::io::BufWriter;
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

lazy_static! {
    static ref SERVER: Mutex<TcpServer> = Mutex::new(TcpServer::default());
}

/// Starts the TCP server if not yet running.
fn ensure_start_tcp_server(port: &str) -> Result<()> {
    if !SERVER.lock().unwrap().is_started() {
        SERVER.lock().unwrap().start(port)?;
    }
    Ok(())
}

/// Represents simple single-threaded TCP server to accept control commands.
#[derive(Default)]
struct TcpServer {
    port: String,
}

use std::net::{TcpListener, TcpStream};

impl TcpServer {
    fn is_started(&self) -> bool {
        !self.port.is_empty()
    }

    fn start(&mut self, port: &str) -> Result<()> {
        assert!(self.port.is_empty());
        self.port = port.into();
        let addr = format!("127.0.0.1:{}", port);
        let listener = TcpListener::bind(&addr)
            .map_err(|e| Error::new(e.kind(), format!("TcpServer {}: '{}'", e, port)))?;
        let _ = std::thread::spawn(move || listen(listener));
        Ok(())
    }
}

/// Starts listening incoming connections.
fn listen(listener: TcpListener) -> Result<()> {
    for stream in listener.incoming() {
        handle_connection(stream?);
    }
    Ok(())
}

/// Accepts a control command and handles it.
fn handle_connection(mut stream: TcpStream) {
    use std::io::Read;
    let mut buf = [0; 512];

    if let Err(err) = stream.read(&mut buf) {
        warn!("{}", err);
        return;
    }

    let input = String::from_utf8_lossy(&buf);
    dbg!(&input);
    stream.write_all(input.as_bytes()).ok();

    set_enable(false);
}
