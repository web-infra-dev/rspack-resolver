use cfg_if::cfg_if;
use futures::future::BoxFuture;
use std::{
    fs, io,
    path::{Path, PathBuf},
};

#[cfg(feature = "yarn_pnp")]
use pnp::fs::{LruZipCache, VPath, VPathInfo, ZipCache};

/// File System abstraction used for `ResolverGeneric`
pub trait FileSystem {
    /// See [std::fs::read]
    ///
    /// # Errors
    ///
    /// See [std::fs::read]
    fn read<'a>(&'a self, path: &'a Path) -> BoxFuture<'a, io::Result<Vec<u8>>>;
    /// See [std::fs::read_to_string]
    ///
    /// # Errors
    ///
    /// * See [std::fs::read_to_string]
    /// ## Warning
    /// Use `&Path` instead of a generic `P: AsRef<Path>` here,
    /// because object safety requirements, it is especially useful, when
    /// you want to store multiple `dyn FileSystem` in a `Vec` or use a `ResolverGeneric<Fs>` in
    /// napi env.
    fn read_to_string<'a>(&'a self, path: &'a Path) -> BoxFuture<'a, io::Result<String>>;

    /// See [std::fs::metadata]
    ///
    /// # Errors
    /// See [std::fs::metadata]
    /// ## Warning
    /// Use `&Path` instead of a generic `P: AsRef<Path>` here,
    /// because object safety requirements, it is especially useful, when
    /// you want to store multiple `dyn FileSystem` in a `Vec` or use a `ResolverGeneric<Fs>` in
    /// napi env.
    fn metadata<'a>(&'a self, path: &'a Path) -> BoxFuture<'a, io::Result<FileMetadata>>;

    /// See [std::fs::symlink_metadata]
    ///
    /// # Errors
    ///
    /// See [std::fs::symlink_metadata]
    /// ## Warning
    /// Use `&Path` instead of a generic `P: AsRef<Path>` here,
    /// because object safety requirements, it is especially useful, when
    /// you want to store multiple `dyn FileSystem` in a `Vec` or use a `ResolverGeneric<Fs>` in
    /// napi env.
    fn symlink_metadata<'a>(&'a self, path: &'a Path) -> BoxFuture<'a, io::Result<FileMetadata>>;

    /// See [std::fs::canonicalize]
    ///
    /// # Errors
    ///
    /// See [std::fs::read_link]
    /// ## Warning
    /// Use `&Path` instead of a generic `P: AsRef<Path>` here,
    /// because object safety requirements, it is especially useful, when
    /// you want to store multiple `dyn FileSystem` in a `Vec` or use a `ResolverGeneric<Fs>` in
    /// napi env.
    fn canonicalize<'a>(&'a self, path: &'a Path) -> BoxFuture<'a, io::Result<PathBuf>>;
}

/// Metadata information about a file
#[derive(Debug, Clone, Copy)]
pub struct FileMetadata {
    pub is_file: bool,
    pub is_dir: bool,
    pub is_symlink: bool,
}

impl FileMetadata {
    pub fn new(is_file: bool, is_dir: bool, is_symlink: bool) -> Self {
        Self { is_file, is_dir, is_symlink }
    }
}

#[cfg(feature = "yarn_pnp")]
impl From<pnp::fs::FileType> for FileMetadata {
    fn from(value: pnp::fs::FileType) -> Self {
        Self::new(value == pnp::fs::FileType::File, value == pnp::fs::FileType::Directory, false)
    }
}

impl From<fs::Metadata> for FileMetadata {
    fn from(metadata: fs::Metadata) -> Self {
        Self::new(metadata.is_file(), metadata.is_dir(), metadata.is_symlink())
    }
}

pub struct FileSystemOptions {
    #[cfg(feature = "yarn_pnp")]
    pub enable_pnp: bool,
}

impl Default for FileSystemOptions {
    fn default() -> Self {
        Self {
            #[cfg(feature = "yarn_pnp")]
            enable_pnp: true,
        }
    }
}

/// Operating System
pub struct FileSystemOs {
    options: FileSystemOptions,
    #[cfg(feature = "yarn_pnp")]
    pnp_lru: LruZipCache<Vec<u8>>,
}

impl Default for FileSystemOs {
    fn default() -> Self {
        Self {
            options: FileSystemOptions::default(),
            #[cfg(feature = "yarn_pnp")]
            pnp_lru: LruZipCache::new(50, pnp::fs::open_zip_via_read_p),
        }
    }
}

impl FileSystem for FileSystemOs {
    fn read<'a>(&'a self, path: &'a Path) -> BoxFuture<'a, io::Result<Vec<u8>>> {
        let fut = async move {
            cfg_if! {
              if #[cfg(feature = "yarn_pnp")] {
                if self.options.enable_pnp {
                    return match VPath::from(path)? {
                        VPath::Zip(info) => self.pnp_lru.read(info.physical_base_path(), info.zip_path),
                        VPath::Virtual(info) => std::fs::read(info.physical_base_path()),
                        VPath::Native(path) => std::fs::read(&path),
                    }
                }
            }}

            fs::read(path)
        };

        Box::pin(fut)
    }

    fn read_to_string<'a>(&'a self, path: &'a Path) -> BoxFuture<'a, io::Result<String>> {
        let fut = async move {
            cfg_if! {
            if #[cfg(feature = "yarn_pnp")] {
            if self.options.enable_pnp {
                return match VPath::from(path)? {
                    VPath::Zip(info) => self.pnp_lru.read_to_string(info.physical_base_path(), info.zip_path),
                    VPath::Virtual(info) => std::fs::read_to_string(info.physical_base_path()),
                    VPath::Native(path) => std::fs::read_to_string(&path),
                    }
                }
            }
            }

            fs::read_to_string(path)
        };

        Box::pin(fut)
    }

    fn metadata<'a>(&'a self, path: &'a Path) -> BoxFuture<'a, io::Result<FileMetadata>> {
        let fut = async move {
            cfg_if! {
                if #[cfg(feature = "yarn_pnp")] {
                if self.options.enable_pnp {
                    return match VPath::from(path)? {
                        VPath::Zip(info) => self
                            .pnp_lru
                            .file_type(info.physical_base_path(), info.zip_path)
                            .map(FileMetadata::from),
                        VPath::Virtual(info) => {
                            fs::metadata(info.physical_base_path()).map(FileMetadata::from)
                        }
                        VPath::Native(path) => fs::metadata(path).map(FileMetadata::from),
                    }
                }
                }
            }

            fs::metadata(path).map(FileMetadata::from)
        };

        Box::pin(fut)
    }

    fn symlink_metadata<'a>(&'a self, path: &'a Path) -> BoxFuture<'a, io::Result<FileMetadata>> {
        let fut = async move { fs::symlink_metadata(path).map(FileMetadata::from) };
        Box::pin(fut)
    }

    fn canonicalize<'a>(&'a self, path: &'a Path) -> BoxFuture<'a, io::Result<PathBuf>> {
        let fut = async move {
            cfg_if! {
                if #[cfg(feature = "yarn_pnp")] {
                if self.options.enable_pnp {
                    return match VPath::from(path)? {
                        VPath::Zip(info) => {
                            dunce::canonicalize(info.physical_base_path().join(info.zip_path))
                        }
                        VPath::Virtual(info) => dunce::canonicalize(info.physical_base_path()),
                        VPath::Native(path) => dunce::canonicalize(path),
                    }
                }
            }
            }

            cfg_if! {
            if #[cfg(not(target_os = "wasi"))]{
                    dunce::canonicalize(path)
                } else {
                    use std::path::Component;
                    let mut path_buf = path.to_path_buf();
                    loop {
                        let link = fs::read_link(&path_buf)?;
                        path_buf.pop();
                        for component in link.components() {
                            match component {
                                Component::ParentDir => {
                                    path_buf.pop();
                                }
                                Component::Normal(seg) => {
                                    #[cfg(target_family = "wasm")]
                                    // Need to trim the extra \0 introduces by https://github.com/nodejs/uvwasi/issues/262
                                    {
                                        path_buf.push(seg.to_string_lossy().trim_end_matches('\0'));
                                    }
                                    #[cfg(not(target_family = "wasm"))]
                                    {
                                        path_buf.push(seg);
                                    }
                                }
                                Component::RootDir => {
                                    path_buf = PathBuf::from("/");
                                }
                                Component::CurDir | Component::Prefix(_) => {}
                            }
                        }
                        if !fs::symlink_metadata(&path_buf)?.is_symlink() {
                            break;
                        }
                    }
                    Ok(path_buf)
                }
            }
        };

        Box::pin(fut)
    }
}

#[tokio::test]
async fn metadata() {
    let meta = FileMetadata { is_file: true, is_dir: true, is_symlink: true };
    assert_eq!(
        format!("{meta:?}"),
        "FileMetadata { is_file: true, is_dir: true, is_symlink: true }"
    );
    let _ = meta;
}
