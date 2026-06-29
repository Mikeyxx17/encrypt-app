use std::path::Path;
pub(crate) fn friendly_error(error: &str) -> String {
    if error.contains("vault.json") && error.contains("os error 2") {
        return "这里还不是保险库。第一次使用请选择一个新的空文件夹，然后点“新建保险库”；已有保险库才点“打开保险库”。".to_string();
    }
    if error.contains("folder is not empty") || error.contains("not empty") {
        return "新建保险库需要一个不存在或空的文件夹。建议填类似 D:\\BaiduNetdiskDownload\\MyVault 这样的新文件夹。".to_string();
    }
    if error.contains("vault already exists") {
        return "这个位置已经有保险库了，请点“打开保险库”。".to_string();
    }
    if error.contains("vault is already open or locked") {
        return "这个保险库已经在另一个窗口打开，或正在被其他进程使用。请先关闭另一个窗口后再打开。".to_string();
    }
    if error.contains("encrypted blob is missing") {
        return "保险库缺少密文文件，索引里记录的某个文件内容已经不在 blobs 目录中。建议先停止使用这个保险库并从备份恢复。".to_string();
    }
    if error.contains("password is incorrect") {
        return "密码不正确，或者这个保险库索引无法验证。".to_string();
    }
    if error.contains("export destination must be outside the vault folder") {
        return "不能导出到保险库文件夹里面。请选择另一个普通文件夹，例如 D:\\DecryptedFiles。"
            .to_string();
    }
    if error.contains("import source must be outside the vault folder") {
        return "不能从保险库文件夹里面导入文件。请选择保险库外面的原始文件或文件夹。".to_string();
    }
    if error.contains("operation was cancelled") {
        return "操作已取消；未完成的临时文件已尽量清理。".to_string();
    }
    if error.contains("not enough disk space") {
        return "磁盘空间不足，请换一个空间更大的位置或清理磁盘后再试。".to_string();
    }
    if error.contains("entry not found") {
        return "选中的文件或文件夹不存在，可能已经被删除或索引已变化。".to_string();
    }
    if error.contains("entry already exists") {
        return "同名文件或文件夹已经存在。导入时可以把同名策略切换为“自动改名”“跳过”或“覆盖”。"
            .to_string();
    }
    if error.contains("parent directory does not exist") {
        return "目标父文件夹不存在。请先进入已有文件夹，或先新建上级文件夹。".to_string();
    }
    if error.contains("cannot overwrite") {
        return "同名位置的类型不同，不能用文件覆盖文件夹，也不能用文件夹覆盖文件。".to_string();
    }
    if error.contains("deleting the vault root") {
        return "不能删除保险库根目录。请选中某个文件或文件夹后再删除。".to_string();
    }
    error.to_string()
}

pub(crate) fn load_cjk_font() -> Option<(&'static str, Vec<u8>)> {
    let candidates = [
        ("Microsoft YaHei", r"C:\Windows\Fonts\msyh.ttc"),
        ("Microsoft YaHei UI", r"C:\Windows\Fonts\msyh.ttc"),
        ("SimHei", r"C:\Windows\Fonts\simhei.ttf"),
        ("SimSun", r"C:\Windows\Fonts\simsun.ttc"),
    ];

    candidates.iter().find_map(|(name, path)| {
        std::fs::read(Path::new(path))
            .ok()
            .map(|bytes| (*name, bytes))
    })
}

pub(crate) fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", bytes, UNITS[unit])
    } else {
        format!("{value:.2} {}", UNITS[unit])
    }
}

pub(crate) fn now_display_time() -> String {
    chrono::Utc::now()
        .format("%Y-%m-%d %H:%M:%S UTC")
        .to_string()
}

pub(crate) fn format_display_time(value: &str) -> String {
    value
        .split_once('.')
        .map(|(prefix, _)| prefix)
        .unwrap_or(value)
        .replace('T', " ")
}

pub(crate) fn format_duration_seconds(seconds: f64) -> String {
    if seconds < 60.0 {
        format!("{}秒", seconds as u64)
    } else if seconds < 3600.0 {
        format!("{}分{}秒", seconds as u64 / 60, seconds as u64 % 60)
    } else {
        let hours = seconds as u64 / 3600;
        let minutes = (seconds as u64 % 3600) / 60;
        format!("{}时{}分", hours, minutes)
    }
}

pub(crate) fn truncate_middle(value: &str, max_chars: usize) -> String {
    let char_count = value.chars().count();
    if char_count <= max_chars || max_chars < 8 {
        return value.to_string();
    }

    let left_len = (max_chars - 1) / 2;
    let right_len = max_chars - 1 - left_len;
    let left = value.chars().take(left_len).collect::<String>();
    let right = value
        .chars()
        .rev()
        .take(right_len)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<String>();
    format!("{left}…{right}")
}
