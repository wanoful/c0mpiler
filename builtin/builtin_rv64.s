	.attribute	4, 16
	.attribute	5, "rv64i2p1_m2p0_a2p1_c2p0_zmmul1p0_zaamo1p0_zalrsc1p0_zca1p0"
	.file	"builtin.c"
	.text
	.globl	to_string                       # -- Begin function to_string
	.p2align	1
	.type	to_string,@function
to_string:                              # @to_string
# %bb.0:
	addi	sp, sp, -32
	sd	ra, 24(sp)                      # 8-byte Folded Spill
	sd	s0, 16(sp)                      # 8-byte Folded Spill
	sd	s1, 8(sp)                       # 8-byte Folded Spill
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
	sd	s0, 0(s1)
	sd	a0, 8(s1)
	ld	ra, 24(sp)                      # 8-byte Folded Reload
	ld	s0, 16(sp)                      # 8-byte Folded Reload
	ld	s1, 8(sp)                       # 8-byte Folded Reload
	addi	sp, sp, 32
	ret
.Lfunc_end0:
	.size	to_string, .Lfunc_end0-to_string
                                        # -- End function
	.globl	string_plus                     # -- Begin function string_plus
	.p2align	1
	.type	string_plus,@function
string_plus:                            # @string_plus
# %bb.0:
	addi	sp, sp, -80
	sd	ra, 72(sp)                      # 8-byte Folded Spill
	sd	s0, 64(sp)                      # 8-byte Folded Spill
	sd	s1, 56(sp)                      # 8-byte Folded Spill
	sd	s2, 48(sp)                      # 8-byte Folded Spill
	sd	s3, 40(sp)                      # 8-byte Folded Spill
	sd	s4, 32(sp)                      # 8-byte Folded Spill
	sd	s5, 24(sp)                      # 8-byte Folded Spill
	sd	s6, 16(sp)                      # 8-byte Folded Spill
	sd	s7, 8(sp)                       # 8-byte Folded Spill
	mv	s6, a3
	mv	s2, a2
	mv	s7, a1
	mv	s3, a0
	ld	s0, 8(a1)
	slli	a0, a3, 32
	srli	s4, a0, 32
	add	s5, s0, s4
	mv	a0, s5
	call	malloc
	mv	s1, a0
	add	a0, a0, s0
	beqz	s0, .LBB1_3
# %bb.1:
	ld	a1, 0(s7)
	mv	a2, s1
.LBB1_2:                                # =>This Inner Loop Header: Depth=1
	lbu	a3, 0(a1)
	sb	a3, 0(a2)
	addi	a2, a2, 1
	addi	a1, a1, 1
	bne	a2, a0, .LBB1_2
.LBB1_3:
	beqz	s6, .LBB1_5
# %bb.4:
	mv	a1, s2
	mv	a2, s4
	call	memcpy
.LBB1_5:
	sd	s1, 0(s3)
	sd	s5, 8(s3)
	ld	ra, 72(sp)                      # 8-byte Folded Reload
	ld	s0, 64(sp)                      # 8-byte Folded Reload
	ld	s1, 56(sp)                      # 8-byte Folded Reload
	ld	s2, 48(sp)                      # 8-byte Folded Reload
	ld	s3, 40(sp)                      # 8-byte Folded Reload
	ld	s4, 32(sp)                      # 8-byte Folded Reload
	ld	s5, 24(sp)                      # 8-byte Folded Reload
	ld	s6, 16(sp)                      # 8-byte Folded Reload
	ld	s7, 8(sp)                       # 8-byte Folded Reload
	addi	sp, sp, 80
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
	addi	sp, sp, -32
	sd	ra, 24(sp)                      # 8-byte Folded Spill
	sd	s0, 16(sp)                      # 8-byte Folded Spill
	sd	s1, 8(sp)                       # 8-byte Folded Spill
	mv	s0, a0
	slli	a1, a1, 32
	srli	a1, a1, 32
	add	s1, a0, a1
.LBB2_2:                                # =>This Inner Loop Header: Depth=1
	lbu	a0, 0(s0)
	call	putchar
	addi	s0, s0, 1
	bne	s0, s1, .LBB2_2
# %bb.3:
	ld	ra, 24(sp)                      # 8-byte Folded Reload
	ld	s0, 16(sp)                      # 8-byte Folded Reload
	ld	s1, 8(sp)                       # 8-byte Folded Reload
	addi	sp, sp, 32
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
	addi	sp, sp, -32
	sd	ra, 24(sp)                      # 8-byte Folded Spill
	sd	s0, 16(sp)                      # 8-byte Folded Spill
	sd	s1, 8(sp)                       # 8-byte Folded Spill
	mv	s0, a0
	slli	a1, a1, 32
	srli	a1, a1, 32
	add	s1, a0, a1
.LBB3_2:                                # =>This Inner Loop Header: Depth=1
	lbu	a0, 0(s0)
	call	putchar
	addi	s0, s0, 1
	bne	s0, s1, .LBB3_2
# %bb.3:
	ld	ra, 24(sp)                      # 8-byte Folded Reload
	ld	s0, 16(sp)                      # 8-byte Folded Reload
	ld	s1, 8(sp)                       # 8-byte Folded Reload
	addi	sp, sp, 32
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
	addi	sp, sp, -64
	sd	ra, 56(sp)                      # 8-byte Folded Spill
	sd	s0, 48(sp)                      # 8-byte Folded Spill
	sd	s1, 40(sp)                      # 8-byte Folded Spill
	sd	s2, 32(sp)                      # 8-byte Folded Spill
	sd	s3, 24(sp)                      # 8-byte Folded Spill
	sd	s4, 16(sp)                      # 8-byte Folded Spill
	sd	s5, 8(sp)                       # 8-byte Folded Spill
	sd	s6, 0(sp)                       # 8-byte Folded Spill
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
	sd	s3, 0(s2)
	sd	s1, 8(s2)
	ld	ra, 56(sp)                      # 8-byte Folded Reload
	ld	s0, 48(sp)                      # 8-byte Folded Reload
	ld	s1, 40(sp)                      # 8-byte Folded Reload
	ld	s2, 32(sp)                      # 8-byte Folded Reload
	ld	s3, 24(sp)                      # 8-byte Folded Reload
	ld	s4, 16(sp)                      # 8-byte Folded Reload
	ld	s5, 8(sp)                       # 8-byte Folded Reload
	ld	s6, 0(sp)                       # 8-byte Folded Reload
	addi	sp, sp, 64
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
	sd	ra, 8(sp)                       # 8-byte Folded Spill
	lui	a0, %hi(.L.str.1)
	addi	a0, a0, %lo(.L.str.1)
	addi	a1, sp, 4
	call	scanf
	lw	a0, 4(sp)
	ld	ra, 8(sp)                       # 8-byte Folded Reload
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
	ld	a2, 0(a1)
	ld	a1, 8(a1)
	sd	a2, 0(a0)
	sd	a1, 8(a0)
	ret
.Lfunc_end8:
	.size	string_as_str, .Lfunc_end8-string_as_str
                                        # -- End function
	.globl	string_len                      # -- Begin function string_len
	.p2align	1
	.type	string_len,@function
string_len:                             # @string_len
# %bb.0:
	ld	a0, 8(a0)
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
