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
