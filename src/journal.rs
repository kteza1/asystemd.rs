use libc::{self, free, c_int, c_char, size_t};
use std::ffi::{CString, CStr};
use log::{self, Log, LogRecord, LogLocation, SetLoggerError};
use std::{self, fmt, ptr, result};
use std::collections::BTreeMap;
use ffi;
use super::Result;

#[derive(PartialEq)]


pub enum SeekRet {
    SeekSuccess,
    ClosestSeek,
}

/// Send preformatted fields to systemd.
///
/// This is a relatively low-level operation and probably not suitable unless
/// you need precise control over which fields are sent to systemd.
pub fn send(args: &[&str]) -> c_int {
    let iovecs = ffi::array_to_iovecs(args);
    unsafe { ffi::sd_journal_sendv(iovecs.as_ptr(), iovecs.len() as c_int) }
}

/// Send a simple message to systemd.
pub fn print(lvl: u32, s: &str) -> c_int {
    send(&[&format!("PRIORITY={}", lvl), &format!("MESSAGE={}", s)])
}

/// Send a `log::LogRecord` to systemd.
pub fn log_record(record: &LogRecord) {
    let lvl: usize = unsafe {
        use std::mem;
        mem::transmute(record.level())
    };
    log(lvl, record.location(), record.args());
}

pub fn log(level: usize, loc: &LogLocation, args: &fmt::Arguments) {
    send(&[&format!("PRIORITY={}", level),
           &format!("MESSAGE={}", args),
           &format!("CODE_LINE={}", loc.line()),
           &format!("CODE_FILE={}", loc.file()),
           &format!("CODE_FUNCTION={}", loc.module_path())]);
}

pub struct JournalLog;
impl Log for JournalLog {
    fn enabled(&self, _metadata: &log::LogMetadata) -> bool {
        true
    }

    fn log(&self, record: &LogRecord) {
        log_record(record);
    }
}

impl JournalLog {
    pub fn init() -> result::Result<(), SetLoggerError> {
        log::set_logger(|_max_log_level| Box::new(JournalLog))
    }
}

pub type JournalRecord = BTreeMap<String, String>;

/// A cursor into the systemd journal.
///
/// Supports read, next, previous, and seek operations.
pub struct Journal {
    j: ffi::sd_journal,
    wait_time: u64,
}

/// Represents the set of journal files to read.
pub enum JournalFiles {
    /// The system-wide journal.
    System,
    /// The current user's journal.
    CurrentUser,
    /// Both the system-wide journal and the current user's journal.
    All,
}

impl Journal {
    /// Open the systemd journal for reading.
    ///
    /// Params:
    ///
    /// * files: the set of journal files to read. If the calling process
    ///   doesn't have permission to read the system journal, a call to
    ///   `Journal::open` with `System` or `All` will succeed, but system
    ///   journal entries won't be included. This behavior is due to systemd.
    /// * runtime_only: if true, include only journal entries from the current
    ///   boot. If false, include all entries.
    /// * local_only: if true, include only journal entries originating from
    ///   localhost. If false, include all entries.
    pub fn open(files: JournalFiles, runtime_only: bool, local_only: bool) -> Result<Journal> {
        let mut flags: c_int = 0;
        if runtime_only {
            flags |= ffi::SD_JOURNAL_RUNTIME_ONLY;
        }
        if local_only {
            flags |= ffi::SD_JOURNAL_LOCAL_ONLY;
        }
        flags |= match files {
            JournalFiles::System => ffi::SD_JOURNAL_SYSTEM,
            JournalFiles::CurrentUser => ffi::SD_JOURNAL_CURRENT_USER,
            JournalFiles::All => 0,
        };

        let journal = Journal {
            j: ptr::null_mut(),
            wait_time: 1 << 63, // wait for infinite time
        };
        sd_try!(ffi::sd_journal_open(&journal.j, flags));
        sd_try!(ffi::sd_journal_seek_head(journal.j));
        Ok(journal)
    }

    pub fn set_iterator_timeout(&mut self, timeout: u64) {
        self.wait_time = timeout * 1000000;
    }

    /// Read the next record from the journal. Returns `io::EndOfFile` if there
    /// are no more records to read.
    pub fn next_record(&self) -> Result<Option<JournalRecord>> {
        if sd_try!(ffi::sd_journal_next(self.j)) == 0 {
            return Ok(None);
        }
        unsafe { ffi::sd_journal_restart_data(self.j) }

        let mut ret: JournalRecord = BTreeMap::new();

        let mut sz: size_t = 0;
        let data: *mut u8 = ptr::null_mut();
        while sd_try!(ffi::sd_journal_enumerate_data(self.j, &data, &mut sz)) > 0 {
            unsafe {
                let b = ::std::slice::from_raw_parts_mut(data, sz as usize);
                let field = ::std::str::from_utf8_unchecked(b);
                let mut name_value = field.splitn(2, '=');
                let name = name_value.next().unwrap();
                let value = name_value.next().unwrap();
                ret.insert(From::from(name), From::from(value));
            }
        }

        Ok(Some(ret))
    }

    pub fn cursor(&self) -> Result<String> {
        let mut c_cursor: *mut c_char = ptr::null_mut();
        let mut cursor: String = "".to_string();
        if sd_try!(ffi::sd_journal_get_cursor(self.j, &mut c_cursor)) == 0 {
            unsafe {
                // Cstr should be used for memory allocated by C
                cursor = CStr::from_ptr(c_cursor as *const _)
                             .to_string_lossy()
                             .into_owned();

                free(c_cursor as *mut libc::c_void);
            }
        }
        Ok(cursor)
    }

    pub fn seek<S>(&self, cursor: S) -> Result<SeekRet>
        where S: Into<String>
    {
        let c_position = CString::new(cursor.into());
        // If no entry matching the specified cursor is found the call will seek to
        // the next closest entry (in terms of time) instead
        sd_try!(ffi::sd_journal_seek_cursor(self.j,
                                            c_position.clone().unwrap().as_ptr() as *const _));

        Ok(SeekRet::SeekSuccess)

        // TODO: Test why sd_journal_test_cursor is failing here

        // match sd_try!(ffi::sd_journal_test_cursor(self.j,
        //                                           c_position.unwrap().as_ptr() as *const _)) {
        //     0 => Ok(SeekRet::ClosestSeek),
        //     e if e > 0 => Ok(SeekRet::SeekSuccess),
        //     e => Err(std::io::Error::from_raw_os_error(-e)),
        // }
    }

    pub fn get_realtime_us(&self) -> Result<u64> {
        let mut timestamp_us = 0;
        sd_try!(ffi::sd_journal_get_realtime_usec(self.j, &mut timestamp_us));
        Ok(timestamp_us)
    }
}


impl<'a> Iterator for &'a Journal {
    type Item = (JournalRecord, String);

    fn next(&mut self) -> Option<(JournalRecord, String)> {
        let next_record = match self.next_record() {
            Ok(r) => r,
            Err(_) => {
                error!("error reading a journal entry. adding dummy entry");
                let mut dummy_tree = BTreeMap::new();
                dummy_tree.insert("Dummy".to_string(), "Dummy".to_string());
                Some(dummy_tree)
            }
        };

        // let wait_time: u64 = 1 << 63;
        match next_record {
            Some(record) => {
                let cursor = self.cursor().unwrap();
                Some((record, cursor))
            }
            None => {
                let w_ret: i32;
                // TODO: https://github.com/sofar/tallow/blob/b81b4404955997801eae4b8fe6799f5b59e7728c/tallow.c#L349-L353
                // Do we need to seek to tail if INVALIDATE happens ??
                unsafe {
                    w_ret = ffi::sd_journal_wait(self.j, self.wait_time);
                }
                if w_ret <= 0 {
                    None
                } else if w_ret == 2 {
                    // TODO: Hitting this condition when there are no more journals to read
                    println!("Journal invalidate ... Rotation or new files");
                    self.next()
                } else {
                    self.next()
                }
            }
        }
    }
}

impl Drop for Journal {
    fn drop(&mut self) {
        if !self.j.is_null() {
            unsafe {
                ffi::sd_journal_close(self.j);
            }
        }
    }
}
