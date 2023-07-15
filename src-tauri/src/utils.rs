use std::fs::File;
use std::io::prelude::*;

#[tauri::command]
pub fn export_sql_to_txt(path: String, data: String) -> String {
    let file = File::create(path);
    match file {
        Ok(mut f) => {
            f.write_all(data.as_bytes());
            String::from("success 导出成功")
        }
        Err(e) => String::from(format!("error: fail to export {}", e)),
    }
}
