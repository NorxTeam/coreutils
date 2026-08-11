#![feature(alloc_error_handler)]
#![no_std]
#![no_main]

extern crate alloc;

use core::alloc::Layout;
use core::arch::global_asm;
use core::panic::PanicInfo;

use norx_coreutils::{run, startup_from_stack, syscall};

#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    syscall::exit(101)
}

#[alloc_error_handler]
fn allocation_error(_layout: Layout) -> ! {
    syscall::exit(12)
}

#[cfg(target_arch = "x86_64")]
global_asm!(
    r#"
        .global _start
        .type _start,@function
    _start:
        mov rdi, rsp
        call norx_start
        ud2
    "#
);

#[cfg(target_arch = "aarch64")]
global_asm!(
    r#"
        .global _start
        .type _start,%function
    _start:
        mov x0, sp
        bl norx_start
        brk #0
    "#
);

#[unsafe(no_mangle)]
pub extern "C" fn norx_start(stack: *const usize) -> ! {
    let startup = unsafe { startup_from_stack(stack) };
    let args = unsafe { norx_coreutils::collect_args(startup) };
    let environment = unsafe { norx_coreutils::collect_environment(startup) };
    let mut command = args.first().copied().unwrap_or(b"coreutils");
    if let Some(name) = command.rsplit(|byte| *byte == b'/').next() {
        command = name;
    }
    let (command, arguments) = if command == b"coreutils" {
        match args.get(1..) {
            Some([name, rest @ ..]) => (*name, rest),
            _ => (command, &[][..]),
        }
    } else {
        (command, args.get(1..).unwrap_or(&[]))
    };
    let status = run(command, arguments, &environment);
    syscall::exit(status)
}
