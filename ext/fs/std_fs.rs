// Copyright 2018-2024 the Deno authors. All rights reserved. MIT license.

#![allow(clippy::disallowed_methods)]

use std::fs;
use std::io;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::rc::Rc;

use deno_core::unsync::spawn_blocking;
use deno_io::fs::File;
use deno_io::fs::FsResult;
use deno_io::fs::FsStat;
use deno_io::StdFileResourceInner;

use crate::interface::FsDirEntry;
use crate::interface::FsFileType;
use crate::FileSystem;
use crate::OpenOptions;

#[cfg(not(unix))]
use deno_io::fs::FsError;

#[derive(Debug, Clone)]
pub struct RealFs;

#[async_trait::async_trait(?Send)]
impl FileSystem for RealFs {
  fn cwd(&self) -> FsResult<PathBuf> {
    std::env::current_dir().map_err(Into::into)
  }

  fn tmp_dir(&self) -> FsResult<PathBuf> {
    Ok(std::env::temp_dir())
  }

  fn chdir(&self, path: &Path) -> FsResult<()> {
    std::env::set_current_dir(path).map_err(Into::into)
  }

  #[cfg(not(unix))]
  fn umask(&self, _mask: Option<u32>) -> FsResult<u32> {
    // TODO implement umask for Windows
    // see https://github.com/nodejs/node/blob/master/src/node_process_methods.cc
    // and https://docs.microsoft.com/fr-fr/cpp/c-runtime-library/reference/umask?view=vs-2019
    Err(FsError::NotSupported)
  }

  #[cfg(unix)]
  fn umask(&self, mask: Option<u32>) -> FsResult<u32> {
    use nix::sys::stat::mode_t;
    use nix::sys::stat::umask;
    use nix::sys::stat::Mode;
    let r = if let Some(mask) = mask {
      // If mask provided, return previous.
      umask(Mode::from_bits_truncate(mask as mode_t))
    } else {
      // If no mask provided, we query the current. Requires two syscalls.
      let prev = umask(Mode::from_bits_truncate(0o777));
      let _ = umask(prev);
      prev
    };
    #[cfg(any(target_os = "android", target_os = "linux"))]
    {
      Ok(r.bits())
    }
    #[cfg(any(
      target_os = "macos",
      target_os = "openbsd",
      target_os = "freebsd"
    ))]
    {
      Ok(r.bits() as u32)
    }
  }

  fn open_sync(
    &self,
    path: &Path,
    options: OpenOptions,
  ) -> FsResult<Rc<dyn File>> {
    let opts = open_options(options);
    let std_file = opts.open(path)?;
    Ok(Rc::new(StdFileResourceInner::file(std_file)))
  }
  async fn open_async(
    &self,
    path: PathBuf,
    options: OpenOptions,
  ) -> FsResult<Rc<dyn File>> {
    let opts = open_options(options);
    let std_file = spawn_blocking(move || opts.open(path)).await??;
    Ok(Rc::new(StdFileResourceInner::file(std_file)))
  }

  fn mkdir_sync(
    &self,
    path: &Path,
    recursive: bool,
    mode: u32,
  ) -> FsResult<()> {
    mkdir(path, recursive, mode)
  }
  async fn mkdir_async(
    &self,
    path: PathBuf,
    recursive: bool,
    mode: u32,
  ) -> FsResult<()> {
    spawn_blocking(move || mkdir(&path, recursive, mode)).await?
  }

  fn chmod_sync(&self, path: &Path, mode: u32) -> FsResult<()> {
    chmod(path, mode)
  }
  async fn chmod_async(&self, path: PathBuf, mode: u32) -> FsResult<()> {
    spawn_blocking(move || chmod(&path, mode)).await?
  }

  fn chown_sync(
    &self,
    path: &Path,
    uid: Option<u32>,
    gid: Option<u32>,
  ) -> FsResult<()> {
    chown(path, uid, gid)
  }
  async fn chown_async(
    &self,
    path: PathBuf,
    uid: Option<u32>,
    gid: Option<u32>,
  ) -> FsResult<()> {
    spawn_blocking(move || chown(&path, uid, gid)).await?
  }

  fn remove_sync(&self, path: &Path, recursive: bool) -> FsResult<()> {
    remove(path, recursive)
  }
  async fn remove_async(&self, path: PathBuf, recursive: bool) -> FsResult<()> {
    spawn_blocking(move || remove(&path, recursive)).await?
  }

  fn copy_file_sync(&self, from: &Path, to: &Path) -> FsResult<()> {
    copy_file(from, to)
  }
  async fn copy_file_async(&self, from: PathBuf, to: PathBuf) -> FsResult<()> {
    spawn_blocking(move || copy_file(&from, &to)).await?
  }

  fn cp_sync(&self, fro: &Path, to: &Path) -> FsResult<()> {
    cp(fro, to)
  }
  async fn cp_async(&self, fro: PathBuf, to: PathBuf) -> FsResult<()> {
    spawn_blocking(move || cp(&fro, &to)).await?
  }

  fn stat_sync(&self, path: &Path) -> FsResult<FsStat> {
    stat(path).map(Into::into)
  }
  async fn stat_async(&self, path: PathBuf) -> FsResult<FsStat> {
    spawn_blocking(move || stat(&path)).await?.map(Into::into)
  }

  fn lstat_sync(&self, path: &Path) -> FsResult<FsStat> {
    lstat(path).map(Into::into)
  }
  async fn lstat_async(&self, path: PathBuf) -> FsResult<FsStat> {
    spawn_blocking(move || lstat(&path)).await?.map(Into::into)
  }

  fn realpath_sync(&self, path: &Path) -> FsResult<PathBuf> {
    realpath(path)
  }
  async fn realpath_async(&self, path: PathBuf) -> FsResult<PathBuf> {
    spawn_blocking(move || realpath(&path)).await?
  }

  fn read_dir_sync(&self, path: &Path) -> FsResult<Vec<FsDirEntry>> {
    read_dir(path)
  }
  async fn read_dir_async(&self, path: PathBuf) -> FsResult<Vec<FsDirEntry>> {
    spawn_blocking(move || read_dir(&path)).await?
  }

  fn rename_sync(&self, oldpath: &Path, newpath: &Path) -> FsResult<()> {
    fs::rename(oldpath, newpath).map_err(Into::into)
  }
  async fn rename_async(
    &self,
    oldpath: PathBuf,
    newpath: PathBuf,
  ) -> FsResult<()> {
    spawn_blocking(move || fs::rename(oldpath, newpath))
      .await?
      .map_err(Into::into)
  }

  fn link_sync(&self, oldpath: &Path, newpath: &Path) -> FsResult<()> {
    fs::hard_link(oldpath, newpath).map_err(Into::into)
  }
  async fn link_async(
    &self,
    oldpath: PathBuf,
    newpath: PathBuf,
  ) -> FsResult<()> {
    spawn_blocking(move || fs::hard_link(oldpath, newpath))
      .await?
      .map_err(Into::into)
  }

  fn symlink_sync(
    &self,
    oldpath: &Path,
    newpath: &Path,
    file_type: Option<FsFileType>,
  ) -> FsResult<()> {
    symlink(oldpath, newpath, file_type)
  }
  async fn symlink_async(
    &self,
    oldpath: PathBuf,
    newpath: PathBuf,
    file_type: Option<FsFileType>,
  ) -> FsResult<()> {
    spawn_blocking(move || symlink(&oldpath, &newpath, file_type)).await?
  }

  fn read_link_sync(&self, path: &Path) -> FsResult<PathBuf> {
    fs::read_link(path).map_err(Into::into)
  }
  async fn read_link_async(&self, path: PathBuf) -> FsResult<PathBuf> {
    spawn_blocking(move || fs::read_link(path))
      .await?
      .map_err(Into::into)
  }

  fn truncate_sync(&self, path: &Path, len: u64) -> FsResult<()> {
    truncate(path, len)
  }
  async fn truncate_async(&self, path: PathBuf, len: u64) -> FsResult<()> {
    spawn_blocking(move || truncate(&path, len)).await?
  }

  fn utime_sync(
    &self,
    path: &Path,
    atime_secs: i64,
    atime_nanos: u32,
    mtime_secs: i64,
    mtime_nanos: u32,
  ) -> FsResult<()> {
    let atime = filetime::FileTime::from_unix_time(atime_secs, atime_nanos);
    let mtime = filetime::FileTime::from_unix_time(mtime_secs, mtime_nanos);
    filetime::set_file_times(path, atime, mtime).map_err(Into::into)
  }
  async fn utime_async(
    &self,
    path: PathBuf,
    atime_secs: i64,
    atime_nanos: u32,
    mtime_secs: i64,
    mtime_nanos: u32,
  ) -> FsResult<()> {
    let atime = filetime::FileTime::from_unix_time(atime_secs, atime_nanos);
    let mtime = filetime::FileTime::from_unix_time(mtime_secs, mtime_nanos);
    spawn_blocking(move || {
      filetime::set_file_times(path, atime, mtime).map_err(Into::into)
    })
    .await?
  }

  fn write_file_sync(
    &self,
    path: &Path,
    options: OpenOptions,
    data: &[u8],
  ) -> FsResult<()> {
    let opts = open_options(options);
    let mut file = opts.open(path)?;
    #[cfg(unix)]
    if let Some(mode) = options.mode {
      use std::os::unix::fs::PermissionsExt;
      file.set_permissions(fs::Permissions::from_mode(mode))?;
    }
    file.write_all(data)?;
    Ok(())
  }

  async fn write_file_async(
    &self,
    path: PathBuf,
    options: OpenOptions,
    data: Vec<u8>,
  ) -> FsResult<()> {
    spawn_blocking(move || {
      let opts = open_options(options);
      let mut file = opts.open(path)?;
      #[cfg(unix)]
      if let Some(mode) = options.mode {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(mode))?;
      }
      file.write_all(&data)?;
      Ok(())
    })
    .await?
  }

  fn read_file_sync(&self, path: &Path) -> FsResult<Vec<u8>> {
    fs::read(path).map_err(Into::into)
  }
  async fn read_file_async(&self, path: PathBuf) -> FsResult<Vec<u8>> {
    spawn_blocking(move || fs::read(path))
      .await?
      .map_err(Into::into)
  }
}

fn mkdir(path: &Path, recursive: bool, mode: u32) -> FsResult<()> {
  let mut builder = fs::DirBuilder::new();
  builder.recursive(recursive);
  #[cfg(unix)]
  {
    use std::os::unix::fs::DirBuilderExt;
    builder.mode(mode);
  }
  #[cfg(not(unix))]
  {
    _ = mode;
  }
  builder.create(path).map_err(Into::into)
}

#[cfg(unix)]
fn chmod(path: &Path, mode: u32) -> FsResult<()> {
  use std::os::unix::fs::PermissionsExt;
  let permissions = fs::Permissions::from_mode(mode);
  fs::set_permissions(path, permissions)?;
  Ok(())
}

// TODO: implement chmod for Windows (#4357)
#[cfg(not(unix))]
fn chmod(path: &Path, _mode: u32) -> FsResult<()> {
  // Still check file/dir exists on Windows
  std::fs::metadata(path)?;
  Err(FsError::NotSupported)
}

#[cfg(unix)]
fn chown(path: &Path, uid: Option<u32>, gid: Option<u32>) -> FsResult<()> {
  use nix::unistd::chown;
  use nix::unistd::Gid;
  use nix::unistd::Uid;
  let owner = uid.map(Uid::from_raw);
  let group = gid.map(Gid::from_raw);
  let res = chown(path, owner, group);
  if let Err(err) = res {
    return Err(io::Error::from_raw_os_error(err as i32).into());
  }
  Ok(())
}

// TODO: implement chown for Windows
#[cfg(not(unix))]
fn chown(_path: &Path, _uid: Option<u32>, _gid: Option<u32>) -> FsResult<()> {
  Err(FsError::NotSupported)
}

fn remove(path: &Path, recursive: bool) -> FsResult<()> {
  // TODO: this is racy. This should open fds, and then `unlink` those.
  let metadata = fs::symlink_metadata(path)?;

  let file_type = metadata.file_type();
  let res = if file_type.is_dir() {
    if recursive {
      fs::remove_dir_all(path)
    } else {
      fs::remove_dir(path)
    }
  } else if file_type.is_symlink() {
    #[cfg(unix)]
    {
      fs::remove_file(path)
    }
    #[cfg(not(unix))]
    {
      use std::os::windows::prelude::MetadataExt;
      use winapi::um::winnt::FILE_ATTRIBUTE_DIRECTORY;
      if metadata.file_attributes() & FILE_ATTRIBUTE_DIRECTORY != 0 {
        fs::remove_dir(path)
      } else {
        fs::remove_file(path)
      }
    }
  } else {
    fs::remove_file(path)
  };

  res.map_err(Into::into)
}

fn copy_file(from: &Path, to: &Path) -> FsResult<()> {
  #[cfg(target_os = "macos")]
  {
    use libc::clonefile;
    use libc::stat;
    use libc::unlink;
    use std::ffi::CString;
    use std::io::Read;
    use std::os::unix::fs::OpenOptionsExt;
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::prelude::OsStrExt;

    let from_str = CString::new(from.as_os_str().as_bytes()).unwrap();
    let to_str = CString::new(to.as_os_str().as_bytes()).unwrap();

    // SAFETY: `from` and `to` are valid C strings.
    // std::fs::copy does open() + fcopyfile() on macOS. We try to use
    // clonefile() instead, which is more efficient.
    unsafe {
      let mut st = std::mem::zeroed();
      let ret = stat(from_str.as_ptr(), &mut st);
      if ret != 0 {
        return Err(io::Error::last_os_error().into());
      }

      if st.st_size > 128 * 1024 {
        // Try unlink. If it fails, we are going to try clonefile() anyway.
        let _ = unlink(to_str.as_ptr());
        // Matches rust stdlib behavior for io::copy.
        // https://github.com/rust-lang/rust/blob/3fdd578d72a24d4efc2fe2ad18eec3b6ba72271e/library/std/src/sys/unix/fs.rs#L1613-L1616
        if clonefile(from_str.as_ptr(), to_str.as_ptr(), 0) == 0 {
          return Ok(());
        }
      } else {
        // Do a regular copy. fcopyfile() is an overkill for < 128KB
        // files.
        let mut buf = [0u8; 128 * 1024];
        let mut from_file = fs::File::open(from)?;
        let perm = from_file.metadata()?.permissions();

        let mut to_file = fs::OpenOptions::new()
          // create the file with the correct mode right away
          .mode(perm.mode())
          .write(true)
          .create(true)
          .truncate(true)
          .open(to)?;
        let writer_metadata = to_file.metadata()?;
        if writer_metadata.is_file() {
          // Set the correct file permissions, in case the file already existed.
          // Don't set the permissions on already existing non-files like
          // pipes/FIFOs or device nodes.
          to_file.set_permissions(perm)?;
        }
        loop {
          let nread = from_file.read(&mut buf)?;
          if nread == 0 {
            break;
          }
          to_file.write_all(&buf[..nread])?;
        }
        return Ok(());
      }
    }

    // clonefile() failed, fall back to std::fs::copy().
  }

  fs::copy(from, to)?;

  Ok(())
}

fn cp(from: &Path, to: &Path) -> FsResult<()> {
  fn cp_(source_meta: fs::Metadata, from: &Path, to: &Path) -> FsResult<()> {
    use rayon::prelude::IntoParallelIterator;
    use rayon::prelude::ParallelIterator;

    let ty = source_meta.file_type();
    if ty.is_dir() {
      #[allow(unused_mut)]
      let mut builder = fs::DirBuilder::new();
      #[cfg(unix)]
      {
        use std::os::unix::fs::DirBuilderExt;
        use std::os::unix::fs::PermissionsExt;
        builder.mode(fs::symlink_metadata(from)?.permissions().mode());
      }
      builder.create(to)?;

      let mut entries: Vec<_> = fs::read_dir(from)?
        .map(|res| res.map(|e| e.file_name()))
        .collect::<Result<_, _>>()?;

      entries.shrink_to_fit();
      entries
        .into_par_iter()
        .map(|file_name| {
          cp_(
            fs::symlink_metadata(from.join(&file_name)).unwrap(),
            &from.join(&file_name),
            &to.join(&file_name),
          )
          .map_err(|err| {
            io::Error::new(
              err.kind(),
              format!(
                "failed to copy '{}' to '{}': {:?}",
                from.join(&file_name).display(),
                to.join(&file_name).display(),
                err
              ),
            )
          })
        })
        .collect::<Result<Vec<_>, _>>()?;

      return Ok(());
    } else if ty.is_symlink() {
      let from = std::fs::read_link(from)?;

      #[cfg(unix)]
      std::os::unix::fs::symlink(from, to)?;
      #[cfg(windows)]
      std::os::windows::fs::symlink_file(from, to)?;

      return Ok(());
    }
    #[cfg(unix)]
    {
      use std::os::unix::fs::FileTypeExt;
      if ty.is_socket() {
        return Err(
          io::Error::new(
            io::ErrorKind::InvalidInput,
            "sockets cannot be copied",
          )
          .into(),
        );
      }
    }
    copy_file(from, to)
  }

  #[cfg(target_os = "macos")]
  {
    // Just clonefile()
    use libc::clonefile;
    use libc::unlink;
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let from_str = CString::new(from.as_os_str().as_bytes()).unwrap();
    let to_str = CString::new(to.as_os_str().as_bytes()).unwrap();

    // SAFETY: `from` and `to` are valid C strings.
    unsafe {
      // Try unlink. If it fails, we are going to try clonefile() anyway.
      let _ = unlink(to_str.as_ptr());

      if clonefile(from_str.as_ptr(), to_str.as_ptr(), 0) == 0 {
        return Ok(());
      }
    }
  }

  let source_meta = fs::symlink_metadata(from)?;

  #[inline]
  fn is_identical(
    source_meta: &fs::Metadata,
    dest_meta: &fs::Metadata,
  ) -> bool {
    #[cfg(unix)]
    {
      use std::os::unix::fs::MetadataExt;
      source_meta.ino() == dest_meta.ino()
    }
    #[cfg(windows)]
    {
      use std::os::windows::fs::MetadataExt;
      // https://learn.microsoft.com/en-us/windows/win32/api/fileapi/ns-fileapi-by_handle_file_information
      //
      // The identifier (low and high parts) and the volume serial number uniquely identify a file on a single computer.
      // To determine whether two open handles represent the same file, combine the identifier and the volume serial
      // number for each file and compare them.
      //
      // Use this code once file_index() and volume_serial_number() is stabalized
      // See: https://github.com/rust-lang/rust/issues/63010
      //
      // source_meta.file_index() == dest_meta.file_index()
      //   && source_meta.volume_serial_number()
      //     == dest_meta.volume_serial_number()
      source_meta.last_write_time() == dest_meta.last_write_time()
        && source_meta.creation_time() == dest_meta.creation_time()
    }
  }

  match (fs::metadata(to), fs::symlink_metadata(to)) {
    (Ok(m), _) if m.is_dir() => cp_(
      source_meta,
      from,
      &to.join(from.file_name().ok_or_else(|| {
        io::Error::new(
          io::ErrorKind::InvalidInput,
          "the source path is not a valid file",
        )
      })?),
    )?,
    (_, Ok(m)) if is_identical(&source_meta, &m) => {
      return Err(
        io::Error::new(
          io::ErrorKind::InvalidInput,
          "the source and destination are the same file",
        )
        .into(),
      )
    }
    _ => cp_(source_meta, from, to)?,
  }

  Ok(())
}

#[cfg(not(windows))]
fn stat(path: &Path) -> FsResult<FsStat> {
  let metadata = fs::metadata(path)?;
  Ok(FsStat::from_std(metadata))
}

#[cfg(windows)]
fn stat(path: &Path) -> FsResult<FsStat> {
  let metadata = fs::metadata(path)?;
  let mut fsstat = FsStat::from_std(metadata);
  use winapi::um::winbase::FILE_FLAG_BACKUP_SEMANTICS;
  let path = path.canonicalize()?;
  stat_extra(&mut fsstat, &path, FILE_FLAG_BACKUP_SEMANTICS)?;
  Ok(fsstat)
}

#[cfg(not(windows))]
fn lstat(path: &Path) -> FsResult<FsStat> {
  let metadata = fs::symlink_metadata(path)?;
  Ok(FsStat::from_std(metadata))
}

#[cfg(windows)]
fn lstat(path: &Path) -> FsResult<FsStat> {
  use winapi::um::winbase::FILE_FLAG_BACKUP_SEMANTICS;
  use winapi::um::winbase::FILE_FLAG_OPEN_REPARSE_POINT;

  let metadata = fs::symlink_metadata(path)?;
  let mut fsstat = FsStat::from_std(metadata);
  stat_extra(
    &mut fsstat,
    path,
    FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
  )?;
  Ok(fsstat)
}

#[cfg(windows)]
fn stat_extra(
  fsstat: &mut FsStat,
  path: &Path,
  file_flags: winapi::shared::minwindef::DWORD,
) -> FsResult<()> {
  use std::os::windows::prelude::OsStrExt;

  use winapi::um::fileapi::CreateFileW;
  use winapi::um::fileapi::OPEN_EXISTING;
  use winapi::um::handleapi::CloseHandle;
  use winapi::um::handleapi::INVALID_HANDLE_VALUE;
  use winapi::um::winnt::FILE_SHARE_DELETE;
  use winapi::um::winnt::FILE_SHARE_READ;
  use winapi::um::winnt::FILE_SHARE_WRITE;

  unsafe fn get_dev(
    handle: winapi::shared::ntdef::HANDLE,
  ) -> std::io::Result<u64> {
    use winapi::shared::minwindef::FALSE;
    use winapi::um::fileapi::GetFileInformationByHandle;
    use winapi::um::fileapi::BY_HANDLE_FILE_INFORMATION;

    let info = {
      let mut info =
        std::mem::MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::zeroed();
      if GetFileInformationByHandle(handle, info.as_mut_ptr()) == FALSE {
        return Err(std::io::Error::last_os_error());
      }

      info.assume_init()
    };

    Ok(info.dwVolumeSerialNumber as u64)
  }

  // SAFETY: winapi calls
  unsafe {
    let mut path: Vec<_> = path.as_os_str().encode_wide().collect();
    path.push(0);
    let file_handle = CreateFileW(
      path.as_ptr(),
      0,
      FILE_SHARE_READ | FILE_SHARE_DELETE | FILE_SHARE_WRITE,
      std::ptr::null_mut(),
      OPEN_EXISTING,
      file_flags,
      std::ptr::null_mut(),
    );
    if file_handle == INVALID_HANDLE_VALUE {
      return Err(std::io::Error::last_os_error().into());
    }

    let result = get_dev(file_handle);
    CloseHandle(file_handle);
    fsstat.dev = result?;

    Ok(())
  }
}

fn realpath(path: &Path) -> FsResult<PathBuf> {
  Ok(deno_core::strip_unc_prefix(path.canonicalize()?))
}

fn read_dir(path: &Path) -> FsResult<Vec<FsDirEntry>> {
  let entries = fs::read_dir(path)?
    .filter_map(|entry| {
      let entry = entry.ok()?;
      let name = entry.file_name().into_string().ok()?;
      let metadata = entry.file_type();
      macro_rules! method_or_false {
        ($method:ident) => {
          if let Ok(metadata) = &metadata {
            metadata.$method()
          } else {
            false
          }
        };
      }
      Some(FsDirEntry {
        name,
        is_file: method_or_false!(is_file),
        is_directory: method_or_false!(is_dir),
        is_symlink: method_or_false!(is_symlink),
      })
    })
    .collect();

  Ok(entries)
}

#[cfg(not(windows))]
fn symlink(
  oldpath: &Path,
  newpath: &Path,
  _file_type: Option<FsFileType>,
) -> FsResult<()> {
  std::os::unix::fs::symlink(oldpath, newpath)?;
  Ok(())
}

#[cfg(windows)]
fn symlink(
  oldpath: &Path,
  newpath: &Path,
  file_type: Option<FsFileType>,
) -> FsResult<()> {
  let file_type = match file_type {
    Some(file_type) => file_type,
    None => {
      let old_meta = fs::metadata(oldpath);
      match old_meta {
        Ok(metadata) => {
          if metadata.is_file() {
            FsFileType::File
          } else if metadata.is_dir() {
            FsFileType::Directory
          } else {
            return Err(FsError::Io(io::Error::new(
              io::ErrorKind::InvalidInput,
              "On Windows the target must be a file or directory",
            )));
          }
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
          return Err(FsError::Io(io::Error::new(
            io::ErrorKind::InvalidInput,
            "On Windows an `options` argument is required if the target does not exist",
          )))
        }
        Err(err) => return Err(err.into()),
      }
    }
  };

  match file_type {
    FsFileType::File => {
      std::os::windows::fs::symlink_file(oldpath, newpath)?;
    }
    FsFileType::Directory => {
      std::os::windows::fs::symlink_dir(oldpath, newpath)?;
    }
  };

  Ok(())
}

fn truncate(path: &Path, len: u64) -> FsResult<()> {
  let file = fs::OpenOptions::new().write(true).open(path)?;
  file.set_len(len)?;
  Ok(())
}

fn open_options(options: OpenOptions) -> fs::OpenOptions {
  let mut open_options = fs::OpenOptions::new();
  if let Some(mode) = options.mode {
    // mode only used if creating the file on Unix
    // if not specified, defaults to 0o666
    #[cfg(unix)]
    {
      use std::os::unix::fs::OpenOptionsExt;
      open_options.mode(mode & 0o777);
    }
    #[cfg(not(unix))]
    let _ = mode; // avoid unused warning
  }
  open_options.read(options.read);
  open_options.create(options.create);
  open_options.write(options.write);
  open_options.truncate(options.truncate);
  open_options.append(options.append);
  open_options.create_new(options.create_new);
  open_options
}
