#![no_std]

extern crate alloc;

use alloc::vec::Vec;
use core::alloc::{GlobalAlloc, Layout};
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};

pub mod syscall {
    use core::arch::asm;

    pub type Word = u64;

    pub const ABI_VERSION: u32 = 2;
    pub const MAX_PATH: usize = 256;
    pub const MAX_DIR_ENTRIES: usize = 16;
    pub const SCHEDULER_HZ: Word = 100;

    #[repr(u64)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Number {
        Read = 0,
        Write = 1,
        Close = 3,
        Exit = 60,
        Yield = 24,
        Sleep = 35,
        Open = 2,
        Mkdir = 420,
        Rmdir = 421,
        Unlink = 422,
        Rename = 423,
        Link = 424,
        Stat = 425,
        ReadDir = 426,
    }

    pub const OPEN_READ: Word = 1 << 0;
    pub const OPEN_WRITE: Word = 1 << 1;
    pub const OPEN_CREATE: Word = 1 << 2;
    pub const OPEN_TRUNCATE: Word = 1 << 3;
    pub const OPEN_APPEND: Word = 1 << 4;
    pub const STAT_REGULAR: u32 = 1;
    pub const STAT_DIRECTORY: u32 = 2;
    pub const NAME_MAX: usize = 31;

    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct Stat {
        pub kind: u32,
        pub mode: u32,
        pub size: u64,
        pub links: u32,
        pub inode: u32,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct DirEntry {
        pub kind: u32,
        pub mode: u32,
        pub size: u64,
        pub links: u32,
        pub name_length: u32,
        pub name: [u8; NAME_MAX + 1],
    }

    impl Default for DirEntry {
        fn default() -> Self {
            Self {
                kind: 0,
                mode: 0,
                size: 0,
                links: 0,
                name_length: 0,
                name: [0; NAME_MAX + 1],
            }
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Error(pub Word);

    impl Error {
        pub const fn errno(self) -> Word {
            0u64.wrapping_sub(self.0)
        }
    }

    pub const fn is_error(value: Word) -> bool {
        value >= Word::MAX - 4095
    }

    fn result(value: Word) -> Result<Word, Error> {
        if is_error(value) {
            Err(Error(value))
        } else {
            Ok(value)
        }
    }

    #[cfg(target_arch = "x86_64")]
    unsafe fn raw6(number: Word, args: [Word; 6]) -> Word {
        let mut result = number;
        asm!(
            "syscall",
            inlateout("rax") result,
            in("rdi") args[0],
            in("rsi") args[1],
            in("rdx") args[2],
            in("r10") args[3],
            in("r8") args[4],
            in("r9") args[5],
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack)
        );
        result
    }

    #[cfg(target_arch = "aarch64")]
    unsafe fn raw6(number: Word, args: [Word; 6]) -> Word {
        let mut result = args[0];
        asm!(
            "svc #0",
            inlateout("x0") result,
            in("x1") args[1],
            in("x2") args[2],
            in("x3") args[3],
            in("x4") args[4],
            in("x5") args[5],
            in("x8") number,
            lateout("x9") _,
            lateout("x10") _,
            lateout("x11") _,
            lateout("x12") _,
            lateout("x13") _,
            lateout("x14") _,
            lateout("x15") _,
            options(nostack)
        );
        result
    }

    pub fn read(fd: Word, buffer: *mut u8, length: usize) -> Result<usize, Error> {
        result(unsafe {
            raw6(
                Number::Read as Word,
                [fd, buffer as Word, length as Word, 0, 0, 0],
            )
        })
        .map(|value| value as usize)
    }

    pub fn write(fd: Word, buffer: *const u8, length: usize) -> Result<usize, Error> {
        result(unsafe {
            raw6(
                Number::Write as Word,
                [fd, buffer as Word, length as Word, 0, 0, 0],
            )
        })
        .map(|value| value as usize)
    }

    pub fn close(fd: Word) -> Result<(), Error> {
        result(unsafe { raw6(Number::Close as Word, [fd, 0, 0, 0, 0, 0]) }).map(|_| ())
    }

    pub fn yield_now() -> Result<(), Error> {
        result(unsafe { raw6(Number::Yield as Word, [0; 6]) }).map(|_| ())
    }

    pub fn sleep(duration: Word) -> Result<(), Error> {
        result(unsafe { raw6(Number::Sleep as Word, [duration, 0, 0, 0, 0, 0]) }).map(|_| ())
    }

    pub fn open(path: &[u8], flags: Word, mode: Word) -> Result<Word, Error> {
        result(unsafe {
            raw6(
                Number::Open as Word,
                [path.as_ptr() as Word, path.len() as Word, flags, mode, 0, 0],
            )
        })
    }

    fn path_call(number: Number, path: &[u8]) -> Result<(), Error> {
        result(unsafe {
            raw6(
                number as Word,
                [path.as_ptr() as Word, path.len() as Word, 0, 0, 0, 0],
            )
        })
        .map(|_| ())
    }

    pub fn mkdir(path: &[u8], mode: Word) -> Result<(), Error> {
        result(unsafe {
            raw6(
                Number::Mkdir as Word,
                [path.as_ptr() as Word, path.len() as Word, mode, 0, 0, 0],
            )
        })
        .map(|_| ())
    }

    pub fn rmdir(path: &[u8]) -> Result<(), Error> {
        path_call(Number::Rmdir, path)
    }

    pub fn unlink(path: &[u8]) -> Result<(), Error> {
        path_call(Number::Unlink, path)
    }

    fn two_path_call(number: Number, old: &[u8], new: &[u8]) -> Result<(), Error> {
        result(unsafe {
            raw6(
                number as Word,
                [
                    old.as_ptr() as Word,
                    old.len() as Word,
                    new.as_ptr() as Word,
                    new.len() as Word,
                    0,
                    0,
                ],
            )
        })
        .map(|_| ())
    }

    pub fn rename(old: &[u8], new: &[u8]) -> Result<(), Error> {
        two_path_call(Number::Rename, old, new)
    }

    pub fn link(old: &[u8], new: &[u8]) -> Result<(), Error> {
        two_path_call(Number::Link, old, new)
    }

    pub fn stat(path: &[u8], output: &mut Stat) -> Result<(), Error> {
        result(unsafe {
            raw6(
                Number::Stat as Word,
                [
                    path.as_ptr() as Word,
                    path.len() as Word,
                    output as *mut Stat as Word,
                    0,
                    0,
                    0,
                ],
            )
        })
        .map(|_| ())
    }

    pub fn read_dir(path: &[u8], output: &mut [DirEntry]) -> Result<usize, Error> {
        result(unsafe {
            raw6(
                Number::ReadDir as Word,
                [
                    path.as_ptr() as Word,
                    path.len() as Word,
                    output.as_mut_ptr() as Word,
                    output.len() as Word,
                    0,
                    0,
                ],
            )
        })
        .map(|value| value as usize)
    }

    pub fn exit(status: i32) -> ! {
        let _ = unsafe { raw6(Number::Exit as Word, [status as Word, 0, 0, 0, 0, 0]) };
        loop {
            core::hint::spin_loop();
        }
    }
}

const HEAP_BYTES: usize = 64 * 1024;
const MAX_INPUT_BYTES: usize = 32 * 1024;
const MAX_SYSCALL_RETRIES: usize = 8;
const EINTR: syscall::Word = 4;
const EAGAIN: syscall::Word = 11;
const EOVERFLOW: syscall::Word = 75;

struct Heap {
    bytes: UnsafeCell<[u8; HEAP_BYTES]>,
    cursor: AtomicUsize,
}

unsafe impl Sync for Heap {}

struct BumpAllocator;

static HEAP: Heap = Heap {
    bytes: UnsafeCell::new([0; HEAP_BYTES]),
    cursor: AtomicUsize::new(0),
};

unsafe impl GlobalAlloc for BumpAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let base = HEAP.bytes.get() as *mut u8 as usize;
        let cursor = HEAP.cursor.load(AtomicOrdering::Relaxed);
        let aligned = (base + cursor + layout.align() - 1) & !(layout.align() - 1);
        let offset = aligned - base;
        let Some(end) = offset.checked_add(layout.size()) else {
            return core::ptr::null_mut();
        };
        if end > HEAP_BYTES {
            return core::ptr::null_mut();
        }
        if HEAP
            .cursor
            .compare_exchange(cursor, end, AtomicOrdering::AcqRel, AtomicOrdering::Relaxed)
            .is_ok()
        {
            aligned as *mut u8
        } else {
            self.alloc(layout)
        }
    }

    unsafe fn dealloc(&self, _pointer: *mut u8, _layout: Layout) {}
}

#[global_allocator]
static ALLOCATOR: BumpAllocator = BumpAllocator;

#[derive(Clone, Copy)]
pub struct Startup {
    pub argc: usize,
    pub argv: *const *const u8,
    pub envp: *const *const u8,
}

pub unsafe fn startup_from_stack(stack: *const usize) -> Startup {
    let argc = *stack;
    let argv = stack.add(1) as *const *const u8;
    let envp = argv.add(argc + 1);
    Startup { argc, argv, envp }
}

unsafe fn c_string(pointer: *const u8) -> &'static [u8] {
    if pointer.is_null() {
        return &[];
    }
    let mut length = 0;
    while length < 4096 && *pointer.add(length) != 0 {
        length += 1;
    }
    core::slice::from_raw_parts(pointer, length)
}

pub unsafe fn collect_args(startup: Startup) -> Vec<&'static [u8]> {
    let mut result = Vec::new();
    for index in 0..startup.argc.min(64) {
        result.push(c_string(*startup.argv.add(index)));
    }
    result
}

pub unsafe fn collect_environment(startup: Startup) -> Vec<&'static [u8]> {
    let mut result = Vec::new();
    for index in 0..64 {
        let pointer = *startup.envp.add(index);
        if pointer.is_null() {
            break;
        }
        result.push(c_string(pointer));
    }
    result
}

fn write_all(fd: syscall::Word, bytes: &[u8]) -> Result<(), syscall::Error> {
    let mut offset = 0;
    while offset < bytes.len() {
        let count = write_retry(fd, bytes[offset..].as_ptr(), bytes.len() - offset)?;
        if count == 0 {
            return Err(syscall::Error(u64::MAX - 5));
        }
        offset += count;
    }
    Ok(())
}

fn retryable(error: syscall::Error) -> bool {
    matches!(error.errno(), EINTR | EAGAIN)
}

fn write_retry(
    fd: syscall::Word,
    buffer: *const u8,
    length: usize,
) -> Result<usize, syscall::Error> {
    for _ in 0..=MAX_SYSCALL_RETRIES {
        match syscall::write(fd, buffer, length) {
            Ok(count) => return Ok(count),
            Err(error) if retryable(error) => {
                let _ = syscall::yield_now();
            }
            Err(error) => return Err(error),
        }
    }
    Err(syscall::Error(0u64.wrapping_sub(EAGAIN)))
}

fn read_retry(fd: syscall::Word, buffer: *mut u8, length: usize) -> Result<usize, syscall::Error> {
    for _ in 0..=MAX_SYSCALL_RETRIES {
        match syscall::read(fd, buffer, length) {
            Ok(count) => return Ok(count),
            Err(error) if retryable(error) => {
                let _ = syscall::yield_now();
            }
            Err(error) => return Err(error),
        }
    }
    Err(syscall::Error(0u64.wrapping_sub(EAGAIN)))
}

fn read_all(fd: syscall::Word, output: &mut Vec<u8>) -> Result<(), syscall::Error> {
    let mut buffer = [0u8; 1024];
    loop {
        match read_retry(fd, buffer.as_mut_ptr(), buffer.len())? {
            0 => return Ok(()),
            count => {
                let Some(new_length) = output.len().checked_add(count) else {
                    return Err(syscall::Error(0u64.wrapping_sub(EOVERFLOW)));
                };
                if new_length > MAX_INPUT_BYTES {
                    return Err(syscall::Error(0u64.wrapping_sub(EOVERFLOW)));
                }
                output.extend_from_slice(&buffer[..count]);
            }
        }
    }
}

fn append_u64(output: &mut Vec<u8>, mut value: u64) {
    let mut digits = [0u8; 20];
    let mut count = 0;
    if value == 0 {
        output.push(b'0');
        return;
    }
    while value != 0 {
        digits[count] = b'0' + (value % 10) as u8;
        count += 1;
        value /= 10;
    }
    while count != 0 {
        count -= 1;
        output.push(digits[count]);
    }
}

fn report(command: &[u8], reason: &[u8], error: Option<syscall::Error>) -> i32 {
    let mut line = Vec::new();
    line.extend_from_slice(b"coreutils: ");
    line.extend_from_slice(command);
    line.extend_from_slice(b": ");
    line.extend_from_slice(reason);
    if let Some(error) = error {
        line.extend_from_slice(b" (errno ");
        append_u64(&mut line, error.errno());
        line.push(b')');
    }
    line.push(b'\n');
    let _ = write_all(2, &line);
    1
}

fn usage(command: &[u8], reason: &[u8]) -> i32 {
    report(command, reason, None) + 1
}

fn env_value<'a>(environment: &'a [&'a [u8]], name: &[u8]) -> Option<&'a [u8]> {
    environment.iter().find_map(|entry| {
        entry
            .strip_prefix(name)
            .and_then(|value| value.strip_prefix(b"="))
    })
}

fn absolute_path<'a>(
    raw: &[u8],
    environment: &[&[u8]],
    output: &'a mut Vec<u8>,
) -> Result<&'a [u8], &'static [u8]> {
    if raw.is_empty() || raw.contains(&0) {
        return Err(b"empty or NUL path");
    }
    output.clear();
    if raw.first() == Some(&b'/') {
        output.extend_from_slice(raw);
    } else {
        let cwd = env_value(environment, b"PWD").unwrap_or(b"/");
        if cwd.first() != Some(&b'/') {
            return Err(b"PWD is not absolute");
        }
        output.extend_from_slice(cwd);
        if output.last() != Some(&b'/') {
            output.push(b'/');
        }
        output.extend_from_slice(raw);
    }
    if output.len() > syscall::MAX_PATH {
        return Err(b"path is too long");
    }
    Ok(output)
}

fn open_read(path: &[u8]) -> Result<syscall::Word, syscall::Error> {
    syscall::open(path, syscall::OPEN_READ, 0)
}

fn copy_fd(input: syscall::Word, output: syscall::Word) -> Result<(), syscall::Error> {
    let mut buffer = [0u8; 1024];
    loop {
        let count = read_retry(input, buffer.as_mut_ptr(), buffer.len())?;
        if count == 0 {
            return Ok(());
        }
        write_all(output, &buffer[..count])?;
    }
}

fn parse_u64(value: &[u8]) -> Option<u64> {
    if value.is_empty() {
        return None;
    }
    let mut result = 0u64;
    for byte in value {
        if !byte.is_ascii_digit() {
            return None;
        }
        result = result.checked_mul(10)?.checked_add((byte - b'0') as u64)?;
    }
    Some(result)
}

fn line_count(bytes: &[u8]) -> usize {
    bytes.iter().filter(|byte| **byte == b'\n').count()
}

fn word_count(bytes: &[u8]) -> usize {
    bytes
        .split(|byte| byte.is_ascii_whitespace())
        .filter(|word| !word.is_empty())
        .count()
}

fn glob_matches(pattern: &[u8], value: &[u8]) -> bool {
    let mut pattern_index = 0;
    let mut value_index = 0;
    let mut star = None;
    let mut retry = 0;
    while value_index < value.len() {
        if pattern_index < pattern.len()
            && (pattern[pattern_index] == b'?' || pattern[pattern_index] == value[value_index])
        {
            pattern_index += 1;
            value_index += 1;
        } else if pattern_index < pattern.len() && pattern[pattern_index] == b'*' {
            star = Some(pattern_index);
            pattern_index += 1;
            retry = value_index;
        } else if let Some(star_index) = star {
            pattern_index = star_index + 1;
            retry += 1;
            value_index = retry;
        } else {
            return false;
        }
    }
    while pattern_index < pattern.len() && pattern[pattern_index] == b'*' {
        pattern_index += 1;
    }
    pattern_index == pattern.len()
}

fn command_path<'a>(
    command: &[u8],
    raw: &[u8],
    environment: &[&[u8]],
    path: &'a mut Vec<u8>,
) -> Result<&'a [u8], i32> {
    absolute_path(raw, environment, path).map_err(|reason| report(command, reason, None))
}

fn cat(command: &[u8], args: &[&[u8]], environment: &[&[u8]]) -> i32 {
    let mut path = Vec::new();
    if args
        .iter()
        .any(|arg| arg.first() == Some(&b'-') && *arg != b"-" && *arg != b"--")
    {
        return usage(command, b"only -- and - are supported options");
    }
    let args = if args.first() == Some(&b"--".as_slice()) {
        &args[1..]
    } else {
        args
    };
    if args.is_empty() {
        return match copy_fd(0, 1) {
            Ok(()) => 0,
            Err(error) => report(command, b"read failed", Some(error)),
        };
    }
    let mut status = 0;
    for raw in args {
        if *raw == b"-" {
            if let Err(error) = copy_fd(0, 1) {
                status = report(command, b"read failed", Some(error));
            }
            continue;
        }
        let absolute = match command_path(command, raw, environment, &mut path) {
            Ok(value) => value,
            Err(error) => return error,
        };
        let fd = match open_read(absolute) {
            Ok(fd) => fd,
            Err(error) => {
                status = report(command, b"open failed", Some(error));
                continue;
            }
        };
        if let Err(error) = copy_fd(fd, 1) {
            status = report(command, b"read failed", Some(error));
        }
        let _ = syscall::close(fd);
    }
    status
}

fn echo(command: &[u8], args: &[&[u8]]) -> i32 {
    let no_newline = args.first() == Some(&b"-n".as_slice());
    let args = if no_newline { &args[1..] } else { args };
    let mut output = Vec::new();
    for (index, arg) in args.iter().enumerate() {
        if index != 0 {
            output.push(b' ');
        }
        output.extend_from_slice(arg);
    }
    if !no_newline {
        output.push(b'\n');
    }
    match write_all(1, &output) {
        Ok(()) => 0,
        Err(error) => report(command, b"write failed", Some(error)),
    }
}

fn env_command(command: &[u8], args: &[&[u8]], environment: &[&[u8]]) -> i32 {
    let mut values = environment.to_vec();
    let mut index = 0;
    if args.first() == Some(&b"-i".as_slice()) {
        values.clear();
        index = 1;
    }
    if args.get(index) == Some(&b"--".as_slice()) {
        index += 1;
    }
    while let Some(arg) = args.get(index) {
        let Some(separator) = arg.iter().position(|byte| *byte == b'=') else {
            return usage(command, b"command execution is not part of env yet");
        };
        if separator == 0 {
            return usage(command, b"invalid empty environment name");
        }
        values.retain(|entry| {
            entry
                .iter()
                .position(|byte| *byte == b'=')
                .is_none_or(|position| entry[..position] != arg[..separator])
        });
        values.push(*arg);
        index += 1;
    }
    let mut output = Vec::new();
    for entry in values {
        output.extend_from_slice(entry);
        output.push(b'\n');
    }
    match write_all(1, &output) {
        Ok(()) => 0,
        Err(error) => report(command, b"write failed", Some(error)),
    }
}

fn pwd(command: &[u8], args: &[&[u8]], environment: &[&[u8]]) -> i32 {
    if !args.is_empty() {
        return usage(command, b"no arguments expected");
    }
    let value = env_value(environment, b"PWD").unwrap_or(b"/");
    let mut output = Vec::from(value);
    output.push(b'\n');
    match write_all(1, &output) {
        Ok(()) => 0,
        Err(error) => report(command, b"write failed", Some(error)),
    }
}

fn stat_one(
    command: &[u8],
    raw: &[u8],
    environment: &[&[u8]],
    path: &mut Vec<u8>,
) -> Result<(), i32> {
    let absolute = command_path(command, raw, environment, path)?;
    let mut value = syscall::Stat::default();
    syscall::stat(absolute, &mut value)
        .map_err(|error| report(command, b"stat failed", Some(error)))?;
    let mut output = Vec::new();
    output.extend_from_slice(b"  File: ");
    output.extend_from_slice(raw);
    output.extend_from_slice(b"\n  Size: ");
    append_u64(&mut output, value.size);
    output.extend_from_slice(b"\n  Type: ");
    output.extend_from_slice(if value.kind == syscall::STAT_DIRECTORY {
        b"directory"
    } else {
        b"regular"
    });
    output.extend_from_slice(b"\n  Mode: ");
    append_u64(&mut output, value.mode as u64);
    output.extend_from_slice(b"\n Links: ");
    append_u64(&mut output, value.links as u64);
    output.extend_from_slice(b"\n Inode: ");
    append_u64(&mut output, value.inode as u64);
    output.push(b'\n');
    write_all(1, &output).map_err(|error| report(command, b"write failed", Some(error)))
}

fn stat_command(command: &[u8], args: &[&[u8]], environment: &[&[u8]]) -> i32 {
    if args.is_empty() {
        return usage(command, b"operand required");
    }
    let mut path = Vec::new();
    let mut status = 0;
    for raw in args {
        if stat_one(command, raw, environment, &mut path).is_err() {
            status = 1;
        }
    }
    status
}

fn ls_one(
    command: &[u8],
    raw: &[u8],
    environment: &[&[u8]],
    path: &mut Vec<u8>,
    all: bool,
    long: bool,
) -> Result<(), i32> {
    let absolute = command_path(command, raw, environment, path)?;
    let mut value = syscall::Stat::default();
    syscall::stat(absolute, &mut value)
        .map_err(|error| report(command, b"stat failed", Some(error)))?;
    if value.kind != syscall::STAT_DIRECTORY {
        let mut output = Vec::from(raw);
        output.push(b'\n');
        return write_all(1, &output)
            .map_err(|error| report(command, b"write failed", Some(error)));
    }
    let mut entries = [syscall::DirEntry::default(); 16];
    let count = syscall::read_dir(absolute, &mut entries)
        .map_err(|error| report(command, b"read directory failed", Some(error)))?;
    if count > entries.len() {
        return Err(report(command, b"malformed directory count", None));
    }
    for entry in entries.iter().take(count) {
        if entry.name_length as usize > syscall::NAME_MAX {
            return Err(report(command, b"malformed directory entry", None));
        }
    }
    entries[..count].sort_unstable_by(|left, right| {
        left.name[..left.name_length as usize].cmp(&right.name[..right.name_length as usize])
    });
    for entry in entries.iter().take(count) {
        let length = usize::try_from(entry.name_length)
            .ok()
            .filter(|length| *length <= syscall::NAME_MAX)
            .ok_or_else(|| report(command, b"malformed directory entry", None))?;
        let mut output = Vec::from(&entry.name[..length]);
        if !all && output.first() == Some(&b'.') {
            continue;
        }
        if long {
            let mut formatted = Vec::new();
            formatted.extend_from_slice(if entry.kind == syscall::STAT_DIRECTORY {
                b"d"
            } else {
                b"-"
            });
            formatted.extend_from_slice(b" ");
            append_u64(&mut formatted, entry.links as u64);
            formatted.extend_from_slice(b" ");
            append_u64(&mut formatted, entry.size);
            formatted.extend_from_slice(b" ");
            formatted.extend_from_slice(&output);
            output = formatted;
        }
        output.push(b'\n');
        write_all(1, &output).map_err(|error| report(command, b"write failed", Some(error)))?;
    }
    Ok(())
}

fn ls(command: &[u8], args: &[&[u8]], environment: &[&[u8]]) -> i32 {
    let mut all = false;
    let mut long = false;
    let mut index = 0;
    while let Some(arg) = args.get(index) {
        match *arg {
            b"--" => {
                index += 1;
                break;
            }
            b"-a" => all = true,
            b"-l" => long = true,
            b"-al" | b"-la" => {
                all = true;
                long = true;
            }
            _ if arg.first() == Some(&b'-') => return usage(command, b"unsupported option"),
            _ => break,
        }
        index += 1;
    }
    let inputs = &args[index..];
    let mut default = b".".as_slice();
    let inputs = if inputs.is_empty() {
        core::slice::from_mut(&mut default)
    } else {
        inputs
    };
    let mut path = Vec::new();
    let mut status = 0;
    for raw in inputs {
        if ls_one(command, raw, environment, &mut path, all, long).is_err() {
            status = 1;
        }
    }
    status
}

fn mkdir(command: &[u8], args: &[&[u8]], environment: &[&[u8]]) -> i32 {
    let mut parents = false;
    let mut paths = Vec::new();
    for arg in args {
        if *arg == b"-p" {
            parents = true;
        } else {
            paths.push(*arg);
        }
    }
    if paths.is_empty() {
        return usage(command, b"operand required");
    }
    let mut path = Vec::new();
    let mut status = 0;
    for raw in paths {
        let absolute = match command_path(command, raw, environment, &mut path) {
            Ok(value) => value.to_vec(),
            Err(error) => {
                status = error;
                continue;
            }
        };
        if parents {
            let mut prefix = Vec::new();
            for (index, byte) in absolute.iter().enumerate() {
                if *byte == b'/' && index != 0 {
                    prefix.clear();
                    prefix.extend_from_slice(&absolute[..index]);
                    let _ = syscall::mkdir(&prefix, 0o755);
                }
            }
        }
        if let Err(error) = syscall::mkdir(&absolute, 0o755) {
            let mut existing = syscall::Stat::default();
            if !(parents
                && syscall::stat(&absolute, &mut existing).is_ok()
                && existing.kind == syscall::STAT_DIRECTORY)
            {
                status = report(command, b"mkdir failed", Some(error));
            }
        }
    }
    status
}

fn simple_path_command(
    command: &[u8],
    args: &[&[u8]],
    environment: &[&[u8]],
    operation: fn(&[u8]) -> Result<(), syscall::Error>,
    reason: &[u8],
) -> i32 {
    if args.is_empty() {
        return usage(command, b"operand required");
    }
    let mut path = Vec::new();
    let mut status = 0;
    for raw in args {
        let absolute = match command_path(command, raw, environment, &mut path) {
            Ok(value) => value.to_vec(),
            Err(error) => {
                status = error;
                continue;
            }
        };
        if let Err(error) = operation(&absolute) {
            status = report(command, reason, Some(error));
        }
    }
    status
}

fn cp(command: &[u8], args: &[&[u8]], environment: &[&[u8]]) -> i32 {
    if args.len() != 2 {
        return usage(command, b"exactly source and destination are required");
    }
    let mut source = Vec::new();
    let mut destination = Vec::new();
    let source = match command_path(command, args[0], environment, &mut source) {
        Ok(value) => value.to_vec(),
        Err(error) => return error,
    };
    let destination = match command_path(command, args[1], environment, &mut destination) {
        Ok(value) => value.to_vec(),
        Err(error) => return error,
    };
    let mut source_stat = syscall::Stat::default();
    if let Err(error) = syscall::stat(&source, &mut source_stat) {
        return report(command, b"stat source failed", Some(error));
    }
    if source == destination {
        return report(command, b"source and destination are the same file", None);
    }
    let mut destination_stat = syscall::Stat::default();
    if syscall::stat(&destination, &mut destination_stat).is_ok()
        && source_stat.inode == destination_stat.inode
    {
        return report(command, b"source and destination are the same file", None);
    }
    let input = match open_read(&source) {
        Ok(fd) => fd,
        Err(error) => return report(command, b"open source failed", Some(error)),
    };
    let output = match syscall::open(
        &destination,
        syscall::OPEN_WRITE | syscall::OPEN_CREATE | syscall::OPEN_TRUNCATE,
        0o666,
    ) {
        Ok(fd) => fd,
        Err(error) => {
            let _ = syscall::close(input);
            return report(command, b"open destination failed", Some(error));
        }
    };
    let result = copy_fd(input, output);
    let _ = syscall::close(input);
    let _ = syscall::close(output);
    match result {
        Ok(()) => 0,
        Err(error) => report(command, b"copy failed", Some(error)),
    }
}

fn touch(command: &[u8], args: &[&[u8]], environment: &[&[u8]]) -> i32 {
    if args.is_empty() {
        return usage(command, b"operand required");
    }
    let mut path = Vec::new();
    let mut status = 0;
    for raw in args {
        let absolute = match command_path(command, raw, environment, &mut path) {
            Ok(value) => value.to_vec(),
            Err(error) => {
                status = error;
                continue;
            }
        };
        match syscall::open(
            &absolute,
            syscall::OPEN_READ | syscall::OPEN_WRITE | syscall::OPEN_CREATE,
            0o666,
        ) {
            Ok(fd) => {
                let _ = syscall::close(fd);
            }
            Err(error) => status = report(command, b"touch failed", Some(error)),
        }
    }
    status
}

fn ln(command: &[u8], args: &[&[u8]], environment: &[&[u8]]) -> i32 {
    if args.first() == Some(&b"-s".as_slice()) {
        return usage(command, b"symbolic links are not in the v1 VFS ABI");
    }
    if args.len() != 2 {
        return usage(command, b"exactly source and destination are required");
    }
    let mut old = Vec::new();
    let mut new = Vec::new();
    let old = match command_path(command, args[0], environment, &mut old) {
        Ok(value) => value.to_vec(),
        Err(error) => return error,
    };
    let new = match command_path(command, args[1], environment, &mut new) {
        Ok(value) => value.to_vec(),
        Err(error) => return error,
    };
    match syscall::link(&old, &new) {
        Ok(()) => 0,
        Err(error) => report(command, b"link failed", Some(error)),
    }
}

fn find_visit(
    command: &[u8],
    path: &[u8],
    name_filter: Option<&[u8]>,
    type_filter: Option<u8>,
    output: &mut Vec<u8>,
    depth: usize,
) -> Result<(), i32> {
    if depth > 32 {
        return Err(report(command, b"directory depth exceeded", None));
    }
    let mut stat = syscall::Stat::default();
    syscall::stat(path, &mut stat).map_err(|error| report(command, b"stat failed", Some(error)))?;
    let basename = path.rsplit(|byte| *byte == b'/').next().unwrap_or(path);
    if name_filter.map_or(true, |filter| glob_matches(filter, basename))
        && type_filter.map_or(true, |kind| {
            (kind == b'f' && stat.kind == syscall::STAT_REGULAR)
                || (kind == b'd' && stat.kind == syscall::STAT_DIRECTORY)
        })
    {
        output.extend_from_slice(path);
        output.push(b'\n');
    }
    if stat.kind != syscall::STAT_DIRECTORY {
        return Ok(());
    }
    let mut entries = [syscall::DirEntry::default(); 16];
    let count = syscall::read_dir(path, &mut entries)
        .map_err(|error| report(command, b"read directory failed", Some(error)))?;
    if count > entries.len() {
        return Err(report(command, b"malformed directory count", None));
    }
    for entry in entries.iter().take(count) {
        if entry.name_length as usize > syscall::NAME_MAX {
            return Err(report(command, b"malformed directory entry", None));
        }
        let mut child = Vec::new();
        child.extend_from_slice(path);
        if child.last() != Some(&b'/') {
            child.push(b'/');
        }
        child.extend_from_slice(&entry.name[..entry.name_length as usize]);
        find_visit(command, &child, name_filter, type_filter, output, depth + 1)?;
    }
    Ok(())
}

fn find(command: &[u8], args: &[&[u8]], environment: &[&[u8]]) -> i32 {
    let (raw, expressions) = if args.first() == Some(&b"--".as_slice()) {
        match &args[1..] {
            [] => (b".".as_slice(), &[][..]),
            [path, rest @ ..] => (*path, rest),
        }
    } else {
        match args {
            [] => (b".".as_slice(), &[][..]),
            [path, rest @ ..] => (*path, rest),
        }
    };
    let mut name_filter = None;
    let mut type_filter = None;
    let mut index = 0;
    while index < expressions.len() {
        match expressions.get(index) {
            Some(expression) if *expression == b"-name" => {
                index += 1;
                name_filter = expressions.get(index).copied();
            }
            Some(expression) if *expression == b"-type" => {
                index += 1;
                type_filter = match expressions.get(index).copied() {
                    Some([kind]) if *kind == b'f' || *kind == b'd' => Some(*kind),
                    _ => return usage(command, b"-type accepts f or d"),
                };
            }
            _ => {
                return usage(
                    command,
                    b"supported syntax is find [path] [-name glob] [-type f|d]",
                )
            }
        }
        index += 1;
    }
    if expressions.iter().any(|expression| *expression == b"-name") && name_filter.is_none() {
        return usage(command, b"-name requires a pattern");
    }
    let mut path = Vec::new();
    let absolute = match command_path(command, raw, environment, &mut path) {
        Ok(value) => value.to_vec(),
        Err(error) => return error,
    };
    let mut output = Vec::new();
    match find_visit(command, &absolute, name_filter, type_filter, &mut output, 0) {
        Ok(()) => match write_all(1, &output) {
            Ok(()) => 0,
            Err(error) => report(command, b"write failed", Some(error)),
        },
        Err(status) => status,
    }
}

fn read_sources(
    command: &[u8],
    args: &[&[u8]],
    environment: &[&[u8]],
    mut callback: impl FnMut(&[u8], &[u8]) -> Result<(), i32>,
) -> i32 {
    let mut input = Vec::new();
    let mut path = Vec::new();
    let sources = if args.is_empty() { &[][..] } else { args };
    let mut default = b"-".as_slice();
    let sources = if sources.is_empty() {
        core::slice::from_mut(&mut default)
    } else {
        sources
    };
    let mut status = 0;
    for raw in sources {
        input.clear();
        if *raw == b"-" {
            if let Err(error) = read_all(0, &mut input) {
                status = report(command, b"read failed", Some(error));
                continue;
            }
        } else {
            let absolute = match command_path(command, raw, environment, &mut path) {
                Ok(value) => value,
                Err(error) => {
                    status = error;
                    continue;
                }
            };
            let fd = match open_read(absolute) {
                Ok(fd) => fd,
                Err(error) => {
                    status = report(command, b"open failed", Some(error));
                    continue;
                }
            };
            if let Err(error) = read_all(fd, &mut input) {
                status = report(command, b"read failed", Some(error));
            }
            let _ = syscall::close(fd);
        }
        if callback(raw, &input).is_err() {
            status = 1;
        }
    }
    status
}

fn grep(command: &[u8], args: &[&[u8]], environment: &[&[u8]]) -> i32 {
    if args.is_empty() {
        return usage(command, b"pattern required");
    }
    let mut numbered = false;
    let mut inverted = false;
    let mut index = 0;
    while let Some(option) = args.get(index) {
        match *option {
            b"--" => {
                index += 1;
                break;
            }
            b"-n" => numbered = true,
            b"-v" => inverted = true,
            b"-nv" | b"-vn" => {
                numbered = true;
                inverted = true;
            }
            _ => break,
        }
        index += 1;
    }
    let Some(pattern) = args.get(index) else {
        return usage(command, b"pattern required");
    };
    if pattern.is_empty() {
        return usage(command, b"empty patterns are not supported");
    }
    let files = &args[index + 1..];
    let mut found = false;
    let status = read_sources(command, files, environment, |name, data| {
        let mut line_number = 0usize;
        let mut output = Vec::new();
        for line in data.split(|byte| *byte == b'\n') {
            line_number += 1;
            let matched = line.windows(pattern.len()).any(|window| window == *pattern);
            if matched != inverted {
                found = true;
                if numbered {
                    append_u64(&mut output, line_number as u64);
                    output.push(b':');
                }
                output.extend_from_slice(line);
                output.push(b'\n');
            }
        }
        let _ = name;
        write_all(1, &output).map_err(|error| report(command, b"write failed", Some(error)))
    });
    if status != 0 {
        status
    } else if found {
        0
    } else {
        1
    }
}

fn line_filter(command: &[u8], args: &[&[u8]], environment: &[&[u8]], take_head: bool) -> i32 {
    let mut count = 10usize;
    let mut index = 0;
    if args.first() == Some(&b"--".as_slice()) {
        index = 1;
    } else if let Some(value) = args.first().and_then(|arg| arg.strip_prefix(b"-n")) {
        let value = if value.is_empty() {
            index = 2;
            args.get(1).copied().unwrap_or(b"")
        } else {
            index = 1;
            value
        };
        count = match parse_u64(value).and_then(|value| usize::try_from(value).ok()) {
            Some(value) => value,
            None => return usage(command, b"invalid line count"),
        };
    } else if args.first().is_some_and(|arg| arg.first() == Some(&b'-')) {
        return usage(command, b"unsupported option");
    }
    let files = &args[index..];
    read_sources(command, files, environment, |_, data| {
        let lines = data
            .split_inclusive(|byte| *byte == b'\n')
            .collect::<Vec<_>>();
        let selected = if take_head {
            &lines[..lines.len().min(count)]
        } else {
            &lines[lines.len().saturating_sub(count)..]
        };
        let output = selected
            .iter()
            .flat_map(|line| line.iter().copied())
            .collect::<Vec<_>>();
        write_all(1, &output).map_err(|error| report(command, b"write failed", Some(error)))
    })
}

fn sort_command(command: &[u8], args: &[&[u8]], environment: &[&[u8]]) -> i32 {
    if args
        .iter()
        .any(|arg| arg.first() == Some(&b'-') && *arg != b"-" && *arg != b"--")
    {
        return usage(command, b"only -- and - are supported options");
    }
    read_sources(command, args, environment, |_, data| {
        let mut lines = data.split(|byte| *byte == b'\n').collect::<Vec<_>>();
        if lines.last() == Some(&&[][..]) {
            let _ = lines.pop();
        }
        lines.sort_unstable_by(|left, right| left.cmp(right));
        let mut output = Vec::new();
        for line in lines {
            output.extend_from_slice(line);
            output.push(b'\n');
        }
        write_all(1, &output).map_err(|error| report(command, b"write failed", Some(error)))
    })
}

fn wc(command: &[u8], args: &[&[u8]], environment: &[&[u8]]) -> i32 {
    let mut selected = false;
    let mut lines = false;
    let mut words = false;
    let mut bytes = false;
    let mut index = 0;
    while let Some(arg) = args.get(index) {
        match *arg {
            b"-l" => {
                selected = true;
                lines = true;
            }
            b"-w" => {
                selected = true;
                words = true;
            }
            b"-c" => {
                selected = true;
                bytes = true;
            }
            b"--" => {
                index += 1;
                break;
            }
            b"-" => break,
            _ if arg.first() == Some(&b'-') => return usage(command, b"unsupported option"),
            _ => break,
        }
        index += 1;
    }
    if !selected {
        lines = true;
        words = true;
        bytes = true;
    }
    let files = &args[index..];
    read_sources(command, files, environment, |name, data| {
        let mut output = Vec::new();
        if lines {
            append_u64(&mut output, line_count(data) as u64);
            output.push(b' ');
        }
        if words {
            append_u64(&mut output, word_count(data) as u64);
            output.push(b' ');
        }
        if bytes {
            append_u64(&mut output, data.len() as u64);
            output.push(b' ');
        }
        if !name.is_empty() && name != b"-" {
            output.extend_from_slice(name);
        }
        output.push(b'\n');
        write_all(1, &output).map_err(|error| report(command, b"write failed", Some(error)))
    })
}

fn sleep_command(command: &[u8], args: &[&[u8]]) -> i32 {
    if args.len() != 1 {
        return usage(command, b"exactly one duration is required");
    }
    let Some(duration) = parse_u64(args[0]) else {
        return usage(command, b"duration must be a non-negative integer");
    };
    let Some(ticks) = duration.checked_mul(syscall::SCHEDULER_HZ) else {
        return usage(command, b"duration is too large");
    };
    match syscall::sleep(ticks) {
        Ok(()) => 0,
        Err(error) => report(command, b"sleep failed", Some(error)),
    }
}

pub fn run(command: &[u8], args: &[&[u8]], environment: &[&[u8]]) -> i32 {
    if !is_supported_command(command) {
        return report(command, b"command is not implemented", None) + 126;
    }
    match command {
        b"cat" => cat(command, args, environment),
        b"echo" => echo(command, args),
        b"env" => env_command(command, args, environment),
        b"ls" => ls(command, args, environment),
        b"pwd" => pwd(command, args, environment),
        b"mkdir" => mkdir(command, args, environment),
        b"rmdir" => {
            simple_path_command(command, args, environment, syscall::rmdir, b"rmdir failed")
        }
        b"cp" => cp(command, args, environment),
        b"mv" => {
            if args.len() != 2 {
                usage(command, b"exactly source and destination are required")
            } else {
                let mut old = Vec::new();
                let mut new = Vec::new();
                let old = match command_path(command, args[0], environment, &mut old) {
                    Ok(value) => value.to_vec(),
                    Err(error) => return error,
                };
                let new = match command_path(command, args[1], environment, &mut new) {
                    Ok(value) => value.to_vec(),
                    Err(error) => return error,
                };
                syscall::rename(&old, &new).map_or_else(
                    |error| report(command, b"rename failed", Some(error)),
                    |_| 0,
                )
            }
        }
        b"rm" => {
            let (force, files) = if args.first() == Some(&b"-f".as_slice()) {
                (true, &args[1..])
            } else {
                (false, args)
            };
            if files.is_empty() {
                usage(command, b"operand required")
            } else {
                let mut path = Vec::new();
                let mut status = 0;
                for raw in files {
                    let absolute = match command_path(command, raw, environment, &mut path) {
                        Ok(value) => value.to_vec(),
                        Err(error) => {
                            status = error;
                            continue;
                        }
                    };
                    if let Err(error) = syscall::unlink(&absolute) {
                        if !force || error.errno() != 2 {
                            status = report(command, b"unlink failed", Some(error));
                        }
                    }
                }
                status
            }
        }
        b"touch" => touch(command, args, environment),
        b"ln" => ln(command, args, environment),
        b"stat" => stat_command(command, args, environment),
        b"find" => find(command, args, environment),
        b"grep" => grep(command, args, environment),
        b"head" => line_filter(command, args, environment, true),
        b"tail" => line_filter(command, args, environment, false),
        b"sort" => sort_command(command, args, environment),
        b"wc" => wc(command, args, environment),
        b"true" if args.is_empty() => 0,
        b"false" if args.is_empty() => 1,
        b"true" | b"false" => usage(command, b"no arguments expected"),
        b"sleep" => sleep_command(command, args),
        _ => unreachable!("supported command missing from dispatch"),
    }
}

fn is_supported_command(command: &[u8]) -> bool {
    matches!(
        command,
        b"cat"
            | b"echo"
            | b"env"
            | b"ls"
            | b"pwd"
            | b"mkdir"
            | b"rmdir"
            | b"cp"
            | b"mv"
            | b"rm"
            | b"touch"
            | b"ln"
            | b"stat"
            | b"find"
            | b"grep"
            | b"head"
            | b"tail"
            | b"sort"
            | b"wc"
            | b"true"
            | b"false"
            | b"sleep"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bounded_decimal_values() {
        assert_eq!(parse_u64(b"0"), Some(0));
        assert_eq!(parse_u64(b"42"), Some(42));
        assert_eq!(parse_u64(b""), None);
        assert_eq!(parse_u64(b"-1"), None);
        assert_eq!(parse_u64(b"999999999999999999999999"), None);
    }

    #[test]
    fn counts_lines_and_shell_words_without_locale() {
        let input = b"one two\nthree\n";
        assert_eq!(line_count(input), 2);
        assert_eq!(word_count(input), 3);
    }

    #[test]
    fn relative_paths_use_pwd() {
        let mut output = Vec::new();
        assert_eq!(
            absolute_path(b"file", &[b"PWD=/tmp"], &mut output),
            Ok(&b"/tmp/file"[..])
        );
        assert_eq!(
            absolute_path(b"/file", &[b"PWD=/tmp"], &mut output),
            Ok(&b"/file"[..])
        );
    }

    #[test]
    fn malformed_paths_and_bounded_globs_are_rejected_deterministically() {
        let mut output = Vec::new();
        assert!(absolute_path(b"", &[b"PWD=/tmp"], &mut output).is_err());
        assert!(absolute_path(b"bad\0path", &[b"PWD=/tmp"], &mut output).is_err());
        assert!(absolute_path(b"file", &[b"PWD=relative"], &mut output).is_err());
        assert!(!absolute_path(&[b'a'; 256], &[b"PWD=/tmp"], &mut output).is_ok());

        assert!(glob_matches(b"*.txt", b"notes.txt"));
        assert!(glob_matches(b"a?c", b"abc"));
        assert!(glob_matches(b"a*c", b"abbbbbc"));
        assert!(!glob_matches(b"*.txt", b"notes.bin"));
        assert!(!glob_matches(b"a?c", b"ac"));
    }

    #[test]
    fn bytewise_sort_is_deterministic() {
        let mut values = [b"z".as_slice(), b"a".as_slice(), b"m".as_slice()];
        values.sort_unstable_by(|left, right| left.cmp(right));
        assert_eq!(values, [b"a".as_slice(), b"m".as_slice(), b"z".as_slice()]);
    }

    #[test]
    fn syscall_structs_match_the_v2_abi() {
        assert_eq!(core::mem::size_of::<syscall::Stat>(), 24);
        assert_eq!(core::mem::size_of::<syscall::DirEntry>(), 56);
        assert_eq!(syscall::ABI_VERSION, 2);
        assert_eq!(syscall::MAX_PATH, 256);
        assert_eq!(syscall::MAX_DIR_ENTRIES, 16);
    }

    #[test]
    fn dispatch_contract_contains_exactly_twenty_two_commands() {
        let commands: &[&[u8]] = &[
            b"cat", b"echo", b"env", b"ls", b"pwd", b"mkdir", b"rmdir", b"cp", b"mv",
            b"rm", b"touch", b"ln", b"stat", b"find", b"grep", b"head", b"tail", b"sort",
            b"wc", b"true", b"false", b"sleep",
        ];
        assert_eq!(commands.len(), 22);
        assert!(commands.iter().all(|command| is_supported_command(command)));
        assert!(!is_supported_command(b"sh"));
    }
}
