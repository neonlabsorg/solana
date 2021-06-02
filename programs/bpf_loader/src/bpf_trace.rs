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
        write(&mut io::stdout(), &tracer.log, program).unwrap();
        return;
    }

    let mut ok = vm.get_program().len() >= cfg.min_program;
    if ok {
        ok = if cfg.multiple_files {
            number_of_running_threads() < cfg.max_threads
        } else {
            number_of_running_threads() == 0
        };
    }

    if ok {
        // Move writing a trace file into a detached thread (shoot-and-forget)
        trace!("BPF Program: {}", program_id);
        let filename = cfg.generate_filename();
        let program_id = program_id.to_string();
        let header = header.to_string();
        let tracer = vm.get_tracer().clone();
        let program = vm.get_program().to_vec();
        trace!(
            "{}: tracer.len={}, program size={} bytes",
            &filename,
            tracer.log.len(),
            program.len()
        );
        let _ =
            thread::spawn(move || write_bpf_trace(filename, program_id, header, tracer, program));
    }
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
    trace!("Threads: {}", n);
    assert!(n >= 0);
}

fn decrement() {
    let n = THREADS.fetch_sub(1, Ordering::Relaxed);
    trace!("Threads: {}", n);
    assert!(n >= 0);
}

/// Writes a BPF trace into a file.
fn write_bpf_trace(
    filename: String,
    program_id: String,
    header: String,
    tracer: Tracer,
    program: Vec<u8>,
) {
    increment();
    trace!(">Start thread for {}", &filename);

    const TITLE: &str = "TRACE solana_bpf_loader_program";
    const FAIL: &str = "Failed to write BPF Trace to file";

    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(filename.clone())
        .map_err(|e| warn!("{}: '{}'", e, filename));
    if file.is_err() {
        warn!("{} {}", FAIL, filename);
        trace!("Finish thread for {}", &filename);
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
        trace!("Finish thread for {}", &filename);
        decrement();
        return;
    }

    let r = write!(file, "[{:?} {}] {}\n", &timestamp, TITLE, header)
        .map_err(|e| warn!("{}: '{}'", e, filename));
    if r.is_err() {
        warn!("{} {}", FAIL, filename);
        trace!("Finish thread for {}", &filename);
        decrement();
        return;
    }

    let r = write(&mut file, &tracer.log, &program).map_err(|e| warn!("{}: '{}'", e, filename));
    if r.is_err() {
        warn!("{} {}", FAIL, filename);
        trace!("Finish thread for {}", &filename);
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
fn write<W: Write>(out: &mut W, log: &[[u64; 12]], program: &[u8]) -> Result<()> {
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

/// Represents parameters to control BPF tracing.
#[derive(Clone)]
struct BpfTraceConfig {
    enable: bool,
    filter: String,
    output: String,
    multiple_files: bool,
    max_threads: usize,
    min_program: usize,
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
        "enable = {}\nfilter = {}\noutput = {}\nmultiple_files = {}\nmax_threads = {}\nmin_program = {}",
        cfg.enable, cfg.filter, cfg.output, cfg.multiple_files, cfg.max_threads, cfg.min_program
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

fn get_multiple_files() -> String {
    format!("multiple_files = {}", CONFIG.lock().unwrap().multiple_files)
}

fn get_max_threads() -> String {
    format!("max_threads = {}", CONFIG.lock().unwrap().max_threads)
}

fn get_min_program() -> String {
    format!("min_program = {}", CONFIG.lock().unwrap().min_program)
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

fn set_multiple_files(value: bool) -> String {
    CONFIG.lock().unwrap().multiple_files = value;
    format!("multiple_files = {}", value)
}

fn set_max_threads(value: usize) -> String {
    CONFIG.lock().unwrap().max_threads = value;
    format!("max_threads = {}", value)
}

fn set_min_program(value: usize) -> String {
    CONFIG.lock().unwrap().min_program = value;
    format!("min_program = {}", value)
}

const DEFAULT_MAX_THREADS: usize = 2;
const DEFAULT_MIN_PROGRAM: usize = 1_000_000;

impl BpfTraceConfig {
    /// Creates config with reasonable initial state.
    fn new() -> Self {
        BpfTraceConfig {
            enable: true,
            filter: String::default(),
            output: "/tmp/trace".into(),
            multiple_files: true,
            max_threads: DEFAULT_MAX_THREADS,
            min_program: DEFAULT_MIN_PROGRAM,
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
    } else if command == "multiple_files" {
        stream.write_all(get_multiple_files().as_bytes()).ok();
        return;
    } else if command == "max_threads" {
        stream.write_all(get_max_threads().as_bytes()).ok();
        return;
    } else if command == "min_program" {
        stream.write_all(get_min_program().as_bytes()).ok();
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
        "multiple_files" => set_multiple_files(value != "false" && value != "0"),
        "max_threads" => set_max_threads(value.parse::<usize>().unwrap_or(DEFAULT_MAX_THREADS)),
        "min_program" => set_min_program(value.parse::<usize>().unwrap_or(DEFAULT_MIN_PROGRAM)),
        _ => {
            let msg = format!("Unsupported BPF trace control parameter '{}'", &command);
            warn!("{}", &msg);
            msg
        }
    };

    stream.write_all(resp.as_bytes()).ok();
}
