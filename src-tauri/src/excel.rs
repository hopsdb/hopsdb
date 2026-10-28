//! Excel / CSV export
//! Uses rust_xlsxwriter for .xlsx (borders + bold header + autofilter) and a
//! plain-text writer for .csv (UTF-8 BOM so Excel reads CJK correctly).
//! Both write to the user's Downloads directory (cross-platform).

use rust_xlsxwriter::{Format, FormatBorder, Workbook};
use serde::Serialize;
use std::path::PathBuf;

/// Export result info
#[derive(Debug, Clone, Serialize)]
pub struct ExportInfo {
    pub file_path: String,
    pub rows: usize,
    pub cols: usize,
}

/// The user's Downloads directory across macOS / Linux / Windows:
/// - macOS, Linux: $HOME/Downloads (XDG fallback env var honoured)
/// - Windows: CSIDL/known-folder FOLDERID_Downloads via the `dirs` crate
/// Falls back to the current directory if it can't be resolved.
pub fn downloads_dir() -> PathBuf {
    // Windows uses the dirs crate (per-user Downloads known folder).
    #[cfg(target_os = "windows")]
    {
        if let Some(d) = dirs::download_dir() {
            return d;
        }
    }
    // macOS / Linux: $XDG_DOWNLOAD_DIR or $HOME/Downloads
    if let Ok(x) = std::env::var("XDG_DOWNLOAD_DIR") {
        let p = PathBuf::from(x);
        if p.is_absolute() {
            return p;
        }
    }
    if let Some(home) = std::env::var_os("HOME") {
        let d = PathBuf::from(home).join("Downloads");
        if d.is_dir() || d.parent().is_some() {
            return d;
        }
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// Export data to xlsx
/// - data: row number (starting at 1) → cell array
/// - row 1 is the header row (bold + border)
/// - other rows get borders
/// - autofilter range A1:last column/last row
pub fn export_xlsx(
    data: &std::collections::BTreeMap<u32, Vec<String>>,
) -> Result<ExportInfo, String> {
    let mut workbook = Workbook::new();

    // Header format: bold + border
    let title_format = Format::new().set_bold().set_border(FormatBorder::Thin);
    // Normal format: border
    let cell_format = Format::new().set_border(FormatBorder::Thin);

    let sheet = workbook.add_worksheet();
    sheet.set_name("Data").map_err(|e| e.to_string())?;

    let mut col_count: u16 = 0;
    let mut row_count: u32 = 0;

    for (rownum, row_data) in data {
        let row_idx = *rownum as u32 - 1; // xlsx is 0-based
        let fmt = if *rownum == 1 {
            &title_format
        } else {
            &cell_format
        };
        let mut col_idx: u16 = 0;
        for cell in row_data {
            sheet
                .write_with_format(row_idx, col_idx, cell, fmt)
                .map_err(|e| e.to_string())?;
            col_idx += 1;
        }
        col_count = col_idx.max(col_count);
        row_count = row_idx + 1;
    }

    // Autofilter
    if col_count > 0 && row_count > 1 {
        let _ = sheet.autofilter(0, 0, row_count - 1, col_count - 1);
    }

    // Timestamped filename (yyyyMMdd_HHmmss.xlsx)
    let ts = chrono_now_ts();
    let file_name = format!("{ts}.xlsx");

    let download = downloads_dir();
    // Ensure the Downloads dir exists before writing.
    if !download.exists() {
        std::fs::create_dir_all(&download).map_err(|e| format!("Failed to create Downloads dir: {e}"))?;
    }
    let path = download.join(&file_name);
    let path_str = path.to_string_lossy().to_string();

    workbook
        .save(&path)
        .map_err(|e| format!("Failed to save Excel: {e}"))?;

    Ok(ExportInfo {
        file_path: path_str,
        rows: row_count as usize,
        cols: col_count as usize,
    })
}

/// Export data to CSV (UTF-8 with BOM so Excel opens CJK correctly). Headers +
/// rows are RFC-4180 escaped (quotes doubled, commas/quotes wrapped in quotes).
pub fn export_csv(
    columns: &[String],
    rows: &[Vec<String>],
) -> Result<ExportInfo, String> {
    let mut out: Vec<u8> = Vec::new();
    // UTF-8 BOM: tells Excel/other apps this is UTF-8 (keeps Chinese intact).
    out.extend_from_slice(&[0xEF, 0xBB, 0xBF]);

    let mut write_row = |cells: &[String]| -> std::io::Result<()> {
        let line: Vec<String> = cells
            .iter()
            .map(|c| {
                let c = c.replace('"', "\"\"");
                if c.contains(',') || c.contains('"') || c.contains('\n') || c.contains('\r') {
                    format!("\"{c}\"")
                } else {
                    c
                }
            })
            .collect();
        out.extend_from_slice(line.join(",").as_bytes());
        out.push(b'\n');
        Ok(())
    };

    write_row(columns).map_err(|e| e.to_string())?;
    for row in rows {
        write_row(row).map_err(|e| e.to_string())?;
    }

    let ts = chrono_now_ts();
    let file_name = format!("{ts}.csv");
    let download = downloads_dir();
    if !download.exists() {
        std::fs::create_dir_all(&download).map_err(|e| format!("Failed to create Downloads dir: {e}"))?;
    }
    let path = download.join(&file_name);
    let path_str = path.to_string_lossy().to_string();

    std::fs::write(&path, &out).map_err(|e| format!("Failed to save CSV: {e}"))?;

    Ok(ExportInfo {
        file_path: path_str,
        rows: rows.len(),
        cols: columns.len(),
    })
}

/// Current timestamp yyyyMMdd_HHmmss (no chrono dep; formatted manually from
/// the system clock)
fn chrono_now_ts() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Simple local time (UTC+0 approximation; fine for a filename)
    let days = secs / 86400;
    let rem = secs % 86400;
    let (y, m, d) = civil_from_days(days as i64);
    let hh = rem / 3600;
    let mm = (rem % 3600) / 60;
    let ss = rem % 60;
    format!("{y:04}{m:02}{d:02}_{hh:02}{mm:02}{ss:02}")
}

/// Days → (year, month, day) proleptic Gregorian conversion (Howard Hinnant algorithm)
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}
