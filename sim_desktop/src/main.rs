mod logger;
mod sim_device;
mod transport;
use std::sync::mpsc;
mod gui;
use crate::{
    logger::{GuiLogger, GuiLoggerBuffer},
    sim_device::DynamicMemory,
    transport::{
        ModbusAsciiAdu, ModbusRouter, ModbusRtuAdu, ModbusTcpAdu, SerialServer, TcpServer,
    },
};
use core_modbus::ModbusDevice;
use eframe::egui;
use log::Level;
use serialport::{DataBits, Parity};
use std::sync::{Arc, Mutex};
use std::thread;
enum ConnectionType {
    RTU,
    ASCII,
    TCP,
}

fn main() {
    let router_arc = Arc::new(Mutex::new(ModbusRouter::new()));
    let mut router = router_arc.lock().unwrap();
    let (tx, rx) = mpsc::channel();
    GuiLogger::init(tx);
    // Wspólna pamięć dla obu wariantów
    let mut my_sim_device = DynamicMemory::new("dis".to_string(), 4);
    let _ = my_sim_device.write_holding_register(1, 42);
    println!("{:?}", my_sim_device);
    router.add_device(1, my_sim_device);
    let mut my_sim_device2 = DynamicMemory::new("Jebany jest".to_string(), 10);
    let _ = my_sim_device2.write_holding_register(3, 86);
    println!("{:?}", my_sim_device2);
    router.add_device(10, my_sim_device2);
    println!("Dostepne urządzenia sieciowe pod id: 1, 10");
    GuiLogger::log_to(
        logger::LogLayer::System,
        Level::Info,
        "Dostępne urządzenia sieciowe pod adresem 1 i 10",
    );
    drop(router);

    let router_handler = router_arc.clone();
    thread::spawn(move || {
        let server_type = ConnectionType::TCP;
        match server_type {
            ConnectionType::RTU => {
                println!("Dostepne urządzenia sieciowe pod id: 1, 10");
                let server = SerialServer::new("/dev/pts/3", 9600, DataBits::Eight, Parity::Even);
                server.run(|rw, tx| ModbusRtuAdu::process(rw, tx, &router_handler));
            }
            ConnectionType::ASCII => {
                let server = SerialServer::new("/dev/pts/3", 9600, DataBits::Eight, Parity::Even);
                server.run(|rw, tx| ModbusAsciiAdu::process(rw, tx, &router_handler));
            }

            ConnectionType::TCP => {
                println!("Server wystartował pod IP: 0.0.0.0:5020");
                let server = TcpServer::new("0.0.0.0:5020");
                server.run(|rw, tx| ModbusTcpAdu::process(rw, tx, &router_handler));
            }
        }
    });

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([600.0, 800.0]),
        ..Default::default()
    };
    let router_gui = router_arc.clone();
    let _ = eframe::run_native(
        "testowanie emu_modbus",
        options,
        Box::new(|cc| Ok(Box::new(gui::MyApp::new(rx, cc, router_gui)))),
    );
}
