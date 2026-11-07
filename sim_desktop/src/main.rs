mod logger;
mod sim_device;
mod transport;
use std::sync::mpsc;

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
use std::thread;
enum ConnectionType {
    RTU,
    ASCII,
    TCP,
}

fn main() {
    let mut router = ModbusRouter::new();
    let (tx, rx) = mpsc::channel();
    GuiLogger::init(tx);
    // Wspólna pamięć dla obu wariantów
    let mut my_sim_device = DynamicMemory::new(4);
    let _ = my_sim_device.write_holding_register(1, 42);
    println!("{:?}", my_sim_device);
    router.add_device(1, my_sim_device);
    let mut my_sim_device2 = DynamicMemory::new(10);
    let _ = my_sim_device2.write_holding_register(3, 86);
    println!("{:?}", my_sim_device2);
    router.add_device(10, my_sim_device2);
    println!("Dostepne urządzenia sieciowe pod id: 1, 10");
    GuiLogger::log_to(
        logger::LogLayer::System,
        Level::Info,
        "Dostępne urządzenia sieciowe pod adresem 1 i 10",
    );

    thread::spawn(move || {
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
    });

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([600.0, 800.0]),
        ..Default::default()
    };

    let _ = eframe::run_native(
        "testowanie emu_modbus",
        options,
        Box::new(|cc| Ok(Box::new(MyApp::new(rx, cc)))),
    );
}

struct MyApp {
    logs: GuiLoggerBuffer,
    reciever: mpsc::Receiver<String>,
}

impl MyApp {
    fn new(message: mpsc::Receiver<String>, cc: &eframe::CreationContext<'_>) -> Self {
        let egui_ctx = cc.egui_ctx.clone();
        let (tx, rx) = mpsc::channel();

        thread::spawn(move || {
            while let Ok(mes) = message.recv() {
                tx.send(mes).unwrap();
                egui_ctx.request_repaint();
            }
        });

        Self {
            logs: GuiLoggerBuffer::new(1000),
            reciever: rx,
        }
    }

    fn update(&mut self) {
        while let Ok(log) = self.reciever.try_recv() {
            self.logs.push(log);
        }
    }
}

impl eframe::App for MyApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        egui::Panel::left("testowe logi").show(ui, |ui| {
            ui.label("testowanie logów");
            ui.separator();
            self.update();
            egui::ScrollArea::vertical()
                .auto_shrink(true)
                .show(ui, |ui| {
                    for log in self.logs.get_logs().iter() {
                        ui.label(log);
                    }
                });
        });
    }
}
