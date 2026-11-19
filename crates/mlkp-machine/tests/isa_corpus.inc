.text
addq %rsi, %rax
addq (%rbx), %rax
addq 16(%rbx), %rax
addq -8(%rbx), %rax
addb %al, %cl
addw $2, %bx
addq $5, %rax
addq $1000, %rdi
addq $100000, %r8
addl $5, %eax
addl $1000, %eax
movb $7, %al
movw $5, %ax
movl $5, %r9d
movq $5, %rax
movq $1234567890123, %rax
movq %rbx, %rax
movq %rax, 8(%rsp)
movq 8(%rsp), %rax
movq $7, 12(%rbx)
movb $3, (%rcx)
movzbq %al, %rdx
movzbl (%rcx), %eax
movswq (%rcx), %rbx
movslq %eax, %rbx
leaq 16(%rax,%rcx,8), %rdx
leaq -16(%rbp), %rax
leaq (,%rsi,4), %rdx
shlq $3, %rax
shlq $1, %rax
shrq %cl, %rax
sarq $2, (%rbx)
imulq %rsi, %rdi
imulq $7, %rsi, %rdi
negq %rax
idivq %rbx
cqto
pushq %r10
pushq $5
pushq $1000
popq %rbp
popq (%rcx)
xorl %eax, %eax
cmpq $0, (%rax)
testq %rax, %rbx
notq (%rcx)
movsd %xmm0, %xmm1
movsd %xmm1, (%rax)
movsd (%rax), %xmm2
movapd %xmm1, %xmm0
addsd %xmm2, %xmm3
sqrtsd %xmm4, %xmm5
ucomisd %xmm6, %xmm7
cvtsi2sdq %rcx, %xmm0
cvttsd2siq %xmm1, %r9
.Lshort:
je .Lshort
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
.Lfar:
jne .Lfar
call helper
ret

