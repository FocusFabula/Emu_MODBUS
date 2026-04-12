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
    fn try_from_req_pdu(req_pdu: &BitVec<u8>) -> Result<Self, ModbusException> {
        if req_pdu.len() < 8 {
            return Err(ModbusException::IllegalFunction);
        }
        let function_bytes = &req_pdu[0..8];
        let function_code: u8 = function_bytes.load_be::<u8>();
        let function = match function_code {
            1 => {
                if req_pdu.len() < 40 {
                    return Err(ModbusException::IllegalFunction);
                }
                Ok(RequestFunction::ReadCoils {
                    start_adress: req_pdu[8..24].load_be::<u16>(),
                    quantity_of_coils: req_pdu[24..40].load_be::<u16>(),
                })
            }
            2 => {
                if req_pdu.len() < 40 {
                    return Err(ModbusException::IllegalFunction);
                }
                Ok(RequestFunction::ReadDiscreteInputs {
                    start_adress: req_pdu[8..24].load_be::<u16>(),
                    quantity_of_inputs: req_pdu[24..40].load_be::<u16>(),
                })
            }
            3 => {
                if req_pdu.len() < 40 {
                    return Err(ModbusException::IllegalFunction);
                }
                Ok(RequestFunction::ReadHoldingRegisters {
                    start_adress: req_pdu[8..24].load_be::<u16>(),
                    quantity_of_registers: req_pdu[24..40].load_be::<u16>(),
                })
            }
            4 => {
                if req_pdu.len() < 40 {
                    return Err(ModbusException::IllegalFunction);
                }
                Ok(RequestFunction::ReadInputRegisters {
                    start_adress: req_pdu[8..24].load_be::<u16>(),
                    quantity_of_input_registers: req_pdu[24..40].load_be::<u16>(),
                })
            }
            5 => {
                if req_pdu.len() < 40 {
                    return Err(ModbusException::IllegalFunction);
                }
                Ok(RequestFunction::WriteSingleCoil {
                    output_adress: req_pdu[8..24].load_be::<u16>(),
                    output_value: req_pdu[24..40].load_be::<u16>(),
                })
            }
            6 => {
                if req_pdu.len() < 40 {
                    return Err(ModbusException::IllegalFunction);
                }
                Ok(RequestFunction::WriteSingleRegister {
                    regiter_adress: req_pdu[8..24].load_be::<u16>(),
                    register_value: req_pdu[24..40].load_be::<u16>(),
                })
            }
            7 => Ok(RequestFunction::ReadExceptionStatus),
            _ => Err(ModbusException::IllegalFunction),
        };
        return function;
    }
}
