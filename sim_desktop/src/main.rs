mod sim_device;
mod transport;
use std::sync::RwLock;

use core_modbus::{ModbusDevice, ModbusException, ModbusRequest, ReqPdu, execute_request};
use serialport::{DataBits, Parity};

use crate::{
    sim_device::DynamicMemory,
    transport::{ModbusRouter, ModbusRtuAdu, SerialServer},
};

fn main() {
    let mut router = ModbusRouter::new();
    // Wspólna pamięć dla obu wariantów
    let mut my_sim_device = DynamicMemory::new(4);
    my_sim_device.write_holding_register(1, 42);
    println!("{:?}", my_sim_device);
    router.add_device(1, my_sim_device);
    let mut my_sim_device2 = DynamicMemory::new(10);
    my_sim_device2.write_holding_register(3, 86);
    println!("{:?}", my_sim_device2);
    router.add_device(10, my_sim_device2);

    println!("Dostepne urządzenia sieciowe pod id: 1, 10");

    let server = SerialServer::new("/dev/pts/3", 9600, DataBits::Eight, Parity::Even);
    server.run(|rw, tx| ModbusRtuAdu::process(rw, tx, &mut router));
}
