; ModuleID = 't'
source_filename = "t"

@burxt.argc = global i64 0
@burxt.argv = global ptr null
@stderr = external global ptr
@panic_msg = private unnamed_addr constant [97 x i8] c"burxt runtime error: arithmetic overflow \E2\80\94 the exact result no longer fits in the value range\0A\00", align 1
@fmt_int = private unnamed_addr constant [5 x i8] c"%lld\00", align 1
@fmt_nl = private unnamed_addr constant [2 x i8] c"\0A\00", align 1
@panic_msg.1 = private unnamed_addr constant [97 x i8] c"burxt runtime error: arithmetic overflow \E2\80\94 the exact result no longer fits in the value range\0A\00", align 1
@str_true = private unnamed_addr constant [5 x i8] c"true\00", align 1
@str_false = private unnamed_addr constant [6 x i8] c"false\00", align 1
@fmt_bool = private unnamed_addr constant [3 x i8] c"%s\00", align 1
@fmt_nl.2 = private unnamed_addr constant [2 x i8] c"\0A\00", align 1

declare i32 @printf(ptr, ...)

define i32 @main(i32 %0, ptr %1) {
entry:
  %i = alloca i64, align 8
  %argc64 = sext i32 %0 to i64
  store i64 %argc64, ptr @burxt.argc, align 4
  store ptr %1, ptr @burxt.argv, align 8
  store i64 0, ptr %i, align 4
  br label %while.cond

while.cond:                                       ; preds = %while.body, %entry
  %i1 = load i64, ptr %i, align 4
  %cmp = icmp slt i64 %i1, 3
  %cmp_i64 = zext i1 %cmp to i64
  %whilecond = icmp ne i64 %cmp_i64, 0
  br i1 %whilecond, label %while.body, label %while.end

while.body:                                       ; preds = %while.cond
  %i2 = load i64, ptr %i, align 4
  %checked = call i64 @burxt.checked.mul(i64 %i2, i64 2)
  %printf_int = call i32 (ptr, ...) @printf(ptr @fmt_int, i64 %checked)
  %printf_nl = call i32 (ptr, ...) @printf(ptr @fmt_nl)
  %i3 = load i64, ptr %i, align 4
  %checked4 = call i64 @burxt.checked.add(i64 %i3, i64 1)
  store i64 %checked4, ptr %i, align 4
  br label %while.cond

while.end:                                        ; preds = %while.cond
  %printf_bool = call i32 (ptr, ...) @printf(ptr @fmt_bool, ptr @str_true)
  %printf_nl5 = call i32 (ptr, ...) @printf(ptr @fmt_nl.2)
  ret i32 0
}

; Function Attrs: nocallback nofree nosync nounwind speculatable willreturn memory(none)
declare { i64, i1 } @llvm.smul.with.overflow.i64(i64, i64) #0

define i64 @burxt.checked.mul(i64 %0, i64 %1) {
entry:
  %op = call { i64, i1 } @llvm.smul.with.overflow.i64(i64 %0, i64 %1)
  %value = extractvalue { i64, i1 } %op, 0
  %overflowed = extractvalue { i64, i1 } %op, 1
  br i1 %overflowed, label %overflow, label %ok

overflow:                                         ; preds = %entry
  %stderr = load ptr, ptr @stderr, align 8
  %fputs = call i32 @fputs(ptr @panic_msg, ptr %stderr)
  call void @exit(i32 70)
  unreachable

ok:                                               ; preds = %entry
  ret i64 %value
}

declare i32 @fputs(ptr, ptr)

declare void @exit(i32)

; Function Attrs: nocallback nofree nosync nounwind speculatable willreturn memory(none)
declare { i64, i1 } @llvm.sadd.with.overflow.i64(i64, i64) #0

define i64 @burxt.checked.add(i64 %0, i64 %1) {
entry:
  %op = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %0, i64 %1)
  %value = extractvalue { i64, i1 } %op, 0
  %overflowed = extractvalue { i64, i1 } %op, 1
  br i1 %overflowed, label %overflow, label %ok

overflow:                                         ; preds = %entry
  %stderr = load ptr, ptr @stderr, align 8
  %fputs = call i32 @fputs(ptr @panic_msg.1, ptr %stderr)
  call void @exit(i32 70)
  unreachable

ok:                                               ; preds = %entry
  ret i64 %value
}

attributes #0 = { nocallback nofree nosync nounwind speculatable willreturn memory(none) }
