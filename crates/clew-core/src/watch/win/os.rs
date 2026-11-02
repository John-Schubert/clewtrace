//! The calls into Windows. A `Reader` owns the folder handle, the read's event, its buffer
//! and its OVERLAPPED; the buffer and the OVERLAPPED are on the heap, so they stay put
//! while a read is pending, and dropping a `Reader` cancels a pending read and waits for
//! it before they are freed.

// @begin CT-EFF-008, CT-EFF-007, CT-USE-009
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::ptr::{null, null_mut};
use std::sync::Arc;
use std::sync::mpsc::Sender;

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_NOTIFY_ENUM_DIR, HANDLE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OVERLAPPED, FILE_LIST_DIRECTORY,
    FILE_NOTIFY_CHANGE_DIR_NAME, FILE_NOTIFY_CHANGE_FILE_NAME, FILE_NOTIFY_CHANGE_LAST_WRITE,
    FILE_NOTIFY_CHANGE_SIZE, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    ReadDirectoryChangesW,
};
use windows_sys::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};
use windows_sys::Win32::System::Threading::{
    CreateEventW, INFINITE, SetEvent, WaitForMultipleObjects,
};

use super::super::{Seen, Trouble};
// @end

// @implements CT-EFF-008
/// The buffer for one read: 64 KB, the most a read over the network may use. When more
/// changes come at once than it holds, Windows says so and the project is read again.
const WORDS: usize = 16 * 1024;

// @implements CT-EFF-008
/// Name and content changes only. Reading a file, and a change to its times, attributes
/// or permissions, are not changes, so the app's own reads never cause a reload.
const WHAT: u32 = FILE_NOTIFY_CHANGE_FILE_NAME
    | FILE_NOTIFY_CHANGE_DIR_NAME
    | FILE_NOTIFY_CHANGE_SIZE
    | FILE_NOTIFY_CHANGE_LAST_WRITE;

// @implements CT-EFF-008
/// A handle closed when dropped.
struct Handle(HANDLE);

// @implements CT-EFF-008
// SAFETY: a kernel handle may be used and closed from any thread.
unsafe impl Send for Handle {}

// @implements CT-EFF-008
// SAFETY: as for Send; SetEvent and WaitForMultipleObjects are thread-safe.
unsafe impl Sync for Handle {}

// @implements CT-EFF-008
impl Handle {
    /// `handle`, or the OS error when it is not a handle.
    fn new(handle: HANDLE) -> io::Result<Self> {
        if handle.is_null() || handle == INVALID_HANDLE_VALUE {
            Err(io::Error::last_os_error())
        } else {
            Ok(Self(handle))
        }
    }

    /// A new auto-reset event, not signaled.
    fn event() -> io::Result<Self> {
        // SAFETY: no attributes and no name; the result is checked.
        Self::new(unsafe { CreateEventW(null(), 0, 0, null()) })
    }
}

// @implements CT-EFF-008
impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: the handle is open and owned here.
        unsafe { CloseHandle(self.0) };
    }
}

// @implements CT-EFF-008
/// Stops the watch when dropped: it signals the stop event, and the watcher thread
/// cancels its read and ends.
pub struct Stop(Arc<Handle>);

// @implements CT-EFF-008
impl Drop for Stop {
    fn drop(&mut self) {
        // SAFETY: the event is open while this Arc holds it.
        unsafe { SetEvent(self.0.0) };
    }
}

// @implements CT-EFF-008, CT-EFF-007
/// A folder handle with its overlapped read of changes.
struct Reader {
    // Dropped after `Drop::drop` has ended any pending read.
    folder: Handle,
    done: Handle,
    buf: Vec<u32>,
    overlapped: Box<OVERLAPPED>,
    pending: bool,
}

// @implements CT-EFF-008
// SAFETY: the OVERLAPPED holds only the event handle, and it and the buffer are on the
// heap, so moving the Reader to its thread moves nothing the kernel writes to.
unsafe impl Send for Reader {}

// @implements CT-EFF-008, CT-EFF-007
impl Reader {
    /// Opens `root` for reading its changes.
    fn open(root: &Path) -> io::Result<Self> {
        let wide: Vec<u16> = root.as_os_str().encode_wide().chain([0]).collect();
        // SAFETY: `wide` ends with a NUL; the result is checked.
        let folder = Handle::new(unsafe {
            CreateFileW(
                wide.as_ptr(),
                FILE_LIST_DIRECTORY,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                null(),
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OVERLAPPED,
                null_mut(),
            )
        })?;
        Ok(Self {
            folder,
            done: Handle::event()?,
            buf: vec![0; WORDS],
            overlapped: Box::default(),
            pending: false,
        })
    }

    /// Starts a read of the next changes, over the whole tree. It returns at once; the
    /// `done` event is signaled when changes come.
    fn read(&mut self) -> io::Result<()> {
        *self.overlapped = OVERLAPPED::default();
        self.overlapped.hEvent = self.done.0;
        // SAFETY: the buffer and the OVERLAPPED live on the heap until the read ends:
        // `wait` or `Drop` collects it before they are reused or freed.
        let ok = unsafe {
            ReadDirectoryChangesW(
                self.folder.0,
                self.buf.as_mut_ptr().cast(),
                (WORDS * 4) as u32,
                1,
                WHAT,
                null_mut(),
                &mut *self.overlapped,
                None,
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        self.pending = true;
        Ok(())
    }

    /// Collects the finished read: its bytes, or `None` when more changed than the
    /// buffer holds.
    fn finish(&mut self) -> io::Result<Option<Vec<u8>>> {
        let mut got = 0;
        // SAFETY: a read was started with this OVERLAPPED; waiting for it is allowed.
        let ok = unsafe { GetOverlappedResult(self.folder.0, &*self.overlapped, &mut got, 1) };
        self.pending = false;
        if ok == 0 {
            let e = io::Error::last_os_error();
            return match e.raw_os_error() {
                Some(code) if code == ERROR_NOTIFY_ENUM_DIR as i32 => Ok(None),
                _ => Err(e),
            };
        }
        if got == 0 {
            return Ok(None);
        }
        let words = (got as usize).div_ceil(4).min(WORDS);
        let mut bytes: Vec<u8> = self.buf[..words]
            .iter()
            .flat_map(|w| w.to_le_bytes())
            .collect();
        bytes.truncate(got as usize);
        Ok(Some(bytes))
    }
}

// @implements CT-EFF-008
impl Drop for Reader {
    fn drop(&mut self) {
        if self.pending {
            // SAFETY: cancels this Reader's own read, then waits until the kernel is
            // done with the buffer.
            unsafe { CancelIoEx(self.folder.0, &*self.overlapped) };
            let _ = self.finish();
        }
    }
}

// @implements CT-EFF-008, CT-EFF-007, CT-USE-009
/// Starts watching the tree under `root`, sending what changes to `tx` from a thread
/// of its own. The first read starts before this returns, so no change made after it
/// is missed. The thread ends, dropping `tx`, when the `Stop` is dropped or the watch
/// fails.
pub fn start(root: &Path, tx: Sender<Seen>) -> io::Result<Stop> {
    let mut reader = Reader::open(root)?;
    let stop = Arc::new(Handle::event()?);
    reader.read()?;
    let root = root.to_path_buf();
    let stopped = Arc::clone(&stop);
    std::thread::Builder::new()
        .name("clew-watch-os".into())
        .spawn(move || run(reader, &stopped, &root, &tx))?;
    Ok(Stop(stop))
}

// @implements CT-EFF-008, CT-EFF-007, CT-USE-009
/// The watcher thread: it waits, with no timeout, for the read to finish or the stop
/// event, passes on what changed, and starts the next read.
fn run(mut reader: Reader, stop: &Handle, root: &Path, tx: &Sender<Seen>) {
    let failed = |e: io::Error| {
        let _ = tx.send(Seen::Trouble(Trouble {
            what: format!("watching the project folder failed: {e}"),
            help: "Reopen the project to see changes made outside Clewtrace.".to_string(),
        }));
    };
    loop {
        let events = [reader.done.0, stop.0];
        // SAFETY: both handles are open events; INFINITE means no timeout.
        let woke = unsafe { WaitForMultipleObjects(2, events.as_ptr(), 0, INFINITE) };
        if woke != WAIT_OBJECT_0 {
            // Stopped. Dropping the reader cancels its read.
            return;
        }
        match reader.finish() {
            Ok(Some(bytes)) => {
                for s in super::seen(root, &super::records(&bytes)) {
                    let _ = tx.send(s);
                }
            }
            Ok(None) => {
                let _ = tx.send(Seen::Everything);
            }
            Err(e) => return failed(e),
        }
        if let Err(e) = reader.read() {
            return failed(e);
        }
    }
}
