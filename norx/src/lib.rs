#![no_std]

extern crate alloc;
#[cfg(test)]
extern crate std;

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
        Fstat = 430,
    }

    pub const OPEN_READ: Word = 1 << 0;
    pub const OPEN_WRITE: Word = 1 << 1;
    pub const OPEN_CREATE: Word = 1 << 2;
    pub const OPEN_TRUNCATE: Word = 1 << 3;
    pub const OPEN_APPEND: Word = 1 << 4;
    pub const OPEN_EXCLUSIVE: Word = 1 << 5;
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

    pub fn fstat(fd: Word, output: &mut Stat) -> Result<(), Error> {
        result(unsafe {
            raw6(
                Number::Fstat as Word,
                [fd, output as *mut Stat as Word, 0, 0, 0, 0],
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
const MAX_ALLOC_RETRIES: usize = 8;
const COMMAND_HEAP_BYTES: usize = 48 * 1024;
const MAX_INPUT_BYTES: usize = 32 * 1024;
const MAX_SYSCALL_RETRIES: usize = 8;
const MAX_TEMP_ATTEMPTS: usize = 16;
const FIND_MAX_DEPTH: usize = 32;
const FIND_MAX_NODES: usize = 1024;
const FIND_MAX_OUTPUT_BYTES: usize = 32 * 1024;
const FIND_MAX_WORK: usize = 64 * 1024;
const EINTR: syscall::Word = 4;
const EAGAIN: syscall::Word = 11;
const EEXIST: syscall::Word = 17;
const EIO: syscall::Word = 5;
const ENOMEM: syscall::Word = 12;
const EOVERFLOW: syscall::Word = 75;

struct Heap {
    bytes: UnsafeCell<[u8; HEAP_BYTES]>,
    cursor: AtomicUsize,
    limit: AtomicUsize,
}

unsafe impl Sync for Heap {}

impl Heap {
    const fn new() -> Self {
        Self {
            bytes: UnsafeCell::new([0; HEAP_BYTES]),
            cursor: AtomicUsize::new(0),
            limit: AtomicUsize::new(HEAP_BYTES),
        }
    }

    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let base = self.bytes.get() as *mut u8 as usize;
        let alignment_mask = layout.align() - 1;
        let limit = self.limit.load(AtomicOrdering::Acquire).min(HEAP_BYTES);
        for _ in 0..MAX_ALLOC_RETRIES {
            let cursor = self.cursor.load(AtomicOrdering::Relaxed);
            let Some(start) = base.checked_add(cursor) else {
                return core::ptr::null_mut();
            };
            let Some(aligned) = start
                .checked_add(alignment_mask)
                .map(|value| value & !alignment_mask)
            else {
                return core::ptr::null_mut();
            };
            let Some(offset) = aligned.checked_sub(base) else {
                return core::ptr::null_mut();
            };
            let Some(end) = offset.checked_add(layout.size()) else {
                return core::ptr::null_mut();
            };
            if end > limit {
                return core::ptr::null_mut();
            }
            if self
                .cursor
                .compare_exchange(cursor, end, AtomicOrdering::AcqRel, AtomicOrdering::Relaxed)
                .is_ok()
            {
                return aligned as *mut u8;
            }
            core::hint::spin_loop();
        }
        core::ptr::null_mut()
    }
}

struct BumpAllocator;

static HEAP: Heap = Heap::new();

unsafe impl GlobalAlloc for BumpAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        HEAP.alloc(layout)
    }

    unsafe fn dealloc(&self, _pointer: *mut u8, _layout: Layout) {}
}

#[cfg_attr(not(test), global_allocator)]
static ALLOCATOR: BumpAllocator = BumpAllocator;

#[cfg(test)]
#[global_allocator]
static TEST_ALLOCATOR: std::alloc::System = std::alloc::System;

struct CommandBudget {
    previous_limit: usize,
}

impl CommandBudget {
    fn enter() -> Self {
        let previous_limit = HEAP.limit.load(AtomicOrdering::Acquire);
        let start = HEAP.cursor.load(AtomicOrdering::Acquire);
        let limit = start
            .saturating_add(COMMAND_HEAP_BYTES)
            .min(HEAP_BYTES)
            .min(previous_limit);
        HEAP.limit.store(limit, AtomicOrdering::Release);
        Self { previous_limit }
    }
}

impl Drop for CommandBudget {
    fn drop(&mut self) {
        HEAP.limit
            .store(self.previous_limit, AtomicOrdering::Release);
    }
}

fn out_of_memory() -> syscall::Error {
    syscall::Error(0u64.wrapping_sub(ENOMEM))
}

fn try_reserve<T>(output: &mut Vec<T>, additional: usize) -> Result<(), syscall::Error> {
    output.try_reserve(additional).map_err(|_| out_of_memory())
}

fn try_reserve_exact<T>(output: &mut Vec<T>, additional: usize) -> Result<(), syscall::Error> {
    output
        .try_reserve_exact(additional)
        .map_err(|_| out_of_memory())
}

fn try_push<T>(output: &mut Vec<T>, value: T) -> Result<(), syscall::Error> {
    try_reserve(output, 1)?;
    output.push(value);
    Ok(())
}

fn try_extend<T: Clone>(output: &mut Vec<T>, input: &[T]) -> Result<(), syscall::Error> {
    try_reserve(output, input.len())?;
    output.extend_from_slice(input);
    Ok(())
}

fn try_clone<T: Clone>(input: &[T]) -> Result<Vec<T>, syscall::Error> {
    let mut output = Vec::new();
    try_reserve_exact(&mut output, input.len())?;
    output.extend_from_slice(input);
    Ok(output)
}

const STARTUP_MAGIC: u64 = 0x4e4f525853544152;
pub const STARTUP_VERSION: u64 = 1;
const STARTUP_HEADER_WORDS: usize = 12;
const STARTUP_HEADER_BYTES: usize = STARTUP_HEADER_WORDS * core::mem::size_of::<u64>();
pub const STARTUP_MAX_BYTES: usize = 4096;
pub const STARTUP_MAX_ARGUMENTS: usize = 16;
pub const STARTUP_MAX_ENVIRONMENT: usize = 16;
pub const STARTUP_REJECT_STATUS: i32 = 127;
const STARTUP_AUXV_ENTRIES: usize = 3;
const AT_PAGESZ: u64 = 6;
const AT_ENTRY: u64 = 9;
const PAGE_SIZE: u64 = 4096;

#[repr(C)]
#[derive(Clone, Copy)]
struct StartupBlock {
    magic: u64,
    version: u64,
    header_size: u64,
    block_size: u64,
    argc: u64,
    envc: u64,
    argv_offset: u64,
    envp_offset: u64,
    auxv_offset: u64,
    auxv_count: u64,
    strings_offset: u64,
    strings_size: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartupError {
    NullStack,
    MisalignedStack,
    InvalidHeader,
    UnsupportedVersion,
    InvalidSize,
    TooManyArguments,
    InvalidLayout,
    InvalidAuxiliaryVector,
    InvalidPointer,
    UnterminatedString,
}

#[derive(Clone, Copy)]
pub struct Startup {
    pub argc: usize,
    pub envc: usize,
    argv: usize,
    envp: usize,
    strings_end: usize,
}

pub unsafe fn startup_from_stack(stack: *const usize) -> Result<Startup, StartupError> {
    if stack.is_null() {
        return Err(StartupError::NullStack);
    }
    let base = stack as usize;
    if base & (core::mem::align_of::<StartupBlock>() - 1) != 0 {
        return Err(StartupError::MisalignedStack);
    }
    let header = stack.cast::<StartupBlock>().read();
    if header.magic != STARTUP_MAGIC {
        return Err(StartupError::InvalidHeader);
    }
    if header.version != STARTUP_VERSION {
        return Err(StartupError::UnsupportedVersion);
    }
    if header.header_size != STARTUP_HEADER_BYTES as u64 {
        return Err(StartupError::InvalidHeader);
    }
    let block_size = usize_from_startup_word(header.block_size).ok_or(StartupError::InvalidSize)?;
    if !(STARTUP_HEADER_BYTES..=STARTUP_MAX_BYTES).contains(&block_size) {
        return Err(StartupError::InvalidSize);
    }
    let block_end = base
        .checked_add(block_size)
        .ok_or(StartupError::InvalidSize)?;
    let argc = usize_from_startup_word(header.argc).ok_or(StartupError::TooManyArguments)?;
    let envc = usize_from_startup_word(header.envc).ok_or(StartupError::TooManyArguments)?;
    if argc > STARTUP_MAX_ARGUMENTS || envc > STARTUP_MAX_ENVIRONMENT {
        return Err(StartupError::TooManyArguments);
    }

    let argv_bytes = argc
        .checked_add(1)
        .and_then(|count| count.checked_mul(core::mem::size_of::<u64>()))
        .ok_or(StartupError::InvalidLayout)?;
    let envp_bytes = envc
        .checked_add(1)
        .and_then(|count| count.checked_mul(core::mem::size_of::<u64>()))
        .ok_or(StartupError::InvalidLayout)?;
    let auxv_bytes = STARTUP_AUXV_ENTRIES
        .checked_mul(2)
        .and_then(|count| count.checked_mul(core::mem::size_of::<u64>()))
        .ok_or(StartupError::InvalidLayout)?;
    let argv_offset = STARTUP_HEADER_BYTES;
    let envp_offset = argv_offset
        .checked_add(argv_bytes)
        .ok_or(StartupError::InvalidLayout)?;
    let auxv_offset = envp_offset
        .checked_add(envp_bytes)
        .ok_or(StartupError::InvalidLayout)?;
    if header.argv_offset != argv_offset as u64
        || header.envp_offset != envp_offset as u64
        || header.auxv_offset != auxv_offset as u64
        || header.auxv_count != STARTUP_AUXV_ENTRIES as u64
    {
        return Err(StartupError::InvalidLayout);
    }
    let argv = bounded_array(base, header.argv_offset, argv_bytes, block_end)?;
    let envp = bounded_array(base, header.envp_offset, envp_bytes, block_end)?;
    let auxv = bounded_array(base, header.auxv_offset, auxv_bytes, block_end)?;

    let strings_offset =
        usize_from_startup_word(header.strings_offset).ok_or(StartupError::InvalidLayout)?;
    let strings_size =
        usize_from_startup_word(header.strings_size).ok_or(StartupError::InvalidLayout)?;
    if strings_offset < auxv_offset + auxv_bytes
        || strings_offset > block_size
        || strings_size != block_size - strings_offset
    {
        return Err(StartupError::InvalidLayout);
    }
    let strings_start = base
        .checked_add(strings_offset)
        .ok_or(StartupError::InvalidLayout)?;
    let strings_end = strings_start
        .checked_add(strings_size)
        .ok_or(StartupError::InvalidLayout)?;
    if strings_end != block_end {
        return Err(StartupError::InvalidLayout);
    }

    if read_word(auxv, 0) != AT_PAGESZ
        || read_word(auxv, 1) != PAGE_SIZE
        || read_word(auxv, 2) != AT_ENTRY
        || read_word(auxv, 3) == 0
        || read_word(auxv, 4) != 0
        || read_word(auxv, 5) != 0
    {
        return Err(StartupError::InvalidAuxiliaryVector);
    }
    for index in 0..argc {
        validate_string(read_word(argv, index), strings_start, strings_end)?;
    }
    if read_word(argv, argc) != 0 {
        return Err(StartupError::InvalidLayout);
    }
    for index in 0..envc {
        validate_string(read_word(envp, index), strings_start, strings_end)?;
    }
    if read_word(envp, envc) != 0 {
        return Err(StartupError::InvalidLayout);
    }
    Ok(Startup {
        argc,
        envc,
        argv,
        envp,
        strings_end,
    })
}

fn usize_from_startup_word(value: u64) -> Option<usize> {
    (value <= usize::MAX as u64).then_some(value as usize)
}

fn bounded_array(
    base: usize,
    offset: u64,
    length: usize,
    block_end: usize,
) -> Result<usize, StartupError> {
    let offset = usize_from_startup_word(offset).ok_or(StartupError::InvalidLayout)?;
    if !offset.is_multiple_of(core::mem::size_of::<u64>()) {
        return Err(StartupError::InvalidLayout);
    }
    let start = base
        .checked_add(offset)
        .ok_or(StartupError::InvalidLayout)?;
    let end = start
        .checked_add(length)
        .ok_or(StartupError::InvalidLayout)?;
    if end > block_end {
        return Err(StartupError::InvalidLayout);
    }
    Ok(start)
}

unsafe fn read_word(address: usize, index: usize) -> u64 {
    let offset = index
        .checked_mul(core::mem::size_of::<u64>())
        .expect("validated startup array index");
    core::ptr::read_unaligned((address + offset) as *const u64)
}

unsafe fn validate_string(
    pointer: u64,
    strings_start: usize,
    strings_end: usize,
) -> Result<(), StartupError> {
    let pointer = usize_from_startup_word(pointer).ok_or(StartupError::InvalidPointer)?;
    if pointer < strings_start || pointer >= strings_end {
        return Err(StartupError::InvalidPointer);
    }
    let mut cursor = pointer;
    while cursor < strings_end {
        if (cursor as *const u8).read() == 0 {
            return Ok(());
        }
        cursor += 1;
    }
    Err(StartupError::UnterminatedString)
}

unsafe fn c_string(pointer: u64, strings_end: usize) -> &'static [u8] {
    let pointer = pointer as usize;
    let mut end = pointer;
    while end < strings_end && (end as *const u8).read() != 0 {
        end += 1;
    }
    core::slice::from_raw_parts(pointer as *const u8, end - pointer)
}

pub fn collect_args(startup: Startup) -> Result<Vec<&'static [u8]>, syscall::Error> {
    let mut result = Vec::new();
    try_reserve_exact(&mut result, startup.argc)?;
    for index in 0..startup.argc {
        result.push(unsafe { c_string(read_word(startup.argv, index), startup.strings_end) });
    }
    Ok(result)
}

pub fn collect_environment(startup: Startup) -> Result<Vec<&'static [u8]>, syscall::Error> {
    let mut result = Vec::new();
    try_reserve_exact(&mut result, startup.envc)?;
    for index in 0..startup.envc {
        result.push(unsafe { c_string(read_word(startup.envp, index), startup.strings_end) });
    }
    Ok(result)
}

fn write_all(fd: syscall::Word, bytes: &[u8]) -> Result<(), syscall::Error> {
    let mut offset = 0;
    while offset < bytes.len() {
        let remaining = bytes.len() - offset;
        let count = write_retry(fd, bytes[offset..].as_ptr(), remaining)?;
        if count == 0 || count > remaining {
            return Err(syscall::Error(0u64.wrapping_sub(EIO)));
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

fn read_all_with(
    output: &mut Vec<u8>,
    mut read: impl FnMut(&mut [u8]) -> Result<usize, syscall::Error>,
) -> Result<(), syscall::Error> {
    if output.len() > MAX_INPUT_BYTES {
        return Err(syscall::Error(0u64.wrapping_sub(EOVERFLOW)));
    }
    let additional = MAX_INPUT_BYTES.saturating_sub(output.capacity());
    try_reserve_exact(output, additional)?;
    let mut buffer = [0u8; 1024];
    loop {
        match read(&mut buffer)? {
            0 => return Ok(()),
            count => {
                if count > buffer.len() {
                    return Err(syscall::Error(0u64.wrapping_sub(EIO)));
                }
                let Some(new_length) = output.len().checked_add(count) else {
                    return Err(syscall::Error(0u64.wrapping_sub(EOVERFLOW)));
                };
                if new_length > MAX_INPUT_BYTES {
                    return Err(syscall::Error(0u64.wrapping_sub(EOVERFLOW)));
                }
                try_extend(output, &buffer[..count])?;
            }
        }
    }
}

fn read_all(fd: syscall::Word, output: &mut Vec<u8>) -> Result<(), syscall::Error> {
    read_all_with(output, |buffer| {
        read_retry(fd, buffer.as_mut_ptr(), buffer.len())
    })
}

fn append_u64(output: &mut Vec<u8>, mut value: u64) -> Result<(), syscall::Error> {
    let mut digits = [0u8; 20];
    let mut count = 0;
    if value == 0 {
        return try_push(output, b'0');
    }
    while value != 0 {
        digits[count] = b'0' + (value % 10) as u8;
        count += 1;
        value /= 10;
    }
    while count != 0 {
        count -= 1;
        try_push(output, digits[count])?;
    }
    Ok(())
}

fn write_u64(fd: syscall::Word, mut value: u64) -> Result<(), syscall::Error> {
    let mut digits = [0u8; 20];
    let mut count = digits.len();
    if value == 0 {
        return write_all(fd, b"0");
    }
    while value != 0 {
        count -= 1;
        digits[count] = b'0' + (value % 10) as u8;
        value /= 10;
    }
    write_all(fd, &digits[count..])
}

fn write_line(fd: syscall::Word, bytes: &[u8]) -> Result<(), syscall::Error> {
    write_all(fd, bytes)?;
    write_all(fd, b"\n")
}

fn report(command: &[u8], reason: &[u8], error: Option<syscall::Error>) -> i32 {
    let _ = write_all(2, b"coreutils: ");
    let _ = write_all(2, command);
    let _ = write_all(2, b": ");
    let _ = write_all(2, reason);
    if let Some(error) = error {
        let _ = write_all(2, b" (errno ");
        let _ = write_u64(2, error.errno());
        let _ = write_all(2, b")");
    }
    let _ = write_all(2, b"\n");
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
    output: &'a mut [u8; syscall::MAX_PATH],
) -> Result<&'a [u8], &'static [u8]> {
    if raw.is_empty() || raw.contains(&0) {
        return Err(b"empty or NUL path");
    }
    let cwd = if raw.first() == Some(&b'/') {
        &[][..]
    } else {
        let cwd = env_value(environment, b"PWD").unwrap_or(b"/");
        if cwd.first() != Some(&b'/') || cwd.contains(&0) {
            return Err(b"PWD is not absolute");
        }
        cwd
    };
    for component in cwd
        .split(|byte| *byte == b'/')
        .chain(raw.split(|byte| *byte == b'/'))
    {
        if component.len() > syscall::NAME_MAX {
            return Err(b"path component is too long");
        }
    }
    let separator = usize::from(!cwd.is_empty() && cwd.last() != Some(&b'/'));
    let length = cwd
        .len()
        .checked_add(separator)
        .and_then(|length| length.checked_add(raw.len()))
        .ok_or(&b"path is too long"[..])?;
    if length > syscall::MAX_PATH {
        return Err(b"path is too long");
    }
    if cwd.is_empty() {
        output[..raw.len()].copy_from_slice(raw);
    } else {
        output[..cwd.len()].copy_from_slice(cwd);
        if separator != 0 {
            output[cwd.len()] = b'/';
        }
        output[cwd.len() + separator..length].copy_from_slice(raw);
    }
    Ok(&output[..length])
}

fn open_read(path: &[u8]) -> Result<syscall::Word, syscall::Error> {
    syscall::open(path, syscall::OPEN_READ, 0)
}

fn temporary_path(
    destination: &[u8],
    attempt: usize,
    output: &mut [u8; syscall::MAX_PATH],
) -> Option<usize> {
    if attempt >= MAX_TEMP_ATTEMPTS {
        return None;
    }
    let Some(separator) = destination.iter().rposition(|byte| *byte == b'/') else {
        return None;
    };
    let suffix = b".norx-cp-";
    let prefix_length = separator + 1;
    let length = prefix_length
        .checked_add(suffix.len())
        .and_then(|length| length.checked_add(1))?;
    if length > syscall::MAX_PATH {
        return None;
    }
    output[..prefix_length].copy_from_slice(&destination[..prefix_length]);
    output[prefix_length..prefix_length + suffix.len()].copy_from_slice(suffix);
    output[length - 1] = b"0123456789abcdef"[attempt];
    Some(length)
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
    path: &'a mut [u8; syscall::MAX_PATH],
) -> Result<&'a [u8], i32> {
    // Keep lexical aliases intact. The kernel VFS resolves them in its
    // namespace, and this ABI has no symlink nodes to follow.
    absolute_path(raw, environment, path).map_err(|reason| report(command, reason, None))
}

fn cat(command: &[u8], args: &[&[u8]], environment: &[&[u8]]) -> i32 {
    let mut path = [0u8; syscall::MAX_PATH];
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
    for (index, arg) in args.iter().enumerate() {
        if index != 0 {
            if let Err(error) = write_all(1, b" ") {
                return report(command, b"write failed", Some(error));
            }
        }
        if let Err(error) = write_all(1, arg) {
            return report(command, b"write failed", Some(error));
        }
    }
    if !no_newline {
        if let Err(error) = write_all(1, b"\n") {
            return report(command, b"write failed", Some(error));
        }
    }
    0
}

fn env_command(command: &[u8], args: &[&[u8]], environment: &[&[u8]]) -> i32 {
    let mut index = 0;
    if args.first() == Some(&b"-i".as_slice()) {
        index = 1;
    }
    if args.get(index) == Some(&b"--".as_slice()) {
        index += 1;
    }
    let overrides = &args[index..];
    for arg in overrides {
        let Some(separator) = arg.iter().position(|byte| *byte == b'=') else {
            return usage(command, b"command execution is not part of env yet");
        };
        if separator == 0 {
            return usage(command, b"invalid empty environment name");
        }
    }
    if args.first() != Some(&b"-i".as_slice()) {
        for entry in environment {
            let overridden =
                entry
                    .iter()
                    .position(|byte| *byte == b'=')
                    .is_some_and(|_separator| {
                        overrides.iter().any(|arg| {
                            let separator = arg.iter().position(|byte| *byte == b'=').unwrap();
                            entry[..separator] == arg[..separator]
                        })
                    });
            if !overridden {
                if let Err(error) = write_line(1, entry) {
                    return report(command, b"write failed", Some(error));
                }
            }
        }
    }
    for (override_index, arg) in overrides.iter().enumerate() {
        let separator = arg.iter().position(|byte| *byte == b'=').unwrap();
        let is_last = overrides[override_index + 1..].iter().all(|later| {
            let later_separator = later.iter().position(|byte| *byte == b'=').unwrap();
            later[..later_separator] != arg[..separator]
        });
        if is_last {
            if let Err(error) = write_line(1, arg) {
                return report(command, b"write failed", Some(error));
            }
        }
    }
    0
}

fn pwd(command: &[u8], args: &[&[u8]], environment: &[&[u8]]) -> i32 {
    if !args.is_empty() {
        return usage(command, b"no arguments expected");
    }
    let value = env_value(environment, b"PWD").unwrap_or(b"/");
    match write_line(1, value) {
        Ok(()) => 0,
        Err(error) => report(command, b"write failed", Some(error)),
    }
}

fn stat_one(
    command: &[u8],
    raw: &[u8],
    environment: &[&[u8]],
    path: &mut [u8; syscall::MAX_PATH],
) -> Result<(), i32> {
    let absolute = command_path(command, raw, environment, path)?;
    let mut value = syscall::Stat::default();
    syscall::stat(absolute, &mut value)
        .map_err(|error| report(command, b"stat failed", Some(error)))?;
    write_all(1, b"  File: ")
        .and_then(|_| write_all(1, raw))
        .and_then(|_| write_all(1, b"\n  Size: "))
        .and_then(|_| write_u64(1, value.size))
        .and_then(|_| write_all(1, b"\n  Type: "))
        .and_then(|_| {
            write_all(
                1,
                if value.kind == syscall::STAT_DIRECTORY {
                    b"directory"
                } else {
                    b"regular"
                },
            )
        })
        .and_then(|_| write_all(1, b"\n  Mode: "))
        .and_then(|_| write_u64(1, value.mode as u64))
        .and_then(|_| write_all(1, b"\n Links: "))
        .and_then(|_| write_u64(1, value.links as u64))
        .and_then(|_| write_all(1, b"\n Inode: "))
        .and_then(|_| write_u64(1, value.inode as u64))
        .and_then(|_| write_all(1, b"\n"))
        .map_err(|error| report(command, b"write failed", Some(error)))
}

fn stat_command(command: &[u8], args: &[&[u8]], environment: &[&[u8]]) -> i32 {
    if args.is_empty() {
        return usage(command, b"operand required");
    }
    let mut path = [0u8; syscall::MAX_PATH];
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
    path: &mut [u8; syscall::MAX_PATH],
    all: bool,
    long: bool,
) -> Result<(), i32> {
    let absolute = command_path(command, raw, environment, path)?;
    let mut value = syscall::Stat::default();
    syscall::stat(absolute, &mut value)
        .map_err(|error| report(command, b"stat failed", Some(error)))?;
    if value.kind != syscall::STAT_DIRECTORY {
        return write_line(1, raw).map_err(|error| report(command, b"write failed", Some(error)));
    }
    let mut entries = [syscall::DirEntry::default(); 16];
    let count = syscall::read_dir(absolute, &mut entries)
        .map_err(|error| report(command, b"read directory failed", Some(error)))?;
    if count > entries.len() {
        return Err(report(command, b"malformed directory count", None));
    }
    for entry in entries.iter().take(count) {
        usize::try_from(entry.name_length)
            .ok()
            .filter(|length| *length <= syscall::NAME_MAX)
            .ok_or_else(|| report(command, b"malformed directory entry", None))?;
    }
    entries[..count].sort_unstable_by(|left, right| {
        left.name[..left.name_length as usize].cmp(&right.name[..right.name_length as usize])
    });
    for entry in entries.iter().take(count) {
        let length = usize::try_from(entry.name_length)
            .ok()
            .filter(|length| *length <= syscall::NAME_MAX)
            .ok_or_else(|| report(command, b"malformed directory entry", None))?;
        let name = &entry.name[..length];
        if !all && name.first() == Some(&b'.') {
            continue;
        }
        if long {
            write_all(
                1,
                if entry.kind == syscall::STAT_DIRECTORY {
                    b"d"
                } else {
                    b"-"
                },
            )
            .and_then(|_| write_all(1, b" "))
            .and_then(|_| write_u64(1, entry.links as u64))
            .and_then(|_| write_all(1, b" "))
            .and_then(|_| write_u64(1, entry.size))
            .and_then(|_| write_all(1, b" "))
            .and_then(|_| write_all(1, name))
            .and_then(|_| write_all(1, b"\n"))
            .map_err(|error| report(command, b"write failed", Some(error)))?;
        } else {
            write_line(1, name).map_err(|error| report(command, b"write failed", Some(error)))?;
        }
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
    let mut path = [0u8; syscall::MAX_PATH];
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
    for arg in args {
        if *arg == b"-p" {
            parents = true;
        }
    }
    if !args.iter().any(|arg| *arg != b"-p".as_slice()) {
        return usage(command, b"operand required");
    }
    let mut path = [0u8; syscall::MAX_PATH];
    let mut status = 0;
    for raw in args.iter().filter(|arg| **arg != b"-p") {
        let absolute = match command_path(command, raw, environment, &mut path) {
            Ok(value) => value,
            Err(error) => {
                status = error;
                continue;
            }
        };
        if parents {
            for (index, byte) in absolute.iter().enumerate() {
                if *byte == b'/' && index != 0 {
                    let _ = syscall::mkdir(&absolute[..index], 0o755);
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
    let mut path = [0u8; syscall::MAX_PATH];
    let mut status = 0;
    for raw in args {
        let absolute = match command_path(command, raw, environment, &mut path) {
            Ok(value) => value,
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
    let mut source_path = [0u8; syscall::MAX_PATH];
    let mut destination_path = [0u8; syscall::MAX_PATH];
    let source = match command_path(command, args[0], environment, &mut source_path) {
        Ok(value) => value,
        Err(error) => return error,
    };
    let destination = match command_path(command, args[1], environment, &mut destination_path) {
        Ok(value) => value,
        Err(error) => return error,
    };
    if source == destination {
        return report(command, b"source and destination are the same file", None);
    }
    let input = match open_read(&source) {
        Ok(fd) => fd,
        Err(error) => return report(command, b"open source failed", Some(error)),
    };
    let mut source_stat = syscall::Stat::default();
    if let Err(error) = syscall::fstat(input, &mut source_stat) {
        let _ = syscall::close(input);
        return report(command, b"stat source failed", Some(error));
    }
    if source_stat.kind != syscall::STAT_REGULAR {
        let _ = syscall::close(input);
        return report(command, b"source is not a regular file", None);
    }
    let mut destination_stat = syscall::Stat::default();
    match syscall::stat(&destination, &mut destination_stat) {
        Ok(()) if source_stat.inode == destination_stat.inode => {
            let _ = syscall::close(input);
            return report(command, b"source and destination are the same file", None);
        }
        Ok(()) => {}
        Err(error) if error.errno() == 2 => {}
        Err(error) => {
            let _ = syscall::close(input);
            return report(command, b"stat destination failed", Some(error));
        }
    }

    let mut temporary = [0u8; syscall::MAX_PATH];
    let mut temporary_length = 0;
    let mut output = None;
    for attempt in 0..MAX_TEMP_ATTEMPTS {
        let Some(length) = temporary_path(&destination, attempt, &mut temporary) else {
            let _ = syscall::close(input);
            return report(command, b"temporary path is too long", None);
        };
        temporary_length = length;
        let temporary = &temporary[..temporary_length];
        match syscall::open(
            temporary,
            syscall::OPEN_WRITE | syscall::OPEN_CREATE | syscall::OPEN_EXCLUSIVE,
            0o666,
        ) {
            Ok(fd) => {
                output = Some(fd);
                break;
            }
            Err(error) if error.errno() == EEXIST => {}
            Err(error) => {
                let _ = syscall::close(input);
                return report(command, b"open temporary failed", Some(error));
            }
        }
    }
    let Some(output) = output else {
        let _ = syscall::close(input);
        return report(command, b"no temporary path available", None);
    };
    let result = copy_fd(input, output);
    let _ = syscall::close(input);
    match result {
        Ok(()) => {
            if let Err(error) = syscall::close(output) {
                let _ = syscall::unlink(&temporary[..temporary_length]);
                return report(command, b"close temporary failed", Some(error));
            }
            match syscall::rename(&temporary[..temporary_length], &destination) {
                Ok(()) => 0,
                Err(error) => {
                    let _ = syscall::unlink(&temporary[..temporary_length]);
                    report(command, b"rename failed", Some(error))
                }
            }
        }
        Err(error) => {
            let _ = syscall::close(output);
            let _ = syscall::unlink(&temporary[..temporary_length]);
            report(command, b"copy failed", Some(error))
        }
    }
}

fn touch(command: &[u8], args: &[&[u8]], environment: &[&[u8]]) -> i32 {
    if args.is_empty() {
        return usage(command, b"operand required");
    }
    let mut path = [0u8; syscall::MAX_PATH];
    let mut status = 0;
    for raw in args {
        let absolute = match command_path(command, raw, environment, &mut path) {
            Ok(value) => value,
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
    let mut old_path = [0u8; syscall::MAX_PATH];
    let mut new_path = [0u8; syscall::MAX_PATH];
    let old = match command_path(command, args[0], environment, &mut old_path) {
        Ok(value) => value,
        Err(error) => return error,
    };
    let new = match command_path(command, args[1], environment, &mut new_path) {
        Ok(value) => value,
        Err(error) => return error,
    };
    match syscall::link(&old, &new) {
        Ok(()) => 0,
        Err(error) => report(command, b"link failed", Some(error)),
    }
}

struct FindState {
    nodes: usize,
    output_bytes: usize,
    work: usize,
    directories: [u32; FIND_MAX_NODES],
    directory_count: usize,
}

impl FindState {
    fn new() -> Self {
        Self {
            nodes: 0,
            output_bytes: 0,
            work: 0,
            directories: [0; FIND_MAX_NODES],
            directory_count: 0,
        }
    }

    fn charge_work(&mut self, units: usize) -> bool {
        let Some(work) = self.work.checked_add(units) else {
            return false;
        };
        if work > FIND_MAX_WORK {
            return false;
        }
        self.work = work;
        true
    }

    fn visit_node(&mut self) -> bool {
        let Some(nodes) = self.nodes.checked_add(1) else {
            return false;
        };
        if nodes > FIND_MAX_NODES {
            return false;
        }
        self.nodes = nodes;
        true
    }

    fn reserve_output(&mut self, bytes: usize) -> bool {
        let Some(output_bytes) = self.output_bytes.checked_add(bytes) else {
            return false;
        };
        if output_bytes > FIND_MAX_OUTPUT_BYTES {
            return false;
        }
        self.output_bytes = output_bytes;
        true
    }

    fn visit_directory(&mut self, inode: u32) -> Option<bool> {
        if self.directories[..self.directory_count]
            .iter()
            .any(|visited| *visited == inode)
        {
            return Some(false);
        }
        let slot = self.directory_count;
        if slot == self.directories.len() {
            return None;
        }
        self.directories[slot] = inode;
        self.directory_count += 1;
        Some(true)
    }
}

fn find_visit(
    command: &[u8],
    path: &mut [u8; syscall::MAX_PATH],
    path_length: usize,
    name_filter: Option<&[u8]>,
    type_filter: Option<u8>,
    state: &mut FindState,
    depth: usize,
) -> Result<(), i32> {
    if depth > FIND_MAX_DEPTH {
        return Err(report(command, b"directory depth exceeded", None));
    }
    if !state.charge_work(1) {
        return Err(report(command, b"work budget exceeded", None));
    }
    if !state.visit_node() {
        return Err(report(command, b"visited node budget exceeded", None));
    }
    let mut stat = syscall::Stat::default();
    syscall::stat(&path[..path_length], &mut stat)
        .map_err(|error| report(command, b"stat failed", Some(error)))?;
    if stat.kind == syscall::STAT_DIRECTORY {
        match state.visit_directory(stat.inode) {
            Some(true) => {}
            Some(false) => return Err(report(command, b"directory cycle detected", None)),
            None => return Err(report(command, b"visited node budget exceeded", None)),
        }
    }
    let basename = path[..path_length]
        .rsplit(|byte| *byte == b'/')
        .next()
        .unwrap_or(&path[..path_length]);
    let match_work = name_filter
        .map(|filter| {
            filter
                .len()
                .saturating_mul(basename.len())
                .saturating_add(filter.len())
                .saturating_add(1)
        })
        .unwrap_or(1);
    if !state.charge_work(match_work) {
        return Err(report(command, b"work budget exceeded", None));
    }
    let name_matches = name_filter.map_or(true, |filter| glob_matches(filter, basename));
    let type_matches = type_filter.map_or(true, |kind| {
        (kind == b'f' && stat.kind == syscall::STAT_REGULAR)
            || (kind == b'd' && stat.kind == syscall::STAT_DIRECTORY)
    });
    if name_matches && type_matches {
        let line_length = path_length
            .checked_add(1)
            .ok_or_else(|| report(command, b"output budget exceeded", None))?;
        if !state.reserve_output(line_length) {
            return Err(report(command, b"output budget exceeded", None));
        }
        if !state.charge_work(line_length) {
            return Err(report(command, b"work budget exceeded", None));
        }
        write_all(1, &path[..path_length])
            .map_err(|error| report(command, b"write failed", Some(error)))?;
        write_all(1, b"\n").map_err(|error| report(command, b"write failed", Some(error)))?;
    }
    if stat.kind != syscall::STAT_DIRECTORY {
        return Ok(());
    }
    let mut entries = [syscall::DirEntry::default(); 16];
    if !state.charge_work(1) {
        return Err(report(command, b"work budget exceeded", None));
    }
    let count = syscall::read_dir(&path[..path_length], &mut entries)
        .map_err(|error| report(command, b"read directory failed", Some(error)))?;
    if count > entries.len() {
        return Err(report(command, b"malformed directory count", None));
    }
    for entry in entries.iter().take(count) {
        let name_length = usize::try_from(entry.name_length)
            .ok()
            .filter(|length| *length <= syscall::NAME_MAX)
            .ok_or_else(|| report(command, b"malformed directory entry", None))?;
        let name = &entry.name[..name_length];
        if name_length == 0
            || name.contains(&0)
            || name.contains(&b'/')
            || name == b"."
            || name == b".."
        {
            return Err(report(command, b"malformed directory entry", None));
        }
        if !state.charge_work(name_length.saturating_add(1)) {
            return Err(report(command, b"work budget exceeded", None));
        }
        if depth == FIND_MAX_DEPTH {
            return Err(report(command, b"directory depth exceeded", None));
        }
        let separator = usize::from(path[..path_length].last() != Some(&b'/'));
        let child_length = path_length
            .checked_add(separator)
            .and_then(|length| length.checked_add(name_length))
            .ok_or_else(|| report(command, b"path is too long", None))?;
        if child_length > syscall::MAX_PATH {
            return Err(report(command, b"path is too long", None));
        }
        if separator != 0 {
            path[path_length] = b'/';
        }
        let name_start = path_length + separator;
        path[name_start..child_length].copy_from_slice(name);
        let result = find_visit(
            command,
            path,
            child_length,
            name_filter,
            type_filter,
            state,
            depth + 1,
        );
        result?;
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
    let mut path = [0u8; syscall::MAX_PATH];
    let path_length = match command_path(command, raw, environment, &mut path) {
        Ok(path) => path.len(),
        Err(error) => return error,
    };
    let mut state = FindState::new();
    match find_visit(
        command,
        &mut path,
        path_length,
        name_filter,
        type_filter,
        &mut state,
        0,
    ) {
        Ok(()) => 0,
        Err(status) => status,
    }
}

fn read_sources_with(
    args: &[&[u8]],
    mut read_source: impl FnMut(&[u8], &mut Vec<u8>) -> Result<(), i32>,
    mut callback: impl FnMut(&[u8], &[u8]) -> Result<(), i32>,
) -> i32 {
    let mut input = Vec::new();
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
        if let Err(error) = read_source(raw, &mut input) {
            status = error;
            continue;
        }
        if callback(raw, &input).is_err() {
            status = 1;
        }
    }
    status
}

fn read_sources(
    command: &[u8],
    args: &[&[u8]],
    environment: &[&[u8]],
    callback: impl FnMut(&[u8], &[u8]) -> Result<(), i32>,
) -> i32 {
    let mut path = [0u8; syscall::MAX_PATH];
    read_sources_with(
        args,
        |raw, input| {
            if raw == b"-" {
                return read_all(0, input)
                    .map_err(|error| report(command, b"read failed", Some(error)));
            }
            let absolute = command_path(command, raw, environment, &mut path)?;
            let fd = open_read(absolute)
                .map_err(|error| report(command, b"open failed", Some(error)))?;
            let result =
                read_all(fd, input).map_err(|error| report(command, b"read failed", Some(error)));
            let _ = syscall::close(fd);
            result
        },
        callback,
    )
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
        for line in data.split(|byte| *byte == b'\n') {
            line_number += 1;
            let matched = line.windows(pattern.len()).any(|window| window == *pattern);
            if matched != inverted {
                found = true;
                if numbered {
                    write_u64(1, line_number as u64)
                        .and_then(|_| write_all(1, b":"))
                        .map_err(|error| report(command, b"write failed", Some(error)))?;
                }
                write_line(1, line)
                    .map_err(|error| report(command, b"write failed", Some(error)))?;
            }
        }
        let _ = name;
        Ok(())
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
        if take_head {
            for line in data.split_inclusive(|byte| *byte == b'\n').take(count) {
                write_all(1, line)
                    .map_err(|error| report(command, b"write failed", Some(error)))?;
            }
        } else {
            let all = data.split_inclusive(|byte| *byte == b'\n').count();
            for line in data
                .split_inclusive(|byte| *byte == b'\n')
                .skip(all.saturating_sub(count))
            {
                write_all(1, line)
                    .map_err(|error| report(command, b"write failed", Some(error)))?;
            }
        }
        Ok(())
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
        let line_count = data.split(|byte| *byte == b'\n').count();
        let mut lines = Vec::new();
        try_reserve_exact(&mut lines, line_count)
            .map_err(|error| report(command, b"out of memory", Some(error)))?;
        lines.extend(data.split(|byte| *byte == b'\n'));
        if lines.last() == Some(&&[][..]) {
            let _ = lines.pop();
        }
        lines.sort_unstable_by(|left, right| left.cmp(right));
        for line in lines {
            write_line(1, line).map_err(|error| report(command, b"write failed", Some(error)))?;
        }
        Ok(())
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
        if lines {
            write_u64(1, line_count(data) as u64)
                .and_then(|_| write_all(1, b" "))
                .map_err(|error| report(command, b"write failed", Some(error)))?;
        }
        if words {
            write_u64(1, word_count(data) as u64)
                .and_then(|_| write_all(1, b" "))
                .map_err(|error| report(command, b"write failed", Some(error)))?;
        }
        if bytes {
            write_u64(1, data.len() as u64)
                .and_then(|_| write_all(1, b" "))
                .map_err(|error| report(command, b"write failed", Some(error)))?;
        }
        if !name.is_empty() && name != b"-" {
            write_all(1, name).map_err(|error| report(command, b"write failed", Some(error)))?;
        }
        write_all(1, b"\n").map_err(|error| report(command, b"write failed", Some(error)))
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
    let _command_budget = CommandBudget::enter();
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
                let mut old_path = [0u8; syscall::MAX_PATH];
                let mut new_path = [0u8; syscall::MAX_PATH];
                let old = match command_path(command, args[0], environment, &mut old_path) {
                    Ok(value) => value,
                    Err(error) => return error,
                };
                let new = match command_path(command, args[1], environment, &mut new_path) {
                    Ok(value) => value,
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
                let mut path = [0u8; syscall::MAX_PATH];
                let mut status = 0;
                for raw in files {
                    let absolute = match command_path(command, raw, environment, &mut path) {
                        Ok(value) => value,
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
    use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

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
        let mut output = [0u8; syscall::MAX_PATH];
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
        let mut output = [0u8; syscall::MAX_PATH];
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
    fn find_budgets_and_directory_identity_are_bounded() {
        let mut state = FindState::new();
        assert!(state.visit_node());
        assert_eq!(state.visit_directory(7), Some(true));
        assert_eq!(state.visit_directory(7), Some(false));
        assert_eq!(state.visit_directory(8), Some(true));

        let mut nodes = FindState::new();
        for _ in 0..FIND_MAX_NODES {
            assert!(nodes.visit_node());
        }
        assert!(!nodes.visit_node());

        let mut output = FindState::new();
        assert!(output.reserve_output(FIND_MAX_OUTPUT_BYTES - 1));
        assert!(!output.reserve_output(2));

        let mut work = FindState::new();
        assert!(work.charge_work(FIND_MAX_WORK));
        assert!(!work.charge_work(1));
    }

    #[test]
    fn bytewise_sort_is_deterministic() {
        let mut values = [b"z".as_slice(), b"a".as_slice(), b"m".as_slice()];
        values.sort_unstable_by(|left, right| left.cmp(right));
        assert_eq!(values, [b"a".as_slice(), b"m".as_slice(), b"z".as_slice()]);
    }

    #[test]
    fn short_reads_are_accumulated_before_callback() {
        let sources = [b"fixture".as_slice()];
        let chunks = [b"ab".as_slice(), b"c".as_slice(), b"".as_slice()];
        let mut next_chunk = 0;
        let mut callback_data = Vec::new();
        let status = read_sources_with(
            &sources,
            |_, input| {
                read_all_with(input, |buffer| {
                    let chunk = chunks[next_chunk];
                    next_chunk += 1;
                    buffer[..chunk.len()].copy_from_slice(chunk);
                    Ok(chunk.len())
                })
                .map_err(|_| 1)
            },
            |_, data| {
                callback_data.extend_from_slice(data);
                Ok(())
            },
        );
        assert_eq!(status, 0);
        assert_eq!(callback_data, b"abc");
    }

    #[test]
    fn partial_read_error_skips_callback() {
        let sources = [b"fixture".as_slice()];
        let mut reads = 0;
        let mut callback_called = false;
        let status = read_sources_with(
            &sources,
            |_, input| {
                read_all_with(input, |buffer| {
                    if reads == 0 {
                        reads += 1;
                        buffer[..7].copy_from_slice(b"partial");
                        Ok(7)
                    } else {
                        Err(syscall::Error(0u64.wrapping_sub(5)))
                    }
                })
                .map_err(|_| 1)
            },
            |_, _| {
                callback_called = true;
                Ok(())
            },
        );
        assert_eq!(status, 1);
        assert!(!callback_called);
    }

    #[test]
    fn syscall_structs_match_the_v2_abi() {
        assert_eq!(core::mem::size_of::<syscall::Stat>(), 24);
        assert_eq!(core::mem::size_of::<syscall::DirEntry>(), 56);
        assert_eq!(syscall::ABI_VERSION, 2);
        assert_eq!(syscall::MAX_PATH, 256);
        assert_eq!(syscall::MAX_DIR_ENTRIES, 16);
        assert_eq!(syscall::Number::Fstat as syscall::Word, 430);
        assert_eq!(syscall::OPEN_EXCLUSIVE, 1 << 5);
    }

    #[test]
    fn cp_temporary_names_stay_in_the_destination_directory() {
        let mut output = [0u8; syscall::MAX_PATH];
        let length = temporary_path(b"/destination", 0, &mut output).unwrap();
        assert_eq!(&output[..length], b"/.norx-cp-0");
        let length = temporary_path(b"/tmp/destination", 15, &mut output).unwrap();
        assert_eq!(&output[..length], b"/tmp/.norx-cp-f");
        assert!(temporary_path(b"/destination", MAX_TEMP_ATTEMPTS, &mut output).is_none());
    }

    #[test]
    fn cp_temporary_name_rejects_path_overflow_before_building_it() {
        let mut destination = [0u8; syscall::MAX_PATH];
        destination[0] = b'/';
        destination[247] = b'/';
        destination[248..251].copy_from_slice(b"dst");
        let mut output = [0u8; syscall::MAX_PATH];
        assert!(temporary_path(&destination[..251], 0, &mut output).is_none());
    }

    #[test]
    fn dispatch_contract_contains_exactly_twenty_two_commands() {
        let commands: &[&[u8]] = &[
            b"cat", b"echo", b"env", b"ls", b"pwd", b"mkdir", b"rmdir", b"cp", b"mv", b"rm",
            b"touch", b"ln", b"stat", b"find", b"grep", b"head", b"tail", b"sort", b"wc", b"true",
            b"false", b"sleep",
        ];
        assert_eq!(commands.len(), 22);
        assert!(commands.iter().all(|command| is_supported_command(command)));
        assert!(!is_supported_command(b"sh"));
    }

    #[test]
    fn allocator_claims_disjoint_ranges_under_contention() {
        const THREADS: usize = 8;
        const ALLOCATIONS: usize = 64;
        const ALLOCATION_SIZE: usize = 16;
        const SLOTS: usize = THREADS * ALLOCATIONS;
        static TEST_HEAP: Heap = Heap::new();

        let start = AtomicBool::new(false);
        let ready = AtomicUsize::new(0);
        let pointers = core::array::from_fn::<_, SLOTS, _>(|_| AtomicUsize::new(0));
        let start_ref = &start;
        let ready_ref = &ready;
        let pointers_ref = &pointers;

        std::thread::scope(|scope| {
            for thread_id in 0..THREADS {
                scope.spawn(move || {
                    ready_ref.fetch_add(1, Ordering::Relaxed);
                    while !start_ref.load(Ordering::Acquire) {
                        core::hint::spin_loop();
                    }
                    let layout = Layout::from_size_align(ALLOCATION_SIZE, 8).unwrap();
                    for allocation in 0..ALLOCATIONS {
                        let pointer = unsafe { TEST_HEAP.alloc(layout) };
                        if !pointer.is_null() {
                            pointers_ref[thread_id * ALLOCATIONS + allocation]
                                .store(pointer as usize, Ordering::Relaxed);
                        }
                    }
                });
            }
            while ready.load(Ordering::Acquire) != THREADS {
                core::hint::spin_loop();
            }
            start.store(true, Ordering::Release);
        });

        let mut addresses = [0usize; SLOTS];
        let mut count = 0;
        for pointer in pointers.iter() {
            let pointer = pointer.load(Ordering::Relaxed);
            if pointer != 0 {
                addresses[count] = pointer;
                count += 1;
            }
        }
        assert!(count >= THREADS);
        addresses[..count].sort_unstable();
        assert!(addresses[..count].windows(2).all(|pair| pair[0] != pair[1]));
        let cursor = TEST_HEAP.cursor.load(Ordering::Acquire);
        assert!(cursor >= count * ALLOCATION_SIZE);
        assert!(cursor <= HEAP_BYTES);
    }

    #[test]
    fn allocator_rejects_exhaustion_and_oversized_layouts() {
        static TEST_HEAP: Heap = Heap::new();
        let full = Layout::from_size_align(HEAP_BYTES, 1).unwrap();
        assert!(!unsafe { TEST_HEAP.alloc(full) }.is_null());
        assert!(unsafe { TEST_HEAP.alloc(Layout::from_size_align(1, 1).unwrap()) }.is_null());
        assert!(unsafe {
            TEST_HEAP.alloc(Layout::from_size_align(isize::MAX as usize, 1).unwrap())
        }
        .is_null());
    }
}
