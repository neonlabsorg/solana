//! mod bpf_trace.

use super::{BpfError, ThisInstructionMeter};
use lazy_static::lazy_static;
use log::{trace, warn};
use solana_rbpf::vm::{EbpfVm, Tracer};
use solana_sdk::pubkey::Pubkey;
use std::io::{self, BufWriter, Result, Write};
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::Mutex;
use std::thread;

const PORT: &str = "SOLANA_BPF_TRACE_CONTROL";
const SERVICE: &str = "BPF Trace Control Service";

/// Controls the trace if the environment variable exists.
pub fn control<'a>(
    header: &str,
    vm: &EbpfVm<'a, BpfError, ThisInstructionMeter>,
    program_id: &Pubkey,
) {
    let port = std::env::var(PORT).unwrap_or_default();
    if port.is_empty() {
        warn!("Variable '{}' does not exist", PORT);
        return;
    }

    if !service_started() {
        warn!("{} did not started", SERVICE);
        return;
    }

    let cfg = config();

    if !cfg.enable {
        trace!("BPF Trace is disabled");
        return;
    }

    if !cfg.passes_program(program_id) {
        trace!("BPF Trace is disabled for program {})", program_id);
        return;
    }

    if cfg.output.is_empty() {
        trace!("{}\n", header);
        let tracer = vm.get_tracer();
        let program = vm.get_program();
        write_disassembled(&mut io::stdout(), &tracer.log, program).unwrap();
        return;
    }

    let mut ok = vm.get_tracer().log.len() >= cfg.min_length;
    if ok {
        ok = if cfg.multiple_files {
            number_of_running_threads() < cfg.max_threads
        } else {
            number_of_running_threads() == 0
        };
    }

    trace!("BPF Program: {}", program_id);

    if !ok {
        warn!(
            "Skipped: trace.len={}, program size={} bytes",
            vm.get_tracer().log.len(),
            vm.get_program().len()
        );
        return;
    }

    // Move writing a trace file into a detached thread (shoot-and-forget)
    let filename = cfg.generate_filename();
    trace!(
        "{}: trace.len={}, program size={} bytes",
        &filename,
        vm.get_tracer().log.len(),
        vm.get_program().len()
    );
    let program_id = program_id.to_string();
    let tracer = vm.get_tracer().clone();
    let _ = if cfg.binary {
        thread::spawn(move || write_binary_trace(filename, program_id, tracer))
    } else {
        let header = header.to_string();
        let program = vm.get_program().to_vec();
        thread::spawn(move || write_formatted_trace(filename, program_id, header, tracer, program))
    };
}

lazy_static! {
    static ref THREADS: AtomicIsize = AtomicIsize::new(0);
}

fn number_of_running_threads() -> usize {
    let n = THREADS.load(Ordering::Relaxed);
    assert!(n >= 0);
    n as usize
}

fn increment() {
    let n = THREADS.fetch_add(1, Ordering::Relaxed);
    trace!("Threads: {}", n + 1);
    assert!(n >= 0);
}

fn decrement() {
    let n = THREADS.fetch_sub(1, Ordering::Relaxed);
    trace!("Threads: {}", n - 1);
    assert!((n - 1) >= 0);
}

const TITLE: &str = "TRACE solana_bpf_loader_program";
const FAIL: &str = "Failed to write BPF Trace to file";

/// Writes binary BPF trace into a file.
fn write_binary_trace(filename: String, _program_id: String, tracer: Tracer) {
    /// Transmutes slice of instructions to slice of u8.
    fn to_byte_slice(v: &[[u64; 12]]) -> &[u8] {
        unsafe {
            std::slice::from_raw_parts(
                v.as_ptr() as *const u8,
                v.len() * std::mem::size_of::<u64>() * 12,
            )
        }
    }

    increment();
    trace!(">Start thread for {}", &filename);

    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(filename.clone())
        .map_err(|e| warn!("{}: '{}'", e, filename));
    if file.is_err() {
        warn!("{} {}", FAIL, filename);
        decrement();
        return;
    }

    /* No need for buffered output here: we write one big chunk of data with single syscall
    let mut file = BufWriter::new(file.unwrap()); */
    let mut file = file.unwrap();

    let r = file
        .write_all(to_byte_slice(&tracer.log))
        .map_err(|e| warn!("{}: '{}'", e, filename));
    if r.is_err() {
        warn!("{} {}", FAIL, filename);
        decrement();
        return;
    }

    trace!("BPF Trace is written to file {}", filename);
    trace!("Finish thread for {}", &filename);
    decrement();
}

/// Writes disassembled BPF trace into a file.
fn write_formatted_trace(
    filename: String,
    program_id: String,
    header: String,
    tracer: Tracer,
    program: Vec<u8>,
) {
    increment();
    trace!(">Start thread for {}", &filename);

    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(filename.clone())
        .map_err(|e| warn!("{}: '{}'", e, filename));
    if file.is_err() {
        warn!("{} {}", FAIL, filename);
        decrement();
        return;
    }

    let mut file = BufWriter::new(file.unwrap());

    let timestamp = std::time::SystemTime::now();
    let r = write!(
        file,
        "[{:?} {}] BPF Program: {}\n",
        &timestamp, TITLE, program_id
    )
    .map_err(|e| warn!("{}: '{}'", e, filename));
    if r.is_err() {
        warn!("{} {}", FAIL, filename);
        decrement();
        return;
    }

    let r = write!(file, "[{:?} {}] {}\n", &timestamp, TITLE, header)
        .map_err(|e| warn!("{}: '{}'", e, filename));
    if r.is_err() {
        warn!("{} {}", FAIL, filename);
        decrement();
        return;
    }

    let r = write_disassembled(&mut file, &tracer.log, &program)
        .map_err(|e| warn!("{}: '{}'", e, filename));
    if r.is_err() {
        warn!("{} {}", FAIL, filename);
        decrement();
        return;
    }

    file.flush().ok();
    trace!("BPF Trace is written to file {}", filename);

    trace!("Finish thread for {}", &filename);
    decrement();
}

/// Disassembles and writes the program into a file.
/// No need to create a string buffer, when we just can write to a file.
/// See also: solana_rbpf::vm::Tracer::write.
fn write_disassembled<W: Write>(out: &mut W, log: &[[u64; 12]], program: &[u8]) -> Result<()> {
    use solana_rbpf::{disassembler, ebpf};

    let disassembled = disassembler::to_insn_vec(program);
    let mut pc_to_instruction_index =
        vec![0usize; disassembled.last().map(|ins| ins.ptr + 2).unwrap_or(0)];
    for index in 0..disassembled.len() {
        pc_to_instruction_index[disassembled[index].ptr] = index;
        pc_to_instruction_index[disassembled[index].ptr + 1] = index;
    }

    for index in 0..log.len() {
        let entry = log[index];
        writeln!(
            out,
            "{:5?} {:016X?} {:5?}: {}",
            index,
            &entry[0..11],
            entry[11] as usize + ebpf::ELF_INSN_DUMP_OFFSET,
            disassembled[pc_to_instruction_index[entry[11] as usize]].desc,
        )?;
    }

    Ok(())
}

const SHOW: &str = "show";
const ENABLE: &str = "enable";
const FILTER: &str = "filter";
const OUTPUT: &str = "output";
const BINARY: &str = "binary";
const MULTIPLE_FILES: &str = "multiple_files";
const MAX_THREADS: &str = "max_threads";
const MIN_LENGTH: &str = "min_length";

/// Represents parameters to control BPF tracing.
#[derive(Clone)]
struct BpfTraceConfig {
    enable: bool,
    filter: String,
    output: String,
    binary: bool,
    multiple_files: bool,
    max_threads: usize,
    min_length: usize,
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
        "{} = {}\n{} = {}\n{} = {}\n{} = {}\n{} = {}\n{} = {}\n{} = {}",
        ENABLE,
        cfg.enable,
        FILTER,
        cfg.filter,
        OUTPUT,
        cfg.output,
        BINARY,
        cfg.binary,
        MULTIPLE_FILES,
        cfg.multiple_files,
        MAX_THREADS,
        cfg.max_threads,
        MIN_LENGTH,
        cfg.min_length
    )
}

fn get_enable() -> String {
    format!("{} = {}", ENABLE, CONFIG.lock().unwrap().enable)
}

fn get_filter() -> String {
    format!("{} = {}", FILTER, CONFIG.lock().unwrap().filter)
}

fn get_output() -> String {
    format!("{} = {}", OUTPUT, CONFIG.lock().unwrap().output)
}

fn get_binary() -> String {
    format!("{} = {}", BINARY, CONFIG.lock().unwrap().binary)
}

fn get_multiple_files() -> String {
    format!(
        "{} = {}",
        MULTIPLE_FILES,
        CONFIG.lock().unwrap().multiple_files
    )
}

fn get_max_threads() -> String {
    format!("{} = {}", MAX_THREADS, CONFIG.lock().unwrap().max_threads)
}

fn get_min_length() -> String {
    format!("{} = {}", MIN_LENGTH, CONFIG.lock().unwrap().min_length)
}

fn set_enable(value: bool) -> String {
    CONFIG.lock().unwrap().enable = value;
    format!("{} = {}", ENABLE, value)
}

fn set_filter(value: &str) -> String {
    CONFIG.lock().unwrap().filter = value.into();
    format!("{} = {}", FILTER, value)
}

fn set_output(value: &str) -> String {
    CONFIG.lock().unwrap().output = value.into();
    format!("{} = {}", OUTPUT, value)
}

fn set_binary(value: bool) -> String {
    CONFIG.lock().unwrap().binary = value;
    format!("{} = {}", BINARY, value)
}

fn set_multiple_files(value: bool) -> String {
    CONFIG.lock().unwrap().multiple_files = value;
    format!("{} = {}", MULTIPLE_FILES, value)
}

fn set_max_threads(value: usize) -> String {
    CONFIG.lock().unwrap().max_threads = value;
    format!("{} = {}", MAX_THREADS, value)
}

fn set_min_length(value: usize) -> String {
    CONFIG.lock().unwrap().min_length = value;
    format!("{} = {}", MIN_LENGTH, value)
}

const DEFAULT_MAX_THREADS: usize = 2;
const DEFAULT_MIN_LENGTH: usize = 1_000_000;

impl BpfTraceConfig {
    /// Creates config with reasonable initial state.
    fn new() -> Self {
        BpfTraceConfig {
            enable: true,
            filter: String::default(),
            output: "/tmp/trace".into(),
            binary: false,
            multiple_files: true,
            max_threads: DEFAULT_MAX_THREADS,
            min_length: DEFAULT_MIN_LENGTH,
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
        let mut name = self.output.clone();
        if name.is_empty() {
            return name;
        }
        if !self.filter.is_empty() {
            let sep = self.filter.find(":").unwrap_or_default();
            if sep != usize::default() {
                name += "_";
                name += &self.filter[0..sep];
            }
        }
        if self.multiple_files {
            let now = std::time::SystemTime::now();
            let epoch = now.duration_since(std::time::SystemTime::UNIX_EPOCH);
            if let Ok(epoch) = epoch {
                name += "_";
                name += &epoch.as_nanos().to_string();
            }
        }
        name
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
            let _ = thread::spawn(move || listen(listener.unwrap()));
            TcpServer { is_running: true }
        }
    }
}

/// Starts listening incoming connections in a separate thread.
fn listen(listener: TcpListener) {
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => handle_connection(stream),
            Err(err) => {
                warn!("{}: incoming connection: {}", SERVICE, err);
            }
        }
    }
}

/// Accepts a control input and handles it.
fn handle_connection(mut stream: TcpStream) {
    use io::Read;
    let mut buf = [0; 256];
    let bytes_read = stream.read(&mut buf).map_err(|e| warn!("{}", e));
    if bytes_read.is_ok() {
        let mut command = String::from_utf8_lossy(&buf[0..bytes_read.unwrap()]).to_string();
        command.retain(|c| !c.is_whitespace());
        let resp = dispatch_command(&command);
        stream.write_all(resp.as_bytes()).ok();
    }
}

/// Executes a command and returns corresponding response.
fn dispatch_command(command: &str) -> String {
    match command {
        SHOW => return config_to_string(),
        ENABLE => return get_enable(),
        FILTER => return get_filter(),
        OUTPUT => return get_output(),
        BINARY => return get_binary(),
        MULTIPLE_FILES => return get_multiple_files(),
        MAX_THREADS => return get_max_threads(),
        MIN_LENGTH => return get_min_length(),
        _ => (), // set-command, falling through
    }

    let sep = command.find("=").unwrap_or_default();
    if sep == usize::default() {
        let resp = format!(
            "Invalid format of BPF trace control parameter '{}'",
            &command
        );
        warn!("{}", &resp);
        return resp;
    }

    let key = &command[0..sep];
    let value = &command[sep + 1..];

    let resp = match key {
        ENABLE => set_enable(value != "false" && value != "0"),
        FILTER => set_filter(value),
        OUTPUT => set_output(value),
        BINARY => set_binary(value != "false" && value != "0"),
        MULTIPLE_FILES => set_multiple_files(value != "false" && value != "0"),
        MAX_THREADS => set_max_threads(value.parse::<usize>().unwrap_or(DEFAULT_MAX_THREADS)),
        MIN_LENGTH => set_min_length(value.parse::<usize>().unwrap_or(DEFAULT_MIN_LENGTH)),
        _ => {
            let msg = format!("Unsupported BPF trace control parameter '{}'", &command);
            warn!("{}", &msg);
            msg
        }
    };
    resp
}
