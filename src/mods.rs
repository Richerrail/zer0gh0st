use std::path::PathBuf;

pub fn project_config_dir() -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    cwd.ancestors()
        .find(|d| d.join(".zer0/project.toml").is_file())
        .map(|d| d.to_path_buf())
}

pub fn project_root() -> PathBuf {
    if let Ok(root) = std::env::var("ZER0_ROOT") {
        let root = root.trim();
        if !root.is_empty() {
            return PathBuf::from(root);
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        for dir in cwd.ancestors() {
            if dir.join(".zer0/project.toml").is_file() || dir.join(".git").exists() {
                return dir.to_path_buf();
            }
        }
        if let Ok(exe) = std::env::current_exe() {
            for dir in exe.ancestors().skip(1) {
                if dir.join("Cargo.toml").is_file() {
                    return dir.to_path_buf();
                }
            }
        }
        return cwd;
    }
    PathBuf::from(".")
}
