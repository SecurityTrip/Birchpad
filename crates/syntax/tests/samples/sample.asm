; A greeting, on Linux x86-64.
section .data
    message db "Hello, world!", 10
    length  equ $ - message

section .text
global _start
_start:
    mov rax, 1
    mov rdi, 1
    mov rsi, message
    mov rdx, length
    syscall
    mov rax, 60
    xor rdi, rdi
    syscall
