use core_modbus::{ModbusDevice, ModbusRequest, ReqPdu, execute_request};
use serialport::{DataBits, Parity, SerialPort, StopBits};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

use crate::sim_device::DynamicMemory;
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
        println!("Server started on IP: {}", self.address);

        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    println!("Zaakceptowano połączenie: {:?}", stream.peer_addr());
                    self.handle_client(stream, &mut handler);
                }
                Err(err) => println!("Could not establish connection {}", err),
            }
        }
    }

    fn handle_client<F>(&self, mut stream: TcpStream, handler: &mut F)
    where
        F: FnMut(&[u8], &mut [u8]) -> usize,
    {
        let mut rx_buffer = [0u8; 1024];
        let mut tx_buffer = [0u8; 1024];

        loop {
            match stream.read(&mut rx_buffer) {
                Ok(0) => {
                    println!("Client closed a connection");
                    break;
                }
                Ok(bytes_read) => {
                    let res_len = handler(&rx_buffer[..bytes_read], &mut tx_buffer);
                    if res_len > 0 {
                        if let Err(err) = stream.write_all(&tx_buffer[..res_len]) {
                            println!("Sending error {}", err);
                            break;
                        }
                    }
                }
                Err(err) => {
                    println!("error while reading from socket: {}", err);
                    break;
                }
            }
        }
    }
}

pub struct ModbusTcpAdu;

impl ModbusTcpAdu {
    /// Funkcja parsująca ADU, wywołująca rdzeń i pakująca odpowiedź.
    pub fn process(
        rx_buffer: &[u8],
        tx_buffer: &mut [u8],
        device: &mut impl ModbusDevice,
    ) -> usize {
        // 1. Walidacja absolutnego minimum (7 bajtów MBAP + min. 1 bajt kodu funkcji)
        if rx_buffer.len() < 8 {
            return 0; // Zignoruj śmieci sieciowe
        }

        // 2. Rozpakowanie nagłówka MBAP
        let tid_hi = rx_buffer[0];
        let tid_lo = rx_buffer[1];
        let pid_hi = rx_buffer[2];
        let pid_lo = rx_buffer[3];
        let len_hi = rx_buffer[4];
        let len_lo = rx_buffer[5];
        let uid = rx_buffer[6];

        // Protokół Modbus TCP zawsze ma PID równe 0x0000
        if pid_hi != 0 || pid_lo != 0 {
            return 0;
        }

        // Oczekiwana długość PDU (Pole Długość z MBAP minus 1 bajt na pole UID)
        let expected_pdu_len = (u16::from_be_bytes([len_hi, len_lo]) - 1) as usize;

        // Czy serwer TCP wczytał całą ramkę?
        if rx_buffer.len() - 7 < expected_pdu_len {
            return 0; // Fragmentacja TCP - w pełnym symulatorze tu nastąpiłoby buforowanie
        }

        // Wycinamy czyste PDU dla rdzenia
        let pdu_data = &rx_buffer[7..7 + expected_pdu_len];

        // 3. Przekazanie do Warstwy 3 (Rdzeń)
        if let Ok(req_pdu) = ReqPdu::parse(pdu_data) {
            if let Ok(modbus_req) = ModbusRequest::from_pdu(&req_pdu) {
                // Magia Rusta! Przekazujemy bufor nadawczy przesunięty o 7 bajtów.
                // Rdzeń myśli, że pisze od zera, a tak naprawdę zostawia miejsce na MBAP.
                let pdu_res_len = execute_request(&modbus_req, &mut tx_buffer[7..], device);

                // 4. Budowa nagłówka MBAP odpowiedzi
                tx_buffer[0] = tid_hi; // Kopiujemy TID
                tx_buffer[1] = tid_lo;
                tx_buffer[2] = 0x00; // PID
                tx_buffer[3] = 0x00;

                // Nowa długość w MBAP to: długość_PDU_odpowiedzi + 1 bajt UID
                let new_mbap_len = (pdu_res_len + 1) as u16;
                let len_bytes = new_mbap_len.to_be_bytes();
                tx_buffer[4] = len_bytes[0];
                tx_buffer[5] = len_bytes[1];

                tx_buffer[6] = uid; // Kopiujemy adres urządzenia

                // Zwracamy całkowitą długość ramki TCP (MBAP + PDU)
                return 7 + pdu_res_len;
            }
        }

        0 // Jeśli coś poszło nie tak (zły kod funkcji), po prostu zrywamy bez odpowiedzi
    }
}

pub struct ModbusRtuAdu;

impl ModbusRtuAdu {
    pub fn process(rx_buffer: &[u8], tx_buffer: &mut [u8], router: &mut ModbusRouter) -> usize {
        // 1. Walidacja absolutnego minimum (Adres(1) + Funkcja(1) + CRC(2))
        if rx_buffer.len() < 4 {
            return 0;
        }

        let slave_id = rx_buffer[0];

        let device = match router.get_device_mut(slave_id) {
            Some(dev) => dev,
            None => {
                println!("Router: Ignored frame for unknown Slave id: {}", slave_id);
                return 0;
            }
        };
        // 2. Walidacja sumy CRC (bierzemy całą ramkę bez dwóch ostatnich bajtów)
        let calc_crc = Self::crc16(&rx_buffer[..rx_buffer.len() - 2]);

        // Modbus RTU wysyła CRC w formacie Little-Endian (Low Byte, High Byte)
        let recv_crc = u16::from_le_bytes([
            rx_buffer[rx_buffer.len() - 2],
            rx_buffer[rx_buffer.len() - 1],
        ]);

        if calc_crc != recv_crc {
            println!("Odrzucono ramkę: błędne CRC!");
            return 0; // Szum na linii RS-485, odrzucamy ramkę
        }

        // 3. Wycinamy PDU dla naszego rdzenia
        let pdu_data = &rx_buffer[1..rx_buffer.len() - 2];
        tx_buffer[0] = slave_id; // Zostawiamy pierwszy bajt wolny na echo adresu

        if let Ok(req_pdu) = ReqPdu::parse(pdu_data) {
            if let Ok(modbus_req) = ModbusRequest::from_pdu(&req_pdu) {
                // Wywołanie rdzenia! (znowu dajemy przesunięty bufor)
                let pdu_res_len = execute_request(&modbus_req, &mut tx_buffer[1..], device);

                // Długość odpowiedzi bez CRC (adres + PDU)
                let total_len_no_crc = pdu_res_len + 1;

                // 4. Obliczamy CRC z gotowej odpowiedzi
                let tx_crc = Self::crc16(&tx_buffer[..total_len_no_crc]);

                // Pakujemy wyliczone CRC (Little-Endian) na koniec ramki
                let crc_bytes = tx_crc.to_le_bytes();
                tx_buffer[total_len_no_crc] = crc_bytes[0];
                tx_buffer[total_len_no_crc + 1] = crc_bytes[1];

                return total_len_no_crc + 2; // Całkowita długość
            }
        }

        0
    }

    /// Funkcja licząca wielomian Modbus CRC-16 (0xA001)
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
    // Konstruktor, w którym ustalamy parametry transmisji
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
        // Konfigurujemy port z odpowiednią ilością bitów i parzystością
        let mut port = serialport::new(&self.port_name, self.baud_rate)
            .data_bits(self.data_bits)
            .parity(self.parity)
            .stop_bits(StopBits::One)
            .timeout(Duration::from_millis(50)) // Timeout nadal działa świetnie jako wyzwalacz!
            .open()
            .expect("Nie udało się otworzyć portu COM!");

        println!(
            "Warstwa 1: Serwer Serial nasłuchuje na {} ({} bps)",
            self.port_name, self.baud_rate
        );

        // ... (reszta pętli czytającej rx_buffer zostaje DOKŁADNIE taka sama jak dla RTU)
        let mut rx_buffer = [0u8; 256];
        let mut tx_buffer = [0u8; 256];
        let mut rx_len = 0;

        loop {
            match port.read(&mut rx_buffer[rx_len..]) {
                Ok(bytes_read) => rx_len += bytes_read,
                Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => {
                    if rx_len > 0 {
                        let response_len = handler(&rx_buffer[..rx_len], &mut tx_buffer);
                        if response_len > 0 {
                            if let Err(err) = port.write_all(&tx_buffer[..response_len]) {
                                println!("Błąd wysyłania do portu COM: {}", err);
                            }
                        }
                        rx_len = 0;
                    }
                }
                Err(e) => {
                    println!("Błąd odczytu sprzętowego: {}", e);
                    break;
                }
            }
        }
    }
}

pub struct ModbusAsciiAdu;

impl ModbusAsciiAdu {
    pub fn process(rx_buffer: &[u8], tx_buffer: &mut [u8], router: &mut ModbusRouter) -> usize {
        // 1. Walidacja minimum: Start ':', Adres(2), Funkcja(2), LRC(2), \r, \n = 9 znaków
        if rx_buffer.len() < 9 || rx_buffer[0] != b':' {
            return 0;
        }

        // Szukamy końca ramki (\r\n)
        let end_idx = rx_buffer.iter().position(|&b| b == b'\r').unwrap_or(0);
        if end_idx < 7 || rx_buffer.get(end_idx + 1) != Some(&b'\n') {
            return 0;
        }

        // 2. Dekodowanie z ASCII (znaki hex) do wartości binarnych
        let mut bin_rx = [0u8; 256];
        let mut bin_len = 0;

        let hex_payload = &rx_buffer[1..end_idx]; // Wycinamy środek bez ':' i '\r\n'

        for chunk in hex_payload.chunks_exact(2) {
            if let (Some(hi), Some(lo)) =
                (Self::decode_nibble(chunk[0]), Self::decode_nibble(chunk[1]))
            {
                bin_rx[bin_len] = (hi << 4) | lo;
                bin_len += 1;
            } else {
                return 0; // Błędny znak w ramce
            }
        }

        // 3. Weryfikacja sumy LRC
        let received_lrc = bin_rx[bin_len - 1];
        let calculated_lrc = Self::calculate_lrc(&bin_rx[..bin_len - 1]);
        if received_lrc != calculated_lrc {
            println!("Modbus ASCII: Błędne LRC!");
            return 0;
        }

        // 4. Przekazanie PDU do rdzenia
        let slave_addr = bin_rx[0];

        let device = match router.get_device_mut(slave_addr) {
            Some(addr) => addr,
            None => {
                println!("Router: Ignored frame for unknown Slave id: {}", slave_addr);
                return 0;
            }
        };

        let pdu_data = &bin_rx[1..bin_len - 1];
        let mut bin_tx = [0u8; 256];
        bin_tx[0] = slave_addr;

        if let Ok(req_pdu) = ReqPdu::parse(pdu_data) {
            if let Ok(modbus_req) = ModbusRequest::from_pdu(&req_pdu) {
                // Czysty rdzeń przetwarza zapytanie
                let pdu_res_len = execute_request(&modbus_req, &mut bin_tx[1..], device);
                let total_res_bin_len = pdu_res_len + 1;

                // 5. Pakowanie binarnej odpowiedzi do ASCII
                tx_buffer[0] = b':';
                let mut tx_idx = 1;

                // Kodujemy bajty na znaki hex
                for &byte in &bin_tx[..total_res_bin_len] {
                    let (hi, lo) = Self::encode_byte(byte);
                    tx_buffer[tx_idx] = hi;
                    tx_buffer[tx_idx + 1] = lo;
                    tx_idx += 2;
                }

                // Dodajemy sumę kontrolną LRC
                let tx_lrc = Self::calculate_lrc(&bin_tx[..total_res_bin_len]);
                let (lrc_hi, lrc_lo) = Self::encode_byte(tx_lrc);
                tx_buffer[tx_idx] = lrc_hi;
                tx_buffer[tx_idx + 1] = lrc_lo;
                tx_idx += 2;

                // Znaczniki końca \r\n
                tx_buffer[tx_idx] = b'\r';
                tx_buffer[tx_idx + 1] = b'\n';
                tx_idx += 2;

                return tx_idx; // Zwracamy długość ramki ASCII
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
