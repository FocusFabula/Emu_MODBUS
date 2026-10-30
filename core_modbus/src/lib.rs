#![no_std]

#[derive(Debug, PartialEq)]
pub struct ReqPdu<'a> {
    function_code: u8,
    request_data: &'a [u8],
}

impl<'a> ReqPdu<'a> {
    pub fn parse(data: &'a [u8]) -> Result<Self, &'static str> {
        if data.is_empty() {
            return Err("data is empty");
        }
        let function_code = data[0];

        let request_data = &data[1..];
        Ok(ReqPdu {
            function_code,
            request_data,
        })
    }
}

// struct ResPdu<'a> {
//     function_code: u8,
//     response_data: &'a [u8],
// }

// struct ExceptionPdu {
//     exception_function_code: u8,
//     exception_code: u8,
// }

pub enum ModbusRequest {
    ReadCoils { start_addr: u16, quantity: u16 },
    ReadDiscreteInputs { start_addr: u16, quantity: u16 },
    ReadHoldingRegisters { start_addr: u16, quantity: u16 },
    ReadInputRegisters { start_addr: u16, quantity: u16 },
    WriteSingleCoil { addr: u16, value: u16 },
    WriteSingleRegister { reg_addr: u16, value: u16 },
}

impl ModbusRequest {
    pub fn read_coils(data: &[u8]) -> Result<ModbusRequest, &'static str> {
        if data.len() != 4 {
            return Err("No enaugh data for function 03 => Holding registers");
        }
        let start_addr = u16::from_be_bytes([data[0], data[1]]);
        let quantity = u16::from_be_bytes([data[2], data[3]]);

        Ok(ModbusRequest::ReadCoils {
            start_addr,
            quantity,
        })
    }

    pub fn read_discrete_inputs(data: &[u8]) -> Result<ModbusRequest, &'static str> {
        if data.len() != 4 {
            return Err("No enaugh data for function 03 => Holding registers");
        }
        let start_addr = u16::from_be_bytes([data[0], data[1]]);
        let quantity = u16::from_be_bytes([data[2], data[3]]);
        Ok(ModbusRequest::ReadDiscreteInputs {
            start_addr,
            quantity,
        })
    }
    pub fn read_holding_registers(data: &[u8]) -> Result<ModbusRequest, &'static str> {
        if data.len() != 4 {
            return Err("No enaugh data for function 03 => Holding registers");
        }
        let start_addr = u16::from_be_bytes([data[0], data[1]]);
        let quantity = u16::from_be_bytes([data[2], data[3]]);
        Ok(ModbusRequest::ReadHoldingRegisters {
            start_addr,
            quantity,
        })
    }
    pub fn read_input_registers(data: &[u8]) -> Result<ModbusRequest, &'static str> {
        if data.len() != 4 {
            return Err("No enaugh data for function 04 => Read Input Registers");
        }
        let start_addr = u16::from_be_bytes([data[0], data[1]]);
        let quantity = u16::from_be_bytes([data[2], data[3]]);
        Ok(ModbusRequest::ReadInputRegisters {
            start_addr,
            quantity,
        })
    }

    pub fn write_single_coil(data: &[u8]) -> Result<ModbusRequest, &'static str> {
        if data.len() != 4 {
            return Err("No enaugh data for function 05 => Write single coil");
        }
        let addr = u16::from_be_bytes([data[0], data[1]]);
        let value = u16::from_be_bytes([data[2], data[3]]);
        Ok(ModbusRequest::WriteSingleCoil { addr, value })
    }
    pub fn write_single_register(data: &[u8]) -> Result<ModbusRequest, &'static str> {
        if data.len() != 4 {
            return Err("No enaugh data for function 06 => Write single register");
        }
        let reg_addr = u16::from_be_bytes([data[0], data[1]]);
        let value = u16::from_be_bytes([data[2], data[3]]);
        Ok(ModbusRequest::WriteSingleRegister { reg_addr, value })
    }

    pub fn from_pdu(pdu: &ReqPdu) -> Result<ModbusRequest, &'static str> {
        let function_code = pdu.function_code;
        let request_data = pdu.request_data;
        match function_code {
            0x01 => Self::read_coils(request_data),
            0x02 => Self::read_discrete_inputs(request_data),
            0x03 => Self::read_holding_registers(request_data),
            0x04 => Self::read_input_registers(request_data),
            0x05 => Self::write_single_register(request_data),
            0x06 => Self::write_single_register(request_data),
            _ => Err("Function code not Found"),
        }
    }
}

#[derive(PartialEq)]
#[repr(u8)]
pub enum ModbusException {
    IllegalFunction = 1,
    IllegalDataAdress = 2,
    IllegalDataValue = 3,
    ServerDeviceFailure = 4,
    Acknowledge = 5,
    ServerDeviceBusy = 6,
    MemoryParityError = 7,
    GatewayPathUnavailable = 10,
    GatewayTargetDeviceFailedToRespond = 11,
}

impl ModbusException {
    fn parse(self, tx_buffer: &mut [u8], function_code: u8) -> usize {
        tx_buffer[0] = function_code | 0x80;
        tx_buffer[1] = self as u8;
        return 2;
    }
}
pub trait ModbusDevice {
    // --- Holding Registers (zapis/odczyt 16-bit) ---
    fn get_holding_register(&self, addr: u16) -> Result<u16, ModbusException>;
    fn write_holding_register(&mut self, addr: u16, value: u16) -> Result<(), ModbusException>;

    // --- Input Registers (tylko odczyt 16-bit, np. pomiary z czujników) ---
    // Zwraca to samo co holding, ale pod spodem używa innej funkcji Modbus (0x04)
    fn get_input_register(&self, addr: u16) -> Result<u16, ModbusException>;

    // --- Coils (zapis/odczyt 1-bit, w Rust najlepiej użyć `bool`) ---
    fn get_coil(&self, addr: u16) -> Result<bool, ModbusException>;
    fn write_coil(&mut self, addr: u16, value: bool) -> Result<(), ModbusException>;

    // --- Discrete Inputs (tylko odczyt 1-bit, np. stan przycisku) ---
    fn get_discrete_input(&self, addr: u16) -> Result<bool, ModbusException>;
}

pub fn execute_request(
    request: &ModbusRequest,
    tx_buffer: &mut [u8],
    device: &mut impl ModbusDevice,
) -> usize {
    match request {
        ModbusRequest::ReadCoils {
            start_addr,
            quantity,
        } => {
            if *quantity < 1 || *quantity > 2000 {
                let exception = ModbusException::IllegalDataValue.parse(tx_buffer, 0x01);
                return exception;
            }
            tx_buffer[0] = 0x01;
            let n: u8 = ((*quantity + 7) / 8) as u8;
            tx_buffer[1] = n;
            for byte_count in 0..n {
                let mut current_byte = 0;
                for bit_count in 0..8 {
                    if *quantity > byte_count as u16 * 8 + bit_count as u16 {
                        match device
                            .get_coil(*start_addr + (byte_count as u16 * 8 + bit_count as u16))
                        {
                            Ok(val) => {
                                if val {
                                    current_byte |= 1 << bit_count;
                                }
                            }
                            Err(err) => {
                                return err.parse(tx_buffer, 0x01);
                            }
                        }
                    }
                }
                tx_buffer[byte_count as usize + 2] = current_byte;
            }
            return (n + 2) as usize;
        }

        ModbusRequest::ReadDiscreteInputs {
            start_addr,
            quantity,
        } => {
            if *quantity < 1 || *quantity > 2000 {
                let exception = ModbusException::IllegalDataValue.parse(tx_buffer, 0x02);
                return exception;
            }

            tx_buffer[0] = 0x02;
            let n = ((*quantity + 7) / 8) as u8;
            tx_buffer[1] = n;

            for byte_count in 0..n {
                let mut current_byte = 0;
                for bit_count in 0..8 {
                    if *quantity > byte_count as u16 * 8 + bit_count as u16 {
                        match device.get_discrete_input(
                            *start_addr + (byte_count as u16 * 8 + bit_count as u16),
                        ) {
                            Ok(val) => {
                                if val {
                                    current_byte |= 1 << bit_count;
                                }
                            }
                            Err(err) => {
                                return err.parse(tx_buffer, 0x02);
                            }
                        }
                    }
                }
                tx_buffer[byte_count as usize + 2] = current_byte;
            }

            return (n + 2) as usize;
        }

        ModbusRequest::ReadHoldingRegisters {
            start_addr,
            quantity,
        } => {
            if *quantity < 1 || *quantity > 125 {
                let exception = ModbusException::IllegalDataValue.parse(tx_buffer, 0x03);
                return exception;
            }
            tx_buffer[0] = 0x03;
            tx_buffer[1] = (*quantity * 2) as u8;
            for (i, addr) in (*start_addr..*start_addr + *quantity).enumerate() {
                match device.get_holding_register(addr) {
                    Ok(value) => {
                        tx_buffer[i * 2 + 2] = value.to_be_bytes()[0];
                        tx_buffer[i * 2 + 3] = value.to_be_bytes()[1];
                    }
                    Err(err) => {
                        // czy trzeba czyścić ten buffer
                        return err.parse(tx_buffer, 0x03);
                    }
                }
            }
            return (quantity * 2 + 2) as usize;
        }

        ModbusRequest::ReadInputRegisters {
            start_addr,
            quantity,
        } => {
            if *quantity < 1 || *quantity > 125 {
                let exception = ModbusException::IllegalDataValue.parse(tx_buffer, 0x03);
                return exception;
            }
            tx_buffer[0] = 0x04;
            tx_buffer[1] = (*quantity * 2) as u8;

            for (i, addr) in (*start_addr..*start_addr + *quantity).enumerate() {
                match device.get_input_register(addr) {
                    Ok(value) => {
                        tx_buffer[i * 2 + 2] = value.to_be_bytes()[0];
                        tx_buffer[i * 2 + 3] = value.to_be_bytes()[1];
                    }
                    Err(err) => {
                        // czy trzeba czyścić ten buffer
                        return err.parse(tx_buffer, 0x04);
                    }
                }
            }
            return (quantity * 2 + 2) as usize;
        }

        ModbusRequest::WriteSingleCoil { addr, value } => {
            if *value == 0x0000 || *value == 0xff00 {
                let exception = ModbusException::IllegalDataValue.parse(tx_buffer, 0x05);
                return exception;
            }
            let mut coil_status = false;
            if *value == 0xff00 {
                coil_status = true;
            }
            match device.write_coil(*addr, coil_status) {
                Ok(()) => (),
                Err(err) => return err.parse(tx_buffer, 0x05),
            }
            tx_buffer[0] = 0x05;
            tx_buffer[1] = addr.to_be_bytes()[0];
            tx_buffer[2] = addr.to_be_bytes()[1];
            tx_buffer[3] = value.to_be_bytes()[0];
            tx_buffer[4] = value.to_be_bytes()[1];
            return 5;
        }

        ModbusRequest::WriteSingleRegister { reg_addr, value } => {
            match device.write_holding_register(*reg_addr, *value) {
                Ok(()) => (),
                Err(err) => return err.parse(tx_buffer, 0x06),
            }
            tx_buffer[0] = 0x06;
            tx_buffer[1] = reg_addr.to_be_bytes()[0];
            tx_buffer[2] = reg_addr.to_be_bytes()[1];
            tx_buffer[3] = value.to_be_bytes()[0];
            tx_buffer[4] = value.to_be_bytes()[1];
            return 5;
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use std::dbg;

    struct DummyDevice {
        holding_registers: [u16; 10],
        coils: [bool; 10],
    }

    impl DummyDevice {
        fn new() -> Self {
            DummyDevice {
                holding_registers: [0, 10, 20, 30, 40, 50, 60, 70, 80, 90],
                coils: [
                    false, true, false, true, false, true, false, true, false, true,
                ],
            }
        }
    }
    impl ModbusDevice for DummyDevice {
        fn get_holding_register(&self, addr: u16) -> Result<u16, ModbusException> {
            if (addr as usize) < self.holding_registers.len() {
                Ok(self.holding_registers[addr as usize])
            } else {
                Err(ModbusException::IllegalDataAdress)
            }
        }

        fn write_holding_register(&mut self, addr: u16, value: u16) -> Result<(), ModbusException> {
            if (addr as usize) < self.holding_registers.len() {
                self.holding_registers[addr as usize] = value;
                Ok(())
            } else {
                Err(ModbusException::IllegalDataAdress)
            }
        }

        fn get_input_register(&self, addr: u16) -> Result<u16, ModbusException> {
            Err(ModbusException::IllegalFunction)
        }
        fn get_coil(&self, addr: u16) -> Result<bool, ModbusException> {
            if (addr as usize) < self.coils.len() {
                Ok(self.coils[addr as usize])
            } else {
                Err(ModbusException::IllegalDataAdress)
            }
        }

        fn write_coil(&mut self, addr: u16, value: bool) -> Result<(), ModbusException> {
            if (addr as usize) < self.coils.len() {
                self.coils[addr as usize] = value;
                Ok(())
            } else {
                Err(ModbusException::IllegalDataAdress)
            }
        }

        fn get_discrete_input(&self, addr: u16) -> Result<bool, ModbusException> {
            Err(ModbusException::IllegalFunction) // Pomińmy
        }
    }

    #[test]
    fn req_pdu_parse() {
        let raw_buffer = [0x03, 0x00, 0x6B, 0x00, 0x03];
        let req = ReqPdu::parse(&raw_buffer).unwrap();
        dbg!(&req);
        assert_eq!(req.function_code, 0x03);
        assert_eq!(req.request_data.len(), 4);
    }

    #[test]
    fn test_read_holding_registers_success() {
        let mut device = DummyDevice::new();

        // Symulujemy ramkę przychodzącą z sieci:
        // Kod funkcji: 03, Adres startowy: 0x0001 (1), Ilość: 0x0002 (2 rejestry)
        let raw_req = [0x03, 0x00, 0x01, 0x00, 0x02];

        let req_pdu = ReqPdu::parse(&raw_req).unwrap();
        let modbus_req = ModbusRequest::from_pdu(&req_pdu).unwrap();

        let mut tx_buffer = [0u8; 256];

        // Wywołujemy nasz rdzeń!
        let len = execute_request(&modbus_req, &mut tx_buffer, &mut device);

        // Oczekiwana długość: 1 (nagłówek) + 1 (ilość bajtów) + 4 (2 rejestry po 2 bajty) = 6
        assert_eq!(len, 6);

        // Oczekiwana ramka:
        // - [0] 0x03 (funkcja)
        // - [1] 0x04 (4 bajty danych, bo 2 rejestry)
        // - [2,3] Wartość z indeksu 1 (10 dziesiętnie to 0x000A)
        // - [4,5] Wartość z indeksu 2 (20 dziesiętnie to 0x0014)
        assert_eq!(&tx_buffer[..len], &[0x03, 0x04, 0x00, 0x0A, 0x00, 0x14]);
    }
}
