use crate::logger::{GuiLogger, LogLayer};
use core_modbus::{ModbusDevice, ModbusException};
use log::Level;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DynamicMemory {
    pub name: String,
    pub data: Vec<u8>,
    pub holding_offs: usize,
    pub input_offs: usize,
    pub coil_offs: usize,
    pub discrete_offs: usize,
    #[serde(default)]
    pub holding_count: u16,
    #[serde(default)]
    pub input_count: u16,
    #[serde(default)]
    pub coil_count: u16,
    #[serde(default)]
    pub discrete_count: u16,
}

impl DynamicMemory {
    pub fn new(name: String, size_bytes: usize) -> Self {
        GuiLogger::log_to(
            LogLayer::Memory,
            Level::Info,
            &format!("Zalokowano pamięć o rozmiarze {} bajtów", size_bytes),
        );
        Self {
            name,
            data: vec![0; size_bytes],
            holding_offs: 0,
            input_offs: 0,
            coil_offs: 0,
            discrete_offs: 0,
            holding_count: 0,
            input_count: 0,
            coil_count: 0,
            discrete_count: 0,
        }
    }

    pub fn capacity_bytes(&self) -> usize {
        self.data.len()
    }

    pub fn set_mapping(&mut self, holding: usize, input: usize, coil: usize, discrete: usize) {
        self.holding_offs = holding;
        self.input_offs = input;
        self.coil_offs = coil;
        self.discrete_offs = discrete;

        GuiLogger::log_to(
            LogLayer::Memory,
            Level::Info,
            &format!(
                "Zmieniono mapping pamięci - Holding: {}, Input: {}, Coil: {}, Discrete: {}",
                holding, input, coil, discrete
            ),
        );
    }

    pub fn set_counts(&mut self, holding: u16, input: u16, coil: u16, discrete: u16) {
        self.holding_count = holding;
        self.input_count = input;
        self.coil_count = coil;
        self.discrete_count = discrete;

        GuiLogger::log_to(
            LogLayer::Memory,
            Level::Info,
            &format!(
                "Ustawiono rozmiary obiektów - Holding: {}, Input: {}, Coil: {}, Discrete: {}",
                holding, input, coil, discrete
            ),
        );
    }

    pub fn required_bytes_for_counts(holding: u16, input: u16, coil: u16, discrete: u16) -> usize {
        (holding as usize) * 2
            + (input as usize) * 2
            + (coil as usize).div_ceil(8)
            + (discrete as usize).div_ceil(8)
    }
}

impl ModbusDevice for DynamicMemory {
    fn get_holding_register(&self, addr: u16) -> Result<u16, ModbusException> {
        if addr >= self.holding_count {
            GuiLogger::log_to(
                LogLayer::Memory,
                Level::Warn,
                &format!(
                    "Próba odczytu Holding Reg [{}] poza limitem ({})! IllegalDataAdress",
                    addr, self.holding_count
                ),
            );
            return Err(ModbusException::IllegalDataAdress);
        }

        let byte_idx = self.holding_offs + (addr as usize) * 2;
        if byte_idx + 1 < self.data.len() {
            let val = u16::from_be_bytes([self.data[byte_idx], self.data[byte_idx + 1]]);
            GuiLogger::log_to(
                LogLayer::Memory,
                Level::Info,
                &format!("Odczyt Holding Reg [{}] -> {} (0x{:04X})", addr, val, val),
            );
            Ok(val)
        } else {
            GuiLogger::log_to(
                LogLayer::Memory,
                Level::Warn,
                &format!(
                    "Próba odczytu Holding Reg [{}] poza zakresem pamięci! IllegalDataAdress",
                    addr
                ),
            );
            Err(ModbusException::IllegalDataAdress)
        }
    }

    fn write_holding_register(&mut self, addr: u16, value: u16) -> Result<(), ModbusException> {
        if addr >= self.holding_count {
            GuiLogger::log_to(
                LogLayer::Memory,
                Level::Warn,
                &format!(
                    "Próba zapisu Holding Reg [{}] poza limitem ({})! IllegalDataAdress",
                    addr, self.holding_count
                ),
            );
            return Err(ModbusException::IllegalDataAdress);
        }

        let byte_idx = self.holding_offs + (addr as usize) * 2;
        if byte_idx + 1 < self.data.len() {
            let bytes = value.to_be_bytes();
            self.data[byte_idx] = bytes[0];
            self.data[byte_idx + 1] = bytes[1];
            GuiLogger::log_to(
                LogLayer::Memory,
                Level::Info,
                &format!("Zapis Holding Reg [{}] = {} (0x{:04X})", addr, value, value),
            );
            Ok(())
        } else {
            GuiLogger::log_to(
                LogLayer::Memory,
                Level::Warn,
                &format!(
                    "Próba zapisu Holding Reg [{}] poza zakresem pamięci! IllegalDataAdress",
                    addr
                ),
            );
            Err(ModbusException::IllegalDataAdress)
        }
    }

    fn get_coil(&self, addr: u16) -> Result<bool, ModbusException> {
        if addr >= self.coil_count {
            GuiLogger::log_to(
                LogLayer::Memory,
                Level::Warn,
                &format!(
                    "Próba odczytu Coil [{}] poza limitem ({})! IllegalDataAdress",
                    addr, self.coil_count
                ),
            );
            return Err(ModbusException::IllegalDataAdress);
        }

        let byte_idx = self.coil_offs + (addr as usize) / 8;
        let bit_idx = (addr as usize) % 8;

        if byte_idx < self.data.len() {
            let bit_val = (self.data[byte_idx] >> bit_idx) & 1;
            GuiLogger::log_to(
                LogLayer::Memory,
                Level::Debug,
                &format!("Odczyt Coil [{}] -> {}", addr, bit_val),
            );
            Ok(bit_val == 1)
        } else {
            GuiLogger::log_to(
                LogLayer::Memory,
                Level::Warn,
                &format!(
                    "Próba odczytu Coil [{}] poza zakresem pamięci! IllegalDataAdress",
                    addr
                ),
            );
            Err(ModbusException::IllegalDataAdress)
        }
    }

    fn write_coil(&mut self, addr: u16, value: bool) -> Result<(), ModbusException> {
        if addr >= self.coil_count {
            GuiLogger::log_to(
                LogLayer::Memory,
                Level::Warn,
                &format!(
                    "Próba zapisu Coil [{}] poza limitem ({})! IllegalDataAdress",
                    addr, self.coil_count
                ),
            );
            return Err(ModbusException::IllegalDataAdress);
        }

        let byte_idx = self.coil_offs + (addr as usize) / 8;
        let bit_idx = (addr as usize) % 8;

        if byte_idx < self.data.len() {
            if value {
                self.data[byte_idx] |= 1 << bit_idx;
            } else {
                self.data[byte_idx] &= !(1 << bit_idx);
            }

            GuiLogger::log_to(
                LogLayer::Memory,
                Level::Info,
                &format!("Zapis Coil [{}] = {}", addr, value),
            );
            Ok(())
        } else {
            GuiLogger::log_to(
                LogLayer::Memory,
                Level::Warn,
                &format!(
                    "Próba zapisu Coil [{}] poza zakresem pamięci! IllegalDataAdress",
                    addr
                ),
            );
            Err(ModbusException::IllegalDataAdress)
        }
    }

    fn get_input_register(&self, addr: u16) -> Result<u16, ModbusException> {
        if addr >= self.input_count {
            GuiLogger::log_to(
                LogLayer::Memory,
                Level::Warn,
                &format!(
                    "Próba odczytu Input Reg [{}] poza limitem ({})! IllegalDataAdress",
                    addr, self.input_count
                ),
            );
            return Err(ModbusException::IllegalDataAdress);
        }

        let byte_idx = self.input_offs + (addr as usize) * 2;
        if byte_idx + 1 < self.data.len() {
            let val = u16::from_be_bytes([self.data[byte_idx], self.data[byte_idx + 1]]);
            GuiLogger::log_to(
                LogLayer::Memory,
                Level::Debug,
                &format!("Odczyt Input Reg [{}] -> {} (0x{:04X})", addr, val, val),
            );
            Ok(val)
        } else {
            GuiLogger::log_to(
                LogLayer::Memory,
                Level::Warn,
                &format!(
                    "Próba odczytu Input Reg [{}] poza zakresem pamięci! IllegalDataAdress",
                    addr
                ),
            );
            Err(ModbusException::IllegalDataAdress)
        }
    }

    fn get_discrete_input(&self, addr: u16) -> Result<bool, ModbusException> {
        if addr >= self.discrete_count {
            GuiLogger::log_to(
                LogLayer::Memory,
                Level::Warn,
                &format!(
                    "Próba odczytu Discrete Input [{}] poza limitem ({})! IllegalDataAdress",
                    addr, self.discrete_count
                ),
            );
            return Err(ModbusException::IllegalDataAdress);
        }

        let byte_idx = self.discrete_offs + (addr as usize) / 8;
        let bit_idx = (addr as usize) % 8;
        if byte_idx < self.data.len() {
            let bit_val = ((self.data[byte_idx] >> bit_idx) & 1) == 1;
            GuiLogger::log_to(
                LogLayer::Memory,
                Level::Debug,
                &format!("Odczyt Discrete Input [{}] -> {}", addr, bit_val),
            );
            Ok(bit_val)
        } else {
            GuiLogger::log_to(
                LogLayer::Memory,
                Level::Warn,
                &format!(
                    "Próba odczytu Discrete Input [{}] poza zakresem pamięci! IllegalDataAdress",
                    addr
                ),
            );
            Err(ModbusException::IllegalDataAdress)
        }
    }
}
