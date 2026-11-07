use log::{Level, Log, Metadata, Record};
use std::sync::mpsc;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LogLayer {
    Transport,
    Protocol,
    Memory,
    System,
}

impl LogLayer {
    fn as_str(&self) -> &'static str {
        match self {
            LogLayer::Transport => "TRANS",
            LogLayer::Protocol => "PROTO",
            LogLayer::Memory => "MEM",
            LogLayer::System => "SYS",
        }
    }
}

#[derive(Clone)]
pub struct GuiLogger {
    sender: mpsc::Sender<String>,
}

impl GuiLogger {
    pub fn init(sender: mpsc::Sender<String>) {
        let logger = Self { sender };
        log::set_max_level(log::LevelFilter::Info);
        let _ = log::set_boxed_logger(Box::new(logger));
    }

    pub fn log_to(layer: LogLayer, level: Level, msg: &str) {
        match level {
            Level::Error => log::error!("[{}] {}", layer.as_str(), msg),
            Level::Warn => log::warn!("[{}] {}", layer.as_str(), msg),
            Level::Info => log::info!("[{}] {}", layer.as_str(), msg),
            Level::Debug => log::debug!("[{}] {}", layer.as_str(), msg),
            Level::Trace => log::trace!("[{}] {}", layer.as_str(), msg),
        }
    }
}

impl Log for GuiLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= log::Level::Info
    }
    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            let msg = record.args().to_string();

            // Odczytujemy warstwę bezpośrednio z makra log (przekazaną w formacie [TAG])
            // Przykład: log::info!("[TRANS] Błąd") -> wyciąga Trans
            let (layer, clean_msg) = if msg.starts_with("[TRANS]") {
                (LogLayer::Transport, msg.replacen("[TRANS] ", "", 1))
            } else if msg.starts_with("[PROTO]") {
                (LogLayer::Protocol, msg.replacen("[PROTO] ", "", 1))
            } else if msg.starts_with("[MEM]") {
                (LogLayer::Memory, msg.replacen("[MEM] ", "", 1))
            } else if msg.starts_with("[SYS]") {
                (
                    LogLayer::System,
                    msg.replacen("[SYS]", "", 1).trim().to_string(),
                )
            } else {
                (LogLayer::System, msg) // Domyślnie System, bez wycinania
            };

            let msg = format!("[{}] [{}] {}", record.level(), layer.as_str(), clean_msg);
            println!("{}", msg);
            let _ = self.sender.send(msg.to_string());
        }
    }

    fn flush(&self) {}
}

pub struct GuiLoggerBuffer {
    logs: Vec<String>,
    max_lines: usize,
}

impl GuiLoggerBuffer {
    pub fn new(max_lines: usize) -> Self {
        Self {
            logs: Vec::new(),
            max_lines,
        }
    }

    pub fn get_logs(&self) -> &Vec<String> {
        &self.logs
    }

    pub fn push(&mut self, message: String) {
        if self.logs.len() > self.max_lines {
            self.logs.push(message);
            self.logs.remove(0);
        } else {
            self.logs.push(message);
        }
    }
}
