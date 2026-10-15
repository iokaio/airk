# Original written assembly 🌱; no execution.
.text
.global record
record:
    mov %rax, %rbx
    call submit
    jne record
    ret
