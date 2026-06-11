	.attribute	4, 16
	.attribute	5, "rv32i2p1_m2p0_a2p1_c2p0_zmmul1p0_zaamo1p0_zalrsc1p0_zca1p0"
	.file	"builtin.c"
	.text
	.globl	to_string                       # -- Begin function to_string
	.p2align	1
	.type	to_string,@function
to_string:                              # @to_string
# %bb.0:
	addi	sp, sp, -16
	sw	ra, 12(sp)                      # 4-byte Folded Spill
	sw	s0, 8(sp)                       # 4-byte Folded Spill
	sw	s1, 4(sp)                       # 4-byte Folded Spill
	mv	s0, a1
	mv	s1, a0
	li	a0, 16
	call	malloc
	lw	a2, 0(s0)
	mv	s0, a0
	lui	a1, %hi(.L.str)
	addi	a1, a1, %lo(.L.str)
	call	sprintf
	mv	a0, s0
	call	strlen
	sw	s0, 0(s1)
	sw	a0, 4(s1)
	lw	ra, 12(sp)                      # 4-byte Folded Reload
	lw	s0, 8(sp)                       # 4-byte Folded Reload
	lw	s1, 4(sp)                       # 4-byte Folded Reload
	addi	sp, sp, 16
	ret
.Lfunc_end0:
	.size	to_string, .Lfunc_end0-to_string
                                        # -- End function
	.globl	string_plus                     # -- Begin function string_plus
	.p2align	1
	.type	string_plus,@function
string_plus:                            # @string_plus
# %bb.0:
	addi	sp, sp, -32
	sw	ra, 28(sp)                      # 4-byte Folded Spill
	sw	s0, 24(sp)                      # 4-byte Folded Spill
	sw	s1, 20(sp)                      # 4-byte Folded Spill
	sw	s2, 16(sp)                      # 4-byte Folded Spill
	sw	s3, 12(sp)                      # 4-byte Folded Spill
	sw	s4, 8(sp)                       # 4-byte Folded Spill
	sw	s5, 4(sp)                       # 4-byte Folded Spill
	sw	s6, 0(sp)                       # 4-byte Folded Spill
	mv	s5, a3
	mv	s3, a2
	mv	s1, a1
	mv	s2, a0
	lw	s0, 4(a1)
	add	s4, s0, a3
	mv	a0, s4
	call	malloc
	mv	s6, a0
	add	a0, a0, s0
	beqz	s0, .LBB1_3
# %bb.1:
	lw	a1, 0(s1)
	mv	a2, s6
.LBB1_2:                                # =>This Inner Loop Header: Depth=1
	lbu	a3, 0(a1)
	sb	a3, 0(a2)
	addi	a2, a2, 1
	addi	a1, a1, 1
	bne	a2, a0, .LBB1_2
.LBB1_3:
	beqz	s5, .LBB1_5
# %bb.4:
	mv	a1, s3
	mv	a2, s5
	call	memcpy
.LBB1_5:
	sw	s6, 0(s2)
	sw	s4, 4(s2)
	lw	ra, 28(sp)                      # 4-byte Folded Reload
	lw	s0, 24(sp)                      # 4-byte Folded Reload
	lw	s1, 20(sp)                      # 4-byte Folded Reload
	lw	s2, 16(sp)                      # 4-byte Folded Reload
	lw	s3, 12(sp)                      # 4-byte Folded Reload
	lw	s4, 8(sp)                       # 4-byte Folded Reload
	lw	s5, 4(sp)                       # 4-byte Folded Reload
	lw	s6, 0(sp)                       # 4-byte Folded Reload
	addi	sp, sp, 32
	ret
.Lfunc_end1:
	.size	string_plus, .Lfunc_end1-string_plus
                                        # -- End function
	.globl	print                           # -- Begin function print
	.p2align	1
	.type	print,@function
print:                                  # @print
# %bb.0:
	beqz	a1, .LBB2_4
# %bb.1:
	addi	sp, sp, -16
	sw	ra, 12(sp)                      # 4-byte Folded Spill
	sw	s0, 8(sp)                       # 4-byte Folded Spill
	sw	s1, 4(sp)                       # 4-byte Folded Spill
	mv	s0, a0
	add	s1, a0, a1
.LBB2_2:                                # =>This Inner Loop Header: Depth=1
	lbu	a0, 0(s0)
	call	putchar
	addi	s0, s0, 1
	bne	s0, s1, .LBB2_2
# %bb.3:
	lw	ra, 12(sp)                      # 4-byte Folded Reload
	lw	s0, 8(sp)                       # 4-byte Folded Reload
	lw	s1, 4(sp)                       # 4-byte Folded Reload
	addi	sp, sp, 16
.LBB2_4:
	ret
.Lfunc_end2:
	.size	print, .Lfunc_end2-print
                                        # -- End function
	.globl	println                         # -- Begin function println
	.p2align	1
	.type	println,@function
println:                                # @println
# %bb.0:
	beqz	a1, .LBB3_4
# %bb.1:
	addi	sp, sp, -16
	sw	ra, 12(sp)                      # 4-byte Folded Spill
	sw	s0, 8(sp)                       # 4-byte Folded Spill
	sw	s1, 4(sp)                       # 4-byte Folded Spill
	mv	s0, a0
	add	s1, a0, a1
.LBB3_2:                                # =>This Inner Loop Header: Depth=1
	lbu	a0, 0(s0)
	call	putchar
	addi	s0, s0, 1
	bne	s0, s1, .LBB3_2
# %bb.3:
	lw	ra, 12(sp)                      # 4-byte Folded Reload
	lw	s0, 8(sp)                       # 4-byte Folded Reload
	lw	s1, 4(sp)                       # 4-byte Folded Reload
	addi	sp, sp, 16
.LBB3_4:
	li	a0, 10
	tail	putchar
.Lfunc_end3:
	.size	println, .Lfunc_end3-println
                                        # -- End function
	.globl	printInt                        # -- Begin function printInt
	.p2align	1
	.type	printInt,@function
printInt:                               # @printInt
# %bb.0:
	lui	a1, %hi(.L.str.1)
	addi	a1, a1, %lo(.L.str.1)
	mv	a2, a0
	mv	a0, a1
	mv	a1, a2
	tail	printf
.Lfunc_end4:
	.size	printInt, .Lfunc_end4-printInt
                                        # -- End function
	.globl	printlnInt                      # -- Begin function printlnInt
	.p2align	1
	.type	printlnInt,@function
printlnInt:                             # @printlnInt
# %bb.0:
	lui	a1, %hi(.L.str.2)
	addi	a1, a1, %lo(.L.str.2)
	mv	a2, a0
	mv	a0, a1
	mv	a1, a2
	tail	printf
.Lfunc_end5:
	.size	printlnInt, .Lfunc_end5-printlnInt
                                        # -- End function
	.globl	getString                       # -- Begin function getString
	.p2align	1
	.type	getString,@function
getString:                              # @getString
# %bb.0:
	addi	sp, sp, -32
	sw	ra, 28(sp)                      # 4-byte Folded Spill
	sw	s0, 24(sp)                      # 4-byte Folded Spill
	sw	s1, 20(sp)                      # 4-byte Folded Spill
	sw	s2, 16(sp)                      # 4-byte Folded Spill
	sw	s3, 12(sp)                      # 4-byte Folded Spill
	sw	s4, 8(sp)                       # 4-byte Folded Spill
	sw	s5, 4(sp)                       # 4-byte Folded Spill
	sw	s6, 0(sp)                       # 4-byte Folded Spill
	mv	s2, a0
	li	a0, 16
	li	s4, 16
	call	malloc
	mv	s3, a0
	li	s1, 0
	li	s5, -1
	li	s6, 10
	j	.LBB6_2
.LBB6_1:                                #   in Loop: Header=BB6_2 Depth=1
	add	a0, s3, s1
	addi	s1, s1, 1
	sb	s0, 0(a0)
.LBB6_2:                                # =>This Inner Loop Header: Depth=1
	call	getchar
	beq	a0, s5, .LBB6_6
# %bb.3:                                #   in Loop: Header=BB6_2 Depth=1
	mv	s0, a0
	beq	a0, s6, .LBB6_6
# %bb.4:                                #   in Loop: Header=BB6_2 Depth=1
	bltu	s1, s4, .LBB6_1
# %bb.5:                                #   in Loop: Header=BB6_2 Depth=1
	slli	s4, s4, 1
	mv	a0, s3
	mv	a1, s4
	call	realloc
	j	.LBB6_1
.LBB6_6:
	sw	s3, 0(s2)
	sw	s1, 4(s2)
	lw	ra, 28(sp)                      # 4-byte Folded Reload
	lw	s0, 24(sp)                      # 4-byte Folded Reload
	lw	s1, 20(sp)                      # 4-byte Folded Reload
	lw	s2, 16(sp)                      # 4-byte Folded Reload
	lw	s3, 12(sp)                      # 4-byte Folded Reload
	lw	s4, 8(sp)                       # 4-byte Folded Reload
	lw	s5, 4(sp)                       # 4-byte Folded Reload
	lw	s6, 0(sp)                       # 4-byte Folded Reload
	addi	sp, sp, 32
	ret
.Lfunc_end6:
	.size	getString, .Lfunc_end6-getString
                                        # -- End function
	.globl	getInt                          # -- Begin function getInt
	.p2align	1
	.type	getInt,@function
getInt:                                 # @getInt
# %bb.0:
	addi	sp, sp, -16
	sw	ra, 12(sp)                      # 4-byte Folded Spill
	lui	a0, %hi(.L.str.1)
	addi	a0, a0, %lo(.L.str.1)
	addi	a1, sp, 8
	call	scanf
	lw	a0, 8(sp)
	lw	ra, 12(sp)                      # 4-byte Folded Reload
	addi	sp, sp, 16
	ret
.Lfunc_end7:
	.size	getInt, .Lfunc_end7-getInt
                                        # -- End function
	.globl	string_as_str                   # -- Begin function string_as_str
	.p2align	1
	.type	string_as_str,@function
string_as_str:                          # @string_as_str
# %bb.0:
	lw	a2, 0(a1)
	lw	a1, 4(a1)
	sw	a2, 0(a0)
	sw	a1, 4(a0)
	ret
.Lfunc_end8:
	.size	string_as_str, .Lfunc_end8-string_as_str
                                        # -- End function
	.globl	string_len                      # -- Begin function string_len
	.p2align	1
	.type	string_len,@function
string_len:                             # @string_len
# %bb.0:
	lw	a0, 4(a0)
	ret
.Lfunc_end9:
	.size	string_len, .Lfunc_end9-string_len
                                        # -- End function
	.type	.L.str,@object                  # @.str
	.section	.rodata.str1.1,"aMS",@progbits,1
.L.str:
	.asciz	"%u"
	.size	.L.str, 3

	.type	.L.str.1,@object                # @.str.1
.L.str.1:
	.asciz	"%d"
	.size	.L.str.1, 3

	.type	.L.str.2,@object                # @.str.2
.L.str.2:
	.asciz	"%d\n"
	.size	.L.str.2, 4

	.ident	"clang version 22.1.6"
	.section	".note.GNU-stack","",@progbits
	.addrsig
