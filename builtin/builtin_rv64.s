	.text
	.section	.rodata.str1.8,"aMS",@progbits,1
	.align	3
.LC0:
	.string	"%u"
	.text
	.align	1
	.globl	to_string
	.type	to_string, @function
to_string:
.LFB0:
	.cfi_startproc
	addi	sp,sp,-48
	.cfi_def_cfa_offset 48
	sd	s0,32(sp)
	.cfi_offset 8, -16
	mv	s0,a0
	li	a0,16
	sd	ra,40(sp)
	sd	s1,24(sp)
	.cfi_offset 1, -8
	.cfi_offset 9, -24
	sd	a1,8(sp)
	call	malloc@plt
	ld	a1,8(sp)
	mv	s1,a0
	lw	a2,0(a1)
	lla	a1,.LC0
	call	sprintf@plt
	mv	a0,s1
	call	strlen@plt
	ld	ra,40(sp)
	.cfi_restore 1
	sd	s1,0(s0)
	sd	a0,8(s0)
	ld	s0,32(sp)
	.cfi_restore 8
	ld	s1,24(sp)
	.cfi_restore 9
	addi	sp,sp,48
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
	addi	sp,sp,-64
	.cfi_def_cfa_offset 64
	sd	s1,40(sp)
	.cfi_offset 9, -24
	ld	s1,8(a1)
	sd	s0,48(sp)
	.cfi_offset 8, -16
	slli	s0,a3,32
	srli	s0,s0,32
	sd	s3,24(sp)
	.cfi_offset 19, -40
	add	s3,s1,s0
	sd	s2,32(sp)
	.cfi_offset 18, -32
	mv	s2,a0
	mv	a0,s3
	sd	s4,16(sp)
	sd	ra,56(sp)
	.cfi_offset 20, -48
	.cfi_offset 1, -8
	sd	a1,8(sp)
	mv	s4,a2
	call	malloc@plt
	mv	a3,a0
	beq	s1,zero,.L5
	ld	a1,8(sp)
	mv	a4,a0
	ld	a5,0(a1)
	add	a1,s1,a5
.L6:
	lbu	a2,0(a5)
	addi	a5,a5,1
	addi	a4,a4,1
	sb	a2,-1(a4)
	bne	a1,a5,.L6
.L5:
	beq	s0,zero,.L7
	add	a0,a3,s1
	mv	a2,s0
	mv	a1,s4
	sd	a3,8(sp)
	call	memcpy@plt
	ld	a3,8(sp)
.L7:
	ld	ra,56(sp)
	.cfi_restore 1
	ld	s0,48(sp)
	.cfi_restore 8
	sd	s3,8(s2)
	sd	a3,0(s2)
	ld	s1,40(sp)
	.cfi_restore 9
	ld	s2,32(sp)
	.cfi_restore 18
	ld	s3,24(sp)
	.cfi_restore 19
	ld	s4,16(sp)
	.cfi_restore 20
	addi	sp,sp,64
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
	addi	sp,sp,-32
	.cfi_def_cfa_offset 32
	sd	s1,8(sp)
	.cfi_offset 9, -24
	slli	s1,a1,32
	srli	s1,s1,32
	sd	s0,16(sp)
	sd	ra,24(sp)
	.cfi_offset 8, -16
	.cfi_offset 1, -8
	mv	s0,a0
	add	s1,a0,s1
.L18:
	lbu	a0,0(s0)
	addi	s0,s0,1
	call	putchar@plt
	bne	s0,s1,.L18
	ld	ra,24(sp)
	.cfi_restore 1
	ld	s0,16(sp)
	.cfi_restore 8
	ld	s1,8(sp)
	.cfi_restore 9
	addi	sp,sp,32
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
	addi	sp,sp,-32
	.cfi_def_cfa_offset 32
	sd	s1,8(sp)
	.cfi_offset 9, -24
	slli	s1,a1,32
	srli	s1,s1,32
	sd	s0,16(sp)
	sd	ra,24(sp)
	.cfi_offset 8, -16
	.cfi_offset 1, -8
	mv	s0,a0
	add	s1,a0,s1
.L29:
	lbu	a0,0(s0)
	addi	s0,s0,1
	call	putchar@plt
	bne	s0,s1,.L29
	ld	s0,16(sp)
	.cfi_restore 8
	ld	ra,24(sp)
	.cfi_restore 1
	ld	s1,8(sp)
	.cfi_restore 9
	li	a0,10
	addi	sp,sp,32
	.cfi_def_cfa_offset 0
	tail	putchar@plt
.L35:
	li	a0,10
	tail	putchar@plt
	.cfi_endproc
.LFE3:
	.size	println, .-println
	.section	.rodata.str1.8
	.align	3
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
	.section	.rodata.str1.8
	.align	3
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
	addi	sp,sp,-48
	.cfi_def_cfa_offset 48
	sd	s4,0(sp)
	.cfi_offset 20, -48
	mv	s4,a0
	li	a0,16
	sd	s1,24(sp)
	sd	s2,16(sp)
	sd	s3,8(sp)
	sd	ra,40(sp)
	sd	s0,32(sp)
	.cfi_offset 9, -24
	.cfi_offset 18, -32
	.cfi_offset 19, -40
	.cfi_offset 1, -8
	.cfi_offset 8, -16
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
	ld	ra,40(sp)
	.cfi_restore 1
	ld	s0,32(sp)
	.cfi_restore 8
	sd	s3,0(s4)
	sd	s1,8(s4)
	ld	s2,16(sp)
	.cfi_restore 18
	ld	s1,24(sp)
	.cfi_restore 9
	ld	s3,8(sp)
	.cfi_restore 19
	ld	s4,0(sp)
	.cfi_restore 20
	addi	sp,sp,48
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
	sd	ra,24(sp)
	.cfi_offset 1, -8
	call	scanf@plt
	ld	ra,24(sp)
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
	ld	a4,0(a1)
	ld	a5,8(a1)
	sd	a4,0(a0)
	sd	a5,8(a0)
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
	ld	a0,8(a0)
	ret
	.cfi_endproc
.LFE9:
	.size	string_len, .-string_len
	.ident	"GCC: (GNU) 15.1.0"
	.section	.note.GNU-stack,"",@progbits
