use bitvec::prelude::*;
mod device;
mod modbus_handle;
mod request;
mod response;
use crate::device::ModbusDevice;

fn main() {
    println!("Hello, world!");
    let conf = device::DeviceConfig::new(20, 20, 20, 20);
    let mem = device::Memory::new(&conf);
    println!("{:?}", conf);
    let mut dev = device::Device::new(conf, mem);
    let test = dev.write_coil(3, true);
    println!("{:?}", test);
    println!("{:?}", dev.get_coil(5));
    let coils = dev.get_multiple_coils(1, 10);
    println!("{:?}", coils);

    let mut new_val = bits![u8, Lsb0;
        1, 1, 1,
    ];

    let write_coils = dev.write_multiple_coils(10, new_val.into());

    println!("{:?}", write_coils);
    println!("{:?}", dev.get_multiple_coils(0, 19));
}
