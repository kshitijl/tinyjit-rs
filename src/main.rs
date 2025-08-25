use libc::{MAP_ANON, MAP_PRIVATE, PROT_EXEC, PROT_READ, PROT_WRITE, mmap, mprotect};
use std::ptr;

unsafe extern "C" {
    fn __clear_cache(start: *const u8, end: *const u8);
}
#[derive(Copy, Clone, Debug)]
enum Op {
    Add,
    Mul,
    Sub,
}

#[derive(Clone, Debug)]
enum Expr {
    Number(u16),
    Op(Op),
    Loop(Vec<Expr>),
    Dup,
    Swap,
    Over,
}

mod arm64 {
    use crate::{Expr, Op};

    pub fn ret() -> u32 {
        0xd65f03c0
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

    fn sub_reg_reg_reg(rd: u8, rn: u8, rm: u8) -> u32 {
        let base = 0xcb000000;
        base | ((rm as u32) << 16) | ((rn as u32) << 5) | (rd as u32)
    }

    fn push_literal(value: u16) -> Vec<u32> {
        let mut instructions = Vec::new();

        // Put the literal in X0: mov x0, #lit
        instructions.push(mov_immediate(0, value));

        instructions.extend(push_x0());

        instructions
    }

    pub fn pop_into_reg(register: u8) -> Vec<u32> {
        let ldr = {
            let base: u32 = 0xf9400120;
            base | (register as u32)
        };

        vec![
            ldr,        // ldr x{register}, [x9]
            0x91002129, // add x29, x29, #8
        ]
    }

    fn push_reg(register: u8) -> Vec<u32> {
        vec![
            0xd1002129,                     // sub x9, x9, 8
            0xf9000120 | (register as u32), // str x{register}, [x9]
        ]
    }

    fn cbnz(by: i32) -> u32 {
        let base = 0xb500000a;
        let mask = 0x00ffffff;

        base | (mask & ((by as u32) << 3))
    }

    fn push_x0() -> Vec<u32> {
        push_reg(0)
    }

    fn do_top_two_and_push(f: fn(u8, u8, u8) -> u32) -> Vec<u32> {
        vec![
            pop_into_reg(2),
            pop_into_reg(1),
            vec![f(0, 1, 2)],
            push_x0(),
        ]
        .into_iter()
        .flatten()
        .collect()
    }

    pub fn codegen(expression: &Vec<Expr>, output: &mut Vec<u32>) {
        for item in expression {
            match item {
                Expr::Number(x) => output.extend(push_literal(*x)),
                Expr::Op(Op::Add) => output.extend(do_top_two_and_push(add_reg_reg_reg)),
                Expr::Op(Op::Mul) => output.extend(do_top_two_and_push(mul_reg_reg_reg)),
                Expr::Op(Op::Sub) => output.extend(do_top_two_and_push(sub_reg_reg_reg)),
                Expr::Dup => {
                    output.extend(pop_into_reg(0));
                    output.extend(push_x0());
                    output.extend(push_x0());
                }

                Expr::Swap => {
                    output.extend(pop_into_reg(0));
                    output.extend(pop_into_reg(1));
                    output.extend(push_reg(0));
                    output.extend(push_reg(1));
                }

                Expr::Over => {
                    output.extend(pop_into_reg(0));
                    output.extend(pop_into_reg(1));
                    output.extend(push_reg(1));
                    output.extend(push_reg(0));
                    output.extend(push_reg(1));
                }
                Expr::Loop(body) => {
                    output.extend(pop_into_reg(10));

                    let loop_start_offset = output.len() as i32;
                    output.push(mov_immediate(11, 1));
                    output.push(sub_reg_reg_reg(10, 10, 11));

                    codegen(&body, output);

                    let loop_end_offset = output.len() as i32;
                    let jump = cbnz(4 * (loop_start_offset - loop_end_offset));
                    output.push(jump);
                }
            }
        }
    }
}

mod parsing {
    use nom::{
        IResult, Parser,
        branch::alt,
        bytes::complete::tag,
        character::complete::{char, digit1, multispace0, multispace1},
        combinator::{map, map_res, value},
        multi::{many_till, separated_list1},
        sequence::preceded,
    };

    use crate::{Expr, Op};

    pub fn parse_expression(input: &str) -> IResult<&str, Vec<Expr>> {
        separated_list1(multispace1, parse_token).parse(input)
    }

    fn parse_token(input: &str) -> IResult<&str, Expr> {
        alt((
            map(parseu16, Expr::Number),
            map(parse_op, Expr::Op),
            parse_loop,
            value(Expr::Dup, tag("dup")),
            value(Expr::Swap, tag("swap")),
            value(Expr::Over, tag("over")),
        ))
        .parse(input)
    }

    fn parse_loop(input: &str) -> IResult<&str, Expr> {
        let (input, _) = tag("times")(input)?;
        let (input, _) = multispace1(input)?;

        let (input, body) = many_till(
            preceded(multispace0, parse_token),
            preceded(multispace0, tag("end")),
        )
        .parse(input)?;

        Ok((input, Expr::Loop(body.0)))
    }

    fn parse_op(input: &str) -> IResult<&str, Op> {
        alt((
            value(Op::Add, char('+')),
            value(Op::Sub, char('-')),
            value(Op::Mul, char('*')),
        ))
        .parse(input)
    }

    fn parseu16(input: &str) -> IResult<&str, u16> {
        map_res(digit1, |s: &str| s.parse::<u16>()).parse(input)
    }
}

fn interpret(expression: &Vec<Expr>, stack: &mut Vec<i32>) {
    for item in expression {
        match item {
            Expr::Number(x) => {
                stack.push(*x as i32);
            }
            Expr::Op(Op::Add) => {
                let a = stack.pop().unwrap();
                let b = stack.pop().unwrap();
                stack.push(a + b);
            }
            Expr::Op(Op::Sub) => {
                let a = stack.pop().unwrap();
                let b = stack.pop().unwrap();
                stack.push(b - a);
            }
            Expr::Op(Op::Mul) => {
                let a = stack.pop().unwrap();
                let b = stack.pop().unwrap();
                stack.push(a * b);
            }

            Expr::Dup => {
                let x = stack.pop().unwrap();
                stack.push(x);
                stack.push(x);
            }

            Expr::Swap => {
                let a = stack.pop().unwrap();
                let b = stack.pop().unwrap();
                stack.push(a);
                stack.push(b);
            }

            Expr::Over => {
                let a = stack.pop().unwrap();
                let b = stack.pop().unwrap();
                stack.push(b);
                stack.push(a);
                stack.push(b);
            }

            Expr::Loop(body) => {
                let times = stack.pop().unwrap();
                for _idx in 0..times {
                    interpret(body, stack);
                }
            }
        }
    }
}

fn main() {
    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .expect("failed to read line");

    let (remainder, expression) = parsing::parse_expression(input.trim()).unwrap();

    println!(
        "Thank you for giving me a program to run. I understand it as:\n{:?}",
        expression
    );
    if remainder.trim().len() > 0 {
        println!(
            "There was some stuff at the end I couldn't parse: {}",
            remainder
        );

        return;
    }

    println!("\nFirst let's interpret your program. Whirr brr..\n");
    let mut stack = Vec::new();
    interpret(&expression, &mut stack);
    let interpreted_result = stack.pop().unwrap();
    println!("All done! The answer is {}", interpreted_result);

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

    arm64::codegen(&expression, &mut instructions);

    // The final result will be at the top of the stack. Pop that result into x0
    // and return it.
    instructions.extend(arm64::pop_into_reg(0));
    instructions.push(arm64::ret());

    println!("\nHere's the raw arm64 machine code for your code:");

    for instruction in instructions.iter() {
        print!("{:#x} ", instruction);
    }

    println!("\n\nNOW LET'S RUN IT FOR REAL! WAHOO!!\n");

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

    if interpreted_result as i64 == result {
        println!("Things are going well. JIT result == interpreted result.");
    } else {
        println!("Bad stuff. Interpreter disagrees with JIT.");
    }
}
