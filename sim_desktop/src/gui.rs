use crate::{DynamicMemory, GuiLoggerBuffer, ModbusRouter};

use eframe::egui;
use egui_extras::{Column, TableBuilder};
use serialport::{self, DataBits, Parity};
use std::rc::Rc;
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::Duration;

#[derive(PartialEq)]
enum CenterView {
    WatchTable,
    NodeEditor,
    Plots,
}

#[derive(Debug)]
enum CellDataType {
    Coil,
    Register,
}

#[derive(Debug)]
enum CellFormat {
    DEC,
    BIN,
    HEX,
}

#[derive(Copy, Clone, PartialEq, Debug)]
enum ActiveCol {
    DeviceName,
    RowId,
    DataType,
    Comment,
    Value,
    Format,
}

#[derive(Debug)]
struct WatchTableRow {
    device: u16,
    id: usize,
    mem_addr: u16,
    datatype: CellDataType,
    value: u16,
    format: CellFormat,
    comment: String,
}

impl WatchTableRow {
    fn new(
        device: u16,
        id: usize,
        mem_addr: u16,
        datatype: CellDataType,
        format: CellFormat,
    ) -> Self {
        let value: u16 = 0;
        let comment = "".to_string();

        Self {
            device,
            id,
            mem_addr,
            datatype,
            value,
            format,
            comment,
        }
    }

    fn render(ui: &mut egui::Ui, active_cols: &Vec<ActiveCol>) {
        for col in active_cols.iter() {
            match col {
                ActiveCol::DeviceName => {
                    ui.label("test name 2");
                }
                _ => {
                    ui.label("test");
                }
            };
        }
    }
}
// watch table gui
struct WatchCeraetionState {
    is_open: bool,
    normal_cols: bool,
    active_cols: Vec<ActiveCol>,
    name_buffer: String,
    period_buffer: u64,
    device_id: u8,
}

impl WatchCeraetionState {
    fn new() -> Self {
        Self {
            is_open: false,
            normal_cols: true,
            active_cols: vec![
                ActiveCol::DeviceName,
                ActiveCol::RowId,
                ActiveCol::Value,
                ActiveCol::Format,
            ],
            name_buffer: "New table".to_string(),
            period_buffer: 500,
            device_id: 1,
        }
    }
}

struct WatchTable {
    name: String,
    rows: Vec<WatchTableRow>,
    period: Duration,
    active_cols: Vec<ActiveCol>,
}

impl WatchTable {
    fn new(name: String, period: Duration, active_cols: Vec<ActiveCol>) -> Self {
        let rows = Vec::new();
        Self {
            name,
            rows,
            period,
            active_cols,
        }
    }

    fn render_creation_window(
        ui: &mut egui::Ui,
        state: &mut WatchCeraetionState,
        router_arc: &Arc<Mutex<ModbusRouter>>,
    ) -> Option<WatchTable> {
        // <-- Funkcja zwraca teraz opcjonalnie nową tabelę!
        let mut closed = false;
        let mut created_table: Option<WatchTable> = None;

        if state.is_open {
            let ctx = ui.ctx().clone();

            egui::Window::new("utwórz nową tabelę")
                .title_bar(true)
                .open(&mut state.is_open)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(&ctx, |ui| {
                    ui.label("Konfiguracja nowej Watch Table");
                    ui.separator();

                    ui.horizontal(|ui| {
                        let router = router_arc.lock().unwrap();
                        let selected_device = &router.get_devices().get(&state.device_id);
                        let device_select_label = ui.label("Choose a device: ");
                        egui::ComboBox::from_id_salt(device_select_label.id)
                            .selected_text(format!("{}", selected_device.unwrap().name))
                            .show_ui(ui, |ui| {
                                for (id, device) in router.get_devices().iter() {
                                    ui.selectable_value(&mut state.device_id, *id, &device.name);
                                }
                            });
                    });
                    // Name of the table (używamy przekazanego bufora)
                    ui.horizontal(|ui| {
                        let table_name_label = ui.label("Watch table name: ");
                        ui.add(
                            egui::TextEdit::singleline(&mut state.name_buffer).desired_width(120.0),
                        )
                        .labelled_by(table_name_label.id);
                    });

                    // Period of sync (używamy przekazanego bufora)
                    ui.horizontal(|ui| {
                        let period_label = ui.label("Sync period (ms): ");
                        ui.add(egui::DragValue::new(&mut state.period_buffer).speed(1.0))
                            .labelled_by(period_label.id)
                    });
                    let period = Duration::from_millis(state.period_buffer);

                    // Cols configuration checkbox
                    ui.horizontal(|ui| {
                        let normal_active_cols_label = ui.label("Normal col configuration: ");
                        ui.checkbox(&mut state.normal_cols, "")
                            .labelled_by(normal_active_cols_label.id);
                    });

                    if !state.normal_cols {
                        ui.separator();
                        ui.label("Przeciągnij i upuść kolumny");

                        let all_cols = [
                            ActiveCol::DeviceName,
                            ActiveCol::RowId,
                            ActiveCol::DataType,
                            ActiveCol::Comment,
                            ActiveCol::Value,
                            ActiveCol::Format,
                        ];

                        let available_cols: Vec<ActiveCol> = all_cols
                            .into_iter()
                            .filter(|c| !state.active_cols.contains(c))
                            .collect();

                        let mut dragged_item: Option<ActiveCol> = None;
                        let mut target_col_idx: Option<usize> = None;
                        let mut item_idx: Option<usize> = None;

                        ui.columns(2, |cols| {
                            // Kolumna 0: Dostępne
                            cols[0].heading("Dostępne kolumny");
                            let (_, drop_avail) = cols[0].dnd_drop_zone::<ActiveCol, _>(
                                egui::Frame::dark_canvas(cols[0].style()),
                                |ui| {
                                    ui.set_min_size(egui::vec2(200.0, 200.0));
                                    for (row_idx, &item) in available_cols.iter().enumerate() {
                                        let item_id = egui::Id::unique("avail_col").with(row_idx);
                                        ui.dnd_drag_source(item_id, item, |ui| {
                                            ui.label(format!("{:?}", item));
                                        });
                                    }
                                },
                            );
                            if let Some(payload) = drop_avail {
                                dragged_item = Some(*payload);
                                target_col_idx = Some(0);
                            }

                            // Kolumna 1: Wybrane (Aktywne)
                            cols[1].heading("Wybrane kolumny");
                            let (_, drop_active) = cols[1].dnd_drop_zone::<ActiveCol, _>(
                                egui::Frame::dark_canvas(cols[1].style()),
                                |ui| {
                                    ui.set_min_size(egui::vec2(200.0, 200.0));
                                    for (row_idx, &item) in state.active_cols.iter().enumerate() {
                                        let item_id = egui::Id::unique("active_col").with(row_idx);
                                        let res = ui
                                            .dnd_drag_source(item_id, item, |ui| {
                                                ui.label(format!("☰ {:?}", item));
                                            })
                                            .response;

                                        if let Some(pointer) = ui.ctx().pointer_hover_pos() {
                                            if res.rect.contains(pointer) {
                                                let is_top_half = pointer.y < res.rect.center().y;
                                                let y_pos = if is_top_half {
                                                    res.rect.top()
                                                } else {
                                                    res.rect.bottom()
                                                };

                                                ui.painter().hline(
                                                    res.rect.x_range(),
                                                    y_pos,
                                                    (2.0, egui::Color32::WHITE),
                                                );

                                                item_idx = Some(if is_top_half {
                                                    row_idx
                                                } else {
                                                    row_idx + 1
                                                });
                                            }
                                        }
                                    }
                                },
                            );
                            if let Some(payload) = drop_active {
                                dragged_item = Some(*payload);
                                target_col_idx = Some(1);
                            }
                        });

                        // Przenoszenie elementów między kolumnami
                        if let (Some(item), Some(col_idx)) = (dragged_item, target_col_idx) {
                            state.active_cols.retain(|&x| x != item);

                            if col_idx == 1 {
                                if let Some(mut insert_idx) = item_idx {
                                    insert_idx = insert_idx.min(state.active_cols.len());
                                    state.active_cols.insert(insert_idx, item);
                                } else {
                                    state.active_cols.push(item);
                                }
                            }
                        }
                    }

                    ui.add_space(10.0);
                    ui.separator();

                    // PRZYCISKI AKCJONUJĄCE NA DOLE OKIENKA
                    ui.horizontal(|ui| {
                        if ui.button("✅ Utwórz Tabelę").clicked() {
                            // Tworzymy nową tabelę z danych w oknie
                            let final_name = if state.name_buffer.trim().is_empty() {
                                "Watch Table".to_string()
                            } else {
                                state.name_buffer.clone()
                            };

                            created_table = Some(WatchTable::new(
                                final_name,
                                period,
                                state.active_cols.clone(), // Klonujemy wybrane kolumny
                            ));

                            closed = true; // Zamykamy okno
                        }

                        if ui.button("Anuluj").clicked() {
                            closed = true;
                        }
                    });
                });

            if closed {
                state.is_open = false;
            }
        }

        created_table // Zwracamy tabelę do głównej pętli aplikacji (jeśli została utworzona)
    }

    fn render(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new(format!("📊 {} (Okres: {:?})", self.name, self.period))
            .default_open(true)
            .show(ui, |ui| {
                // Pobieramy dane z routera raz na klatkę

                // Definiujemy dostępną wysokość dla tabeli (np. max 300px, potem pojawia się pionowy suwak)
                // lub używamy ScrollArea, ale TableBuilder ma to już wbudowane!
                let available_height = 250.0;

                TableBuilder::new(ui)
                    .striped(true) // Co drugi wiersz ma lekko inny kolor (czytelność!)
                    .resizable(true) // Użytkownik może myszką zmieniać szerokość kolumn!
                    .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                    .min_scrolled_height(0.0)
                    .max_scroll_height(available_height) // Włącza pionowy suwak, gdy wierszy jest dużo
                    // Definicja kolumn na podstawie tego, co użytkownik wybrał w `self.active_cols`
                    .columns(Column::auto().at_least(80.0), self.active_cols.len())
                    // RYSOWANIE NAGŁÓWKÓW
                    .header(20.0, |mut header| {
                        for col in &self.active_cols {
                            header.col(|ui| {
                                // Dynamicznie wypisujemy nagłówek kolumny
                                let name = match col {
                                    ActiveCol::DeviceName => "Urządzenie",
                                    ActiveCol::RowId => "ID",
                                    ActiveCol::DataType => "Typ",
                                    ActiveCol::Comment => "Komentarz",
                                    ActiveCol::Value => "Wartość",
                                    ActiveCol::Format => "Format",
                                };
                                ui.strong(name);
                            });
                        }
                    })
                    // RYSOWANIE WIERSZY (Wirtualizowanych!)
                    .body(|mut body| {
                        // Iterujemy po wierszach tabeli
                        for (row_idx, row_data) in self.rows.iter_mut().enumerate() {
                            body.row(18.0, |mut row| {
                                // Dla każdego wiersza rysujemy komórki w kolejności z `self.active_cols`
                                for col in &self.active_cols {
                                    row.col(|ui| {
                                        match col {
                                            ActiveCol::DeviceName => {
                                                ui.label(format!("Slave {}", row_data.device));
                                            }
                                            ActiveCol::RowId => {
                                                ui.label(row_data.id.to_string());
                                            }
                                            ActiveCol::DataType => {
                                                ui.label(format!("{:?}", row_data.datatype));
                                            }
                                            ActiveCol::Comment => {
                                                ui.label(&row_data.comment);
                                            }
                                            ActiveCol::Value => {
                                                // Tutaj w przyszłości zrobisz formatowanie DEC/HEX/BIN
                                                ui.label(row_data.value.to_string());
                                            }
                                            ActiveCol::Format => {
                                                ui.label(format!("{:?}", row_data.format));
                                            }
                                        }
                                    });
                                }
                            });
                        }
                    });
            });
    }
}

struct GuiDevice {
    id: u8,
    name: String,
}

// Creation of a new port

#[derive(PartialEq, Eq, Debug, Clone)]
struct TcpConnect {
    ip: String,
    port: usize,
    timeout: Duration,
}

impl TcpConnect {
    fn new() -> Self {
        Self {
            ip: "127.0.0.1".to_string(),
            port: 520,
            timeout: Duration::from_millis(3000),
        }
    }
    fn render_edit_window(ui: &mut egui::Ui, state: &mut TcpConnect) {
        ui.horizontal(|ui| {
            let host_label = ui.label("Host");
            ui.add(egui::TextEdit::singleline(&mut state.ip))
                .labelled_by(host_label.id);
        });

        ui.horizontal(|ui| {
            let host_label = ui.label("Host");
            ui.add(
                egui::DragValue::new(&mut state.port)
                    .speed(1.0)
                    .range(1..=65535),
            )
            .labelled_by(host_label.id);
        });
        ui.horizontal(|ui| {
            let timeout_label = ui.label("Host");
            let mut timeout_value = state.timeout.as_millis() as usize;
            ui.add(
                egui::DragValue::new(&mut timeout_value)
                    .speed(1.0)
                    .range(1..=5000),
            )
            .labelled_by(timeout_label.id);
            state.timeout = Duration::from_millis(timeout_value as u64);
        });
    }
    fn to_string(&self) -> String {
        format!("{}:{}", self.ip, self.port)
    }
}

#[derive(PartialEq, Eq, Debug, Clone)]
struct SerialConnect {
    scan: bool,
    avalible_ports: Vec<String>,
    serial_port_buffer: String,
    port_id: usize,
    bud_rate_values: [usize; 8],
    bud_rate_id: usize,
    bud_rate_buffer: usize,

    data_bits: DataBits,
    parity: Parity,
    stop_bits: usize,
}

impl SerialConnect {
    fn new(data_bits: DataBits, parity: Parity) -> Self {
        let bud_rates = [1200, 2400, 4800, 9600, 19200, 38400, 57600, 115200];
        Self {
            scan: false,
            avalible_ports: Vec::new(),
            port_id: 0,
            serial_port_buffer: "/dev/pts/3".to_string(),
            bud_rate_values: bud_rates,
            bud_rate_id: 0,
            bud_rate_buffer: bud_rates[3],
            data_bits,
            parity,
            stop_bits: 2,
        }
    }

    fn render_edit_window(state: &mut SerialConnect, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let port_type_label = ui.label("Serial Port");
            ui.add(egui::TextEdit::singleline(&mut state.serial_port_buffer))
                .labelled_by(port_type_label.id);
            if ui.button("Scan").clicked() {
                match serialport::available_ports() {
                    Ok(mut ports) => {
                        if ports.len() != 0 {
                            ports.sort_unstable_by_key(|i| i.port_name.clone());
                            for port in ports {
                                state.avalible_ports.push(port.port_name);
                            }
                        }
                    }
                    Err(err) => {
                        ui.label(err.to_string());
                    }
                }
                state.scan = true;
            }
            if state.scan {
                egui::ComboBox::from_id_salt(port_type_label.id)
                    .selected_text(state.avalible_ports[state.port_id].to_string())
                    .show_ui(ui, |ui| {
                        for (i, port) in state.avalible_ports.iter().enumerate() {
                            ui.selectable_value(&mut state.port_id, i, port);
                        }
                        state.serial_port_buffer = state.avalible_ports[state.port_id].to_string();
                    });
            }
        });
        ui.horizontal(|ui| {
            let bud_rate_label = ui.label("Bud rate");
            egui::ComboBox::from_id_salt(bud_rate_label.id)
                .selected_text(state.bud_rate_buffer.to_string())
                .show_ui(ui, |ui| {
                    for (id, rate) in state.bud_rate_values.iter().enumerate() {
                        ui.selectable_value(&mut state.bud_rate_id, id, rate.to_string());
                    }
                    state.bud_rate_buffer = state.bud_rate_values[state.bud_rate_id];
                });
        });
        ui.horizontal(|ui| {
            let parity_label = ui.label("Parity");
            egui::ComboBox::from_id_salt(parity_label.id)
                .selected_text(state.parity.to_string())
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut state.parity, Parity::None, "None");
                    ui.selectable_value(&mut state.parity, Parity::Odd, "ODD");
                    ui.selectable_value(&mut state.parity, Parity::Even, "EVEN");
                });
        });
        ui.horizontal(|ui| {
            let stop_label = ui.label("Stop bits");
            egui::ComboBox::from_id_salt(stop_label.id)
                .selected_text(state.stop_bits.to_string())
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut state.stop_bits, 1, "1");
                    ui.selectable_value(&mut state.stop_bits, 2, "2");
                });
        });
    }
}

#[derive(PartialEq, Eq, Debug, Clone)]
enum ConnectionType {
    TCP(TcpConnect),
    ASCII(SerialConnect),
    RTU(SerialConnect),
}
impl ConnectionType {
    fn to_str(&self) -> String {
        match self {
            ConnectionType::TCP(_) => "TCP".to_string(),
            ConnectionType::ASCII(_) => "ASCII".to_string(),
            ConnectionType::RTU(_) => "RTU".to_string(),
        }
    }
}

struct GuiPortState {
    is_open: bool,
    port_type: ConnectionType,
    connection_tcp: TcpConnect,
    connection_ascii: SerialConnect,
    connection_rtu: SerialConnect,
}

impl GuiPortState {
    fn new() -> Self {
        let connection_tcp = TcpConnect::new();

        let connection_tcp2 = TcpConnect::new();

        let connection_ascii = SerialConnect::new(DataBits::Eight, Parity::Even);
        let connection_rtu = SerialConnect::new(DataBits::Eight, Parity::Even);
        let bud_rates = [1200, 2400, 4800, 9600, 19200, 38400, 57600, 115200];
        Self {
            is_open: false,
            port_type: ConnectionType::TCP(connection_tcp2),
            connection_tcp,
            connection_ascii,
            connection_rtu,
        }
    }
}
#[derive(Debug)]
struct GuiPort {
    port_type: ConnectionType,
}

impl GuiPort {
    fn new() -> Self {
        let connection = SerialConnect::new(DataBits::Eight, Parity::Even);
        Self {
            port_type: ConnectionType::RTU(connection),
        }
    }

    fn render(ui: &mut egui::Ui, state: &mut GuiPortState) -> Option<GuiPort> {
        let mut close = false;
        let mut port_connection = None;
        if state.is_open {
            let ctx = ui.ctx().clone();
            egui::Window::new("Port editor")
                .title_bar(true)
                .open(&mut state.is_open)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(&ctx, |ui| {
                    ui.label("Connection conf");
                    ui.separator();
                    ui.horizontal(|ui| {
                        let conn_label = ui.label("Connection type");
                        egui::ComboBox::from_id_salt(conn_label.id)
                            .selected_text(format!("{}", state.port_type.to_str()))
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut state.port_type,
                                    ConnectionType::TCP(state.connection_tcp.clone()),
                                    "TCP",
                                );
                                ui.selectable_value(
                                    &mut state.port_type,
                                    ConnectionType::RTU(state.connection_rtu.clone()),
                                    "RTU",
                                );
                                let connection_serial_ascii =
                                    SerialConnect::new(DataBits::Eight, Parity::Even);
                                ui.selectable_value(
                                    &mut state.port_type,
                                    ConnectionType::ASCII(connection_serial_ascii),
                                    "ASCII",
                                );
                            });
                    });
                    match &mut state.port_type {
                        ConnectionType::TCP(conn) => {
                            TcpConnect::render_edit_window(ui, conn);
                        }
                        ConnectionType::ASCII(conn) | ConnectionType::RTU(conn) => {
                            SerialConnect::render_edit_window(conn, ui)
                        }
                    };
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("save").clicked() {
                                println!("{:?}", state.port_type);
                                close = true;
                                port_connection = Some(GuiPort {
                                    port_type: state.port_type.clone(),
                                });
                            }
                            if ui.button("cancel").clicked() {
                                close = true;
                            }
                        });
                    });
                });
            if close {
                state.is_open = false;
            }
        }

        port_connection
    }
}

#[derive(Clone, Debug)]
struct DeviceCreationState {
    pub is_open: bool,
    pub show_advanced: bool, // Przełącznik trybu zaawansowanego (offsety)

    // Podstawowe dane
    pub name_buffer: String,
    pub device_id: u8,
    pub total_size_bytes: usize, // Całkowity rozmiar pamięci w bajtach

    // Liczba elementów (rejestrów / coili)
    pub holding_registers_count: u16,
    pub input_registers_count: u16,
    pub coils_count: u16,
    pub discrete_inputs_count: u16,

    // Offsety bajtowe (widoczne w trybie zaawansowanym)
    pub holding_offs: usize,
    pub input_offs: usize,
    pub coil_offs: usize,
    pub discrete_offs: usize,
}

impl DeviceCreationState {
    fn new() -> Self {
        Self {
            is_open: false,
            show_advanced: false,
            name_buffer: "Nowe Urządzenie".to_string(),
            device_id: 1,
            total_size_bytes: 64, // Domyślnie 64 bajty pamięci

            holding_registers_count: 10,
            input_registers_count: 10,
            coils_count: 16,
            discrete_inputs_count: 16,

            holding_offs: 0,
            input_offs: 20,
            coil_offs: 40,
            discrete_offs: 44,
        }
    }
}

struct GuiDeviceCreator;

impl GuiDeviceCreator {
    fn render(ctx: &egui::Context, state: &mut DeviceCreationState) -> Option<(u8, DynamicMemory)> {
        let mut created_result = None;
        let mut close = false;
        egui::Window::new("Create new device")
            .title_bar(true)
            .open(&mut state.is_open)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .resizable(true)
            .show(ctx, |ui| {
                ui.heading("Device configuration");
                ui.separator();

                // --- PODSTAWOWE DANE ---
                ui.horizontal(|ui| {
                    ui.label("Device name: ");
                    ui.text_edit_singleline(&mut state.name_buffer);
                });

                ui.horizontal(|ui| {
                    ui.label("Modbus Slave ID:");
                    ui.add(
                        egui::DragValue::new(&mut state.device_id)
                            .speed(1.0)
                            .range(1..=247),
                    );
                });

                ui.horizontal(|ui| {
                    ui.label("Mem capacity");
                    ui.add(
                        egui::DragValue::new(&mut state.total_size_bytes)
                            .speed(1.0)
                            .range(4..=4096),
                    );
                });

                ui.add_space(10.0);
                ui.separator();

                // --- ROZMIARI TABLIC ---
                ui.strong("Quantity of: ");
                ui.horizontal(|ui| {
                    ui.label("Holding Registers:");
                    ui.add(egui::DragValue::new(&mut state.holding_registers_count).speed(1.0));
                });
                ui.horizontal(|ui| {
                    ui.label("Input Registers:");
                    ui.add(egui::DragValue::new(&mut state.input_registers_count).speed(1.0));
                });
                ui.horizontal(|ui| {
                    ui.label("Coils:");
                    ui.add(egui::DragValue::new(&mut state.coils_count).speed(1.0));
                });
                ui.horizontal(|ui| {
                    ui.label("Discrete Inputs:");
                    ui.add(egui::DragValue::new(&mut state.discrete_inputs_count).speed(1.0));
                });

                ui.add_space(10.0);

                // --- TRYB ZAAWANSOWANY (OFFSETY BAJTOWE) ---
                ui.checkbox(&mut state.show_advanced, "Advanced mode");
                if state.show_advanced {
                    ui.indent("advanced_offsets", |ui| {
                        ui.horizontal(|ui| {
                            ui.label("Holding registers offset:");
                            ui.add(egui::DragValue::new(&mut state.holding_offs).speed(1.0));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Input Registers offset:");
                            ui.add(egui::DragValue::new(&mut state.input_offs).speed(1.0));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Coils offset:");
                            ui.add(egui::DragValue::new(&mut state.coil_offs).speed(1.0));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Discrete inputs offset:");
                            ui.add(egui::DragValue::new(&mut state.discrete_offs).speed(1.0));
                        });
                    });
                }

                ui.add_space(5.0);
                ui.strong("Memory Layout visualization");

                let available_width = ui.available_width() - 20.0;
                let track_height = 18.0; // Wysokość pojedynczego paska
                let num_tracks = 4; // Holding, Input, Coils, Discrete
                let total_height = (track_height * num_tracks as f32) + 12.0;

                let (rect, _response) = ui.allocate_exact_size(
                    egui::vec2(available_width, total_height),
                    egui::Sense::hover(),
                );

                let painter = ui.painter();
                // Tło całego kontenera wizualizacji
                painter.rect_filled(rect, 4.0, egui::Color32::from_gray(30));

                if state.total_size_bytes > 0 {
                    let byte_width = available_width / state.total_size_bytes as f32;

                    // Obliczamy rozmiary w bajtach dla każdego obszaru
                    let hr_bytes = (state.holding_registers_count as usize) * 2;
                    let ir_bytes = (state.input_registers_count as usize) * 2;
                    let coil_bytes = (state.coils_count as usize + 7) / 8;
                    let disc_bytes = (state.discrete_inputs_count as usize + 7) / 8;

                    // Funkcja pomocnicza do rysowania pojedynczej ścieżki (paska)
                    let draw_track = |track_idx: usize,
                                      start: usize,
                                      len: usize,
                                      color: egui::Color32,
                                      label: &str| {
                        let y_offset = rect.min.y + 6.0 + (track_idx as f32 * track_height);

                        // Etykieta po lewej stronie paska (wewnątrz)
                        painter.text(
                            egui::pos2(rect.min.x + 6.0, y_offset + (track_height / 2.0)),
                            egui::Align2::LEFT_CENTER,
                            label,
                            egui::FontId::proportional(10.0),
                            egui::Color32::WHITE,
                        );

                        let end = (start + len).min(state.total_size_bytes);
                        if start < state.total_size_bytes && end > start {
                            // Przesuwamy początek paska trochę w prawo, żeby zmieścić etykietę ścieżki lub rysujemy na pełnej szerokości
                            let x_start = rect.min.x + (start as f32 * byte_width);
                            let width = ((end - start) as f32 * byte_width).max(4.0);

                            let track_rect = egui::Rect::from_min_size(
                                egui::pos2(x_start, y_offset + 2.0),
                                egui::vec2(width, track_height - 4.0),
                            );

                            // Używamy półprzezroczystości (Color32::linear_multiply),
                            // dzięki czemu nałożone na siebie paski ładnie się prześwietlą!
                            let transparent_color = color.linear_multiply(0.85);
                            painter.rect_filled(track_rect, 2.0, transparent_color);
                            painter.rect_stroke(
                                track_rect,
                                2.0,
                                (1.0, color),
                                egui::StrokeKind::Inside,
                            ); // Wyraźna ramka
                        }
                    };

                    // Rysujemy każdy typ pamięci na osobnym poziomie pod sobą
                    draw_track(
                        0,
                        state.holding_offs,
                        hr_bytes,
                        egui::Color32::from_rgb(46, 134, 222),
                        "HR",
                    );
                    draw_track(
                        1,
                        state.input_offs,
                        ir_bytes,
                        egui::Color32::from_rgb(84, 160, 255),
                        "IR",
                    );
                    draw_track(
                        2,
                        state.coil_offs,
                        coil_bytes,
                        egui::Color32::from_rgb(46, 204, 113),
                        "Coils",
                    );
                    draw_track(
                        3,
                        state.discrete_offs,
                        disc_bytes,
                        egui::Color32::from_rgb(26, 188, 156),
                        "DI",
                    );
                }
                ui.add_space(15.0);

                // --- PRZYCISKI ---
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Save").clicked() {
                            let name = if state.name_buffer.trim().is_empty() {
                                "Urządzenie Modbus".to_string()
                            } else {
                                state.name_buffer.clone()
                            };

                            // Tworzymy nową pamięć dynamiczną
                            let mut memory = DynamicMemory::new(name, state.total_size_bytes);
                            memory.set_mapping(
                                state.holding_offs,
                                state.input_offs,
                                state.coil_offs,
                                state.discrete_offs,
                            );

                            created_result = Some((state.device_id, memory));
                            close = true;
                        }

                        if ui.button("Cancel").clicked() {
                            close = true;
                        }
                    });
                });
            });
        if close {
            state.is_open = false;
        }

        created_result
    }
}

pub struct MyApp {
    pub logs: GuiLoggerBuffer,
    pub reciever: mpsc::Receiver<String>,
    pub router: Arc<Mutex<ModbusRouter>>,
    // Window config
    left: bool,
    top: bool,
    bottom: bool,
    center: bool,
    // center view
    center_view: CenterView,
    // Watch table
    watch_table_state: WatchCeraetionState,
    watch_tables: Vec<WatchTable>,
    // port
    port_state: GuiPortState,
    port: Option<GuiPort>,

    // device
    devices_state: DeviceCreationState,
}

impl MyApp {
    pub fn new(
        message: mpsc::Receiver<String>,
        cc: &eframe::CreationContext<'_>,
        router: Arc<Mutex<ModbusRouter>>,
    ) -> Self {
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
            router,

            left: true,
            top: true,
            bottom: true,
            center: true,
            center_view: CenterView::WatchTable,
            watch_table_state: WatchCeraetionState::new(),
            watch_tables: Vec::new(),
            port_state: GuiPortState::new(),
            port: None,
            devices_state: DeviceCreationState::new(),
        }
    }

    fn update(&mut self) {
        while let Ok(log) = self.reciever.try_recv() {
            self.logs.push(log);
        }
    }

    fn render_watch_tables(&mut self, ui: &mut egui::Ui) {
        if ui.button("New table").clicked() {
            self.watch_table_state.is_open = true;
            self.watch_table_state.name_buffer =
                format!("Watch Table ({})", self.watch_tables.len() + 1);
        }
        if let Some(new_table) =
            WatchTable::render_creation_window(ui, &mut self.watch_table_state, &self.router)
        {
            self.watch_tables.push(new_table);
        }

        for watch_table in self.watch_tables.iter_mut() {
            watch_table.render(ui);
        }
    }

    fn render_top_panel(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("Top Panel")
            .resizable(true)
            .show_collapsible(ui, &mut self.top, |ui| {
                // opcje górne
                egui::MenuBar::new().ui(ui, |ui| {
                    ui.menu_button("Project", |ui| {
                        if ui.button("new").clicked() {
                            println!("nowy projekt");
                        }
                        if ui.button("Open").clicked() {
                            println!("otwóż projekt");
                        }
                        ui.separator();
                        if ui.button("Save").clicked() {
                            println!("Zapisz projekt");
                        }
                        if ui.button("Save as").clicked() {
                            println!("Zapipsz projekt jako");
                        }

                        ui.separator();
                        if ui.button("Quit").clicked() {
                            println!("Zamknij");
                        }
                    });
                    ui.menu_button("Server", |ui| {
                        if ui.button("New").clicked() {
                            self.port_state.is_open = true;
                        }
                        if ui.button("Edit connection").clicked() {
                            println!("edycja połączenia");
                        }
                        if ui.button("close connection").clicked() {
                            println!("cakończenie połączenia");
                        }
                    });
                    ui.menu_button("Device", |ui| {
                        if ui.button("New Device").clicked() {
                            self.devices_state.is_open = true;
                        }
                        if ui.button("Edit Device").clicked() {
                            println!("edytowanie urządzenia");
                        }
                        if ui.button("Delete Device").clicked() {
                            println!("Delete a device");
                        }
                    });

                    ui.menu_button("Simulation", |ui| {
                        if ui.button("Start Simulation").clicked() {
                            println!("Symulacja wystartowała");
                        }
                        if ui.button("Stop Simulation").clicked() {
                            println!("Symulacja się zatrzymała");
                        }
                        if ui.button("").clicked() {
                            println!("Symulacja się zatrzymała");
                        }
                    });

                    ui.menu_button("Watch table", |ui| {
                        if ui.button("Add watch table").clicked() {
                            println!("Watch table added");
                        }
                        if ui.button("Add row").clicked() {
                            println!("adding row to watch table");
                        }
                    });

                    ui.menu_button("View", |ui| {
                        ui.checkbox(&mut self.bottom, "Logs");
                        ui.checkbox(&mut self.left, "Devices");
                        ui.checkbox(&mut self.center, "Simulation");
                    });

                    ui.menu_button("help", |ui| {
                        if ui.button("See Documentation").clicked() {
                            println!("Documentation Seaker");
                        }
                    })
                });
                ui.separator();
                // TODO trzeba zastanowić się nad przyciskami szybkiego wyboru
                ui.horizontal_top(|ui| {
                    if ui.button("start simulation").clicked() {
                        println!("Simulation started");
                    }
                })
            });
        if self.port_state.is_open {
            if let Some(port) = GuiPort::render(ui, &mut self.port_state) {
                println!("{:?}", port);
            }
        }
        if self.devices_state.is_open {
            if let Some(device) = GuiDeviceCreator::render(ui, &mut self.devices_state) {
                println!("{:?}", device);
            }
        }
    }

    fn render_bottom_panel(&mut self, ui: &mut egui::Ui) {
        // log integration
        self.update();
        egui::Panel::bottom("Logi")
            .resizable(true)
            .show_collapsible(ui, &mut self.bottom, |ui| {
                ui.label("[LOGS]");
                ui.separator();
                egui::ScrollArea::vertical()
                    .auto_shrink(false)
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        for log in self.logs.get_logs() {
                            ui.label(log);
                        }
                    });
            });
    }

    fn render_left_panel(&mut self, ui: &mut egui::Ui) {
        // Device render
        egui::Panel::left("Devices")
            .resizable(true)
            .show_collapsible(ui, &mut self.left, |ui| {
                ui.label("urządzenia");
            });
    }
    fn render_central_panel(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.horizontal_top(|ui| {
                ui.selectable_value(
                    &mut self.center_view,
                    CenterView::WatchTable,
                    "Watch Tables",
                );
                ui.selectable_value(&mut self.center_view, CenterView::NodeEditor, "Node Editor");
                ui.selectable_value(&mut self.center_view, CenterView::Plots, "Plots Editor");
            });
            ui.separator();
            match self.center_view {
                CenterView::WatchTable => {
                    self.render_watch_tables(ui);
                }
                CenterView::NodeEditor => {}
                CenterView::Plots => {}
            }
        });
    }
}

impl eframe::App for MyApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.render_top_panel(ui);
        self.render_bottom_panel(ui);
        self.render_left_panel(ui);
        self.render_central_panel(ui);
    }
}
