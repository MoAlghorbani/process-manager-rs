fn main() {
    // If embedded_tools.zip does not exist (e.g. freshly cloned from git),
    // write a valid empty zip archive (22 bytes) so include_bytes! succeeds.
    let zip_path = std::path::Path::new("embedded_tools.zip");
    if !zip_path.exists() {
        let empty_zip: [u8; 22] = [
            0x50, 0x4b, 0x05, 0x06, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        let _ = std::fs::write(zip_path, empty_zip);
    }
    tauri_build::build();
}

