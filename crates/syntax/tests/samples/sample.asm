.intel_syntax noprefix  # A greeting on Linux x86-64, in GAS syntax
.section .rodata
message:
.string "Hello, world!"

.section .text
.globl _start
_start:
    mov rax, 1          # write
    mov rdi, 1          // to standard output
    lea rsi, [rip + message]
    mov rdx, 13
    syscall
    mov rax, 60         ; exit
    xor rdi, rdi
    syscall             /* done */
