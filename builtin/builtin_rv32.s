	.text
	.section	.rodata.str1.4,"aMS",@progbits,1
	.align	2
.LC0:
	.string	"%u"
	.text
	.align	1
	.globl	to_string
	.type	to_string, @function
to_string:
.LFB0:
	.cfi_startproc
	addi	sp,sp,-32
	.cfi_def_cfa_offset 32
	sw	s0,24(sp)
	.cfi_offset 8, -8
	mv	s0,a0
	li	a0,16
	sw	ra,28(sp)
	sw	s1,20(sp)
	.cfi_offset 1, -4
	.cfi_offset 9, -12
	sw	a1,12(sp)
	call	malloc@plt
	lw	a1,12(sp)
	mv	s1,a0
	lw	a2,0(a1)
	lla	a1,.LC0
	call	sprintf@plt
	mv	a0,s1
	call	strlen@plt
	lw	ra,28(sp)
	.cfi_restore 1
	sw	s1,0(s0)
	sw	a0,4(s0)
	lw	s0,24(sp)
	.cfi_restore 8
	lw	s1,20(sp)
	.cfi_restore 9
	addi	sp,sp,32
	.cfi_def_cfa_offset 0
	jr	ra
	.cfi_endproc
.LFE0:
	.size	to_string, .-to_string
	.align	1
	.globl	string_plus
	.type	string_plus, @function
string_plus:
.LFB1:
	.cfi_startproc
	addi	sp,sp,-48
	.cfi_def_cfa_offset 48
	sw	s0,40(sp)
	.cfi_offset 8, -8
	lw	s0,4(a1)
	sw	s2,32(sp)
	sw	s1,36(sp)
	.cfi_offset 18, -16
	.cfi_offset 9, -12
	add	s2,s0,a3
	mv	s1,a0
	mv	a0,s2
	sw	s3,28(sp)
	sw	a3,8(sp)
	sw	ra,44(sp)
	.cfi_offset 19, -20
	.cfi_offset 1, -4
	sw	a1,12(sp)
	mv	s3,a2
	call	malloc@plt
	lw	a3,8(sp)
	mv	a6,a0
	beq	s0,zero,.L5
	lw	a1,12(sp)
	mv	a4,a0
	lw	a5,0(a1)
	add	a1,s0,a5
.L6:
	lbu	a2,0(a5)
	addi	a5,a5,1
	addi	a4,a4,1
	sb	a2,-1(a4)
	bne	a1,a5,.L6
.L5:
	beq	a3,zero,.L7
	add	a0,a6,s0
	mv	a2,a3
	mv	a1,s3
	sw	a6,8(sp)
	call	memcpy@plt
	lw	a6,8(sp)
.L7:
	lw	ra,44(sp)
	.cfi_restore 1
	lw	s0,40(sp)
	.cfi_restore 8
	sw	s2,4(s1)
	sw	a6,0(s1)
	lw	s2,32(sp)
	.cfi_restore 18
	lw	s1,36(sp)
	.cfi_restore 9
	lw	s3,28(sp)
	.cfi_restore 19
	addi	sp,sp,48
	.cfi_def_cfa_offset 0
	jr	ra
	.cfi_endproc
.LFE1:
	.size	string_plus, .-string_plus
	.align	1
	.globl	print
	.type	print, @function
print:
.LFB2:
	.cfi_startproc
	beq	a1,zero,.L24
	addi	sp,sp,-16
	.cfi_def_cfa_offset 16
	sw	s0,8(sp)
	sw	s1,4(sp)
	sw	ra,12(sp)
	.cfi_offset 8, -8
	.cfi_offset 9, -12
	.cfi_offset 1, -4
	mv	s0,a0
	add	s1,a0,a1
.L18:
	lbu	a0,0(s0)
	addi	s0,s0,1
	call	putchar@plt
	bne	s0,s1,.L18
	lw	ra,12(sp)
	.cfi_restore 1
	lw	s0,8(sp)
	.cfi_restore 8
	lw	s1,4(sp)
	.cfi_restore 9
	addi	sp,sp,16
	.cfi_def_cfa_offset 0
	jr	ra
.L24:
	ret
	.cfi_endproc
.LFE2:
	.size	print, .-print
	.align	1
	.globl	println
	.type	println, @function
println:
.LFB3:
	.cfi_startproc
	beq	a1,zero,.L35
	addi	sp,sp,-16
	.cfi_def_cfa_offset 16
	sw	s0,8(sp)
	sw	s1,4(sp)
	sw	ra,12(sp)
	.cfi_offset 8, -8
	.cfi_offset 9, -12
	.cfi_offset 1, -4
	mv	s0,a0
	add	s1,a0,a1
.L29:
	lbu	a0,0(s0)
	addi	s0,s0,1
	call	putchar@plt
	bne	s0,s1,.L29
	lw	s0,8(sp)
	.cfi_restore 8
	lw	ra,12(sp)
	.cfi_restore 1
	lw	s1,4(sp)
	.cfi_restore 9
	li	a0,10
	addi	sp,sp,16
	.cfi_def_cfa_offset 0
	tail	putchar@plt
.L35:
	li	a0,10
	tail	putchar@plt
	.cfi_endproc
.LFE3:
	.size	println, .-println
	.section	.rodata.str1.4
	.align	2
.LC1:
	.string	"%d"
	.text
	.align	1
	.globl	printInt
	.type	printInt, @function
printInt:
.LFB4:
	.cfi_startproc
	mv	a1,a0
	lla	a0,.LC1
	tail	printf@plt
	.cfi_endproc
.LFE4:
	.size	printInt, .-printInt
	.section	.rodata.str1.4
	.align	2
.LC2:
	.string	"%d\n"
	.text
	.align	1
	.globl	printlnInt
	.type	printlnInt, @function
printlnInt:
.LFB5:
	.cfi_startproc
	mv	a1,a0
	lla	a0,.LC2
	tail	printf@plt
	.cfi_endproc
.LFE5:
	.size	printlnInt, .-printlnInt
	.align	1
	.globl	getString
	.type	getString, @function
getString:
.LFB6:
	.cfi_startproc
	addi	sp,sp,-32
	.cfi_def_cfa_offset 32
	sw	s4,8(sp)
	.cfi_offset 20, -24
	mv	s4,a0
	li	a0,16
	sw	s1,20(sp)
	sw	s2,16(sp)
	sw	s3,12(sp)
	sw	ra,28(sp)
	sw	s0,24(sp)
	.cfi_offset 9, -12
	.cfi_offset 18, -16
	.cfi_offset 19, -20
	.cfi_offset 1, -4
	.cfi_offset 8, -8
	call	malloc@plt
	mv	s3,a0
	li	s1,0
	li	s2,16
	j	.L41
.L42:
	add	a5,s3,s1
	sb	s0,0(a5)
	addi	s1,s1,1
.L41:
	call	getchar@plt
	addi	a5,a0,1
	mv	s0,a0
	addi	a4,a0,-10
	beq	a5,zero,.L45
	beq	a4,zero,.L45
	bgtu	s2,s1,.L42
	slli	s2,s2,1
	mv	a1,s2
	mv	a0,s3
	call	realloc@plt
	j	.L42
.L45:
	lw	ra,28(sp)
	.cfi_restore 1
	lw	s0,24(sp)
	.cfi_restore 8
	sw	s3,0(s4)
	sw	s1,4(s4)
	lw	s2,16(sp)
	.cfi_restore 18
	lw	s1,20(sp)
	.cfi_restore 9
	lw	s3,12(sp)
	.cfi_restore 19
	lw	s4,8(sp)
	.cfi_restore 20
	addi	sp,sp,32
	.cfi_def_cfa_offset 0
	jr	ra
	.cfi_endproc
.LFE6:
	.size	getString, .-getString
	.align	1
	.globl	getInt
	.type	getInt, @function
getInt:
.LFB7:
	.cfi_startproc
	addi	sp,sp,-32
	.cfi_def_cfa_offset 32
	addi	a1,sp,12
	lla	a0,.LC1
	sw	ra,28(sp)
	.cfi_offset 1, -4
	call	scanf@plt
	lw	ra,28(sp)
	.cfi_restore 1
	lw	a0,12(sp)
	addi	sp,sp,32
	.cfi_def_cfa_offset 0
	jr	ra
	.cfi_endproc
.LFE7:
	.size	getInt, .-getInt
	.align	1
	.globl	string_as_str
	.type	string_as_str, @function
string_as_str:
.LFB8:
	.cfi_startproc
	lw	a4,0(a1)
	lw	a5,4(a1)
	sw	a4,0(a0)
	sw	a5,4(a0)
	ret
	.cfi_endproc
.LFE8:
	.size	string_as_str, .-string_as_str
	.align	1
	.globl	string_len
	.type	string_len, @function
string_len:
.LFB9:
	.cfi_startproc
	lw	a0,4(a0)
	ret
	.cfi_endproc
.LFE9:
	.size	string_len, .-string_len
	.ident	"GCC: (GNU) 15.1.0"
	.section	.note.GNU-stack,"",@progbits
