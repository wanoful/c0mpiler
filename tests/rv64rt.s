.section .text

.globl _start
_start:
    call main
    li a7, 93
    ecall

.globl sys_write
sys_write:
    li a7, 64
    ecall
    ret

.globl sys_read
sys_read:
    li a7, 63
    ecall
    ret

.globl sys_brk
sys_brk:
    li a7, 214
    ecall
    ret

.globl sys_exit
sys_exit:
    li a7, 93
    ecall
    ret
