use bitvec::prelude::*;

use crate::modbus_handle::ModbusException;

enum FunctionResponse {
    Echo {
        function_code: u8,
        byte_count: u8,
        status: BitVec<u16, Msb0>,
    },
    FatEcho {
        function_code: u8,
        byte_count: u16,
        status: BitVec<u16, Msb0>,
    },
}

impl FunctionResponse {
    fn get_byte_count() {
        todo!();
    }
    fn build(function_code: u8, status: BitVec<u16, Msb0>) -> Result<Self, ModbusException> {
        todo!();
    }
}
