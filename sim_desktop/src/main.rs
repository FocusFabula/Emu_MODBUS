mod logger;
mod sim_device;
mod transport;
use std::sync::RwLock;

use crate::{
    logger::GuiLogger,
    sim_device::DynamicMemory,
    transport::{
        ModbusAsciiAdu, ModbusRouter, ModbusRtuAdu, ModbusTcpAdu, SerialServer, TcpServer,
    },
};
use core_modbus::{ModbusDevice, ModbusException, ModbusRequest, ReqPdu, execute_request};
use log::Level;
use serialport::{DataBits, Parity};

enum ConnectionType {
    RTU,
    ASCII,
    TCP,
}

fn main() {
    let mut router = ModbusRouter::new();
    let mut logger = GuiLogger::new(1000);
    logger.init();
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
    GuiLogger::log_to(
        logger::LogLayer::System,
        Level::Info,
        "Dostępne urządzenia sieciowe pod adresem 1 i 10",
    );
    let server_type = ConnectionType::TCP;
    match server_type {
        ConnectionType::RTU => {
            println!("Dostepne urządzenia sieciowe pod id: 1, 10");
            let server = SerialServer::new("/dev/pts/3", 9600, DataBits::Eight, Parity::Even);
            server.run(|rw, tx| ModbusRtuAdu::process(rw, tx, &mut router));
        }
        ConnectionType::ASCII => {
            let server = SerialServer::new("/dev/pts/3", 9600, DataBits::Eight, Parity::Even);
            server.run(|rw, tx| ModbusAsciiAdu::process(rw, tx, &mut router));
        }

        ConnectionType::TCP => {
            println!("Server wystartował pod IP: 0.0.0.0:5020");
            let server = TcpServer::new("0.0.0.0:5020");
            server.run(|rw, tx| ModbusTcpAdu::process(rw, tx, &mut router));
        }
    }
}
