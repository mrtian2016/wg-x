use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use std::fs;
use tauri::command;

#[command]
pub fn get_platform() -> String {
    std::env::consts::OS.to_string()
}

#[command]
pub fn generate_qrcode(content: String) -> Result<String, String> {
    use qrcode::render::svg;
    use qrcode::QrCode;

    let code = QrCode::new(content.as_bytes()).map_err(|e| format!("生成二维码失败: {}", e))?;

    let svg = code.render::<svg::Color>().min_dimensions(200, 200).build();

    let data_url = format!(
        "data:image/svg+xml;base64,{}",
        BASE64.encode(svg.as_bytes())
    );

    Ok(data_url)
}

#[command]
pub fn save_config_to_path(content: String, file_path: String) -> Result<(), String> {
    fs::write(&file_path, content).map_err(|e| format!("保存文件失败: {}", e))?;
    Ok(())
}
