use std::io;
use std::path;

fn list_files_vec(paths: &mut Vec<path::PathBuf>, path: &path::Path) -> Result<(), io::Error> {
    if path.is_dir() {
        for entry in path.read_dir()? {
            let dir = entry?.path();
            list_files_vec(paths, dir.as_path())?;
        }
    } else if path.exists() {
        paths.push(path.to_path_buf());
    } else {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            path.as_os_str().to_str().expect("BadPath"),
        ));
    }

    Ok(())
}

pub fn list_files(path: &path::Path) -> Result<Vec<path::PathBuf>, io::Error> {
    let mut paths = Vec::new();
    list_files_vec(&mut paths, path)?;
    Ok(paths)
}
