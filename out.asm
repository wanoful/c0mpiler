    .text

    .globl main
    .p2align 2
main:
.Lmain_entry:
    addi sp, sp, -48
    sd s0, 16(sp)
    sd s2, 24(sp)
    sd s5, 32(sp)
    addi a4, sp, 0
    addi a5, a4, 0
    li t3, 10
    sw t3, 0(a5)
    addi s0, a4, 4
    li s2, 20
    sw s2, 0(s0)
    addi s5, a4, 8
    li a4, 30
    sw a4, 0(s5)
    mv a0, zero
    ld s5, 32(sp)
    ld s2, 24(sp)
    ld s0, 16(sp)
    addi sp, sp, 48
    ret

    .p2align 2
str.len:
.Lstr.len_entry:
    mv a0, a1
    ret

