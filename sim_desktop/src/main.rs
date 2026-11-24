mod logger;
mod persistence;
mod sim_device;
mod transport;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};
mod gui;
use crate::{
    logger::{GuiLogger, GuiLoggerBuffer},
    sim_device::DynamicMemory,
    transport::{
        ModbusAsciiAdu, ModbusRouter, ModbusRtuAdu, ModbusTcpAdu, SerialServer, TcpServer,
    },
};
use core_modbus::ModbusDevice;
use eframe::egui::{self, Order};
use log::Level;
use serialport::{DataBits, Parity};
use std::sync::{Arc, Mutex};
use std::thread;

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
    let server_running: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
    let server_running_clone = server_running.clone();
    let (server_tx, server_rx) = mpsc::channel();

    thread::spawn(move || {
        let mut active_server_handle: Option<thread::JoinHandle<()>> = None;

        while let Ok(cmd) = server_rx.recv() {
            match cmd {
                gui::ServerCommand::Start(conn) => {
                    // 1. Zatrzymaj ewentualny poprzedni serwer i poczekaj aż zwolni port/socket
                    server_running_clone.store(false, Ordering::SeqCst);
                    if let Some(handle) = active_server_handle.take() {
                        let _ = handle.join();
                    }

                    // 2. Włącz flagę dla nowego serwera
                    server_running_clone.store(true, Ordering::SeqCst);
                    let running_flag = server_running_clone.clone();
                    let router = router_handler.clone();

                    // 3. Odpal nowy serwer w osobnym wątku, żeby pętla komend nie blokowała się
                    active_server_handle = Some(thread::spawn(move || match conn {
                        gui::ConnectionType::TCP(conf) => {
                            let addr = conf.to_string();
                            println!("Server TCP started at: {}", addr);
                            let server = TcpServer::new(&addr);
                            server.run(running_flag, |rw, tx| {
                                ModbusTcpAdu::process(rw, tx, &router)
                            });
                        }
                        gui::ConnectionType::RTU(conf) => {
                            let port_name = conf.serial_port_buffer;
                            let bud_rate = conf.bud_rate_buffer as u32;
                            let parity = conf.parity;
                            let data_bits = conf.data_bits;
                            println!("Server RTU started at: {} ({})", port_name, bud_rate);
                            let server = SerialServer::new(&port_name, bud_rate, data_bits, parity);
                            server.run(running_flag, |rw, tx| {
                                ModbusRtuAdu::process(rw, tx, &router)
                            });
                        }
                        gui::ConnectionType::ASCII(conf) => {
                            let port_name = conf.serial_port_buffer;
                            let bud_rate = conf.bud_rate_buffer as u32;
                            let parity = conf.parity;
                            let data_bits = conf.data_bits;
                            println!("Server ASCII started at: {} ({})", port_name, bud_rate);
                            let server = SerialServer::new(&port_name, bud_rate, data_bits, parity);
                            server.run(running_flag, |rw, tx| {
                                ModbusAsciiAdu::process(rw, tx, &router)
                            });
                        }
                    }));
                }
                gui::ServerCommand::Stop => {
                    server_running_clone.store(false, Ordering::SeqCst);
                    if let Some(handle) = active_server_handle.take() {
                        let _ = handle.join();
                    }
                    println!("Server stopped");
                }
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
        Box::new(|cc| Ok(Box::new(gui::MyApp::new(rx, cc, router_gui, server_tx)))),
    );
}
