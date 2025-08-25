use libc::{MAP_ANON, MAP_PRIVATE, PROT_EXEC, PROT_READ, PROT_WRITE, mmap, mprotect};
use std::ptr;

unsafe extern "C" {
    fn __clear_cache(start: *const u8, end: *const u8);
    // fn sys_icache_invalidate(start: *const libc::c_void, size: libc::size_t);
}

fn emit_mov_x0_immediate(value: u16) -> u32 {
    let base_mov_instruction: u32 = 0xd2800000;
    base_mov_instruction | ((value as u32) << 5)
}

fn main() {
    let size = 4096;

    let code_ptr = unsafe {
        mmap(
            ptr::null_mut(),
            size,
            PROT_READ | PROT_WRITE,
            MAP_PRIVATE | MAP_ANON,
            -1,
            0,
        )
    };

    let mut instructions: Vec<u32> = Vec::new();
    instructions.push(emit_mov_x0_immediate(42));
    instructions.push(0xD65F03C0);

    unsafe {
        std::ptr::copy_nonoverlapping(
            instructions.as_ptr() as *const u8,
            code_ptr as *mut u8,
            instructions.len() * 4,
        );
    }

    unsafe {
        let as_u8: *const u8 = std::mem::transmute(code_ptr);
        __clear_cache(as_u8, as_u8.add(size));
        // sys_icache_invalidate(memory, size);
        mprotect(code_ptr, size, PROT_READ | PROT_EXEC);
    }

    type JitFn = unsafe extern "C" fn() -> i64;
    let jit_fn: JitFn = unsafe { std::mem::transmute(code_ptr) };

    let result = unsafe { jit_fn() };

    println!("Answer: {}", result);
}
