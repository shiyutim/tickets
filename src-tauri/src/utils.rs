use std::fs::File;
use std::io::prelude::*;

#[tauri::command]
pub fn export_sql_to_txt(path: String, data: String) -> String {
    let file = File::create(path);
    match file {
        Ok(mut f) => match f.write_all(data.as_bytes()) {
            Ok(()) => String::from("success 导出成功"),
            Err(e) => format!("error: {e}"),
        },
        Err(e) => String::from(format!("error: fail to export {}", e)),
    }
}
