use crate::logger::{GuiLogger, LogLayer};
use core_modbus::{ModbusDevice, ModbusException};
use log::Level;

#[derive(Debug)]
pub struct DynamicMemory {
    pub data: Vec<u8>,
    pub holding_offs: usize,
    pub input_offs: usize,
    pub coil_offs: usize,
    pub discrete_offs: usize,
}

impl DynamicMemory {
    pub fn new(size_bytes: usize) -> Self {
        GuiLogger::log_to(
            LogLayer::Memory,
            Level::Info,
            &format!("Zalokowano pamięć o rozmiarze {} bajtów", size_bytes),
        );
        Self {
            data: vec![0; size_bytes],
            holding_offs: 0,
            input_offs: 0,
            coil_offs: 0,
            discrete_offs: 0,
        }
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
}

impl ModbusDevice for DynamicMemory {
    fn get_holding_register(&self, addr: u16) -> Result<u16, ModbusException> {
        let byte_idx = self.holding_offs + (addr as usize) * 2;

        if byte_idx + 1 < self.data.len() {
            let hi = self.data[byte_idx];
            let lo = self.data[byte_idx + 1];
            GuiLogger::log_to(
                LogLayer::Memory,
                Level::Info,
                &format!(
                    "Odczyt Holding Reg [{}] -> {} (0x{:04X})",
                    addr,
                    u16::from_be_bytes([hi, lo]),
                    u16::from_be_bytes([hi, lo])
                ),
            );
            Ok(u16::from_be_bytes([hi, lo]))
        } else {
            GuiLogger::log_to(
                LogLayer::Memory,
                Level::Warn,
                &format!(
                    "Próba odczytu Holding Reg [{}] poza zakresem! IllegalDataAdress",
                    addr
                ),
            );
            Err(ModbusException::IllegalDataAdress)
        }
    }
    fn write_holding_register(&mut self, addr: u16, value: u16) -> Result<(), ModbusException> {
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
                    "Próba zapisu Holding Reg [{}] poza zakresem! IllegalDataAdress",
                    addr
                ),
            );
            Err(ModbusException::IllegalDataAdress)
        }
    }

    fn get_coil(&self, addr: u16) -> Result<bool, ModbusException> {
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
                &format!("Próba odczytu Coil [{}] poza zakresem!", addr),
            );
            Err(ModbusException::IllegalDataAdress)
        }
    }

    fn write_coil(&mut self, addr: u16, value: bool) -> Result<(), ModbusException> {
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
                &format!("Próba zapisu Coil [{}] poza zakresem!", addr),
            );
            Err(ModbusException::IllegalDataAdress)
        }
    }

    fn get_input_register(&self, addr: u16) -> Result<u16, ModbusException> {
        let byte_idx = self.input_offs + (addr as usize) * 2;
        if byte_idx + 1 < self.data.len() {
            GuiLogger::log_to(
                LogLayer::Memory,
                Level::Debug,
                &format!(
                    "Odczyt Input Reg [{}] -> {} (0x{:04X})",
                    addr,
                    u16::from_be_bytes([self.data[byte_idx], self.data[byte_idx + 1],]),
                    u16::from_be_bytes([self.data[byte_idx], self.data[byte_idx + 1],])
                ),
            );
            Ok(u16::from_be_bytes([
                self.data[byte_idx],
                self.data[byte_idx + 1],
            ]))
        } else {
            GuiLogger::log_to(
                LogLayer::Memory,
                Level::Warn,
                &format!("Próba odczytu Input Reg [{}] poza zakresem!", addr),
            );
            Err(ModbusException::IllegalDataAdress)
        }
    }

    fn get_discrete_input(&self, addr: u16) -> Result<bool, ModbusException> {
        let byte_idx = self.discrete_offs + (addr as usize) / 8;
        let bit_idx = (addr as usize) % 8;
        if byte_idx < self.data.len() {
            GuiLogger::log_to(
                LogLayer::Memory,
                Level::Debug,
                &format!(
                    "Odczyt Discrete Input [{}] -> {}",
                    addr,
                    ((self.data[byte_idx] >> bit_idx) & 1) == 1
                ),
            );
            Ok(((self.data[byte_idx] >> bit_idx) & 1) == 1)
        } else {
            GuiLogger::log_to(
                LogLayer::Memory,
                Level::Warn,
                &format!("Próba odczytu Discrete Input [{}] poza zakresem!", addr),
            );

            Err(ModbusException::IllegalDataAdress)
        }
    }
}
