use std::fs::{self, File};
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::Mutex;

static EMBEDDED_ARCHIVE: &[u8] = include_bytes!("../embedded_tools.zip");
static EXTRACT_LOCK: Mutex<()> = Mutex::new(());

pub fn get_embedded_bin_dir() -> PathBuf {
    if let Ok(local_appdata) = std::env::var("LOCALAPPDATA") {
        let bin_dir = PathBuf::from(local_appdata)
            .join("ProcessManager")
            .join("bin");
        let _ = fs::create_dir_all(&bin_dir);
        return bin_dir;
    }
    PathBuf::from("bin")
}

pub fn are_tools_ready() -> bool {
    let bin_dir = get_embedded_bin_dir();
    let ffmpeg_path = bin_dir.join("ffmpeg.exe");
    let ffprobe_path = bin_dir.join("ffprobe.exe");
    let ytdlp_path = bin_dir.join("yt-dlp.exe");

    let ffmpeg_ok = ffmpeg_path.exists() && fs::metadata(&ffmpeg_path).map(|m| m.len() > 100_000_000).unwrap_or(false);
    let ffprobe_ok = ffprobe_path.exists() && fs::metadata(&ffprobe_path).map(|m| m.len() > 100_000_000).unwrap_or(false);
    let ytdlp_ok = ytdlp_path.exists() && fs::metadata(&ytdlp_path).map(|m| m.len() > 10_000_000).unwrap_or(false);

    ffmpeg_ok && ffprobe_ok && ytdlp_ok
}

pub fn ensure_embedded_tools() {
    if are_tools_ready() {
        return;
    }

    let _guard = EXTRACT_LOCK.lock().unwrap();
    if are_tools_ready() {
        return;
    }

    let bin_dir = get_embedded_bin_dir();
    let cursor = Cursor::new(EMBEDDED_ARCHIVE);

    if let Ok(mut archive) = zip::ZipArchive::new(cursor) {
        for i in 0..archive.len() {
            if let Ok(mut file) = archive.by_index(i) {
                let outpath = match file.enclosed_name() {
                    Some(path) => bin_dir.join(path),
                    None => continue,
                };

                if file.is_dir() {
                    let _ = fs::create_dir_all(&outpath);
                    continue;
                }

                if let Some(p) = outpath.parent() {
                    let _ = fs::create_dir_all(p);
                }

                let needs_extract = !outpath.exists()
                    || fs::metadata(&outpath).map(|m| m.len() < file.size()).unwrap_or(true);

                if needs_extract {
                    let tmp_path = outpath.with_extension("tmp_extract");
                    if let Ok(mut outfile) = File::create(&tmp_path) {
                        if std::io::copy(&mut file, &mut outfile).is_ok() {
                            drop(outfile);
                            let _ = fs::rename(&tmp_path, &outpath);
                        } else {
                            let _ = fs::remove_file(&tmp_path);
                        }
                    }
                }
            }
        }
    }
}
