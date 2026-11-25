use crate::DynamicMemory;
use crate::gui::{ConnectionType, GuiPort, SerialConnect, TcpConnect, WatchTable};
use crate::transport::ModbusRouter;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub const PROJECT_FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectFile {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub connection: Option<ConnectionType>,
    #[serde(default)]
    pub devices: HashMap<u8, DynamicMemory>,
    #[serde(default)]
    pub watch_tables: Vec<WatchTable>,
}

impl ProjectFile {
    pub fn capture(
        port: Option<&GuiPort>,
        router: &ModbusRouter,
        watch_tables: &[WatchTable],
    ) -> Self {
        let devices = router
            .get_devices()
            .iter()
            .map(|(id, dev)| (*id, dev.clone()))
            .collect();

        Self {
            version: PROJECT_FORMAT_VERSION,
            connection: port.map(|p| p.port_type.clone()),
            devices,
            watch_tables: watch_tables.to_vec(),
        }
    }

    pub fn apply_to(
        mut self,
        router: &mut ModbusRouter,
        port: &mut Option<GuiPort>,
        watch_tables: &mut Vec<WatchTable>,
    ) {
        *port = self
            .connection
            .take()
            .map(|port_type| GuiPort { port_type });

        // Odtwarzamy urządzenia w routerze
        router.get_devices_mut().clear();
        for (id, dev) in self.devices {
            router.add_device(id, dev);
        }

        // Resetujemy stan tymczasowy UI w watch tables i synchronizujemy z routerem
        for table in &mut self.watch_tables {
            table.last_sync = None;
            table.delete_requested = false;
            table.row_select_modal = None;
            table.row_ctx_menu = None;
            table.add_row_draft.is_open = false;
            for row in &mut table.rows {
                row.selected = false;
                row.sync_from_router(router);
            }
        }

        *watch_tables = self.watch_tables;
    }
}

fn default_version() -> u32 {
    PROJECT_FORMAT_VERSION
}

pub fn save_project_to_file(path: &Path, project: &ProjectFile) -> Result<(), String> {
    let json = serde_json::to_string_pretty(project)
        .map_err(|e| format!("Błąd serializacji projektu do JSON: {e}"))?;
    fs::write(path, json).map_err(|e| format!("Błąd zapisu pliku {}: {e}", path.display()))?;
    Ok(())
}

pub fn load_project_from_file(path: &Path) -> Result<ProjectFile, String> {
    let content = fs::read_to_string(path)
        .map_err(|e| format!("Błąd odczytu pliku {}: {e}", path.display()))?;
    let mut project: ProjectFile = serde_json::from_str(&content)
        .map_err(|e| format!("Błąd parsowania JSON projektu: {e}"))?;

    // Reset stanów runtime po wczytaniu
    for table in &mut project.watch_tables {
        table.last_sync = None;
        table.delete_requested = false;
        table.row_select_modal = None;
        table.row_ctx_menu = None;
        for row in &mut table.rows {
            row.selected = false;
        }
    }

    Ok(project)
}
pub fn pick_save_project_path(default_path: Option<&Path>) -> Option<PathBuf> {
    let mut dialog = rfd::FileDialog::new()
        .add_filter("Modbus Project (JSON)", &["json"])
        .set_file_name("modbus_project.json");

    if let Some(dir) = default_path
        .and_then(|p| p.parent())
        .filter(|d| !d.as_os_str().is_empty())
    {
        dialog = dialog.set_directory(dir);
    }

    dialog.save_file().map(|mut path| {
        if path.extension().is_none() {
            path.set_extension("json");
        }
        path
    })
}

pub fn pick_open_project_path(default_path: Option<&Path>) -> Option<PathBuf> {
    let mut dialog = rfd::FileDialog::new().add_filter("Modbus Project (JSON)", &["json"]);

    if let Some(dir) = default_path
        .and_then(|p| p.parent())
        .filter(|d| !d.as_os_str().is_empty())
    {
        dialog = dialog.set_directory(dir);
    }

    dialog.pick_file()
}
