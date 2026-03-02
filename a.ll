; ModuleID = 'lang'
source_filename = "lang"
target datalayout = "e-m:o-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-n32:64-S128-Fn32"

@str = private unnamed_addr constant [11 x i8] c"Hi, Septem\00", align 1

define i64 @main() {
entry:
  %0 = call i64 (ptr, ...) @printf(ptr @str)
  ret i64 42
  ret void
}

declare i64 @printf(ptr, ...)
