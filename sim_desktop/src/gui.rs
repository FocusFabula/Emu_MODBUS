use crate::{GuiLoggerBuffer, ModbusRouter};
use eframe::egui;
use egui_extras::{Column, TableBuilder};
use serialport::{self, DataBits, Parity};
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

struct TcpConnect {
    ip: String,
    port: usize,
}

impl TcpConnect {
    fn new(ip: String, port: usize) -> Self {
        Self { ip, port }
    }

    fn to_string(&self) -> String {
        format!("{}:{}", self.ip, self.port)
    }
}

struct SerialConnect {
    is_open: bool,
    scan: bool,
    avalible_ports: Vec<String>,
    port_id: usize,
    serial_port: String,
    bud_rate_selected: usize,
    bud_rate: [usize; 8],
    data_bits: DataBits,
    parity: Parity,
    stop_bits: usize,
}

impl SerialConnect {
    fn new(serial_port: String, data_bits: DataBits, parity: Parity) -> Self {
        let bud_rate = [1200, 2400, 4800, 9600, 19200, 38400, 57600, 115200];
        Self {
            // GUI parameters
            is_open: false,
            scan: false,
            avalible_ports: Vec::new(),
            port_id: 0,
            // serial parameters
            serial_port,
            bud_rate,
            bud_rate_selected: bud_rate[3],
            data_bits,
            parity,
            stop_bits: 2,
        }
    }

    fn render_edit_window(state: &mut SerialConnect, ui: egui::Ui) {
        let mut close = false;
        if state.is_open {
            let ctx = ui.ctx().clone();
            egui::Window::new("Port editor")
                .title_bar(true)
                .open(&mut state.is_open)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(&ctx, |ui| {
                    ui.horizontal(|ui| {
                        let port_type_label = ui.label("Serial Port");
                        ui.add(egui::TextEdit::singleline(&mut state.serial_port))
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
                                });
                        }
                    });
                });
        }
    }
}

enum ConnectionType {
    TCP(TcpConnect),
    ASCII(SerialConnect),
    RTU(SerialConnect),
}

struct GuiPort {
    port_type: ConnectionType,
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
        }
    }

    fn update(&mut self) {
        while let Ok(log) = self.reciever.try_recv() {
            self.logs.push(log);
        }
    }

    fn render_watch_tables(&mut self, ui: &mut egui::Ui) {
        if ui.button("nowa tabela").clicked() {
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
                            println!("Nowe połączenie");
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
                            println!("Nowe urządzenie");
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
