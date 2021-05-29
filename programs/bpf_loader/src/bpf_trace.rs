//! mod bpf_trace.

use lazy_static::lazy_static;
use log::{trace, warn};
use solana_sdk::pubkey::Pubkey;
use std::io::{Error, Result, Write};
use std::sync::Mutex;

const PORT: &str = "SOLANA_BPF_TRACE_CONTROL";
const SERVICE: &str = "BPF Trace Control Service";

/// Controls the trace if the environment variable exists.
pub fn control(header: &str, trace: &str, program_id: &Pubkey) {
    trace!("BPF Program: {}\n", &program_id);

    let port = std::env::var(PORT).unwrap_or_default();
    if port.is_empty() {
        trace!("{}\n{}", header, trace);
        return;
    }

    if !service_started() {
        warn!("{} did not started", SERVICE);
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
    if filename.is_empty() {
        trace!("{}\n{}", header, trace);
        return;
    }

    if let Err(err) = write_bpf_trace(&filename, &program_id.to_string(), header, trace) {
        warn!("{}", err);
        trace!("{}\n{}", header, trace);
        return;
    }
    trace!("BPF Trace is written to file {}", &filename);
}

/// Represents parameters to control BPF tracing.
#[derive(Clone)]
struct BpfTraceConfig {
    enable: bool,
    filter: String,
    output: String,
    multiple: bool,
}

lazy_static! {
    static ref CONFIG: Mutex<BpfTraceConfig> = Mutex::new(BpfTraceConfig::new());
}

fn config() -> BpfTraceConfig {
    CONFIG.lock().unwrap().clone()
}

fn config_to_string() -> String {
    let cfg = CONFIG.lock().unwrap();
    format!(
        "enable = {}\nfilter = {}\noutput = {}\nmultiple = {}",
        cfg.enable, cfg.filter, cfg.output, cfg.multiple
    )
}

fn get_enable() -> String {
    format!("enable = {}", CONFIG.lock().unwrap().enable)
}

fn get_filter() -> String {
    format!("filter = {}", CONFIG.lock().unwrap().filter)
}

fn get_output() -> String {
    format!("output = {}", CONFIG.lock().unwrap().output)
}

fn get_multiple() -> String {
    format!("multiple = {}", CONFIG.lock().unwrap().multiple)
}

fn set_enable(value: bool) -> String {
    CONFIG.lock().unwrap().enable = value;
    format!("enable = {}", value)
}

fn set_filter(value: &str) -> String {
    CONFIG.lock().unwrap().filter = value.into();
    format!("filter = {}", value)
}

fn set_output(value: &str) -> String {
    CONFIG.lock().unwrap().output = value.into();
    format!("output = {}", value)
}

fn set_multiple(value: bool) -> String {
    CONFIG.lock().unwrap().multiple = value;
    format!("multiple = {}", value)
}

impl BpfTraceConfig {
    /// Creates config with reasonable initial state.
    fn new() -> Self {
        BpfTraceConfig {
            enable: true,
            filter: String::default(),
            output: "/tmp/trace".into(),
            multiple: true,
        }
    }

    /// Checks if the program id is in the filter. Example:
    /// evm_loader:3CMCRJieHS3sWWeovyFyH4iRyX4rHf3u2zbC5RCFrRex
    /// Empty filter passes everything.
    fn passes_program(&self, id: &Pubkey) -> bool {
        self.filter.is_empty() || {
            let sep = self.filter.find(":").unwrap_or_default();
            sep != usize::default() && id.to_string() == &self.filter[sep + 1..]
        }
    }

    /// Creates new output filename.
    fn generate_filename(&self) -> String {
        let mut result = self.output.clone();
        if result.is_empty() {
            return result;
        }
        if !self.filter.is_empty() {
            let sep = self.filter.find(":").unwrap_or_default();
            if sep != usize::default() {
                result += "_";
                result += &self.filter[0..sep];
            }
        }
        if self.multiple {
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

/// Represents simple single-threaded TCP server to accept control commands.
#[derive(Default)]
struct TcpServer {
    is_running: bool,
}

lazy_static! {
    static ref SERVER: Mutex<TcpServer> = Mutex::new(TcpServer::start());
}

fn service_started() -> bool {
    SERVER.lock().unwrap().is_running
}

use std::net::{TcpListener, TcpStream};

impl TcpServer {
    fn start() -> Self {
        let port = std::env::var(PORT).unwrap_or_default();
        if port.is_empty() {
            return TcpServer::default();
        }

        let addr = format!("127.0.0.1:{}", &port);
        let listener = TcpListener::bind(&addr).map_err(|e| warn!("{} {}: '{}'", SERVICE, e, port));
        if listener.is_err() {
            TcpServer::default()
        } else {
            let _ = std::thread::spawn(move || listen(listener.unwrap()));
            TcpServer { is_running: true }
        }
    }
}

/// Starts listening incoming connections.
fn listen(listener: TcpListener) -> Result<()> {
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => handle_connection(stream),
            Err(err) => {
                warn!("{}: incoming connection: {}", SERVICE, err);
            }
        }
    }
    Ok(())
}

/// Accepts a control input and handles it.
fn handle_connection(mut stream: TcpStream) {
    use std::io::Read;
    let mut buf = [0; 256];
    let bytes_read = stream.read(&mut buf).map_err(|e| warn!("{}", e));
    if bytes_read.is_ok() {
        let mut command = String::from_utf8_lossy(&buf[0..bytes_read.unwrap()]).to_string();
        command.retain(|c| !c.is_whitespace());
        dispatch_command(&command, stream);
    }
}

/// Executes a command.
fn dispatch_command(command: &str, mut stream: TcpStream) {
    if command == "show" {
        stream.write_all(config_to_string().as_bytes()).ok();
        return;
    } else if command == "enable" {
        stream.write_all(get_enable().as_bytes()).ok();
        return;
    } else if command == "filter" {
        stream.write_all(get_filter().as_bytes()).ok();
        return;
    } else if command == "output" {
        stream.write_all(get_output().as_bytes()).ok();
        return;
    } else if command == "multiple" {
        stream.write_all(get_multiple().as_bytes()).ok();
        return;
    }

    let sep = command.find("=").unwrap_or_default();
    if sep == usize::default() {
        let msg = format!(
            "Invalid format of BPF trace control parameter '{}'",
            &command
        );
        warn!("{}", &msg);
        stream.write_all(msg.as_bytes()).ok();
        return;
    }

    let key = &command[0..sep];
    let value = &command[sep + 1..];

    let resp = match key {
        "enable" => set_enable(value != "false" && value != "0"),
        "filter" => set_filter(value),
        "output" => set_output(value),
        "multiple" => set_multiple(value != "false" && value != "0"),
        _ => {
            let msg = format!("Unsupported BPF trace control parameter '{}'", &command);
            warn!("{}", &msg);
            msg
        }
    };

    stream.write_all(resp.as_bytes()).ok();
}

/// Writes a BPF trace into file.
fn write_bpf_trace(filename: &str, program_id: &str, header: &str, trace: &str) -> Result<()> {
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(filename)
        .map_err(|e| Error::new(e.kind(), format!("{}: '{}'", e, filename)))?;
    let timestamp = std::time::SystemTime::now();
    write!(
        file,
        "[{:?} TRACE solana_bpf_loader_program] BPF Program: {}",
        &timestamp, program_id
    )?;
    write!(
        file,
        "[{:?} TRACE solana_bpf_loader_program] {}\n{}",
        &timestamp, header, trace
    )
}
