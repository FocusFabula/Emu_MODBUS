use log::{Level, Log, Metadata, Record, SetLoggerError};
use std::fs::File;
use std::io::Write;
use std::sync::{Arc, Mutex};

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
pub struct LogEntry {
    pub level: Level,
    pub layer: LogLayer,
    pub message: String,
}

#[derive(Clone)]
pub struct GuiLogger {
    logs: Arc<Mutex<Vec<LogEntry>>>,
    max_lines: usize,
}

impl GuiLogger {
    pub fn new(max_lines: usize) -> Self {
        Self {
            logs: Arc::new(Mutex::new(Vec::new())),
            max_lines,
        }
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
    pub fn get_logs(&self) -> Vec<LogEntry> {
        let guard = self.logs.lock().unwrap();
        guard.clone()
    }
    pub fn save_to_file(&self, filepath: &str) -> std::io::Result<()> {
        let guard = self.logs.lock().unwrap();
        let mut file = File::create(filepath)?;

        for entry in guard.iter() {
            writeln!(
                file,
                "[{}] [{}] {}",
                entry.level,
                entry.layer.as_str(),
                entry.message
            )?;
        }
        Ok(())
    }
    pub fn init(self) -> Result<(), SetLoggerError> {
        log::set_max_level(log::LevelFilter::Info);
        log::set_boxed_logger(Box::new(self))
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

            let entry = LogEntry {
                level: record.level(),
                layer,
                message: clean_msg,
            };

            // Ładny, czytelny format na konsoli: [INFO] [SYS  ] Treść wiadomości
            println!(
                "[{:<5}] [{}] {}",
                entry.level,
                entry.layer.as_str(),
                entry.message
            );

            // Zapis do bufora dla GUI
            let mut guard = self.logs.lock().unwrap();
            if guard.len() >= self.max_lines {
                guard.remove(0);
            }
            guard.push(entry);
        }
    }

    fn flush(&self) {}
}
