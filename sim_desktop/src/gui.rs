use crate::{DynamicMemory, GuiLoggerBuffer, ModbusDevice, ModbusRouter};
use eframe::egui;
use egui_extras::{Column, TableBuilder};
use serde::{Deserialize, Serialize};
use serialport::{self, DataBits, Parity};
use std::path::{Path, PathBuf};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CellDataType {
    Coil,
    DiscreteInput,
    #[serde(alias = "Register")]
    HoldingRegister,
    InputRegister,
}

impl CellDataType {
    pub fn is_writable(&self) -> bool {
        matches!(self, Self::Coil | Self::HoldingRegister)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CellFormat {
    DEC,
    BIN,
    HEX,
}

impl CellFormat {
    pub fn parse_value(&self, raw: &str, datatype: &CellDataType) -> Result<u16, &'static str> {
        let cleaned: String = raw
            .trim()
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '_')
            .collect();

        if cleaned.is_empty() {
            return Err("Pusta wartość");
        }

        let parsed: u16 = if let Some(hex) = cleaned
            .strip_prefix("0x")
            .or_else(|| cleaned.strip_prefix("0X"))
        {
            u16::from_str_radix(hex, 16).map_err(|_| "Błędny zapis HEX")?
        } else if let Some(bin) = cleaned
            .strip_prefix("0b")
            .or_else(|| cleaned.strip_prefix("0B"))
        {
            u16::from_str_radix(bin, 2).map_err(|_| "Błędny zapis BIN")?
        } else {
            match self {
                CellFormat::DEC => cleaned
                    .parse::<u16>()
                    .map_err(|_| "Błędna liczba DEC (0..65535)")?,
                CellFormat::HEX => {
                    u16::from_str_radix(&cleaned, 16).map_err(|_| "Błędna liczba HEX")?
                }
                CellFormat::BIN => {
                    u16::from_str_radix(&cleaned, 2).map_err(|_| "Błędna liczba BIN")?
                }
            }
        };

        match datatype {
            CellDataType::Coil => Ok(if parsed != 0 { 1 } else { 0 }),
            CellDataType::HoldingRegister
            | CellDataType::DiscreteInput
            | CellDataType::InputRegister => Ok(parsed),
        }
    }

    pub fn format_value(&self, val: u16, datatype: &CellDataType) -> String {
        match datatype {
            CellDataType::Coil | CellDataType::DiscreteInput => {
                let bit = if val != 0 { 1 } else { 0 };
                match self {
                    CellFormat::DEC => format!("{}", bit),
                    CellFormat::BIN => format! {"0b{:08b}", bit},
                    CellFormat::HEX => format! {"0x{:02x}", bit},
                }
            }
            _ => match self {
                CellFormat::DEC => format!("{}", val),
                CellFormat::BIN => format! {"0b{:016b}", val},
                CellFormat::HEX => format! {"0x{:04x}", val},
            },
        }
    }
}

#[derive(Copy, Clone, PartialEq, Debug, Serialize, Deserialize)]
pub enum ActiveCol {
    DeviceName,
    RowId,
    MemAddr,
    DataType,
    Comment,
    Value,
    Format,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchTableRow {
    pub device: u8,
    pub id: usize,
    pub mem_addr: u16,
    pub datatype: CellDataType,
    pub value: u16,
    pub format: CellFormat,
    pub comment: String,
    #[serde(skip)]
    pub selected: bool,
    #[serde(skip)]
    pub edit_buffer: Option<String>,
    #[serde(skip)]
    pub edit_error: Option<&'static str>,
}

impl WatchTableRow {
    fn new(
        device: u8,
        id: usize,
        mem_addr: u16,
        datatype: CellDataType,
        format: CellFormat,
        comment: String,
    ) -> Self {
        let value: u16 = 0;

        Self {
            device,
            id,
            mem_addr,
            datatype,
            value,
            format,
            comment,
            selected: false,
            edit_buffer: None,
            edit_error: None,
        }
    }

    fn memory_index(&self) -> Option<usize> {
        if self.mem_addr == 0 {
            None
        } else {
            Some((self.mem_addr - 1) as usize)
        }
    }

    fn begin_edit(&mut self) {
        let initial = match self.datatype {
            CellDataType::Coil => {
                let bit = if self.value != 0 { 1 } else { 0 };
                match self.format {
                    CellFormat::DEC => format!("{}", bit),
                    CellFormat::BIN => format!("{:08b}", bit),
                    CellFormat::HEX => format!("{:02x}", bit),
                }
            }
            _ => match self.format {
                CellFormat::DEC => format!("{}", self.value),
                CellFormat::BIN => format!("{:016b}", self.value),
                CellFormat::HEX => format!("{:04x}", self.value),
            },
        };
        self.edit_buffer = Some(initial);
        self.edit_error = None;
    }

    pub fn sync_from_router(&mut self, router: &ModbusRouter) {
        let Some(dev) = router.get_devices().get(&self.device) else {
            return;
        };

        let Some(idx) = self.memory_index() else {
            self.value = 0;
            self.edit_error = Some("Adres musi być >= 1");
            return;
        };

        match self.datatype {
            CellDataType::HoldingRegister => {
                if let Ok(val) = dev.get_holding_register(idx as u16) {
                    self.value = val;
                    self.edit_error = None;
                } else {
                    self.value = 0;
                    self.edit_error = Some("Nieprawidłowy adres Holding Register");
                }
            }
            CellDataType::InputRegister => {
                // Dopasuj do nazwy metody na DynamicMemory, np. get_input_register(idx as u16)
                if let Ok(val) = dev.get_input_register(idx as u16) {
                    self.value = val;
                    self.edit_error = None;
                } else {
                    self.value = 0;
                    self.edit_error = Some("Nieprawidłowy adres Input Register");
                }
            }
            CellDataType::Coil => {
                if let Ok(val) = dev.get_coil(idx as u16) {
                    self.value = if val { 1 } else { 0 };
                    self.edit_error = None;
                } else {
                    self.value = 0;
                    self.edit_error = Some("Nieprawidłowy adres Coil");
                }
            }
            CellDataType::DiscreteInput => {
                // Dopasuj do nazwy metody na DynamicMemory, np. get_discrete_input(idx as u16)
                if let Ok(val) = dev.get_discrete_input(idx as u16) {
                    self.value = if val { 1 } else { 0 };
                    self.edit_error = None;
                } else {
                    self.value = 0;
                    self.edit_error = Some("Nieprawidłowy adres Discrete Input");
                }
            }
        }
    }

    pub fn write_to_router(&self, router: &mut ModbusRouter) {
        if !self.datatype.is_writable() {
            return;
        }

        let Some(dev) = router.get_devices_mut().get_mut(&self.device) else {
            return;
        };

        let Some(idx) = self.memory_index() else {
            return;
        };

        match self.datatype {
            CellDataType::HoldingRegister => {
                let _ = dev.write_holding_register(idx as u16, self.value);
            }
            CellDataType::Coil => {
                let _ = dev.write_coil(idx as u16, self.value != 0);
            }
            CellDataType::InputRegister | CellDataType::DiscreteInput => {}
        }
    }
}
#[derive(Debug, Clone)]
pub struct AddRowDraft {
    pub is_open: bool,
    pub device_id: u8,
    pub mem_addr: u16,
    pub datatype: CellDataType,
    pub format: CellFormat,
    pub comment: String,
}

impl Default for AddRowDraft {
    fn default() -> Self {
        Self {
            is_open: false,
            device_id: 1,
            mem_addr: 0,
            datatype: CellDataType::HoldingRegister,
            format: CellFormat::DEC,
            comment: String::new(),
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
                ActiveCol::MemAddr,
                ActiveCol::Value,
                ActiveCol::Format,
            ],
            name_buffer: "New table".to_string(),
            period_buffer: 500,
            device_id: 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RowSelectTarget {
    Format,
    Device,
}

#[derive(Debug, Clone)]
pub struct RowSelectModal {
    pub row_idx: usize,
    target: RowSelectTarget,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WatchTable {
    pub name: String,
    pub rows: Vec<WatchTableRow>,
    pub period: Duration,
    pub active_cols: Vec<ActiveCol>,
    #[serde(skip)]
    pub last_sync: Option<std::time::Instant>,
    #[serde(skip)]
    pub add_row_draft: AddRowDraft,
    /// Akcja z menu kontekstowego na poziomie całej tabeli (np. usunięcie tabeli)
    #[serde(skip)]
    pub delete_requested: bool,
    #[serde(skip)]
    pub row_select_modal: Option<RowSelectModal>,
    #[serde(skip)]
    pub row_ctx_menu: Option<(usize, egui::Pos2)>,
    #[serde(skip)]
    pub dirty_project: bool,
}

impl WatchTable {
    fn new(name: String, period: Duration, active_cols: Vec<ActiveCol>) -> Self {
        let rows = Vec::new();
        Self {
            name,
            rows,
            period,
            active_cols,
            last_sync: None,
            add_row_draft: AddRowDraft::default(),
            delete_requested: false,
            row_select_modal: None,
            row_ctx_menu: None,
            dirty_project: false,
        }
    }

    fn open_add_row(&mut self, router: &ModbusRouter) {
        let first_id = router.get_devices().keys().next().copied().unwrap_or(1);
        self.add_row_draft = AddRowDraft {
            is_open: true,
            device_id: first_id,
            mem_addr: 0,
            datatype: CellDataType::HoldingRegister,
            format: CellFormat::DEC,
            comment: String::new(),
        };
    }

    fn sync_rows(&mut self, router: &ModbusRouter) {
        for row in &mut self.rows {
            if row.edit_buffer.is_none() {
                row.sync_from_router(router);
            }
        }
    }

    fn render_add_row_window(&mut self, ctx: &egui::Context, router: &ModbusRouter) {
        if !self.add_row_draft.is_open {
            return;
        }

        let devices: Vec<(u8, String)> = {
            let mut list: Vec<(u8, String)> = router
                .get_devices()
                .iter()
                .map(|(id, dev)| (*id, dev.name.clone()))
                .collect();
            list.sort_by_key(|(id, _)| *id);
            list
        };

        let mut close = false;
        let mut new_row: Option<WatchTableRow> = None;

        egui::Window::new(format!("Dodaj wiersz — {}", self.name))
            .collapsible(false)
            .resizable(false)
            .open(&mut self.add_row_draft.is_open)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                if devices.is_empty() {
                    ui.colored_label(
                        egui::Color32::LIGHT_RED,
                        "Brak urządzeń w routerze. Najpierw dodaj urządzenie.",
                    );
                    if ui.button("Zamknij").clicked() {
                        close = true;
                    }
                    return;
                }

                // Zapewnij, że wybrane urządzenie istnieje
                if !devices
                    .iter()
                    .any(|(id, _)| *id == self.add_row_draft.device_id)
                {
                    self.add_row_draft.device_id = devices[0].0;
                }

                let current_name = devices
                    .iter()
                    .find(|(id, _)| *id == self.add_row_draft.device_id)
                    .map(|(id, name)| format!("[ID {}] {}", id, name))
                    .unwrap_or_default();

                ui.horizontal(|ui| {
                    ui.label("Urządzenie:");
                    egui::ComboBox::from_id_salt(("wt_add_dev", &self.name))
                        .selected_text(current_name)
                        .show_ui(ui, |ui| {
                            for (id, name) in &devices {
                                ui.selectable_value(
                                    &mut self.add_row_draft.device_id,
                                    *id,
                                    format!("[ID {}] {}", id, name),
                                );
                            }
                        });
                });

                ui.horizontal(|ui| {
                    ui.label("Adres pamięci:");
                    ui.add(
                        egui::DragValue::new(&mut self.add_row_draft.mem_addr)
                            .speed(1.0)
                            .range(0..=65535),
                    );
                });

                ui.horizontal(|ui| {
                    ui.label("Typ danych:");
                    egui::ComboBox::from_id_salt(("wt_add_dtype", &self.name))
                        .selected_text(format!("{:?}", self.add_row_draft.datatype))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.add_row_draft.datatype,
                                CellDataType::HoldingRegister,
                                "HoldingRegister",
                            );
                            ui.selectable_value(
                                &mut self.add_row_draft.datatype,
                                CellDataType::InputRegister,
                                "InputRegister",
                            );
                            ui.selectable_value(
                                &mut self.add_row_draft.datatype,
                                CellDataType::DiscreteInput,
                                "DiscreteInput",
                            );

                            ui.selectable_value(
                                &mut self.add_row_draft.datatype,
                                CellDataType::Coil,
                                "Coil",
                            );
                        });
                });

                ui.horizontal(|ui| {
                    ui.label("Format:");
                    egui::ComboBox::from_id_salt(("wt_add_fmt", &self.name))
                        .selected_text(format!("{:?}", self.add_row_draft.format))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.add_row_draft.format,
                                CellFormat::DEC,
                                "DEC",
                            );
                            ui.selectable_value(
                                &mut self.add_row_draft.format,
                                CellFormat::BIN,
                                "BIN",
                            );
                            ui.selectable_value(
                                &mut self.add_row_draft.format,
                                CellFormat::HEX,
                                "HEX",
                            );
                        });
                });

                ui.horizontal(|ui| {
                    ui.label("Komentarz:");
                    ui.text_edit_singleline(&mut self.add_row_draft.comment);
                });

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Dodaj").clicked() {
                        let next_row_id = self.rows.len() + 1;
                        new_row = Some(WatchTableRow::new(
                            self.add_row_draft.device_id,
                            next_row_id,
                            self.add_row_draft.mem_addr,
                            self.add_row_draft.datatype,
                            self.add_row_draft.format,
                            self.add_row_draft.comment.clone(),
                        ));
                        close = true;
                    }
                    if ui.button("Anuluj").clicked() {
                        close = true;
                    }
                });
            });

        if let Some(mut row) = new_row {
            row.sync_from_router(router);
            self.dirty_project = true;
            self.rows.push(row);
        }
        if close {
            self.add_row_draft.is_open = false;
        }
    }

    fn render(
        &mut self,
        ui: &mut egui::Ui,
        router_arc: &Arc<Mutex<ModbusRouter>>,
        dirty: &mut bool,
    ) {
        // Okresowy odczyt danych z ModbusRouter zgodnie z `self.period`
        let now = std::time::Instant::now();
        let should_poll = match self.last_sync {
            None => true,
            Some(prev) => now.duration_since(prev) >= self.period,
        };
        if should_poll {
            let router = router_arc.lock().unwrap();
            self.sync_rows(&router);
            self.last_sync = Some(now);
        }

        if *dirty && self.dirty_project {
            self.dirty_project = false;
        }
        if !*dirty && self.dirty_project {
            *dirty = self.dirty_project;
        }

        // Render okna dodawania wiersza (jeśli otwarte)
        {
            let router = router_arc.lock().unwrap();
            let ctx = ui.ctx().clone();
            self.render_add_row_window(&ctx, &router);
        }

        let mut action_add_row = false;
        let mut action_delete_table = false;
        let mut action_delete_col: Option<ActiveCol> = None;
        let mut action_delete_row_idx: Option<usize> = None;
        let mut action_delete_selected_rows = false;
        let mut row_to_write_back: Option<usize> = None;
        let mut row_to_sync_idx: Option<usize> = None;
        let mut action_duplicate_row_idx: Option<usize> = None;

        let any_selected = self.rows.iter().any(|r| r.selected);

        let header_res =
            egui::CollapsingHeader::new(format!("📊 {} (Okres: {:?})", self.name, self.period))
                .default_open(true)
                .show(ui, |ui| {
                    if self.active_cols.is_empty() {
                        ui.colored_label(
                            egui::Color32::GRAY,
                            "Brak aktywnych kolumn w tej tabeli.",
                        );
                        return;
                    }
                    let mut all_selected =
                        !self.rows.is_empty() && self.rows.iter().all(|r| r.selected);

                    let available_height = 250.0;
                    let pointer_pos = ui.ctx().pointer_latest_pos();
                    let secondary_just_clicked = ui.input(|i| i.pointer.secondary_clicked());
                    let mut rows_to_highlight: Vec<(egui::Rect, egui::Color32)> = Vec::new();

                    TableBuilder::new(ui)
                        .striped(true)
                        .resizable(true)
                        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                        .min_scrolled_height(0.0)
                        .max_scroll_height(available_height)
                        // +1 kolumna na checkbox zaznaczania wiersza
                        .column(Column::exact(24.0))
                        .columns(Column::auto().at_least(85.0), self.active_cols.len())
                        .header(22.0, |mut header| {
                            header.col(|ui| {
                                if ui.checkbox(&mut all_selected, "").changed() {
                                    for row in &mut self.rows {
                                        row.selected = all_selected;
                                    }
                                }
                            });

                            for (col_idx, col) in self.active_cols.iter().enumerate() {
                                header.col(|ui| {
                                    let name = match col {
                                        ActiveCol::DeviceName => "Urządzenie",
                                        ActiveCol::RowId => "ID",
                                        ActiveCol::DataType => "Typ",
                                        ActiveCol::MemAddr => "Adres w pamięci",
                                        ActiveCol::Comment => "Komentarz",
                                        ActiveCol::Value => "Wartość",
                                        ActiveCol::Format => "Format",
                                    };
                                    let resp = ui.strong(name);

                                    // PPM na nagłówku kolumny -> usunięcie konkretnej kolumny
                                    resp.context_menu(|ui| {
                                        ui.label(
                                            egui::RichText::new(format!("Kolumna: {}", name))
                                                .strong(),
                                        );
                                        ui.separator();
                                        if ui
                                            .button(
                                                egui::RichText::new("🗑 Usuń tę kolumnę")
                                                    .color(egui::Color32::LIGHT_RED),
                                            )
                                            .clicked()
                                        {
                                            action_delete_col = Some(*col);
                                            ui.close();
                                        }
                                    });
                                });
                            }
                        })
                        .body(|mut body| {
                            let router_names: std::collections::BTreeMap<u8, String> = {
                                let router = router_arc.lock().unwrap();
                                router
                                    .get_devices()
                                    .iter()
                                    .map(|(id, d)| (*id, d.name.clone()))
                                    .collect()
                            };
                            let _available_ids: Vec<u8> = router_names.keys().copied().collect();

                            for (row_idx, row_data) in self.rows.iter_mut().enumerate() {
                                let row_resp = body.row(24.0, |mut row| {
                                    let is_menu_target = self
                                        .row_ctx_menu
                                        .as_ref()
                                        .is_some_and(|(idx, _)| *idx == row_idx);

                                    let mut chk_hovered = false;
                                    let mut chk_secondary = false;

                                    // Kolumna zaznaczenia wiersza
                                    row.col(|ui| {
                                        let chk = ui.checkbox(&mut row_data.selected, "");
                                        chk_hovered = chk.hovered();
                                        chk_secondary = chk.secondary_clicked();
                                    });

                                    let mut any_cell_hovered = chk_hovered;
                                    let mut any_cell_secondary = chk_secondary;
                                    let mut secondary_pos: Option<egui::Pos2> = None;

                                    for (col_idx, col) in self.active_cols.iter().enumerate() {
                                        row.col(|ui| {
                                            let cell_rect = ui.available_rect_before_wrap();
                                            let cell_resp = match col {
                                                ActiveCol::DeviceName => {
                                                    let current_label = router_names
                                                        .get(&row_data.device)
                                                        .map(|name| {
                                                            format!(
                                                                "[{}] {}",
                                                                row_data.device, name
                                                            )
                                                        })
                                                        .unwrap_or_else(|| {
                                                            format!(
                                                                "Slave {} (brak)",
                                                                row_data.device
                                                            )
                                                        });

                                                    let resp = ui.button(current_label);
                                                    if resp.clicked() {
                                                        self.row_select_modal =
                                                            Some(RowSelectModal {
                                                                row_idx,
                                                                target: RowSelectTarget::Device,
                                                            });
                                                    }
                                                    resp
                                                }
                                                ActiveCol::MemAddr => {
                                                    let addr_resp = ui.add(
                                                        egui::DragValue::new(
                                                            &mut row_data.mem_addr,
                                                        )
                                                        .speed(1.0)
                                                        .range(0..=65535),
                                                    );
                                                    if addr_resp.changed() {
                                                        let router = router_arc.lock().unwrap();
                                                        row_data.sync_from_router(&router);
                                                    }
                                                    addr_resp
                                                }
                                                ActiveCol::RowId => ui.label(format!(
                                                    "{} (addr {})",
                                                    row_data.id, row_data.mem_addr
                                                )),

                                                ActiveCol::DataType => {
                                                    ui.label(format!("{:?}", row_data.datatype))
                                                }
                                                ActiveCol::Comment => ui.add(
                                                    egui::TextEdit::singleline(
                                                        &mut row_data.comment,
                                                    )
                                                    .desired_width(100.0),
                                                ),
                                                ActiveCol::Value => {
                                                    let mut commit_row = false;

                                                    if let Some(buf) = row_data.edit_buffer.as_mut()
                                                    {
                                                        let edit_id = ui.make_persistent_id((
                                                            "wt_val_edit",
                                                            &self.name,
                                                            row_idx,
                                                        ));
                                                        let te = ui.add(
                                                            egui::TextEdit::singleline(buf)
                                                                .id(edit_id)
                                                                .desired_width(115.0)
                                                                .font(egui::TextStyle::Monospace),
                                                        );

                                                        // Od razu ustaw fokus na polu edycji po wejściu
                                                        if te.gained_focus() {
                                                            row_data.edit_error = None;
                                                        } else if !te.has_focus() {
                                                            ui.memory_mut(|m| {
                                                                m.request_focus(edit_id)
                                                            });
                                                        }

                                                        let enter_pressed = ui.input(|i| {
                                                            i.key_pressed(egui::Key::Enter)
                                                        });
                                                        let esc_pressed = ui.input(|i| {
                                                            i.key_pressed(egui::Key::Escape)
                                                        });

                                                        if esc_pressed {
                                                            row_data.edit_buffer = None;
                                                            row_data.edit_error = None;
                                                        } else if enter_pressed || te.lost_focus() {
                                                            match row_data.format.parse_value(
                                                                buf,
                                                                &row_data.datatype,
                                                            ) {
                                                                Ok(new_val) => {
                                                                    row_data.value = new_val;
                                                                    row_data.edit_buffer = None;
                                                                    row_data.edit_error = None;
                                                                    commit_row = true;
                                                                }
                                                                Err(err_msg) => {
                                                                    row_data.edit_error =
                                                                        Some(err_msg);
                                                                    // Zostawiamy fokus i bufor, żeby użytkownik mógł poprawić literówkę
                                                                    ui.memory_mut(|m| {
                                                                        m.request_focus(edit_id)
                                                                    });
                                                                }
                                                            }
                                                        }

                                                        if let Some(err) = row_data.edit_error {
                                                            te.clone().on_hover_text(err);
                                                        }

                                                        if commit_row {
                                                            row_to_write_back = Some(row_idx);
                                                        }
                                                        te
                                                    } else {
                                                        let formatted =
                                                            row_data.format.format_value(
                                                                row_data.value,
                                                                &row_data.datatype,
                                                            );
                                                        let btn = ui.add(
                                                            egui::Button::new(
                                                                egui::RichText::new(formatted)
                                                                    .monospace()
                                                                    .strong(),
                                                            )
                                                            .frame(false),
                                                        );
                                                        if btn.clicked() {
                                                            row_data.begin_edit();
                                                        }
                                                        btn
                                                    }
                                                }
                                                ActiveCol::Format => {
                                                    let fmt_label = match row_data.format {
                                                        CellFormat::DEC => "DEC",
                                                        CellFormat::BIN => "BIN",
                                                        CellFormat::HEX => "HEX",
                                                    };

                                                    let resp = ui.button(fmt_label);
                                                    if resp.clicked() {
                                                        self.row_select_modal =
                                                            Some(RowSelectModal {
                                                                row_idx,
                                                                target: RowSelectTarget::Format,
                                                            });
                                                    }
                                                    resp
                                                }
                                            };

                                            if cell_resp.hovered() {
                                                any_cell_hovered = true;
                                            }

                                            cell_resp.context_menu(|ui| {
                                                ui.label(
                                                    egui::RichText::new(format!(
                                                        "Wiersz #{} (ID {})",
                                                        row_data.id, row_data.device
                                                    ))
                                                    .strong(),
                                                );
                                                ui.separator();
                                                if ui.button("➕ Dodaj wiersz").clicked() {
                                                    action_add_row = true;
                                                    ui.close();
                                                }

                                                if ui.button("📋 Duplikuj wiersz").clicked() {
                                                    action_duplicate_row_idx = Some(row_idx);
                                                    ui.close();
                                                }

                                                if any_selected {
                                                    if ui
                                                        .button(
                                                            egui::RichText::new(
                                                                "🗑 Usuń zaznaczone wiersze",
                                                            )
                                                            .color(egui::Color32::LIGHT_RED),
                                                        )
                                                        .clicked()
                                                    {
                                                        action_delete_selected_rows = true;
                                                        ui.close();
                                                    }
                                                }
                                                if ui
                                                    .button(
                                                        egui::RichText::new("🗑 Usuń ten wiersz")
                                                            .color(egui::Color32::LIGHT_RED),
                                                    )
                                                    .clicked()
                                                {
                                                    action_delete_row_idx = Some(row_idx);
                                                    ui.close();
                                                }
                                            });

                                            // Przechwyt PPM na całym prostokącie komórki bez blokowania lewego kliknięcia / drag
                                            // let row_ctx_id = ui.make_persistent_id((
                                            //     "wt_row_ctx_menu",
                                            //     &self.name,
                                            //     row_idx,
                                            //     col_idx,
                                            // ));

                                            // sec_resp.context_menu(|ui| {
                                            //     ui.label(
                                            //         egui::RichText::new(format!(
                                            //             "Wiersz #{} (ID {})",
                                            //             row_data.id, row_data.device
                                            //         ))
                                            //         .strong(),
                                            //     );
                                            //     ui.separator();
                                            //     if ui.button("➕ Dodaj wiersz").clicked() {
                                            //         action_add_row = true;
                                            //         ui.close();
                                            //     }
                                            //     if any_selected {
                                            //         if ui
                                            //             .button(
                                            //                 egui::RichText::new(
                                            //                     "🗑 Usuń zaznaczone wiersze",
                                            //                 )
                                            //                 .color(egui::Color32::LIGHT_RED),
                                            //             )
                                            //             .clicked()
                                            //         {
                                            //             action_delete_selected_rows = true;
                                            //             ui.close();
                                            //         }
                                            //     }
                                            //     if ui
                                            //         .button(
                                            //             egui::RichText::new("🗑 Usuń ten wiersz")
                                            //                 .color(egui::Color32::LIGHT_RED),
                                            //         )
                                            //         .clicked()
                                            //     {
                                            //         action_delete_row_idx = Some(row_idx);
                                            //         ui.close();
                                            //     }
                                            // });
                                        });
                                    }

                                    let row_rect = row.response().rect;

                                    let pointer_in_row =
                                        pointer_pos.is_some_and(|p| row_rect.contains(p));
                                    if pointer_in_row && secondary_just_clicked {
                                        self.row_ctx_menu = Some((
                                            row_idx,
                                            pointer_pos.unwrap_or_else(|| row_rect.left_bottom()),
                                        ));
                                    }

                                    let row_hovered = any_cell_hovered
                                        || row.response().hovered()
                                        || pointer_in_row;

                                    if row_hovered || is_menu_target || row_data.selected {
                                        let fill = if is_menu_target {
                                            egui::Color32::from_rgba_unmultiplied(90, 140, 220, 45)
                                        } else if row_hovered {
                                            egui::Color32::from_rgba_unmultiplied(255, 255, 255, 18)
                                        } else {
                                            egui::Color32::from_rgba_unmultiplied(90, 180, 120, 25)
                                        };
                                        rows_to_highlight.push((row_rect, fill));
                                    }

                                    if any_cell_secondary
                                        || row.response().secondary_clicked()
                                        || (secondary_just_clicked && pointer_in_row)
                                    {
                                        let pos = secondary_pos
                                            .or(pointer_pos)
                                            .unwrap_or_else(|| row_rect.left_bottom());
                                        self.row_ctx_menu = Some((row_idx, pos));
                                    }
                                });
                            }
                        });

                    for (rect, fill) in rows_to_highlight {
                        ui.painter().rect_filled(rect, 2.0, fill);
                    }
                });

        header_res.header_response.context_menu(|ui| {
            ui.label(egui::RichText::new(&self.name).strong());
            ui.separator();
            if ui.button("➕ Dodaj wiersz").clicked() {
                action_add_row = true;
                ui.close();
            }
            if any_selected {
                if ui
                    .button(
                        egui::RichText::new("🗑 Usuń zaznaczone wiersze")
                            .color(egui::Color32::LIGHT_RED),
                    )
                    .clicked()
                {
                    action_delete_selected_rows = true;
                    ui.close();
                }
            }
            ui.separator();
            if ui
                .button(egui::RichText::new("❌ Usuń tabelę").color(egui::Color32::LIGHT_RED))
                .clicked()
            {
                action_delete_table = true;
                ui.close();
            }
        });

        // Okienko wyboru Format / Device poza TableBuilder
        if let Some(modal) = self.row_select_modal.clone() {
            let mut open = true;
            let mut close = false;

            let title = match modal.target {
                RowSelectTarget::Format => "Wybierz format",
                RowSelectTarget::Device => "Wybierz urządzenie",
            };

            egui::Window::new(title)
                .collapsible(false)
                .resizable(false)
                .open(&mut open)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ui.ctx(), |ui| {
                    let Some(row) = self.rows.get_mut(modal.row_idx) else {
                        close = true;
                        return;
                    };

                    match modal.target {
                        RowSelectTarget::Format => {
                            for (fmt, name) in [
                                (CellFormat::DEC, "DEC"),
                                (CellFormat::BIN, "BIN"),
                                (CellFormat::HEX, "HEX"),
                            ] {
                                if ui.selectable_label(row.format == fmt, name).clicked() {
                                    row.format = fmt;
                                    self.dirty_project = true;
                                    close = true;
                                }
                            }
                        }
                        RowSelectTarget::Device => {
                            let devices: Vec<(u8, String)> = {
                                let router = router_arc.lock().unwrap();
                                let mut list: Vec<(u8, String)> = router
                                    .get_devices()
                                    .iter()
                                    .map(|(id, dev)| (*id, dev.name.clone()))
                                    .collect();
                                list.sort_by_key(|(id, _)| *id);
                                list
                            };

                            for (id, name) in devices {
                                let label = format!("[{}] {}", id, name);
                                if ui.selectable_label(row.device == id, label).clicked() {
                                    row.device = id;
                                    row_to_sync_idx = Some(modal.row_idx);
                                    close = true;
                                }
                            }
                        }
                    }

                    ui.separator();
                    if ui.button("Zamknij").clicked() {
                        close = true;
                    }
                });

            if close || !open {
                self.row_select_modal = None;
            }
        }

        if let Some(idx) = row_to_sync_idx {
            if let Some(row) = self.rows.get_mut(idx) {
                let router = router_arc.lock().unwrap();
                row.sync_from_router(&router);
            }
        }

        if let Some(idx) = row_to_write_back {
            if let Some(row) = self.rows.get_mut(idx) {
                let mut router = router_arc.lock().unwrap();
                row.write_to_router(&mut router);
            }
        }

        if action_add_row {
            let router = router_arc.lock().unwrap();
            self.open_add_row(&router);
        }

        if let Some(idx) = action_duplicate_row_idx {
            if let Some(source) = self.rows.get(idx).cloned() {
                let next_id = self.rows.iter().map(|r| r.id).max().unwrap_or(0) + 1;
                let mut duplicated = source;
                duplicated.id = next_id;
                duplicated.mem_addr = duplicated.mem_addr.saturating_add(1);
                duplicated.selected = false;
                duplicated.edit_buffer = None;
                duplicated.edit_error = None;

                {
                    let router = router_arc.lock().unwrap();
                    duplicated.sync_from_router(&router);
                }

                self.rows.insert(idx + 1, duplicated);
            }
        }

        if let Some(col_to_remove) = action_delete_col {
            self.active_cols.retain(|c| *c != col_to_remove);
        }

        if let Some(idx) = action_delete_row_idx {
            if idx < self.rows.len() {
                self.rows.remove(idx);
                self.row_ctx_menu = None;
                self.row_select_modal = None;
                self.dirty_project = true;
            }
        }

        if action_delete_selected_rows {
            self.rows.retain(|r| !r.selected);
            self.row_ctx_menu = None;
            self.row_select_modal = None;
            self.dirty_project = true;
        }

        if action_delete_table {
            self.delete_requested = true;
            self.dirty_project = true;
        }

        if !self.rows.is_empty() {
            ui.ctx().request_repaint_after(self.period);
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
                        let selected_name = selected_device
                            .map(|d| d.name.as_str())
                            .unwrap_or("Device not found");
                        egui::ComboBox::from_id_salt(device_select_label.id)
                            .selected_text(format!("{}", selected_name))
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
}

struct GuiDevice {
    id: u8,
    name: String,
}

// Creation of a new port

#[derive(PartialEq, Eq, Debug, Clone, Serialize, Deserialize)]
pub struct TcpConnect {
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
    pub fn to_string(&self) -> String {
        format!("{}:{}", self.ip, self.port)
    }
}

#[derive(PartialEq, Eq, Debug, Clone, Serialize, Deserialize)]
pub struct SerialConnect {
    scan: bool,
    avalible_ports: Vec<String>,
    pub serial_port_buffer: String,
    port_id: usize,
    bud_rate_values: [usize; 8],
    bud_rate_id: usize,
    pub bud_rate_buffer: usize,

    pub data_bits: DataBits,
    pub parity: Parity,
    pub stop_bits: usize,
    pub scan_error: Option<String>,
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
            scan_error: None,
        }
    }

    fn render_edit_window(state: &mut SerialConnect, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let port_type_label = ui.label("Serial Port");
            ui.add(egui::TextEdit::singleline(&mut state.serial_port_buffer))
                .labelled_by(port_type_label.id);
            if ui.button("Scan").clicked() {
                state.avalible_ports.clear();
                state.port_id = 0;
                state.scan_error = None;
                match serialport::available_ports() {
                    Ok(mut ports) => {
                        if !ports.is_empty() {
                            ports.sort_unstable_by_key(|i| i.port_name.clone());
                            state.avalible_ports = ports.into_iter().map(|p| p.port_name).collect();
                            state.serial_port_buffer = state.avalible_ports[0].clone();
                        } else {
                            state.scan_error =
                                Some("there is none serial ports avaliabele".to_string());
                        }
                    }
                    Err(err) => {
                        state.scan_error = Some(format!("Scan error: {}", err));
                    }
                }
                state.scan = true;
            }
            if let Some(ref err) = state.scan_error {
                ui.colored_label(egui::Color32::LIGHT_RED, err);
            }
            if state.scan && !state.avalible_ports.is_empty() {
                if state.port_id >= state.avalible_ports.len() {
                    state.port_id = 0;
                }
                let selected_port = state.avalible_ports[state.port_id].clone();
                egui::ComboBox::from_id_salt(port_type_label.id)
                    .selected_text(&selected_port)
                    .show_ui(ui, |ui| {
                        for (i, port) in state.avalible_ports.iter().enumerate() {
                            if ui.selectable_value(&mut state.port_id, i, port).clicked() {
                                state.serial_port_buffer = port.clone();
                            }
                        }
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

#[derive(PartialEq, Eq, Debug, Clone, Serialize, Deserialize)]
pub enum ConnectionType {
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

#[derive(Debug, Clone)]
struct GuiPortState {
    is_open: bool,
    no_port: bool,
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
        Self {
            is_open: false,
            no_port: false,
            port_type: ConnectionType::TCP(connection_tcp2),
            connection_tcp,
            connection_ascii,
            connection_rtu,
        }
    }
    fn from_port(port: GuiPort) -> Self {
        let connection_tcp = TcpConnect::new();
        let connection_ascii = SerialConnect::new(DataBits::Eight, Parity::Even);
        let connection_rtu = SerialConnect::new(DataBits::Eight, Parity::Even);
        Self {
            is_open: false,
            no_port: false,
            port_type: port.port_type,
            connection_tcp,
            connection_ascii,
            connection_rtu,
        }
    }
}
#[derive(Debug, Clone)]
pub struct GuiPort {
    pub port_type: ConnectionType,
}

impl GuiPort {
    fn new() -> Self {
        let connection = SerialConnect::new(DataBits::Eight, Parity::Even);
        Self {
            port_type: ConnectionType::RTU(connection),
        }
    }
    fn render_no_port(ui: &mut egui::Ui, state: &mut GuiPortState) {
        let mut open = state.no_port;
        egui::Window::new("Port not configured")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ui.ctx(), |ui| {
                ui.with_layout(
                    egui::Layout::top_down_justified(egui::Align::Center),
                    |ui| {
                        let text = egui::RichText::new("No port configured! (TCP / ASCII / RTU)")
                            .color(egui::Color32::LIGHT_RED);
                        ui.label(text);
                        if ui.button("Ok").clicked() {
                            state.no_port = false;
                        }
                    },
                );
            });
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
    pub original_id: Option<u8>,
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
    pub error_message: Option<String>,
}

impl DeviceCreationState {
    pub fn from_device(id: u8, dev: &DynamicMemory) -> Self {
        let total_size_bytes = dev.capacity_bytes().max(4);
        let has_custom_offsets = dev.holding_offs != 0
            || dev.input_offs != 0
            || dev.coil_offs != 0
            || dev.discrete_offs != 0;

        Self {
            is_open: true,
            show_advanced: has_custom_offsets,
            name_buffer: dev.name.clone(),
            device_id: id,
            original_id: Some(id),
            total_size_bytes,

            holding_registers_count: 10,
            input_registers_count: 10,
            coils_count: 16,
            discrete_inputs_count: 16,

            holding_offs: dev.holding_offs,
            input_offs: dev.input_offs,
            coil_offs: dev.coil_offs,
            discrete_offs: dev.discrete_offs,

            error_message: None,
        }
    }

    fn new() -> Self {
        Self {
            is_open: false,
            show_advanced: false,
            name_buffer: "Nowe Urządzenie".to_string(),
            device_id: 1,
            original_id: None,
            total_size_bytes: 64, // Domyślnie 64 bajty pamięci

            holding_registers_count: 10,
            input_registers_count: 10,
            coils_count: 16,
            discrete_inputs_count: 16,

            holding_offs: 0,
            input_offs: 20,
            coil_offs: 40,
            discrete_offs: 44,
            error_message: None,
        }
    }
}

struct GuiDeviceCreator;

impl GuiDeviceCreator {
    fn render(
        ctx: &egui::Context,
        state: &mut DeviceCreationState,
        occupied_devices: &[(u8, String)],
    ) -> Option<(u8, DynamicMemory)> {
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
                    let changed = ui
                        .add(
                            egui::DragValue::new(&mut state.device_id)
                                .speed(1.0)
                                .range(1..=247),
                        )
                        .changed();

                    if changed {
                        state.error_message = None;
                    }
                });

                // Podpowiedź / ostrzeżenie na czerwono bezpośrednio w oknie kreatora/edytora
                if let Some(ref err) = state.error_message {
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new(format!("⚠ {}", err))
                            .color(egui::Color32::from_rgb(230, 80, 80)),
                    );
                }

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
                            let conflict = occupied_devices.iter().find(|(id, _)| {
                                *id == state.device_id && state.original_id != Some(*id)
                            });

                            if let Some((id, name)) = conflict {
                                state.error_message = Some(format!(
                                    "Slave ID {} jest już zajęte przez \"{}\".",
                                    id, name
                                ));
                            } else {
                                let name = if state.name_buffer.trim().is_empty() {
                                    "Urządzenie Modbus".to_string()
                                } else {
                                    state.name_buffer.clone()
                                };

                                let mut memory = DynamicMemory::new(name, state.total_size_bytes);
                                memory.set_mapping(
                                    state.holding_offs,
                                    state.input_offs,
                                    state.coil_offs,
                                    state.discrete_offs,
                                );

                                created_result = Some((state.device_id, memory));
                                state.error_message = None;
                                close = true;
                            }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeviceSelectMode {
    Edit,
    Delete,
}

#[derive(Default, Debug, Clone)]
struct DevicesPanelState {
    pub selected_id: Option<u8>,
    pub editing_id: Option<u8>,
    pub pending_delete_id: Option<u8>,
    pub filter_query: String,
    pub select_dialog_mode: Option<DeviceSelectMode>,
    pub picker_selected_id: Option<u8>,
}

impl DevicesPanelState {
    fn new() -> Self {
        Self::default()
    }

    fn open_device_picker(&mut self, mode: DeviceSelectMode, available_ids: &[u8]) {
        self.select_dialog_mode = Some(mode);
        self.picker_selected_id = self
            .selected_id
            .filter(|id| available_ids.contains(id))
            .or_else(|| available_ids.first().copied());
    }

    fn start_creating(&mut self, creator: &mut DeviceCreationState) {
        self.editing_id = None;
        self.pending_delete_id = None;
        *creator = DeviceCreationState::new();
        creator.is_open = true;
    }

    fn start_editing(&mut self, id: u8, dev: &DynamicMemory, creator: &mut DeviceCreationState) {
        self.editing_id = Some(id);
        self.selected_id = Some(id);
        self.pending_delete_id = None;
        *creator = DeviceCreationState::from_device(id, dev);
    }
}

pub enum ServerCommand {
    Start(ConnectionType),
    Stop,
}

fn find_id_conflict(
    router: &ModbusRouter,
    candidate_id: u8,
    editing_id: Option<u8>,
) -> Option<String> {
    // Jeśli edytujemy urządzenie i ID się nie zmieniło, to nie jest duplikat
    if editing_id == Some(candidate_id) {
        return None;
    }

    if let Some(existing) = router.get_devices().get(&candidate_id) {
        return Some(format!(
            "Slave ID {} jest już zajęte przez urządzenie \"{}\".",
            candidate_id, existing.name
        ));
    }

    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PendingProjectAction {
    New,
    Open,
    Quit,
}
pub struct MyApp {
    pub logs: GuiLoggerBuffer,
    pub reciever: mpsc::Receiver<String>,
    pub router: Arc<Mutex<ModbusRouter>>,
    pub server_tx: mpsc::Sender<ServerCommand>,
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
    port_started: bool,
    // device
    devices_state: DeviceCreationState,
    devices: Vec<DynamicMemory>,
    devices_panel: DevicesPanelState,
    // saving
    project_path: Option<PathBuf>,
    project_dirty: bool,
    pending_project_action: Option<PendingProjectAction>,
    pending_quit: bool,
}

impl MyApp {
    pub fn new(
        message: mpsc::Receiver<String>,
        cc: &eframe::CreationContext<'_>,
        router: Arc<Mutex<ModbusRouter>>,
        server: mpsc::Sender<ServerCommand>,
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
            server_tx: server,

            left: true,
            top: true,
            bottom: true,
            center: true,
            center_view: CenterView::WatchTable,
            watch_table_state: WatchCeraetionState::new(),
            watch_tables: Vec::new(),
            port_state: GuiPortState::new(),
            port: None,
            port_started: false,
            devices_state: DeviceCreationState::new(),
            devices: Vec::new(),
            devices_panel: DevicesPanelState::new(),
            project_path: None,
            project_dirty: true,
            pending_project_action: None,
            pending_quit: false,
        }
    }

    fn request_project_action(&mut self, action: PendingProjectAction) {
        if !self.project_dirty {
            self.execute_project_action(action);
        } else {
            self.pending_project_action = Some(action);
        }
    }

    fn execute_project_action(&mut self, action: PendingProjectAction) {
        match action {
            PendingProjectAction::New => {
                self.new_project();
            }
            PendingProjectAction::Open => {
                self.open_project();
            }
            PendingProjectAction::Quit => {
                if self.port_started {
                    let _ = self.server_tx.send(ServerCommand::Stop);
                    self.port_started = false;
                }
                self.pending_quit = true;
            }
        }
    }

    fn new_project(&mut self) {
        if self.port_started {
            let _ = self.server_tx.send(ServerCommand::Stop);
            self.port_started = false;
        }

        self.port = None;
        self.port_state = GuiPortState::new();
        self.project_path = None;

        {
            let mut router = self.router.lock().unwrap();
            router.get_devices_mut().clear();
        }

        self.devices.clear();
        self.devices_state = DeviceCreationState::new();
        self.devices_panel = DevicesPanelState::new();

        self.watch_tables.clear();
        self.watch_table_state = WatchCeraetionState::new();

        self.logs.push(
            "Utworzono nowy projekt (zresetowano połączenie, urządzenia i Watch Table)."
                .to_string(),
        );
        self.project_dirty = false;
    }

    fn save_project_as(&mut self) {
        if let Some(path) = crate::persistence::pick_save_project_path(self.project_path.as_deref())
        {
            let project = {
                let router = self.router.lock().unwrap();
                crate::persistence::ProjectFile::capture(
                    self.port.as_ref(),
                    &router,
                    &self.watch_tables,
                )
            };
            match crate::persistence::save_project_to_file(&path, &project) {
                Ok(()) => {
                    self.project_path = Some(path.clone());
                    self.logs
                        .push(format!("Zapisano projekt do {}", path.display()));

                    self.mark_project_saved(path.clone());
                }
                Err(err) => {
                    self.logs.push(format!("Błąd zapisu projektu: {err}"));
                    self.mark_project_dirty();
                }
            }
        }
    }

    fn save_project(&mut self) {
        let Some(path) = self.project_path.clone() else {
            self.save_project_as();
            return;
        };

        let project = {
            let router = self.router.lock().unwrap();
            crate::persistence::ProjectFile::capture(
                self.port.as_ref(),
                &router,
                &self.watch_tables,
            )
        };
        match crate::persistence::save_project_to_file(&path, &project) {
            Ok(()) => {
                self.logs
                    .push(format!("Zapisano projekt do {}", path.display()));

                self.mark_project_saved(path.clone());
            }
            Err(err) => {
                self.logs.push(format!("Błąd zapisu projektu: {err}"));
                self.mark_project_dirty();
            }
        }
    }

    fn open_project(&mut self) {
        if let Some(path) = crate::persistence::pick_open_project_path(self.project_path.as_deref())
        {
            match crate::persistence::load_project_from_file(&path) {
                Ok(project) => {
                    if self.port_started {
                        let _ = self.server_tx.send(ServerCommand::Stop);
                        self.port_started = false;
                    }

                    {
                        let mut router = self.router.lock().unwrap();
                        project.apply_to(&mut router, &mut self.port, &mut self.watch_tables);
                    }

                    self.port_state = match self.port.clone() {
                        Some(p) => GuiPortState::from_port(p),
                        None => GuiPortState::new(),
                    };
                    self.project_path = Some(path.clone());
                    self.mark_project_saved(path.clone());
                    self.logs
                        .push(format!("Wczytano projekt z {}", path.display()));
                }
                Err(err) => {
                    self.logs.push(format!("Błąd odczytu projektu: {err}"));
                }
            }
        }
    }

    fn mark_project_dirty(&mut self) {
        self.project_dirty = true;
    }

    fn mark_project_saved(&mut self, path: PathBuf) {
        self.project_path = Some(path);
        self.project_dirty = false;
    }

    fn update(&mut self) {
        while let Ok(log) = self.reciever.try_recv() {
            self.logs.push(log);
        }
    }

    fn render_unsaved_changes_modal(&mut self, ui: &mut egui::Ui) {
        let Some(action) = self.pending_project_action else {
            return;
        };

        let action_desc = match action {
            PendingProjectAction::New => "utworzeniem nowego projektu",
            PendingProjectAction::Open => "otwarciem innego projektu",
            PendingProjectAction::Quit => "zamknięciem aplikacji",
        };

        let mut open = true;
        let mut choice: Option<&'static str> = None;

        egui::Window::new("Niezapisane zmiany")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ui.ctx(), |ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "Projekt ma niezapisane zmiany przed {}.",
                        action_desc
                    ))
                    .color(egui::Color32::from_rgb(245, 180, 50)),
                );
                ui.add_space(10.0);

                ui.horizontal(|ui| {
                    if ui.button("💾 Zapisz i kontynuuj").clicked() {
                        choice = Some("save");
                    }
                    if ui.button("Kontynuuj bez zapisu").clicked() {
                        choice = Some("discard");
                    }
                    if ui.button("Anuluj").clicked() {
                        choice = Some("cancel");
                    }
                });
            });

        match choice {
            Some("save") => {
                self.save_project();
                // Wykonujemy akcję tylko jeśli zapis się powiódł (project_dirty spadło do false)
                if !self.project_dirty {
                    self.pending_project_action = None;
                    self.execute_project_action(action);
                } else {
                    self.pending_project_action = None;
                }
            }
            Some("discard") => {
                self.pending_project_action = None;
                self.execute_project_action(action);
            }
            Some("cancel") => {
                self.pending_project_action = None;
            }
            _ if !open => {
                self.pending_project_action = None;
            }
            _ => {}
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
            self.mark_project_dirty();
            self.watch_tables.push(new_table);
        }

        for watch_table in self.watch_tables.iter_mut() {
            watch_table.render(ui, &self.router, &mut self.project_dirty);
        }

        let before_len = self.watch_tables.len();
        self.watch_tables.retain(|t| !t.delete_requested);
        if self.watch_tables.len() != before_len {
            self.mark_project_dirty();
        }
    }

    fn render_top_panel(&mut self, ui: &mut egui::Ui) {
        let mut new_project_action = false;

        let mut action_open_project = false;
        let mut action_save_project = false;
        let mut action_save_project_as = false;
        let mut action_dirty_project = false;

        let mut action_request_new = false;
        let mut action_request_open = false;
        let mut action_request_quit = false;

        egui::Panel::top("Top Panel")
            .resizable(true)
            .show_collapsible(ui, &mut self.top, |ui| {
                // opcje górne
                egui::MenuBar::new().ui(ui, |ui| {
                    ui.menu_button("Project", |ui| {
                        if ui.button("new").clicked() {
                            new_project_action = true;
                            ui.close();
                        }
                        if ui.button("Open").clicked() {
                            action_open_project = true;
                            ui.close();
                            // match load_watch_tables_from_file(&self.project_path) {
                            //     Ok(loaded_tables) => {
                            //         self.watch_tables = loaded_tables;
                            //         self.logs.push(format!(
                            //             "Wczytano Watch Tables z {}",
                            //             self.project_path.display()
                            //         ));
                            //     }
                            //     Err(err) => {
                            //         self.logs.push(format!("Błąd odczytu: {err}"));
                            //     }
                            // }
                            // ui.close();
                        }
                        ui.separator();
                        if ui.button("Save").clicked() {
                            action_save_project = true;
                            ui.close();
                        }
                        if ui.button("Save as").clicked() {
                            action_save_project_as = true;
                            ui.close();
                            println!("Zapipsz projekt jako");
                        }

                        ui.separator();
                        if ui.button("Quit").clicked() {
                            action_request_quit = true;
                            ui.close();
                        }
                    });
                    ui.menu_button("Server", |ui| {
                        if ui.button("New").clicked() {
                            self.port_state = GuiPortState::new();
                            self.port_state.is_open = true;
                        }
                        if ui.button("Edit connection").clicked() {
                            if let Some(port) = self.port.clone() {
                                self.port_state = GuiPortState::from_port(port);
                            } else {
                                self.port_state = GuiPortState::new();
                            }
                            self.port_state.is_open = true;
                        }
                        if ui.button("start server").clicked() {
                            if !self.port_started {
                                if let Some(port) = self.port.clone() {
                                    let _ =
                                        self.server_tx.send(ServerCommand::Start(port.port_type));
                                    self.port_started = true;
                                } else {
                                    self.port_state.no_port = true;
                                }
                            }
                        }
                        if ui.button("close connection").clicked() {
                            let _ = self.server_tx.send(ServerCommand::Stop);
                            self.port_started = false;
                            println!("cakończenie połączenia");
                        }
                    });
                    ui.menu_button("Device", |ui| {
                        if ui.button("New Device").clicked() {
                            self.devices_panel.start_creating(&mut self.devices_state);

                            let router = self.router.lock().unwrap();
                            if let Some(new_id) = router.next_free_id() {
                                self.devices_state.device_id = new_id;
                            }
                        }
                        if ui.button("Edit Device").clicked() {
                            let ids: Vec<u8> = {
                                let router = self.router.lock().unwrap();
                                let mut keys: Vec<u8> =
                                    router.get_devices().keys().copied().collect();
                                keys.sort_unstable();
                                keys
                            };
                            self.devices_panel
                                .open_device_picker(DeviceSelectMode::Edit, &ids);
                            ui.close();

                            println!("edytowanie urządzenia");
                        }
                        if ui.button("Delete Device").clicked() {
                            let ids: Vec<u8> = {
                                let router = self.router.lock().unwrap();
                                let mut keys: Vec<u8> =
                                    router.get_devices().keys().copied().collect();
                                keys.sort_unstable();
                                keys
                            };
                            self.devices_panel
                                .open_device_picker(DeviceSelectMode::Delete, &ids);
                            ui.close();

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
                    // szybki zapis
                    let (save_label, save_color) = if self.project_dirty {
                        (
                            egui::RichText::new("💾 Zapisz projekt*")
                                .color(egui::Color32::from_rgb(245, 180, 50))
                                .strong(),
                            egui::Color32::from_rgb(245, 180, 50),
                        )
                    } else {
                        (
                            egui::RichText::new("💾 Zapisano")
                                .color(egui::Color32::from_rgb(130, 190, 140)),
                            egui::Color32::from_rgb(130, 190, 140),
                        )
                    };

                    let save_btn = ui.add(
                        egui::Button::new(save_label)
                            .stroke(egui::Stroke::new(1.0, save_color.linear_multiply(0.5))),
                    );

                    let hover_msg = match (&self.project_path, self.project_dirty) {
                        (Some(p), true) => format!(
                            "Niezapisane zmiany — kliknij, aby zapisać do {}",
                            p.display()
                        ),
                        (Some(p), false) => format!("Projekt zapisany ({})", p.display()),
                        (None, true) => {
                            "Niezapisany nowy projekt — kliknij, aby zapisać jako plik JSON"
                                .to_string()
                        }
                        (None, false) => "Brak zmian do zapisania".to_string(),
                    };

                    if save_btn.on_hover_text(hover_msg).clicked() {
                        action_save_project = true;
                    }

                    // start symulacji
                    if ui.button("start simulation").clicked() {
                        println!("Simulation started");
                    }

                    let (label, color) = if self.port_started {
                        (
                            egui::RichText::new("⏹ Stop Server")
                                .color(egui::Color32::from_rgb(220, 70, 70)),
                            egui::Color32::from_rgb(220, 70, 70),
                        )
                    } else {
                        (
                            egui::RichText::new("▶ Start Server")
                                .color(egui::Color32::from_rgb(80, 200, 120)),
                            egui::Color32::from_rgb(80, 200, 120),
                        )
                    };

                    let _ = color; // opcjonalnie do ramki/tła jeśli chcesz rozbudować przycisk

                    if ui.button(label).clicked() {
                        if self.port_started {
                            // Serwer działa -> zatrzymujemy go
                            let _ = self.server_tx.send(ServerCommand::Stop);
                            self.port_started = false;
                        } else {
                            // Serwer stoi -> odpalamy, jeśli jest skonfigurowany port
                            match self.port.clone() {
                                Some(conn) => {
                                    let _ =
                                        self.server_tx.send(ServerCommand::Start(conn.port_type));
                                    self.port_started = true;
                                }
                                None => {
                                    self.port_state.no_port = true;
                                }
                            }
                        }
                    }

                    let status_text = match (&self.port, self.port_started) {
                        (Some(p), true) => match &p.port_type {
                            ConnectionType::TCP(tcp) => {
                                format!("● TCP {} (aktywny)", tcp.to_string())
                            }
                            ConnectionType::RTU(rtu) => format!(
                                "● RTU {} @ {} (aktywny)",
                                rtu.serial_port_buffer, rtu.bud_rate_buffer
                            ),
                            ConnectionType::ASCII(asc) => format!(
                                "● ASCII {} @ {} (aktywny)",
                                asc.serial_port_buffer, asc.bud_rate_buffer
                            ),
                        },
                        (Some(p), false) => match &p.port_type {
                            ConnectionType::TCP(tcp) => {
                                format!("○ TCP {} (zatrzymany)", tcp.to_string())
                            }
                            ConnectionType::RTU(rtu) => format!(
                                "○ RTU {} @ {} (zatrzymany)",
                                rtu.serial_port_buffer, rtu.bud_rate_buffer
                            ),
                            ConnectionType::ASCII(asc) => format!(
                                "○ ASCII {} @ {} (zatrzymany)",
                                asc.serial_port_buffer, asc.bud_rate_buffer
                            ),
                        },
                        (None, _) => "○ Brak skonfigurowanego portu".to_string(),
                    };

                    let status_color = if self.port_started {
                        egui::Color32::from_rgb(80, 200, 120)
                    } else if self.port.is_some() {
                        egui::Color32::from_rgb(220, 190, 80)
                    } else {
                        egui::Color32::GRAY
                    };

                    ui.separator();
                    ui.colored_label(status_color, status_text);
                });
                if self.port_state.is_open {
                    self.port = GuiPort::render(ui, &mut self.port_state);
                    action_dirty_project = true;
                }

                if self.port_state.no_port {
                    GuiPort::render_no_port(ui, &mut self.port_state);
                }
                if self.devices_state.is_open {
                    let occupied_devices: Vec<(u8, String)> = {
                        let router = self.router.lock().unwrap();
                        router
                            .get_devices()
                            .iter()
                            .map(|(id, dev)| (*id, dev.name.clone()))
                            .collect()
                    };
                    if let Some((id, mut device)) =
                        GuiDeviceCreator::render(ui, &mut self.devices_state, &occupied_devices)
                    {
                        let mut router = self.router.lock().unwrap();

                        if let Some(old_id) = self.devices_panel.editing_id.take() {
                            if let Some(old_dev) = router.get_devices_mut().remove(&old_id) {
                                let copy_len = old_dev.data.len().min(device.data.len());
                                device.data[..copy_len].copy_from_slice(&old_dev.data[..copy_len]);
                            }
                        }
                        router.add_device(id, device);
                        action_dirty_project = true;
                        self.devices_panel.selected_id = Some(id);
                    }
                }
                // Wywołanie okienka wyboru urządzenia (Edit / Delete z górnego menu)
            });
        let ctx = ui.ctx().clone();
        self.render_device_picker(&ctx);
        if new_project_action {
            self.request_project_action(PendingProjectAction::New);
        }
        if action_dirty_project {
            self.mark_project_dirty();
        }
        if action_open_project {
            self.request_project_action(PendingProjectAction::Open);
        }
        if action_save_project {
            self.save_project();
        }
        if action_save_project_as {
            self.save_project_as();
        }

        if action_request_quit {
            self.request_project_action(PendingProjectAction::Quit);
        }
    }

    fn render_device_picker(&mut self, ctx: &egui::Context) {
        let Some(mode) = self.devices_panel.select_dialog_mode else {
            return;
        };

        // Migawka urządzeń z routera bez trzymania locka podczas rysowania UI
        let devices: Vec<(u8, String)> = {
            let router = self.router.lock().unwrap();
            let mut list: Vec<(u8, String)> = router
                .get_devices()
                .iter()
                .map(|(id, dev)| (*id, dev.name.clone()))
                .collect();
            list.sort_by_key(|(id, _)| *id);
            list
        };

        let title = match mode {
            DeviceSelectMode::Edit => "Select device to edit",
            DeviceSelectMode::Delete => "Select device to delete",
        };

        let mut open = true;
        let mut confirmed_id: Option<u8> = None;
        let mut cancel = false;

        egui::Window::new(title)
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                if devices.is_empty() {
                    ui.colored_label(egui::Color32::GRAY, "Brak urządzeń w projekcie.");
                    ui.add_space(8.0);
                    if ui.button("OK").clicked() {
                        cancel = true;
                    }
                    return;
                }

                // Upewniamy się, że wybrane ID nadal istnieje
                if self
                    .devices_panel
                    .picker_selected_id
                    .map_or(true, |sel| !devices.iter().any(|(id, _)| *id == sel))
                {
                    self.devices_panel.picker_selected_id = Some(devices[0].0);
                }

                let current_id = self.devices_panel.picker_selected_id.unwrap();
                let current_label = devices
                    .iter()
                    .find(|(id, _)| *id == current_id)
                    .map(|(id, name)| format!("[ID {}] {}", id, name))
                    .unwrap_or_default();

                ui.horizontal(|ui| {
                    ui.label("Device:");
                    egui::ComboBox::from_id_salt("device_picker_combo")
                        .selected_text(current_label)
                        .show_ui(ui, |ui| {
                            for (id, name) in &devices {
                                ui.selectable_value(
                                    &mut self.devices_panel.picker_selected_id,
                                    Some(*id),
                                    format!("[ID {}] {}", id, name),
                                );
                            }
                        });
                });

                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let confirm_text = match mode {
                            DeviceSelectMode::Edit => egui::RichText::new("Edit"),
                            DeviceSelectMode::Delete => {
                                egui::RichText::new("Delete").color(egui::Color32::LIGHT_RED)
                            }
                        };

                        if ui.button(confirm_text).clicked() {
                            confirmed_id = self.devices_panel.picker_selected_id;
                        }
                        if ui.button("Cancel").clicked() {
                            cancel = true;
                        }
                    });
                });
            });

        if let Some(id) = confirmed_id {
            self.devices_panel.select_dialog_mode = None;
            match mode {
                DeviceSelectMode::Edit => {
                    let dev_snapshot = {
                        let router = self.router.lock().unwrap();
                        router.get_devices().get(&id).cloned()
                    };
                    if let Some(dev) = dev_snapshot {
                        self.devices_panel
                            .start_editing(id, &dev, &mut self.devices_state);
                    }
                }
                DeviceSelectMode::Delete => {
                    self.devices_panel.pending_delete_id = Some(id);
                }
            }
        } else if cancel || !open {
            self.devices_panel.select_dialog_mode = None;
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
        let mut open_new_device = false;
        let mut edit_target: Option<(u8, String)> = None;
        let mut duplicate_id: Option<u8> = None;
        let mut request_delete_id: Option<u8> = None;

        egui::Panel::left("Devices")
            .resizable(true)
            .show_collapsible(ui, &mut self.left, |ui| {
                ui.horizontal(|ui| {
                    ui.strong("Devices");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("+").on_hover_text("New Device").clicked() {
                            let router = self.router.lock().unwrap();

                            self.devices_state = DeviceCreationState::new();
                            if let Some(new_id) = router.next_free_id() {
                                self.devices_state.device_id = new_id;
                            }
                            open_new_device = true;
                        }
                    });
                });

                ui.add(
                    egui::TextEdit::singleline(&mut self.devices_panel.filter_query)
                        .hint_text("Filter (Name or ID)..."),
                );
                ui.separator();

                let query = self.devices_panel.filter_query.trim().to_lowercase();

                let devices_snapshot: Vec<(u8, String)> = {
                    let router = self.router.lock().unwrap();
                    let mut items: Vec<(u8, String)> = router
                        .get_devices()
                        .iter()
                        .filter(|(id, dev)| {
                            query.is_empty()
                                || id.to_string().contains(&query)
                                || dev.name.to_lowercase().contains(&query)
                        })
                        .map(|(id, dev)| (*id, dev.name.clone()))
                        .collect();
                    items.sort_by_key(|(id, _)| *id);
                    items
                };

                let mut clicked_on_device = false;

                egui::ScrollArea::vertical().show(ui, |ui| {
                    if devices_snapshot.is_empty() {
                        ui.colored_label(egui::Color32::GRAY, "Brak urządzeń (PPM → New Device)");
                    }

                    for (id, name) in &devices_snapshot {
                        let is_selected = self.devices_panel.selected_id == Some(*id);
                        let response =
                            ui.selectable_label(is_selected, format!("[ID {}] {}", id, name));

                        if response.clicked() {
                            self.devices_panel.selected_id = Some(*id);
                        }

                        response.context_menu(|ui| {
                            clicked_on_device = true;
                            ui.label(egui::RichText::new(format!("Device ID {}", id)).strong());
                            ui.separator();

                            if ui.button("✏ Edit Device").clicked() {
                                edit_target = Some((*id, name.clone()));
                                ui.close();
                            }
                            if ui.button("📋 Duplicate Device").clicked() {
                                duplicate_id = Some(*id);
                                ui.close();
                            }
                            ui.separator();
                            if ui
                                .button(
                                    egui::RichText::new("🗑 Delete Device")
                                        .color(egui::Color32::LIGHT_RED),
                                )
                                .clicked()
                            {
                                request_delete_id = Some(*id);
                                ui.close();
                            }
                        });
                    }

                    // Pusty obszar panelu – prawy klik otwiera opcję dodania nowego urządzenia
                    let remaining = ui.available_size();
                    if remaining.y > 0.0 {
                        let (_, empty_resp) = ui.allocate_exact_size(
                            egui::vec2(remaining.x.max(1.0), remaining.y),
                            egui::Sense::click(),
                        );
                        empty_resp.context_menu(|ui| {
                            if !clicked_on_device {
                                if ui.button("➕ New Device").clicked() {
                                    self.devices_state = DeviceCreationState::new();
                                    let router = self.router.lock().unwrap();
                                    if let Some(new_id) = router.next_free_id() {
                                        self.devices_state.device_id = new_id;
                                    }
                                    open_new_device = true;
                                    ui.close();
                                }
                            }
                        });
                    }
                });
            });

        // Zastosowanie akcji z menu kontekstowego
        if open_new_device {
            self.devices_panel.start_creating(&mut self.devices_state);
        }

        if let Some((id, _)) = edit_target {
            let dev_snapshot: Option<DynamicMemory> = {
                let router = self.router.lock().unwrap();
                router.get_devices().get(&id).cloned()
            };
            if let Some(dev) = dev_snapshot {
                self.devices_panel
                    .start_editing(id, &dev, &mut self.devices_state);
            }
        }

        if let Some(id) = duplicate_id {
            let mut router = self.router.lock().unwrap();
            if let Some(source) = router.get_devices().get(&id) {
                let next_id =
                    (1u8..=247).find(|candidate| !router.get_devices().contains_key(candidate));
                if let Some(new_id) = next_id {
                    let mut cloned_dev = source.clone();
                    cloned_dev.name = format!("{} (Copy)", source.name); // dopasuj konstruktor / kopiowanie do swojej implementacji DynamicMemory
                    router.add_device(new_id, cloned_dev);
                    self.devices_panel.selected_id = Some(new_id);
                }
            }
        }

        if let Some(id) = request_delete_id {
            self.devices_panel.pending_delete_id = Some(id);
        }

        // Potwierdzenie usunięcia urządzenia zapisane w `devices_panel.pending_delete_id`
        if let Some(id) = self.devices_panel.pending_delete_id {
            let mut open = true;
            let mut confirm_delete = false;
            let mut cancel_delete = false;

            egui::Window::new("Confirm delation")
                .collapsible(false)
                .resizable(false)
                .open(&mut open)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ui.ctx(), |ui| {
                    ui.label(
                        egui::RichText::new(format!("Delete device: ID {}?", id))
                            .color(egui::Color32::LIGHT_RED),
                    );
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Delete").clicked() {
                            confirm_delete = true;
                        }
                        if ui.button("Cancel").clicked() {
                            cancel_delete = true;
                        }
                    });
                });

            if confirm_delete {
                let mut router = self.router.lock().unwrap();
                let _ = router.get_devices_mut().remove(&id);
                if self.devices_panel.selected_id == Some(id) {
                    self.devices_panel.selected_id = None;
                }
                self.devices_panel.pending_delete_id = None;
            } else if cancel_delete || !open {
                self.devices_panel.pending_delete_id = None;
            }
        }
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
        // 1. Przechwycenie zamknięcia okna ('X'), gdy są niezapisane zmiany
        if !self.pending_quit && self.project_dirty && self.pending_project_action.is_none() {
            if ui.ctx().input(|i| i.viewport().close_requested()) {
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.pending_project_action = Some(PendingProjectAction::Quit);
            }
        }

        // 2. Główne panele
        self.render_top_panel(ui);
        self.render_bottom_panel(ui);
        self.render_left_panel(ui);
        self.render_central_panel(ui);

        // 3. Podpięcie okna potwierdzenia niezapisanych zmian
        self.render_unsaved_changes_modal(ui);

        // 4. Faktyczne zamknięcie okna aplikacji
        if self.pending_quit {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            ui.ctx().request_repaint();
        }
    }
}
