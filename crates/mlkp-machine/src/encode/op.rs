// CEP:FILE: crates/mlkp-machine/src/encode/op.rs
// CEP:WHAT: Opcode byte constants named per the Intel manual so encoders and decoders read like the ISA.
// CEP:WHY: The byte-level x86-64 encoder and the byte-level decoder share one opcode table.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: Returns the EncodeError variants documented per function; no silent truncation of any value.
// CEP:ASSUMES: none
// CEP:COST: cold; O(1) constants.
// CEP:EVIDENCE: crates/mlkp-machine/tests/encode_test.rs, crates/mlkp-machine/tests/exec_test.rs.

/// REX prefix base.
pub const REX_BASE: u8 = 0x40;
/// REX.W bit (64-bit operand size).
pub const REX_W: u8 = 0x08;
/// REX.R bit (ModRM.reg extension).
pub const REX_R: u8 = 0x04;
/// REX.B bit (ModRM.rm / SIB.base extension).
pub const REX_B: u8 = 0x01;
/// Group with immediate byte (add/sub/cmp imm8 forms).
pub const GRP1_IB: u8 = 0x83;
/// Group with immediate dword (add/sub/cmp imm32 forms).
pub const GRP1_ID: u8 = 0x81;
/// MOV r/m64, r64.
pub const MOV_MR: u8 = 0x89;
/// MOV r64, r/m64.
pub const MOV_RM: u8 = 0x8B;
/// MOV r/m64, imm32 (sign-extended).
pub const MOV_IMM32: u8 = 0xC7;
/// MOV r64, imm64 (movabs) base opcode B8+r.
pub const MOV_IMM64: u8 = 0xB8;
/// ADD r/m64, r64.
pub const ADD_MR: u8 = 0x01;
/// SUB r/m64, r64.
pub const SUB_MR: u8 = 0x29;
/// ADD r64, r/m64 (memory-source load form).
pub const ADD_RM: u8 = 0x03;
/// SUB r64, r/m64 (memory-source load form).
pub const SUB_RM: u8 = 0x2B;
/// CMP r64, r/m64 (memory-source load form).
pub const CMP_RM: u8 = 0x3B;
/// ADD EAX, imm32 short form.
pub const ADD_EAX_ID: u8 = 0x05;
/// SUB EAX, imm32 short form.
pub const SUB_EAX_ID: u8 = 0x2D;
/// CMP EAX, imm32 short form.
pub const CMP_EAX_ID: u8 = 0x3D;
/// Two-byte opcode escape.
pub const ESCAPE: u8 = 0x0F;
/// CMP r/m64, r64.
pub const CMP_MR: u8 = 0x39;
/// IMUL r64, r/m64 (after the escape).
pub const IMUL_RM: u8 = 0xAF;
/// ADD group /0 extension in ModRM.reg.
pub const EXT_ADD: u8 = 0;
/// SUB group /5 extension.
pub const EXT_SUB: u8 = 5;
/// CMP group /7 extension.
pub const EXT_CMP: u8 = 7;
/// PUSH r64 base opcode 50+r.
pub const PUSH_R: u8 = 0x50;
/// POP r64 base opcode 58+r.
pub const POP_R: u8 = 0x58;
/// JMP rel8.
pub const JMP_REL8: u8 = 0xEB;
/// JMP rel32.
pub const JMP_REL32: u8 = 0xE9;
/// JL rel8.
pub const JL_REL8: u8 = 0x7C;
/// JL rel32 second byte after the escape.
pub const JL_REL32: u8 = 0x8C;
/// JGE rel8.
pub const JGE_REL8: u8 = 0x7D;
/// JGE rel32 second byte after the escape.
pub const JGE_REL32: u8 = 0x8D;
/// CALL rel32.
pub const CALL_REL32: u8 = 0xE8;
/// RET near.
pub const RET: u8 = 0xC3;
/// SSE scalar-double prefix F2.
pub const PRE_F2: u8 = 0xF2;
/// SSE packed-double prefix 66.
pub const PRE_66: u8 = 0x66;
/// ADDSD (after the escape).
pub const ADDSD: u8 = 0x58;
/// MULSD (after the escape).
pub const MULSD: u8 = 0x59;
/// MOVAPD (reg-reg xmm move, after the escape).
pub const MOVAPD: u8 = 0x28;
/// MOVSD load form xmm, m64 (after the escape).
pub const MOVSD_LOAD: u8 = 0x10;
/// MOVSD store form m64, xmm (after the escape).
pub const MOVSD_STORE: u8 = 0x11;
/// PUSH RBP (no register field).
pub const PUSH_RBP: u8 = 0x55;
/// POP RBP (no register field).
pub const POP_RBP: u8 = 0x5D;
/// REX byte used for R12..R15 pushes and pops (B bit set).
pub const REX_B_ONLY: u8 = 0x41;
/// AND r/m64, r64.
pub const AND_MR: u8 = 0x21;
/// AND r64, r/m64 (memory-source load form).
pub const AND_RM: u8 = 0x23;
/// OR r/m64, r64.
pub const OR_MR: u8 = 0x09;
/// OR r64, r/m64.
pub const OR_RM: u8 = 0x0B;
/// XOR r/m64, r64.
pub const XOR_MR: u8 = 0x31;
/// XOR r64, r/m64.
pub const XOR_RM: u8 = 0x33;
/// AND EAX, imm32 short form.
pub const AND_EAX_ID: u8 = 0x25;
/// OR EAX, imm32 short form.
pub const OR_EAX_ID: u8 = 0x0D;
/// XOR EAX, imm32 short form.
pub const XOR_EAX_ID: u8 = 0x35;
/// OR group /1 extension.
pub const EXT_OR: u8 = 1;
/// AND group /4 extension.
pub const EXT_AND: u8 = 4;
/// XOR group /6 extension.
pub const EXT_XOR: u8 = 6;
/// NOT group /2 extension.
pub const EXT_NOT: u8 = 2;
/// NEG group /3 extension.
pub const EXT_NEG: u8 = 3;
/// IDIV group /7 extension.
pub const EXT_IDIV: u8 = 7;
/// TEST group /0 extension.
pub const EXT_TEST: u8 = 0;
/// TEST r/m64, r64.
pub const TEST_MR: u8 = 0x85;
/// Unary group with id immediates and division (F7 /0 /2 /3 /7).
pub const GRP3: u8 = 0xF7;
/// TEST EAX, imm32 short form.
pub const TEST_EAX_ID: u8 = 0xA9;
/// Shift immediate group opcode.
pub const SHIFT_IMM: u8 = 0xC1;
/// Shift by CL group opcode.
pub const SHIFT_CL: u8 = 0xD3;
/// Shift with the implicit count of one (word/dword/qword form).
pub const SHIFT_ONE: u8 = 0xD1;
/// SHL group /4 extension.
pub const EXT_SHL: u8 = 4;
/// SHR group /5 extension.
pub const EXT_SHR: u8 = 5;
/// CQO (sign-extend RAX into RDX).
pub const CQO: u8 = 0x99;
/// JE rel8.
pub const JE_REL8: u8 = 0x74;
/// JNE rel8.
pub const JNE_REL8: u8 = 0x75;
/// JLE rel8.
pub const JLE_REL8: u8 = 0x7E;
/// JG rel8.
pub const JG_REL8: u8 = 0x7F;
/// JA rel8.
pub const JA_REL8: u8 = 0x77;
/// JAE rel8.
pub const JAE_REL8: u8 = 0x73;
/// JB rel8.
pub const JB_REL8: u8 = 0x72;
/// JBE rel8.
pub const JBE_REL8: u8 = 0x76;
/// JS rel8.
pub const JS_REL8: u8 = 0x78;
/// JNS rel8.
pub const JNS_REL8: u8 = 0x79;
/// JO rel8.
pub const JO_REL8: u8 = 0x70;
/// JNO rel8.
pub const JNO_REL8: u8 = 0x71;
/// JP rel8.
pub const JP_REL8: u8 = 0x7A;
/// JNP rel8.
pub const JNP_REL8: u8 = 0x7B;
/// JE rel32 second byte.
pub const JE_REL32: u8 = 0x84;
/// JNE rel32 second byte.
pub const JNE_REL32: u8 = 0x85;
/// JLE rel32 second byte.
pub const JLE_REL32: u8 = 0x8E;
/// JG rel32 second byte.
pub const JG_REL32: u8 = 0x8F;
/// JA rel32 second byte.
pub const JA_REL32: u8 = 0x87;
/// JAE rel32 second byte.
pub const JAE_REL32: u8 = 0x83;
/// JB rel32 second byte.
pub const JB_REL32: u8 = 0x82;
/// JBE rel32 second byte.
pub const JBE_REL32: u8 = 0x86;
/// JS rel32 second byte.
pub const JS_REL32: u8 = 0x88;
/// JNS rel32 second byte.
pub const JNS_REL32: u8 = 0x89;
/// JO rel32 second byte.
pub const JO_REL32: u8 = 0x80;
/// JNO rel32 second byte.
pub const JNO_REL32: u8 = 0x81;
/// JP rel32 second byte.
pub const JP_REL32: u8 = 0x8A;
/// JNP rel32 second byte.
pub const JNP_REL32: u8 = 0x8B;
/// LEA r64, m.
pub const LEA: u8 = 0x8D;
/// IMUL r64, r/m64, imm8 (three-operand form).
pub const IMUL3_IMM8: u8 = 0x6B;
/// IMUL r64, r/m64, imm32 (three-operand form).
pub const IMUL3_IMM32: u8 = 0x69;
/// MOVZX r64, r/m8 (after the escape).
pub const MOVZ8: u8 = 0xB6;
/// MOVSXD r64, r/m32 (after the escape).
pub const MOVSX32: u8 = 0x63;
/// PUSH imm8.
pub const PUSH_IMM8: u8 = 0x6A;
/// PUSH imm32.
pub const PUSH_IMM32: u8 = 0x68;
/// SUBSD (after the escape).
pub const SUBSD: u8 = 0x5C;
/// DIVSD (after the escape).
pub const DIVSD: u8 = 0x5E;
/// SQRTSD (after the escape).
pub const SQRTSD: u8 = 0x51;
/// UCOMISD (after the escape).
pub const UCOMISD: u8 = 0x2E;
/// CVTSI2SD (after the escape, with REX.W).
pub const CVTSI2SD: u8 = 0x2A;
/// CVTTSD2SI (after the escape, with REX.W).
pub const CVTTSD2SI: u8 = 0x2C;
/// MOV r/m8, r8 (byte store).
pub const MOV8_MR: u8 = 0x88;
/// ModRM of movq %rsp, %rbp (reg = RSP, rm = RBP in hardware numbers).
pub const MODRM_MOV_RSP_RBP: u8 = 0xE5;
/// ModRM of movq %rbp, %rsp (reg = RBP, rm = RSP in hardware numbers).
pub const MODRM_MOV_RBP_RSP: u8 = 0xEC;
