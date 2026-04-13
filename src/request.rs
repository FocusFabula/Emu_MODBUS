use crate::modbus_handle::ModbusException;
use bitvec::prelude::*;

#[derive(Debug)]
pub enum RequestFunction {
    //Phisical discrete inputs
    ReadDiscreteInputs {
        start_adress: u16,
        quantity_of_inputs: u16,
    },

    //Internak Bits or Phisical coils
    ReadCoils {
        start_adress: u16,
        quantity_of_coils: u16,
    },
    WriteSingleCoil {
        output_adress: u16,
        output_value: u16,
    },
    WriteMultipleCoils {
        starting_adress: u8,
        quantity_of_outputs: u16,
        byte_count: u8,
        output_value: Vec<u8>,
    },

    //Phisical Input registers
    ReadInputRegisters {
        start_adress: u16,
        quantity_of_input_registers: u16,
    },

    //Internal Registers or Physical output Registers
    ReadHoldingRegisters {
        start_adress: u16,
        quantity_of_registers: u16,
    },
    WriteSingleRegister {
        regiter_adress: u16,
        register_value: u16,
    },
    WriteMultipleRegisters {
        starting_adress: u16,
        quantity_of_registers: u16,
        byte_count: u8,
        registers_value: Vec<u16>,
    },
    ReadWriteMultipleRegisters {
        read_starting_adress: u16,
        quantity_to_read: u16,
        write_starting_adress: u16,
        quantity_to_write: u16,
        write_byte_count: u8,
        write_registers_value: Vec<u16>,
    },
    MaskWriteMultipleRegisters {
        reference_adress: u16,
        and_mask: u16,
        or_mask: u16,
    },
    ReadFIFOqueue {
        fifo_pointer_adress: u16,
    },

    //File record acces
    ReadFileRecord {
        byte_count: u8,
        sub_req_type: u8,
        sub_req_file: u16,
    },
    WriteFileRecord {
        request_data_length: u8,
        sub_req_type: u8,
        sub_req_file: u16,
        sub_req_record_number: u16,
    },

    //Diagnostics
    ReadExceptionStatus,
    Diagnostic {
        sub_function: u16,
        data: Vec<u16>,
    },
    GetComEventCounter,
    GetComEventLog,
    ReportServerID,
    ReadDeviceIdentification,

    //Other
    EncapsulatedInterfaceTransport {
        mei_type: u8,
        mei_type_specyfic_data: Vec<u8>,
    },
    CANopenGeneralReference,
}

impl RequestFunction {
    fn from_16bit(req_pdu: &BitVec<u8, Msb0>) -> Result<(u16, u16), ModbusException> {
        if req_pdu.len() < 40 {
            return Err(ModbusException::IllegalFunction);
        }
        let start = req_pdu[8..24].load_be::<u16>();
        let data = req_pdu[24..40].load_be::<u16>();
        return Ok((start, data));
    }

    fn build_2bytes_pdu(
        &self,
        function_number: u8,
        start: u16,
        data: u16,
        req_pdu: &mut BitVec<u8, Msb0>,
    ) {
        req_pdu.extend_from_bitslice(function_number.view_bits::<Msb0>());
        req_pdu.extend_from_bitslice(start.view_bits::<Msb0>());
        req_pdu.extend_from_bitslice(data.view_bits::<Msb0>());
    }
    pub fn get_request_function_code(&self) -> Result<u8, ModbusException> {
        match self {
            Self::ReadCoils { .. } => Ok(1),
            Self::ReadDiscreteInputs { .. } => Ok(2),
            Self::ReadHoldingRegisters { .. } => Ok(3),
            Self::ReadInputRegisters { .. } => Ok(4),
            Self::WriteSingleCoil { .. } => Ok(5),
            Self::WriteSingleRegister { .. } => Ok(6),
            Self::ReadExceptionStatus => Ok(7),
            _ => Err(ModbusException::IllegalFunction),
        }
    }
    pub fn try_from_req_pdu(req_pdu: &BitVec<u8, Msb0>) -> Result<Self, ModbusException> {
        if req_pdu.len() < 8 {
            return Err(ModbusException::IllegalFunction);
        }
        let function_bytes = &req_pdu[0..8];
        let function_code: u8 = function_bytes.load_be::<u8>();
        let (v1, v2) = Self::from_16bit(req_pdu)?;

        let function = match function_code {
            1 => Ok(Self::ReadCoils {
                start_adress: v1,
                quantity_of_coils: v2,
            }),
            2 => Ok(Self::ReadDiscreteInputs {
                start_adress: v1,
                quantity_of_inputs: v2,
            }),
            3 => Ok(Self::ReadHoldingRegisters {
                start_adress: v1,
                quantity_of_registers: v2,
            }),
            4 => Ok(Self::ReadInputRegisters {
                start_adress: v1,
                quantity_of_input_registers: v2,
            }),
            5 => Ok(Self::WriteSingleCoil {
                output_adress: v1,
                output_value: v2,
            }),

            6 => Ok(Self::WriteSingleRegister {
                regiter_adress: v1,
                register_value: v2,
            }),
            7 => Ok(Self::ReadExceptionStatus),
            _ => Err(ModbusException::IllegalFunction),
        };
        return function;
    }

    pub fn try_to_req_pdu(&self) -> Result<BitVec<u8, Msb0>, ModbusException> {
        use RequestFunction::*;
        let mut req_pdu = BitVec::<u8, Msb0>::new();
        match self {
            ReadCoils {
                start_adress,
                quantity_of_coils,
            } => {
                self.build_2bytes_pdu(1, *start_adress, *quantity_of_coils, &mut req_pdu);
                Ok(req_pdu)
            }

            ReadDiscreteInputs {
                start_adress,
                quantity_of_inputs,
            } => {
                self.build_2bytes_pdu(2, *start_adress, *quantity_of_inputs, &mut req_pdu);
                Ok(req_pdu)
            }
            ReadHoldingRegisters {
                start_adress,
                quantity_of_registers,
            } => {
                self.build_2bytes_pdu(3, *start_adress, *quantity_of_registers, &mut req_pdu);
                Ok(req_pdu)
            }
            ReadInputRegisters {
                start_adress,
                quantity_of_input_registers,
            } => {
                self.build_2bytes_pdu(4, *start_adress, *quantity_of_input_registers, &mut req_pdu);
                Ok(req_pdu)
            }
            WriteSingleCoil {
                output_adress,
                output_value,
            } => {
                self.build_2bytes_pdu(5, *output_adress, *output_value, &mut req_pdu);
                Ok(req_pdu)
            }
            WriteSingleRegister {
                regiter_adress,
                register_value,
            } => {
                self.build_2bytes_pdu(6, *regiter_adress, *register_value, &mut req_pdu);
                Ok(req_pdu)
            }
            ReadExceptionStatus => {
                req_pdu.extend_from_bitslice(&0x07u8.view_bits::<Msb0>());
                Ok(req_pdu)
            }
            _ => Err(ModbusException::IllegalFunction),
        }
    }
}
