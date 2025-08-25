## what is this

A tiny JIT that allocates a page, writes some code there and executes it. The language it JITs is a very simple stack-based thing. To help debug things, it also first interprets the program before JIT-compiling it and executing the machine code.

It's written in Rust because I like programming in Rust.

## platfrom support

This only works on ARM64 machines. The machine code instructions are hardcoded in there. This isn't meant to be some platform-independendent thing, it's just a toy to help learn. I made it on my M1 MacBook Pro. It should work on other ARM macs. Maybe ARM Linux too?

## why did you make it

I made it during the "hour of hard things" at Recurse Center. Which turned into an entire day of hard thing. Now, seven and a half hours later, we can finally calculate the factorial of 5.

## what's all the machine code stuff I mean come on

I think JITs are cool magical technology. V8! The JVM! But here's a really tiny JIT that I made in an afternoon, and so can you. The code is ... uhh, if not exactly *clean*, well, *simple*. There are no layers of abstraction at all. It uses hardly any libraries.

## how does it work please please please say so

First it parses the input using the `nom` library. That's not the point of this project so I won't say more about that here. The language is very simple so parsing it is just a few lines of code.

Then it *interprets* the parsed program, just to help me debug the JIT compilation. I didn't have an interpreter until the final 45 minutes of the day, and that was a big mistake.

Then it calls `mmap` to get a chunk of writable memory from the OS. Then it iterates over the parsed program and emits machine code. Yep, the source code is littered with hardcoded hex constants and bit twiddling. Then it copies the machine code into this memory. Then it calls `mprotect` to turn the memory read-only and executable. This is because modern operating systems don't want you to have writable executable memory, because that makes life easier for malware because they can then write malicious machine code to buffers in memory and then execute that code. Having to call `mprotect` puts another barrier in the way.

Then it executes that code!

You might notice another call to `mmap`. That's to allocate the stack for the computation this language does. This language uses a stack instead of registers. On x86 there are instructions for manipulating the process stack. On my mac, manipulating the stack pointer for the process `sp` led to segfaults and bus errors. So instead I just allocate a chunk of memory and pass it in for the generated code to use as a stack.

How does this argument go in? How does the result get out? This is a question about calling conventions. In this case I marked both as `extern "C"`, so we use the C calling convention on arm64, which means first arg goes in register `x0`, and the return value is also on `x0`. So from the Rust code, I just pass in the pointer to the allocated scratch stack memory, and get a return value in the usual way as when one calls a function. And inside the machine code, I get the pointer to the scratch stack memory in `x0` at the beginning and immediately put it in `x9`, which is subsequently used as the stack pointer; and at the end, I put the final value, the top of the stack, in `x0` for the Rust program to get as the return value.

I used Godbolt a LOT to make this. Thank you Godbolt.

That's it!

## how to run it

Clone this repo, then 

`cargo run`

Then type one of the programs below and hit enter.

## basic example programs

Here's it adding two numbers:

`1 1 +`

That means "push 1 to the top of the stack", "push 1 to the top of the stack", and finally "pop the top two items, add them, and push the result back onto the stack."

You can add three numbers:

`1 2 3 + +`

which evaluates to 6.

A program in this language evaluates to whatever's at the top of the stack at the end.

The language has simple loops:

`1 5 times 1 + end`

Let's unpack that! `1` pushes the number 1 onto the stack. So the stack is now `[1]` (left is bottom right is top). Then we push 5 to the stack, so the stack is `[1 5]`. Then we have the `times ... end` construct, which pops the top of the stack and then executes its body that many times. So the 5 we just pushed onto the stack gets popped and when we enter the body of the loop for the first time, the stack is `[1]`. 

Now! Each time inside the loop, we push 1 to the stack, then pop the two items in the stack, then push the added result back onto the stack. So the final result of all that is ...

```
# 06-tinyjit-rs git:(main) cargo run
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.04s
     Running `target/debug/tinyjit`
1 5 times 1 + end
Thank you for giving me a program to run. I understand it as:
[Number(1), Number(5), Loop([Number(1), Op(Add)])]

First let's interpret your program. Whirr brr..

All done! The answer is 6

Here's the raw arm64 machine code for your code:
0xaa0003e9 0xd2800020 0xd1002129 0xf9000120 0xd28000a0 0xd1002129 0xf9000120 0xf940012a 0x91002129 0xd280002b 0xcb0b014a 0xd2800020 0xd1002129 0xf9000120 0xf9400122 0x91002129 0xf9400121 0x91002129 0x8b020020 0xd1002129 0xf9000120 0xb5fffe8a 0xf9400120 0x91002129 0xd65f03c0

NOW LET'S RUN IT FOR REAL! WAHOO!!

Answer: 6
Things are going well. JIT result == interpreted result.
```
That is, we start with 1, and add 1 to it 5 times, to get 6.

## more operations

`swap` exchanges the top two items of the stack.
`dup` duplicates the top of the stack.
`over` makes a copy of the *second* item on the stack and pushes it to the top of the stack.

## please show me the factorial program I was promised

Here it is!

```
1 1 5 times swap over * swap 1 + end swap
```

This computes `5!`:

```
06-tinyjit-rs git:(main) cargo run
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.04s
     Running `target/debug/tinyjit`
1 1 5 times swap over * swap 1 + end swap
Thank you for giving me a program to run. I understand it as:
[Number(1), Number(1), Number(5), Loop([Swap, Over, Op(Mul), Swap, Number(1), Op(Add)]), Swap]

First let's interpret your program. Whirr brr..

All done! The answer is 120

Here's the raw arm64 machine code for your code:
0xaa0003e9 0xd2800020 0xd1002129 0xf9000120 0xd2800020 0xd1002129 0xf9000120 0xd28000a0 0xd1002129 0xf9000120 0xf940012a 0x91002129 0xd280002b 0xcb0b014a 0xf9400120 0x91002129 0xf9400121 0x91002129 0xd1002129 0xf9000120 0xd1002129 0xf9000121 0xf9400120 0x91002129 0xf9400121 0x91002129 0xd1002129 0xf9000121 0xd1002129 0xf9000120 0xd1002129 0xf9000121 0xf9400122 0x91002129 0xf9400121 0x91002129 0x9b027c20 0xd1002129 0xf9000120 0xf9400120 0x91002129 0xf9400121 0x91002129 0xd1002129 0xf9000120 0xd1002129 0xf9000121 0xd2800020 0xd1002129 0xf9000120 0xf9400122 0x91002129 0xf9400121 0x91002129 0x8b020020 0xd1002129 0xf9000120 0xb5fffa6a 0xf9400120 0x91002129 0xf9400121 0x91002129 0xd1002129 0xf9000120 0xd1002129 0xf9000121 0xf9400120 0x91002129 0xd65f03c0

NOW LET'S RUN IT FOR REAL! WAHOO!!

Answer: 120
Things are going well. JIT result == interpreted result.
```

## come on please explain how that factorial program works

The idea is that we maintain the current loop counter and the current value of the factorial on the top of the stack, and every loop iteration updates them.

I'll use the term `acc` to refer to the accumulator, i.e., the current value of the factorial, starting with 1, then 2, then 6, etc. And `ctr` will be the loop counter.

So going into the loop the stack is `[acc ctr]` (reminder: bottom of the stack is on the left, top is on the right).


|Instruction   | Stack|
|--------------|--------------|
|`swap`        | `ctr, acc`|
|`over`        | `ctr, acc, ctr`|
|`mul`         | `ctr, acc*ctr`|
|`swap`        | `acc*ctr, ctr`|
|`1`           | `acc*ctr, ctr, 1`|
|`+`           | `acc*ctr, ctr+1`|

So, excitingly, by the end of the loop, the top two items on the stack are the new updated `acc` and `ctr`.

Finally, after the loop is done, we `swap` to put the final accumulator on the top of the stack, since that's what we're interested in.
