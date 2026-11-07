use core_modbus::{ModbusDevice, ModbusException, ModbusRequest, ReqPdu, execute_request};
use serialport::{DataBits, Parity, SerialPort, StopBits};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

use crate::logger::{GuiLogger, LogLayer};
use crate::sim_device::DynamicMemory;
use log::Level;

fn format_hex(data: &[u8]) -> String {
    let hex_strings: Vec<String> = data.iter().map(|b| format!("{:02X}", b)).collect();
    hex_strings.join(" ")
}

pub struct TcpServer {
    address: String,
}

impl TcpServer {
    pub fn new(address: &str) -> Self {
        Self {
            address: address.to_string(),
        }
    }

    pub fn run<F>(&self, mut handler: F)
    where
        F: FnMut(&[u8], &mut [u8]) -> usize,
    {
        let listener = TcpListener::bind(&self.address).expect("Could not bind to port");

        GuiLogger::log_to(
            LogLayer::System,
            Level::Info,
            &format!("Serwer TCP uruchomiony i nasłuchuje na {}", self.address),
        );

        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    let peer = stream
                        .peer_addr()
                        .map(|a| a.to_string())
                        .unwrap_or_else(|_| "Nieznany".to_string());
                    GuiLogger::log_to(
                        LogLayer::Transport,
                        Level::Info,
                        &format!("Zaakceptowano nowe połączenie TCP od: {}", peer),
                    );
                    self.handle_client(stream, &mut handler, &peer);
                }
                Err(err) => GuiLogger::log_to(
                    LogLayer::Transport,
                    Level::Error,
                    &format!("Błąd przy nawiązywaniu połączenia TCP: {}", err),
                ),
            }
        }
    }

    fn handle_client<F>(&self, mut stream: TcpStream, handler: &mut F, peer: &str)
    where
        F: FnMut(&[u8], &mut [u8]) -> usize,
    {
        let mut rx_buffer = [0u8; 1024];
        let mut tx_buffer = [0u8; 1024];

        loop {
            match stream.read(&mut rx_buffer) {
                Ok(0) => {
                    GuiLogger::log_to(
                        LogLayer::Transport,
                        Level::Info,
                        &format!("Klient {} zamknął połączenie.", peer),
                    );
                    break;
                }
                Ok(bytes_read) => {
                    let rx_data = &rx_buffer[..bytes_read];
                    GuiLogger::log_to(
                        LogLayer::Transport,
                        Level::Info, // Zwykle ukryte, włączane przy głębokim debugowaniu
                        &format!("TCP RX ({}): [{}]", peer, format_hex(rx_data)),
                    );

                    let res_len = handler(rx_data, &mut tx_buffer);

                    if res_len > 0 {
                        let tx_data = &tx_buffer[..res_len];
                        if let Err(err) = stream.write_all(tx_data) {
                            GuiLogger::log_to(
                                LogLayer::Transport,
                                Level::Error,
                                &format!("Błąd wysyłania odpowiedzi TCP do {}: {}", peer, err),
                            );
                            break;
                        } else {
                            GuiLogger::log_to(
                                LogLayer::Transport,
                                Level::Info,
                                &format!("TCP TX ({}): [{}]", peer, format_hex(tx_data)),
                            );
                        }
                    }
                }
                Err(err) => {
                    GuiLogger::log_to(
                        LogLayer::Transport,
                        Level::Error,
                        &format!("Błąd odczytu z gniazda TCP ({}): {}", peer, err),
                    );
                    break;
                }
            }
        }
    }
}

pub struct ModbusTcpAdu;

impl ModbusTcpAdu {
    pub fn process(rx_buffer: &[u8], tx_buffer: &mut [u8], router: &mut ModbusRouter) -> usize {
        if rx_buffer.len() < 8 {
            return 0;
        }

        let tid_hi = rx_buffer[0];
        let tid_lo = rx_buffer[1];
        let pid_hi = rx_buffer[2];
        let pid_lo = rx_buffer[3];
        let len_hi = rx_buffer[4];
        let len_lo = rx_buffer[5];
        let uid = rx_buffer[6];

        let device = match router.get_device_mut(uid) {
            Some(dev) => dev,
            None => {
                GuiLogger::log_to(
                    LogLayer::Protocol,
                    Level::Warn,
                    &format!(
                        "TCP: Zignorowano ramkę. Nieznane urządzenie (Unit ID: {})",
                        uid
                    ),
                );
                return 0;
            }
        };

        if pid_hi != 0 || pid_lo != 0 {
            return 0;
        }

        let expected_pdu_len = (u16::from_be_bytes([len_hi, len_lo]) - 1) as usize;

        if rx_buffer.len() - 7 < expected_pdu_len {
            return 0;
        }

        let pdu_data = &rx_buffer[7..7 + expected_pdu_len];

        if let Ok(req_pdu) = ReqPdu::parse(pdu_data) {
            if let Ok(modbus_req) = ModbusRequest::from_pdu(&req_pdu) {
                GuiLogger::log_to(
                    LogLayer::Protocol,
                    Level::Info,
                    &format!("TCP (UID: {}): Przetwarzanie zapytania...", uid),
                );

                let pdu_res_len = execute_request(&modbus_req, &mut tx_buffer[7..], device);

                tx_buffer[0] = tid_hi;
                tx_buffer[1] = tid_lo;
                tx_buffer[2] = 0x00;
                tx_buffer[3] = 0x00;

                let new_mbap_len = (pdu_res_len + 1) as u16;
                let len_bytes = new_mbap_len.to_be_bytes();
                tx_buffer[4] = len_bytes[0];
                tx_buffer[5] = len_bytes[1];
                tx_buffer[6] = uid;

                return 7 + pdu_res_len;
            }
        }
        0
    }
}

pub struct ModbusRtuAdu;

impl ModbusRtuAdu {
    pub fn process(rx_buffer: &[u8], tx_buffer: &mut [u8], router: &mut ModbusRouter) -> usize {
        if rx_buffer.len() < 4 {
            return 0;
        }

        let slave_id = rx_buffer[0];

        let device = match router.get_device_mut(slave_id) {
            Some(dev) => dev,
            None => {
                GuiLogger::log_to(
                    LogLayer::Protocol,
                    Level::Warn,
                    &format!(
                        "RTU: Zignorowano ramkę dla nieznanego Slave ID: {}",
                        slave_id
                    ),
                );
                return 0;
            }
        };

        let calc_crc = Self::crc16(&rx_buffer[..rx_buffer.len() - 2]);
        let recv_crc = u16::from_le_bytes([
            rx_buffer[rx_buffer.len() - 2],
            rx_buffer[rx_buffer.len() - 1],
        ]);

        if calc_crc != recv_crc {
            GuiLogger::log_to(
                LogLayer::Transport,
                Level::Error,
                &format!(
                    "RTU: Odrzucono ramkę - błędne CRC! (Oczekiwano: {:04X}, Otrzymano: {:04X})",
                    calc_crc, recv_crc
                ),
            );
            return 0;
        }

        let pdu_data = &rx_buffer[1..rx_buffer.len() - 2];
        tx_buffer[0] = slave_id;

        if let Ok(req_pdu) = ReqPdu::parse(pdu_data) {
            if let Ok(modbus_req) = ModbusRequest::from_pdu(&req_pdu) {
                GuiLogger::log_to(
                    LogLayer::Protocol,
                    Level::Info,
                    &format!("RTU (ID: {}): Przetwarzanie poprawnej ramki.", slave_id),
                );

                let pdu_res_len = execute_request(&modbus_req, &mut tx_buffer[1..], device);
                let total_len_no_crc = pdu_res_len + 1;
                let tx_crc = Self::crc16(&tx_buffer[..total_len_no_crc]);

                let crc_bytes = tx_crc.to_le_bytes();
                tx_buffer[total_len_no_crc] = crc_bytes[0];
                tx_buffer[total_len_no_crc + 1] = crc_bytes[1];

                return total_len_no_crc + 2;
            }
        }
        0
    }

    fn crc16(data: &[u8]) -> u16 {
        let mut crc = 0xFFFFu16;
        for &byte in data {
            crc ^= byte as u16;
            for _ in 0..8 {
                if crc & 1 != 0 {
                    crc >>= 1;
                    crc ^= 0xA001;
                } else {
                    crc >>= 1;
                }
            }
        }
        crc
    }
}

pub struct SerialServer {
    port_name: String,
    baud_rate: u32,
    data_bits: DataBits,
    parity: Parity,
}

impl SerialServer {
    pub fn new(port_name: &str, baud_rate: u32, data_bits: DataBits, parity: Parity) -> Self {
        Self {
            port_name: port_name.to_string(),
            baud_rate,
            data_bits,
            parity,
        }
    }

    pub fn run<F>(&self, mut handler: F)
    where
        F: FnMut(&[u8], &mut [u8]) -> usize,
    {
        let port_result = serialport::new(&self.port_name, self.baud_rate)
            .data_bits(self.data_bits)
            .parity(self.parity)
            .stop_bits(StopBits::One)
            .timeout(Duration::from_millis(50))
            .open();

        let mut port = match port_result {
            Ok(p) => p,
            Err(e) => {
                GuiLogger::log_to(
                    LogLayer::System,
                    Level::Error,
                    &format!("Nie udało się otworzyć portu {}: {}", self.port_name, e),
                );
                return; // Przerwij, zamiast robić panic!
            }
        };

        GuiLogger::log_to(
            LogLayer::System,
            Level::Info,
            &format!(
                "Serwer Serial otwarty pomyślnie: {} ({} bps)",
                self.port_name, self.baud_rate
            ),
        );

        let mut rx_buffer = [0u8; 256];
        let mut tx_buffer = [0u8; 256];
        let mut rx_len = 0;

        loop {
            match port.read(&mut rx_buffer[rx_len..]) {
                Ok(bytes_read) => rx_len += bytes_read,
                Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => {
                    if rx_len > 0 {
                        GuiLogger::log_to(
                            LogLayer::Transport,
                            Level::Info,
                            &format!("Serial RX: [{}]", format_hex(&rx_buffer[..rx_len])),
                        );

                        let response_len = handler(&rx_buffer[..rx_len], &mut tx_buffer);

                        if response_len > 0 {
                            let tx_data = &tx_buffer[..response_len];
                            if let Err(err) = port.write_all(tx_data) {
                                GuiLogger::log_to(
                                    LogLayer::Transport,
                                    Level::Error,
                                    &format!("Błąd wysyłania do portu COM: {}", err),
                                );
                            } else {
                                GuiLogger::log_to(
                                    LogLayer::Transport,
                                    Level::Info,
                                    &format!("Serial TX: [{}]", format_hex(tx_data)),
                                );
                            }
                        }
                        rx_len = 0;
                    }
                }
                Err(e) => {
                    GuiLogger::log_to(
                        LogLayer::Transport,
                        Level::Error,
                        &format!("Krytyczny błąd sprzętowy portu COM: {}", e),
                    );
                    break;
                }
            }
        }
    }
}

pub struct ModbusAsciiAdu;

impl ModbusAsciiAdu {
    pub fn process(rx_buffer: &[u8], tx_buffer: &mut [u8], router: &mut ModbusRouter) -> usize {
        if rx_buffer.len() < 9 || rx_buffer[0] != b':' {
            return 0;
        }

        let end_idx = rx_buffer.iter().position(|&b| b == b'\r').unwrap_or(0);
        if end_idx < 7 || rx_buffer.get(end_idx + 1) != Some(&b'\n') {
            return 0;
        }

        let mut bin_rx = [0u8; 256];
        let mut bin_len = 0;
        let hex_payload = &rx_buffer[1..end_idx];

        for chunk in hex_payload.chunks_exact(2) {
            if let (Some(hi), Some(lo)) =
                (Self::decode_nibble(chunk[0]), Self::decode_nibble(chunk[1]))
            {
                bin_rx[bin_len] = (hi << 4) | lo;
                bin_len += 1;
            } else {
                GuiLogger::log_to(
                    LogLayer::Transport,
                    Level::Warn,
                    "ASCII: Odebrano błędny znak (non-hex) w ramce",
                );
                return 0;
            }
        }

        let received_lrc = bin_rx[bin_len - 1];
        let calculated_lrc = Self::calculate_lrc(&bin_rx[..bin_len - 1]);

        if received_lrc != calculated_lrc {
            GuiLogger::log_to(
                LogLayer::Transport,
                Level::Error,
                &format!(
                    "ASCII: Błędne LRC! (Oczekiwano: {:02X}, Otrzymano: {:02X})",
                    calculated_lrc, received_lrc
                ),
            );
            return 0;
        }

        let slave_addr = bin_rx[0];
        let device = match router.get_device_mut(slave_addr) {
            Some(addr) => addr,
            None => {
                GuiLogger::log_to(
                    LogLayer::Protocol,
                    Level::Warn,
                    &format!(
                        "ASCII: Zignorowano ramkę dla nieznanego Slave ID: {}",
                        slave_addr
                    ),
                );
                return 0;
            }
        };

        let pdu_data = &bin_rx[1..bin_len - 1];
        let mut bin_tx = [0u8; 256];
        bin_tx[0] = slave_addr;

        if let Ok(req_pdu) = ReqPdu::parse(pdu_data) {
            if let Ok(modbus_req) = ModbusRequest::from_pdu(&req_pdu) {
                GuiLogger::log_to(
                    LogLayer::Protocol,
                    Level::Info,
                    &format!("ASCII (ID: {}): Przetwarzanie poprawnej ramki.", slave_addr),
                );

                let pdu_res_len = execute_request(&modbus_req, &mut bin_tx[1..], device);
                let total_res_bin_len = pdu_res_len + 1;

                tx_buffer[0] = b':';
                let mut tx_idx = 1;

                for &byte in &bin_tx[..total_res_bin_len] {
                    let (hi, lo) = Self::encode_byte(byte);
                    tx_buffer[tx_idx] = hi;
                    tx_buffer[tx_idx + 1] = lo;
                    tx_idx += 2;
                }

                let tx_lrc = Self::calculate_lrc(&bin_tx[..total_res_bin_len]);
                let (lrc_hi, lrc_lo) = Self::encode_byte(tx_lrc);
                tx_buffer[tx_idx] = lrc_hi;
                tx_buffer[tx_idx + 1] = lrc_lo;
                tx_idx += 2;

                tx_buffer[tx_idx] = b'\r';
                tx_buffer[tx_idx + 1] = b'\n';
                tx_idx += 2;

                return tx_idx;
            }
        }
        0
    }

    // --- Funkcje pomocnicze do konwersji i LRC ---

    fn decode_nibble(c: u8) -> Option<u8> {
        match c {
            b'0'..=b'9' => Some(c - b'0'),
            b'A'..=b'F' => Some(c - b'A' + 10),
            b'a'..=b'f' => Some(c - b'a' + 10),
            _ => None,
        }
    }

    fn encode_byte(b: u8) -> (u8, u8) {
        let encode_nibble = |n: u8| -> u8 { if n < 10 { b'0' + n } else { b'A' + (n - 10) } };
        (encode_nibble(b >> 4), encode_nibble(b & 0x0F))
    }

    fn calculate_lrc(data: &[u8]) -> u8 {
        let mut sum = 0u8;
        for &byte in data {
            sum = sum.wrapping_add(byte);
        }
        sum.wrapping_neg() // Odpowiednik uzupełnienia do 2
    }
}

pub struct ModbusRouter {
    devices: HashMap<u8, DynamicMemory>,
}

impl ModbusRouter {
    pub fn new() -> Self {
        Self {
            devices: HashMap::new(),
        }
    }

    pub fn add_device(&mut self, slave_id: u8, device: DynamicMemory) {
        self.devices.insert(slave_id, device);
    }

    pub fn get_device_mut(&mut self, slave_id: u8) -> Option<&mut DynamicMemory> {
        self.devices.get_mut(&slave_id)
    }
}
