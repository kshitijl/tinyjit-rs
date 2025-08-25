use libc::{MAP_ANON, MAP_PRIVATE, PROT_EXEC, PROT_READ, PROT_WRITE, mmap, mprotect};
use nom::{
    IResult, Parser,
    branch::alt,
    character::complete::{char, digit1, multispace1},
    combinator::{map, map_res, value},
    multi::separated_list1,
};
use std::ptr;

unsafe extern "C" {
    fn __clear_cache(start: *const u8, end: *const u8);
}

fn mov_immediate(register: u8, value: u16) -> u32 {
    assert!(register < 32);
    let base: u32 = 0xd2800000;
    base | ((value as u32) << 5) | (register as u32)
}

fn add_reg_reg_reg(rd: u8, rn: u8, rm: u8) -> u32 {
    let base = 0x8b000000;
    base | ((rm as u32) << 16) | ((rn as u32) << 5) | (rd as u32)
}

fn mul_reg_reg_reg(rd: u8, rn: u8, rm: u8) -> u32 {
    let base = 0x9b007c00;
    base | ((rm as u32) << 16) | ((rn as u32) << 5) | (rd as u32)
}

fn ret() -> u32 {
    0xd65f03c0
}

fn push_literal(value: u16) -> Vec<u32> {
    let mut instructions = Vec::new();

    // Put the literal in X0: mov x0, #lit
    instructions.push(mov_immediate(0, value));

    instructions.extend(push_x0());

    instructions
}

fn pop_into_reg(register: u8) -> Vec<u32> {
    let ldr = {
        let base: u32 = 0xf9400120;
        base | (register as u32)
    };

    vec![
        ldr,        // ldr x{register}, [x9]
        0x91002129, // add x29, x29, #8
    ]
}

fn push_x0() -> Vec<u32> {
    vec![
        0xd1002129, // sub x9, x9, 8
        0xf9000120, // str x0, [x9]
    ]
}

fn add_top_two_and_push() -> Vec<u32> {
    vec![
        pop_into_reg(1),
        pop_into_reg(2),
        vec![add_reg_reg_reg(0, 1, 2)],
        push_x0(),
    ]
    .into_iter()
    .flatten()
    .collect()
}

fn mul_top_two_and_push() -> Vec<u32> {
    vec![
        pop_into_reg(1),
        pop_into_reg(2),
        vec![mul_reg_reg_reg(0, 1, 2)],
        push_x0(),
    ]
    .into_iter()
    .flatten()
    .collect()
}

#[derive(Copy, Clone, Debug)]
enum Op {
    Add,
    Mul,
}

#[derive(Copy, Clone, Debug)]
enum Token {
    Number(u16),
    Op(Op),
}

fn parse_expression(input: &str) -> IResult<&str, Vec<Token>> {
    separated_list1(multispace1, parse_token).parse(input)
}

fn parse_token(input: &str) -> IResult<&str, Token> {
    alt((map(parseu16, Token::Number), map(parse_op, Token::Op))).parse(input)
}

fn parse_op(input: &str) -> IResult<&str, Op> {
    alt((value(Op::Add, char('+')), value(Op::Mul, char('*')))).parse(input)
}
fn parseu16(input: &str) -> IResult<&str, u16> {
    map_res(digit1, |s: &str| s.parse::<u16>()).parse(input)
}

fn main() {
    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .expect("failed to read line");

    let (_remainder, expression) = parse_expression(input.trim()).unwrap();

    println!("{:?}", expression);
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

    let stack_size = 4096;
    let stack_ptr = unsafe {
        mmap(
            ptr::null_mut(),
            stack_size,
            PROT_READ | PROT_WRITE,
            MAP_PRIVATE | MAP_ANON,
            -1,
            0,
        )
    };

    let stack_top = unsafe { (stack_ptr as *mut u8).add(stack_size) };

    let mut instructions: Vec<u32> = Vec::new();
    // The JIT function is called with the stack we allocated for its use. The
    // very first instruction we execute takes that argument from x0 and puts
    // it in x9. The rest of the instructions will use register x9 as the top of
    // their stack. I did this because manipulating sp would segfault. Literally
    // even subtracting from it: "sub sp, sp, #8" would segfault. So we allocate
    // our own memory for the stack and pass it in.
    let mov_x9_x0 = 0xaa0003e9;
    instructions.push(mov_x9_x0);

    for item in expression {
        match item {
            Token::Number(x) => instructions.extend(push_literal(x)),
            Token::Op(Op::Add) => instructions.extend(add_top_two_and_push()),
            Token::Op(Op::Mul) => instructions.extend(mul_top_two_and_push()),
        }
    }

    // The final result will be at the top of the stack. Pop that result into x0
    // and return it.
    instructions.extend(pop_into_reg(0));
    instructions.push(ret());

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

    type JitFn = unsafe extern "C" fn(*mut u8) -> i64;

    let jit_fn: JitFn = unsafe { std::mem::transmute(code_ptr) };

    let result = unsafe { jit_fn(stack_top as *mut u8) };

    println!("Answer: {}", result);
}
