use bitvec::prelude::*;
#[derive(Debug)]
pub struct DeviceConfig {
    coils: u16,
    descrete_inputs: u16,
    input_registers: u16,
    holding_registers: u16,
}

impl DeviceConfig {
    pub fn new(
        coils: u16,
        descrete_inputs: u16,
        input_registers: u16,
        holding_registers: u16,
    ) -> Self {
        DeviceConfig {
            coils,
            descrete_inputs,
            input_registers,
            holding_registers,
        }
    }
}

/*
 * coils 1-bit * adress 16-bit = 65536 = 8kB
 * descrete inputs 1-bit * adress 16-bit = 65536 = 8kB
 * input registers 16-bit * adress 16-bit = 2 byte * 65536 = 128kB
 * holding registers 16-bit * adress 16-bit = 2 byte * 65536 = 128kB
 *
 */
#[derive(Debug)]
pub struct Memory {
    coils: BitVec<u8, Lsb0>,
    descrete_inputs: BitVec<u8, Lsb0>,
    input_registers: Vec<u16>,
    holding_registers: Vec<u16>,
}
#[derive(Debug)]
pub enum MemoryArea {
    Coil,
    DiscreteInput,
    HoldingRegister,
    InputRegister,
}
#[derive(Debug)]
pub enum MemoryError {
    InvalidAddress(MemoryArea),
    OutOfRange(MemoryArea),
}

impl Memory {
    pub fn new(config: &DeviceConfig) -> Self {
        Memory {
            coils: BitVec::repeat(false, config.coils.into()),
            descrete_inputs: BitVec::repeat(false, config.descrete_inputs.into()),
            input_registers: vec![0u16; config.input_registers.into()],
            holding_registers: vec![0u16; config.holding_registers.into()],
        }
    }
    fn get_bit(&self, adress: u16, area: MemoryArea) -> Result<bool, MemoryError> {
        let memory_place = match area {
            MemoryArea::Coil => &self.coils,
            MemoryArea::DiscreteInput => &self.descrete_inputs,
            _ => unreachable!("Coil or DiscreteInput idiot!"),
        };
        if let Some(place) = memory_place.get(adress as usize).as_deref() {
            Ok(*place)
        } else {
            Err(MemoryError::InvalidAddress(area))
        }
    }
    fn get_multiple_bits(
        &self,
        start_adress: u16,
        quantity: u16,
        area: MemoryArea,
    ) -> Result<&BitSlice<u8>, MemoryError> {
        let memory_place = match area {
            MemoryArea::Coil => &self.coils,
            MemoryArea::DiscreteInput => &self.descrete_inputs,
            _ => unreachable!("Coil or DiscreteInput idiot!"),
        };
        if start_adress > (memory_place.len() as u16) {
            return Err(MemoryError::InvalidAddress(area));
        } else if (start_adress as usize) + (quantity as usize) > (memory_place.len() as usize) {
            return Err(MemoryError::OutOfRange(area));
        }
        let return_place = &memory_place
            [(start_adress as usize)..((start_adress as usize) + (quantity as usize) + 1)];
        return Ok(return_place);
    }

    fn write_bit(&mut self, adress: u16, state: bool, area: MemoryArea) -> Result<(), MemoryError> {
        let memory_place = match area {
            MemoryArea::Coil => &mut self.coils,
            MemoryArea::DiscreteInput => &mut self.descrete_inputs,
            _ => unreachable!("Coil or DiscreteInput idiot!"),
        };

        if (adress as usize) < memory_place.len() {
            memory_place.set(adress as usize, state);
            return Ok(());
        } else {
            Err(MemoryError::InvalidAddress(area))
        }
    }

    fn write_multiple_bits(
        &mut self,
        start_address: u16,
        values: BitVec<u8>,
        area: MemoryArea,
    ) -> Result<(), MemoryError> {
        let memory_place = match area {
            MemoryArea::Coil => &mut self.coils,
            MemoryArea::DiscreteInput => &mut self.descrete_inputs,
            _ => unreachable!("Coil or DiscreteInput idiot!"),
        };
        if (start_address as usize) > memory_place.len() {
            return Err(MemoryError::InvalidAddress(area));
        } else if ((start_address as usize) + (values.len() as usize)) > memory_place.len() {
            return Err(MemoryError::OutOfRange(area));
        } else {
            //trzeba zachować pętlę for bo .copy_from_slice nie zadziała bo Vec<bool> jest typem
            //specjalnym i nie implementuje traita odpowiedniego bo pakuje bity po 8
            memory_place[(start_address as usize)..((start_address as usize) + values.len())]
                .copy_from_bitslice(&values);

            return Ok(());
        }
    }

    //Registers
    fn get_register(&self, adress: u16, area: MemoryArea) -> Result<u16, MemoryError> {
        let memory_place = match area {
            MemoryArea::InputRegister => &self.input_registers,
            MemoryArea::HoldingRegister => &self.holding_registers,
            _ => unreachable!("Input or Holding registers idiot!"),
        };
        if let Some(place) = memory_place.get(adress as usize).copied() {
            Ok(place)
        } else {
            Err(MemoryError::InvalidAddress(area))
        }
    }

    fn get_multiple_registers(
        &self,
        start_adress: u16,
        quantity: u16,
        area: MemoryArea,
    ) -> Result<&[u16], MemoryError> {
        let memory_place = match area {
            MemoryArea::InputRegister => &self.input_registers,
            MemoryArea::HoldingRegister => &self.holding_registers,
            _ => unreachable!("Input or Holding registers idiot!"),
        };
        if start_adress > (memory_place.len() as u16) {
            return Err(MemoryError::InvalidAddress(area));
        } else if (start_adress as usize) + (quantity as usize) > (memory_place.len() as usize) {
            return Err(MemoryError::OutOfRange(area));
        }
        let return_place = &memory_place
            [(start_adress as usize)..((start_adress as usize) + (quantity as usize) + 1)];
        return Ok(return_place);
    }

    fn write_register(
        &mut self,
        adress: u16,
        state: u16,
        area: MemoryArea,
    ) -> Result<(), MemoryError> {
        let memory_place = match area {
            MemoryArea::InputRegister => &mut self.input_registers,
            MemoryArea::HoldingRegister => &mut self.holding_registers,
            _ => unreachable!("Input or Holding registers idiot!"),
        };
        if (adress as usize) < memory_place.len() {
            memory_place[adress as usize] = state;
            return Ok(());
        } else {
            Err(MemoryError::InvalidAddress(area))
        }
    }
    fn write_multiple_registers(
        &mut self,
        start_address: u16,
        values: Vec<u16>,
        area: MemoryArea,
    ) -> Result<(), MemoryError> {
        let memory_place = match area {
            MemoryArea::InputRegister => &mut self.input_registers,
            MemoryArea::HoldingRegister => &mut self.holding_registers,
            _ => unreachable!("Input or Holding registers idiot!"),
        };
        if (start_address as usize) > memory_place.len() {
            return Err(MemoryError::InvalidAddress(area));
        } else if ((start_address as usize) + (values.len() as usize)) > memory_place.len() {
            return Err(MemoryError::OutOfRange(area));
        } else {
            memory_place[(start_address as usize)..(start_address as usize) + values.len()]
                .copy_from_slice(&values);
            return Ok(());
        }
    }
}

#[derive(Debug)]
pub struct Device {
    config: DeviceConfig,
    memory: Memory,
}

impl Device {
    pub fn new(config: DeviceConfig, memory: Memory) -> Self {
        Device { config, memory }
    }
}

pub trait ModbusDevice {
    fn get_coil(&self, adress: u16) -> Result<bool, MemoryError>;
    fn get_multiple_coils(
        &self,
        start_adress: u16,
        quantity: u16,
    ) -> Result<&BitSlice<u8>, MemoryError>;
    fn write_coil(&mut self, adress: u16, state: bool) -> Result<(), MemoryError>;
    fn write_multiple_coils(
        &mut self,
        start_address: u16,
        values: BitVec<u8>,
    ) -> Result<(), MemoryError>;

    fn get_discrete_input(&self, adress: u16) -> Result<bool, MemoryError>;
    fn get_multiple_discrete_input(
        &self,
        start_adress: u16,
        quantity: u16,
    ) -> Result<&BitSlice<u8>, MemoryError>;
    fn write_discrete_input(&mut self, adress: u16, state: bool) -> Result<(), MemoryError>;
    fn write_multiple_discrete_input(
        &mut self,
        start_address: u16,
        values: BitVec<u8>,
    ) -> Result<(), MemoryError>;

    fn get_holding_register(&self, adress: u16) -> Result<u16, MemoryError>;
    fn get_multiple_holding_registers(
        &self,
        start_adress: u16,
        quantity: u16,
    ) -> Result<&[u16], MemoryError>;
    fn write_holding_register(&mut self, adress: u16, state: u16) -> Result<(), MemoryError>;
    fn write_multiple_holding_registers(
        &mut self,
        start_address: u16,
        values: Vec<u16>,
    ) -> Result<(), MemoryError>;

    fn get_input_registers(&self, adress: u16) -> Result<u16, MemoryError>;
    fn get_multiple_input_registers(
        &self,
        start_adress: u16,
        quantity: u16,
    ) -> Result<&[u16], MemoryError>;
    fn write_input_register(&mut self, adress: u16, state: u16) -> Result<(), MemoryError>;
    fn write_multiple_input_register(
        &mut self,
        start_address: u16,
        values: Vec<u16>,
    ) -> Result<(), MemoryError>;
}

impl ModbusDevice for Device {
    fn get_coil(&self, adress: u16) -> Result<bool, MemoryError> {
        self.memory.get_bit(adress, MemoryArea::Coil)
    }

    fn get_multiple_coils(
        &self,
        start_adress: u16,
        quantity: u16,
    ) -> Result<&BitSlice<u8>, MemoryError> {
        self.memory
            .get_multiple_bits(start_adress, quantity, MemoryArea::Coil)
    }

    fn write_coil(&mut self, adress: u16, state: bool) -> Result<(), MemoryError> {
        self.memory.write_bit(adress, state, MemoryArea::Coil)
    }

    fn write_multiple_coils(
        &mut self,
        start_address: u16,
        values: BitVec<u8>,
    ) -> Result<(), MemoryError> {
        self.memory
            .write_multiple_bits(start_address, values, MemoryArea::Coil)
    }

    //discrete Inputs

    fn get_discrete_input(&self, adress: u16) -> Result<bool, MemoryError> {
        self.memory.get_bit(adress, MemoryArea::DiscreteInput)
    }

    fn get_multiple_discrete_input(
        &self,
        start_adress: u16,
        quantity: u16,
    ) -> Result<&BitSlice<u8>, MemoryError> {
        self.memory
            .get_multiple_bits(start_adress, quantity, MemoryArea::DiscreteInput)
    }

    fn write_discrete_input(&mut self, adress: u16, state: bool) -> Result<(), MemoryError> {
        self.memory
            .write_bit(adress, state, MemoryArea::DiscreteInput)
    }

    fn write_multiple_discrete_input(
        &mut self,
        start_address: u16,
        values: BitVec<u8>,
    ) -> Result<(), MemoryError> {
        self.memory
            .write_multiple_bits(start_address, values, MemoryArea::DiscreteInput)
    }

    //holding registers

    fn get_holding_register(&self, adress: u16) -> Result<u16, MemoryError> {
        self.memory
            .get_register(adress, MemoryArea::HoldingRegister)
    }

    fn get_multiple_holding_registers(
        &self,
        start_adress: u16,
        quantity: u16,
    ) -> Result<&[u16], MemoryError> {
        self.memory
            .get_multiple_registers(start_adress, quantity, MemoryArea::HoldingRegister)
    }

    fn write_holding_register(&mut self, adress: u16, state: u16) -> Result<(), MemoryError> {
        self.memory
            .write_register(adress, state, MemoryArea::HoldingRegister)
    }

    fn write_multiple_holding_registers(
        &mut self,
        start_address: u16,
        values: Vec<u16>,
    ) -> Result<(), MemoryError> {
        self.memory
            .write_multiple_registers(start_address, values, MemoryArea::HoldingRegister)
    }

    //input registers

    fn get_input_registers(&self, adress: u16) -> Result<u16, MemoryError> {
        self.memory.get_register(adress, MemoryArea::InputRegister)
    }

    fn get_multiple_input_registers(
        &self,
        start_adress: u16,
        quantity: u16,
    ) -> Result<&[u16], MemoryError> {
        self.memory
            .get_multiple_registers(start_adress, quantity, MemoryArea::InputRegister)
    }

    fn write_input_register(&mut self, adress: u16, state: u16) -> Result<(), MemoryError> {
        self.memory
            .write_register(adress, state, MemoryArea::InputRegister)
    }

    fn write_multiple_input_register(
        &mut self,
        start_address: u16,
        values: Vec<u16>,
    ) -> Result<(), MemoryError> {
        self.memory
            .write_multiple_registers(start_address, values, MemoryArea::InputRegister)
    }
}
