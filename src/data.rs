#[derive(Debug, PartialEq)]
struct ReqPdu<'a> {
    function_code: u8,
    request_data: &'a [u8],
}

impl<'a> ReqPdu<'a> {
    fn parse(data: &'a [u8]) -> Result<Self, &'static str> {
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

struct ResPdu<'a> {
    function_code: u8,
    response_data: &'a [u8],
}

struct ExceptionPdu {
    exception_function_code: u8,
    exception_code: u8,
}

enum ModbusRequest {
    ReadCoils { start_addr: u16, quantity: u16 },
    ReadHoldingRegisters { start_addr: u16, quantity: u16 },
    WriteSingleRegister { reg_addr: u16, value: u16 },
}

impl ModbusRequest {
    fn read_coils(data: &[u8]) -> Result<ModbusRequest, &'static str> {
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
    fn read_holding_registers(data: &[u8]) -> Result<ModbusRequest, &'static str> {
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

    fn write_single_register(data: &[u8]) -> Result<ModbusRequest, &'static str> {
        if data.len() != 4 {
            return Err("No enaugh data for function 06 => Write single register");
        }
        let reg_addr = u16::from_be_bytes([data[0], data[1]]);
        let value = u16::from_be_bytes([data[2], data[3]]);
        Ok(ModbusRequest::WriteSingleRegister { reg_addr, value })
    }

    fn from_pdu(pdu: &ReqPdu) -> Result<ModbusRequest, &'static str> {
        let function_code = pdu.function_code;
        let request_data = pdu.request_data;
        match function_code {
            0x01 => Self::read_coils(request_data),
            0x03 => Self::read_holding_registers(request_data),
            0x06 => Self::write_single_register(request_data),
            _ => Err("Function code not Found"),
        }
    }
}

#[derive(PartialEq)]
#[repr(u8)]
enum ModbusException {
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
    fn write_holding_register(&self, addr: u16, value: u16) -> Result<(), ModbusException>;

    // --- Input Registers (tylko odczyt 16-bit, np. pomiary z czujników) ---
    // Zwraca to samo co holding, ale pod spodem używa innej funkcji Modbus (0x04)
    fn get_input_register(&self, addr: u16) -> Result<u16, ModbusException>;

    // --- Coils (zapis/odczyt 1-bit, w Rust najlepiej użyć `bool`) ---
    fn get_coil(&self, addr: u16) -> Result<bool, ModbusException>;
    fn write_coil(&self, addr: u16, value: bool) -> Result<(), ModbusException>;

    // --- Discrete Inputs (tylko odczyt 1-bit, np. stan przycisku) ---
    fn get_discrete_input(&self, addr: u16) -> Result<bool, ModbusException>;
}

fn excetute_request(
    request: &ModbusRequest,
    tx_buffer: &mut [u8],
    device: &mut impl ModbusDevice,
) -> usize {
    match request {
        ModbusRequest::ReadCoils {
            start_addr,
            quantity,
        } => {
            if *quantity < 1 || *quantity > 125 {
                let exception = ModbusException::IllegalDataValue.parse(tx_buffer, 0x03);
                return exception;
            }
            tx_buffer[0] = 0x01;
            let n: u8;
            if (*quantity % 8) == 0 {
                n = (*quantity / 8) as u8;
            } else {
                n = (*quantity / 8) as u8 + 1;
            }
            tx_buffer[1] = n;
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
        ModbusRequest::WriteSingleRegister { reg_addr, value } => {
            todo!();
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use std::{dbg, println};

    #[test]
    fn ReqPduParse() {
        let raw_buffer = [0x03, 0x00, 0x6B, 0x00, 0x03];
        let req = ReqPdu::parse(&raw_buffer).unwrap();
        dbg!(&req);
        assert_eq!(req.function_code, 0x03);
        assert_eq!(req.request_data.len(), 4);
    }
}
