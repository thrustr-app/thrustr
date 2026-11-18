use std::path::Path;

pub fn data() -> &'static Path {
    Path::new("/data")
}

pub fn download() -> &'static Path {
    Path::new("/download")
}

pub fn scratch() -> &'static Path {
    Path::new("/download/.thrustr")
}
