use std::{
    fs,
    path::{Component, Path, PathBuf},
};

fn canonicalize_existing(path: &Path) -> Result<PathBuf, String> {
    path.canonicalize()
        .map_err(|e| format!("failed to canonicalize {}: {e}", path.display()))
}

fn narrative_base(repo_root: &str) -> Result<PathBuf, String> {
    let root = PathBuf::from(repo_root);
    if !root.exists() {
        return Err(format!("repo root does not exist: {repo_root}"));
    }
    let root = canonicalize_existing(&root)?;
    Ok(root.join(".narrative"))
}

fn validate_rel(rel: &str) -> Result<PathBuf, String> {
    let p = PathBuf::from(rel);
    if p.is_absolute() {
        return Err("relative path must not be absolute".into());
    }
    for c in p.components() {
        match c {
            Component::Normal(_) => {}
            _ => return Err("relative path contains invalid components".into()),
        }
    }
    Ok(p)
}

#[tauri::command(rename_all = "camelCase")]
pub fn ensure_narrative_dirs(repo_root: String) -> Result<(), String> {
    let base = narrative_base(&repo_root)?;
    fs::create_dir_all(base.join("meta/commits")).map_err(|e| e.to_string())?;
    fs::create_dir_all(base.join("meta/branches")).map_err(|e| e.to_string())?;
    fs::create_dir_all(base.join("sessions/imported")).map_err(|e| e.to_string())?;
    fs::create_dir_all(base.join("trace")).map_err(|e| e.to_string())?;
    fs::create_dir_all(base.join("trace/generated")).map_err(|e| e.to_string())?;
    fs::create_dir_all(base.join("rules")).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command(rename_all = "camelCase")]
pub fn write_narrative_file(
    repo_root: String,
    relative_path: String,
    contents: String,
) -> Result<(), String> {
    let base = narrative_base(&repo_root)?;
    let rel = validate_rel(&relative_path)?;
    let target = base.join(rel);

    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    fs::write(&target, contents).map_err(|e| format!("write failed: {e}"))?;
    Ok(())
}

#[tauri::command(rename_all = "camelCase")]
pub fn read_narrative_file(repo_root: String, relative_path: String) -> Result<String, String> {
    let base = narrative_base(&repo_root)?;
    let rel = validate_rel(&relative_path)?;
    let target = base.join(rel);

    fs::read_to_string(&target).map_err(|e| format!("read failed: {e}"))
}

fn walk_files(dir: &Path, base: &Path, out: &mut Vec<String>) -> Result<(), String> {
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let p = entry.path();
        if p.is_dir() {
            walk_files(&p, base, out)?;
        } else if p.is_file() {
            let rel = p
                .strip_prefix(base)
                .map_err(|_| "strip_prefix failed".to_string())?
                .to_string_lossy()
                .replace('\\', "/");
            out.push(rel);
        }
    }
    Ok(())
}

#[tauri::command(rename_all = "camelCase")]
pub fn list_narrative_files(
    repo_root: String,
    relative_dir: String,
) -> Result<Vec<String>, String> {
    let base = narrative_base(&repo_root)?;
    let rel = validate_rel(&relative_dir)?;
    let dir = base.join(rel);

    if !dir.exists() {
        return Ok(vec![]);
    }
    if !dir.is_dir() {
        return Err("relative_dir must point to a directory".into());
    }

    let mut out: Vec<String> = vec![];
    walk_files(&dir, &base, &mut out)?;
    out.sort();
    Ok(out)
}

#[tauri::command]
pub fn read_text_file(path: String) -> Result<String, String> {
    let p = PathBuf::from(&path);
    if !p.exists() || !p.is_file() {
        return Err("path does not exist or is not a file".into());
    }

    let meta = fs::metadata(&p).map_err(|e| e.to_string())?;
    // Hard cap (MVP): 5MB
    if meta.len() > 5 * 1024 * 1024 {
        return Err("file too large (max 5MB)".into());
    }

    fs::read_to_string(&p).map_err(|e| format!("read failed: {e}"))
}
