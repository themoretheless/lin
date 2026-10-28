define internal fastcc noundef zeroext i1 @_RNvNtCsagrXUsJVDRq_3lin5index17index_covers_pred(ptr noalias nofree noundef nonnull readonly align 8 captures(address, read_provenance) dereferenceable(72) %pred, ptr noalias nofree noundef nonnull readonly align 8 captures(address, read_provenance) %uses.0, i64 noundef range(i64 0, 39755913951960241) %uses.1) unnamed_addr #2 personality ptr @_RNvCsfmtydf4BlqR_7___rustc19rust_eh_personality !guid !249574 {
start:
  %_6.i.i.i.i.i.i = alloca [24 x i8], align 8
  %v.i20.i.i.i.i = alloca [24 x i8], align 8
  %v.i.i.i.i.i = alloca [24 x i8], align 8
  %_51.i.i.i.i = alloca [24 x i8], align 8
  %_12.i = alloca [24 x i8], align 8
  %_8 = alloca [24 x i8], align 8
  %branches = alloca [24 x i8], align 8
  %0 = icmp eq i64 %uses.1, 1
  %1 = load i64, ptr %pred, align 8, !range !41674
  %_51 = icmp eq i64 %1, -9223372036854775805
  %or.cond = select i1 %0, i1 %_51, i1 false
  br i1 %or.cond, label %bb2, label %bb5

bb5:                                              ; preds = %start
  call void @llvm.lifetime.start.p0(ptr nonnull %_8)
; call lin::index::dnf
  call fastcc void @_RNvNtCsagrXUsJVDRq_3lin5index3dnf(ptr noalias nofree noundef align 8 captures(address) dereferenceable(24) %_8, ptr noalias nofree noundef readonly align 8 captures(address, read_provenance) dereferenceable(72) %pred)
  %2 = load i64, ptr %_8, align 8, !range !3853, !noundef !2388
  %.not = icmp eq i64 %2, -1
  br i1 %.not, label %bb8, label %bb7

bb2:                                              ; preds = %start
  tail call void @llvm.experimental.noalias.scope.decl(metadata !249575)
  tail call void @llvm.experimental.noalias.scope.decl(metadata !249578)
  %3 = getelementptr inbounds nuw i8, ptr %uses.0, i64 72
  %4 = load i64, ptr %3, align 8, !range !41402, !alias.scope !249578, !noalias !249575, !noundef !2388
  %.not.i = icmp eq i64 %4, -1
  br i1 %.not.i, label %bb4.i, label %bb31.i

bb31.i:                                           ; preds = %bb2
; call lin::index::index_covers_single_cmp::contains_atom
  %_3.i = tail call fastcc noundef zeroext i1 @_RNvNvNtCsagrXUsJVDRq_3lin5index23index_covers_single_cmp13contains_atom(ptr noalias nofree noundef readonly align 8 captures(address, read_provenance) dereferenceable(72) %3, ptr noalias nofree noundef nonnull readonly align 8 captures(address, read_provenance) dereferenceable(72) %pred)
  br i1 %_3.i, label %bb14, label %bb4.i

bb4.i:                                            ; preds = %bb2, %bb31.i
  %5 = getelementptr inbounds nuw i8, ptr %pred, i64 56
  %6 = getelementptr inbounds nuw i8, ptr %pred, i64 32
  %7 = getelementptr inbounds nuw i8, ptr %pred, i64 24
  %_44.i = load i64, ptr %7, align 8, !alias.scope !249575, !noalias !249578, !noundef !2388
  %8 = icmp eq i64 %_44.i, 1
  br i1 %8, label %bb34.i, label %bb33.i

bb34.i:                                           ; preds = %bb4.i
  %9 = getelementptr inbounds nuw i8, ptr %pred, i64 16
  %_45.i = load ptr, ptr %9, align 8, !alias.scope !249575, !noalias !249578, !nonnull !2388, !noundef !2388
  %10 = getelementptr inbounds nuw i8, ptr %_45.i, i64 8
  %_50.i = load ptr, ptr %10, align 8, !noalias !249580, !nonnull !2388, !noundef !2388
  %11 = getelementptr inbounds nuw i8, ptr %_45.i, i64 16
  %_49.i = load i64, ptr %11, align 8, !noalias !249580, !noundef !2388
  br label %bb7.i

bb33.i:                                           ; preds = %bb4.i
  call void @llvm.lifetime.start.p0(ptr nonnull %_12.i), !noalias !249580
  tail call void @llvm.experimental.noalias.scope.decl(metadata !249581)
  %12 = getelementptr inbounds nuw i8, ptr %pred, i64 16
  %_6.i.i = load ptr, ptr %12, align 8, !alias.scope !249584, !noalias !249585, !nonnull !2388, !noundef !2388
; call alloc::str::join_generic_copy::<str, u8, alloc::string::String>
  call fastcc void @_RINvNtCsj8lNtgb2Igp_5alloc3str17join_generic_copyehNtNtB4_6string6StringECsagrXUsJVDRq_3lin(ptr noalias nofree noundef align 8 captures(none) dereferenceable(24) %_12.i, ptr noalias nofree noundef nonnull readonly align 8 captures(address, read_provenance) %_6.i.i, i64 noundef %_44.i, ptr noalias nofree noundef nonnull readonly captures(address, read_provenance) @alloc_0f52f98e9fa44caf6668aeb0b1c94ca5, i64 noundef 1), !noalias !249587
  %field.sroa.0.0.copyload.i = load i64, ptr %_12.i, align 8, !noalias !249580
  %field.sroa.8.0._12.sroa_idx.i = getelementptr inbounds nuw i8, ptr %_12.i, i64 8
  %field.sroa.8.0.copyload.i = load ptr, ptr %field.sroa.8.0._12.sroa_idx.i, align 8, !noalias !249580
  %field.sroa.14.0._12.sroa_idx.i = getelementptr inbounds nuw i8, ptr %_12.i, i64 16
  %field.sroa.14.0.copyload.i = load i64, ptr %field.sroa.14.0._12.sroa_idx.i, align 8, !noalias !249580
  call void @llvm.lifetime.end.p0(ptr nonnull %_12.i), !noalias !249580
  br label %bb7.i

bb7.i:                                            ; preds = %bb33.i, %bb34.i
  %field.sroa.0.0.i = phi i64 [ -1, %bb34.i ], [ %field.sroa.0.0.copyload.i, %bb33.i ]
  %field.sroa.8.0.i = phi ptr [ %_50.i, %bb34.i ], [ %field.sroa.8.0.copyload.i, %bb33.i ]
  %field.sroa.14.0.i = phi i64 [ %_49.i, %bb34.i ], [ %field.sroa.14.0.copyload.i, %bb33.i ]
  %13 = load i8, ptr %5, align 8, !range !2751, !alias.scope !249575, !noalias !249578, !noundef !2388
  switch i8 %13, label %default.unreachable [
    i8 0, label %bb10.i
    i8 2, label %bb9.i
    i8 3, label %bb9.i
    i8 4, label %bb9.i
    i8 5, label %bb9.i
    i8 1, label %bb24.i
  ]

default.unreachable:                              ; preds = %bb7.i
  unreachable

bb10.i:                                           ; preds = %bb7.i
  %14 = load i64, ptr %6, align 8, !range !41674, !alias.scope !249575, !noalias !249578, !noundef !2388
  switch i64 %14, label %bb15.i [
    i64 -9223372036854775807, label %bb12.i
    i64 -9223372036854775806, label %bb16.i
  ]

bb9.i:                                            ; preds = %bb7.i, %bb7.i, %bb7.i, %bb7.i
  %15 = getelementptr inbounds nuw i8, ptr %uses.0, i64 176
  %16 = load i64, ptr %15, align 8, !range !3853, !alias.scope !249578, !noalias !249575, !noundef !2388
  %.not2.i = icmp eq i64 %16, -1
  br i1 %.not2.i, label %bb24.i, label %bb36.i

bb12.i:                                           ; preds = %bb10.i
  %17 = getelementptr inbounds nuw i8, ptr %pred, i64 40
  %_19.i = load i64, ptr %17, align 8, !alias.scope !249575, !noalias !249578, !noundef !2388
  %18 = add i64 %_19.i, -9007199254740992
  %_17.i = icmp ult i64 %18, -18014398509481983
  br i1 %_17.i, label %bb22.i, label %bb15.i

bb22.i:                                           ; preds = %bb16.i, %bb12.i
  %_1.0.val.off.i.i = add i64 %field.sroa.0.0.i, -1
  %switch.i.i = icmp ult i64 %_1.0.val.off.i.i, -2
  br i1 %switch.i.i, label %bb27.sink.split.i, label %bb14

bb16.i:                                           ; preds = %bb10.i
  %19 = getelementptr inbounds nuw i8, ptr %pred, i64 40
  %_24.i = load double, ptr %19, align 8, !alias.scope !249575, !noalias !249578, !noundef !2388
  %_55.i = tail call double @llvm.fabs.f64(double %_24.i)
  %_23.i = fcmp one double %_55.i, +inf
  %_25.i = fcmp ult double %_55.i, f0x4340000000000000
  %or.cond.i = and i1 %_23.i, %_25.i
  br i1 %or.cond.i, label %bb15.i, label %bb22.i

bb15.i:                                           ; preds = %bb16.i, %bb12.i, %bb10.i
  %20 = getelementptr inbounds nuw i8, ptr %uses.0, i64 160
  %_59.i = load ptr, ptr %20, align 8, !alias.scope !249578, !noalias !249575, !nonnull !2388, !noundef !2388
  %21 = getelementptr inbounds nuw i8, ptr %uses.0, i64 168
  %_58.i = load i64, ptr %21, align 8, !alias.scope !249578, !noalias !249575, !noundef !2388
  %_63.idx.i = mul nuw nsw i64 %_58.i, 48
  %_63.i = getelementptr inbounds nuw i8, ptr %_59.i, i64 %_63.idx.i
  %_123.not.i.i = icmp eq i64 %_58.i, 0
  br i1 %_123.not.i.i, label %bb24.i, label %bb13.i.i

bb13.i.i:                                         ; preds = %bb15.i, %bb1.backedge.i.i
  %_2224.i.i = phi ptr [ %_22.i.i, %bb1.backedge.i.i ], [ %_59.i, %bb15.i ]
  %_22.i.i = getelementptr inbounds nuw i8, ptr %_2224.i.i, i64 48
  tail call void @llvm.experimental.noalias.scope.decl(metadata !249588)
  %v.i.i.i = getelementptr inbounds nuw i8, ptr %_2224.i.i, i64 24
  %22 = getelementptr inbounds nuw i8, ptr %_2224.i.i, i64 16
  %_19.i.i.i = load i64, ptr %22, align 8, !alias.scope !249588, !noalias !249591, !noundef !2388
  %_24.i.i.i = icmp eq i64 %_19.i.i.i, %field.sroa.14.0.i
  br i1 %_24.i.i.i, label %bb8.i.i.i, label %bb1.backedge.i.i

bb8.i.i.i:                                        ; preds = %bb13.i.i
  %23 = getelementptr inbounds nuw i8, ptr %_2224.i.i, i64 8
  %_20.i.i.i = load ptr, ptr %23, align 8, !alias.scope !249588, !noalias !249591, !nonnull !2388, !noundef !2388
  %_30.i.i.i = tail call i32 @memcmp(ptr nonnull %_20.i.i.i, ptr nonnull %field.sroa.8.0.i, i64 %field.sroa.14.0.i), !noalias !249596
  %_5.i.i.i = icmp eq i32 %_30.i.i.i, 0
  br i1 %_5.i.i.i, label %bb1.i.i.i, label %bb1.backedge.i.i

bb1.i.i.i:                                        ; preds = %bb8.i.i.i
; call <lin::ast::Value as core::cmp::PartialEq>::eq
  %24 = tail call fastcc noundef zeroext i1 @_RNvXs1s_NtCsagrXUsJVDRq_3lin3astNtB6_5ValueNtNtCsaLbZTW5namK_4core3cmp9PartialEq2eq(ptr noalias nofree noundef readonly align 8 captures(address, read_provenance) dereferenceable(24) %v.i.i.i, ptr noalias nofree noundef readonly align 8 captures(address, read_provenance) dereferenceable(24) %6), !noalias !249597
  br i1 %24, label %bb24.i, label %bb1.backedge.i.i

bb1.backedge.i.i:                                 ; preds = %bb1.i.i.i, %bb8.i.i.i, %bb13.i.i
  %_12.not.i.i = icmp eq ptr %_22.i.i, %_63.i
  br i1 %_12.not.i.i, label %bb24.i, label %bb13.i.i

bb27.sink.split.i:                                ; preds = %bb24.i, %bb22.i
  %_0.sroa.0.0.off0.ph.i = phi i1 [ %_0.sroa.0.1.off0.i, %bb24.i ], [ false, %bb22.i ]
  call void @llvm.assume(i1 true) [ "nonnull"(ptr %field.sroa.8.0.i) ]
; call __rustc::__rust_dealloc
  tail call void @_RNvCsfmtydf4BlqR_7___rustc14___rust_dealloc(ptr noundef nonnull %field.sroa.8.0.i, i64 noundef %field.sroa.0.0.i, i64 noundef range(i64 1, -9223372036854775807) 1) #86, !noalias !249580
  br label %bb14

bb24.i:                                           ; preds = %bb1.backedge.i.i, %bb1.i.i.i, %bb2.i.i, %bb10.i.i, %bb36.i, %bb15.i, %bb9.i, %bb7.i
  %_0.sroa.0.1.off0.i = phi i1 [ false, %bb7.i ], [ false, %bb10.i.i ], [ false, %bb15.i ], [ false, %bb36.i ], [ false, %bb9.i ], [ %29, %bb2.i.i ], [ false, %bb1.backedge.i.i ], [ true, %bb1.i.i.i ]
  %_1.0.val.off.i15.i = add i64 %field.sroa.0.0.i, -1
  %switch.i16.i = icmp ult i64 %_1.0.val.off.i15.i, -2
  br i1 %switch.i16.i, label %bb27.sink.split.i, label %bb14

bb36.i:                                           ; preds = %bb9.i
  tail call void @llvm.experimental.noalias.scope.decl(metadata !249598)
  %range_value.i.i = getelementptr inbounds nuw i8, ptr %uses.0, i64 208
  %25 = getelementptr inbounds nuw i8, ptr %uses.0, i64 192
  %_22.i19.i = load i64, ptr %25, align 8, !alias.scope !249601, !noalias !249602, !noundef !2388
  %_27.i.i = icmp eq i64 %_22.i19.i, %field.sroa.14.0.i
  br i1 %_27.i.i, label %bb10.i.i, label %bb24.i

bb10.i.i:                                         ; preds = %bb36.i
  %26 = getelementptr inbounds nuw i8, ptr %uses.0, i64 184
  %_23.i.i = load ptr, ptr %26, align 8, !alias.scope !249601, !noalias !249602, !nonnull !2388, !noundef !2388
  %_33.i.i = tail call i32 @memcmp(ptr nonnull %_23.i.i, ptr nonnull %field.sroa.8.0.i, i64 %field.sroa.14.0.i), !noalias !249604
  %_6.i20.i = icmp eq i32 %_33.i.i, 0
  %27 = getelementptr inbounds nuw i8, ptr %uses.0, i64 200
  %28 = load i8, ptr %27, align 8, !range !2751, !alias.scope !249578, !noalias !249575
  %_8.i.i = icmp eq i8 %28, %13
  %or.cond23.i = select i1 %_6.i20.i, i1 %_8.i.i, i1 false
  br i1 %or.cond23.i, label %bb2.i.i, label %bb24.i

bb2.i.i:                                          ; preds = %bb10.i.i
; call <lin::ast::Value as core::cmp::PartialEq>::eq
  %29 = tail call fastcc noundef zeroext i1 @_RNvXs1s_NtCsagrXUsJVDRq_3lin3astNtB6_5ValueNtNtCsaLbZTW5namK_4core3cmp9PartialEq2eq(ptr noalias nofree noundef readonly align 8 captures(address, read_provenance) dereferenceable(24) %range_value.i.i, ptr noalias nofree noundef readonly align 8 captures(address, read_provenance) dereferenceable(24) %6), !noalias !249605
  br label %bb24.i

bb14:                                             ; preds = %bb24.i, %bb27.sink.split.i, %bb22.i, %bb31.i, %bb8, %_RINvNtCsaLbZTW5namK_4core3ptr9drop_glueINtNtCsj8lNtgb2Igp_5alloc3vec3VecNtNtCsagrXUsJVDRq_3lin3ast4PredEEB1c_.exit, %_RINvNtCsaLbZTW5namK_4core3ptr9drop_glueINtNtCsj8lNtgb2Igp_5alloc3vec3VecNtNtCsagrXUsJVDRq_3lin3ast4PredEEB1c_.exit31
  %_0.sroa.0.0.shrunk = phi i1 [ false, %bb8 ], [ false, %_RINvNtCsaLbZTW5namK_4core3ptr9drop_glueINtNtCsj8lNtgb2Igp_5alloc3vec3VecNtNtCsagrXUsJVDRq_3lin3ast4PredEEB1c_.exit ], [ %_3.i7, %_RINvNtCsaLbZTW5namK_4core3ptr9drop_glueINtNtCsj8lNtgb2Igp_5alloc3vec3VecNtNtCsagrXUsJVDRq_3lin3ast4PredEEB1c_.exit31 ], [ true, %bb31.i ], [ %_0.sroa.0.0.off0.ph.i, %bb27.sink.split.i ], [ %_0.sroa.0.1.off0.i, %bb24.i ], [ false, %bb22.i ]
  ret i1 %_0.sroa.0.0.shrunk

bb7:                                              ; preds = %bb5
  call void @llvm.lifetime.start.p0(ptr nonnull %branches)
  call void @llvm.memcpy.p0.p0.i64(ptr noundef nonnull align 8 dereferenceable(24) %branches, ptr noundef nonnull align 8 dereferenceable(24) %_8, i64 24, i1 false)
  call void @llvm.lifetime.end.p0(ptr nonnull %_8)
  %30 = getelementptr inbounds nuw i8, ptr %branches, i64 16
  %_11 = load i64, ptr %30, align 8, !noundef !2388
  %_16 = icmp ult i64 %_11, 128102389400760776
  tail call void @llvm.assume(i1 %_16)
  %_10.not = icmp eq i64 %_11, %uses.1
  br i1 %_10.not, label %bb11, label %bb9

bb8:                                              ; preds = %bb5
  call void @llvm.lifetime.end.p0(ptr nonnull %_8)
  br label %bb14

bb9:                                              ; preds = %bb7
  tail call void @llvm.experimental.noalias.scope.decl(metadata !249606)
  %31 = getelementptr inbounds nuw i8, ptr %branches, i64 8
  %_1.val.i = load ptr, ptr %31, align 8, !alias.scope !249606, !nonnull !2388, !noundef !2388
  %_7.i.i.i184 = icmp eq i64 %_11, 0
  br i1 %_7.i.i.i184, label %bb4.i4, label %bb5.i.i.i

bb6.i.i.i:                                        ; preds = %bb5.i.i.i
  %_7.i.i.i = icmp eq i64 %32, %_11
  br i1 %_7.i.i.i, label %bb4.i4, label %bb5.i.i.i

bb5.i.i.i:                                        ; preds = %bb9, %bb6.i.i.i
  %_3.sroa.0.0.i.i.i185 = phi i64 [ %32, %bb6.i.i.i ], [ 0, %bb9 ]
  %_6.i.i.i = getelementptr inbounds nuw [72 x i8], ptr %_1.val.i, i64 %_3.sroa.0.0.i.i.i185
  %32 = add nuw nsw i64 %_3.sroa.0.0.i.i.i185, 1
; invoke core::ptr::drop_glue::<lin::ast::Pred>
  invoke fastcc void @_RINvNtCsaLbZTW5namK_4core3ptr9drop_glueNtNtCsagrXUsJVDRq_3lin3ast4PredEBF_(ptr noalias nofree noundef align 8 dereferenceable(72) %_6.i.i.i)
          to label %bb6.i.i.i unwind label %cleanup.i.i.i, !noalias !249606

bb4.i.i.i:                                        ; preds = %bb3.i.i.i
  %33 = add i64 %_3.sroa.0.1.i.i.i187, 1
  %_5.i.i.i2 = icmp eq i64 %33, %_11
  br i1 %_5.i.i.i2, label %cleanup.body.i, label %bb3.i.i.i

cleanup.i.i.i:                                    ; preds = %bb5.i.i.i
  %34 = landingpad { ptr, i32 }
          cleanup
  %_5.i.i.i2186 = icmp eq i64 %32, %_11
  br i1 %_5.i.i.i2186, label %cleanup.body.i, label %bb3.i.i.i

bb3.i.i.i:                                        ; preds = %cleanup.i.i.i, %bb4.i.i.i
  %_3.sroa.0.1.i.i.i187 = phi i64 [ %33, %bb4.i.i.i ], [ %32, %cleanup.i.i.i ]
  %_4.i.i.i = getelementptr inbounds nuw [72 x i8], ptr %_1.val.i, i64 %_3.sroa.0.1.i.i.i187
; invoke core::ptr::drop_glue::<lin::ast::Pred>
  invoke fastcc void @_RINvNtCsaLbZTW5namK_4core3ptr9drop_glueNtNtCsagrXUsJVDRq_3lin3ast4PredEBF_(ptr noalias nofree noundef align 8 dereferenceable(72) %_4.i.i.i) #87
          to label %bb4.i.i.i unwind label %terminate.i.i.i, !noalias !249606

terminate.i.i.i:                                  ; preds = %bb3.i.i.i
  %35 = landingpad { ptr, i32 }
          filter [0 x ptr] zeroinitializer
; call core::panicking::panic_in_cleanup
  tail call void @_RNvNtCsaLbZTW5namK_4core9panicking16panic_in_cleanup() #88, !noalias !249606
  unreachable

cleanup.body.i:                                   ; preds = %bb4.i.i.i, %cleanup.i.i.i
  %_1.val2.i = load i64, ptr %branches, align 8, !range !2805, !alias.scope !249606, !noundef !2388
  %36 = icmp eq i64 %_1.val2.i, 0
  br i1 %36, label %common.resume, label %common.resume.sink.split

bb4.i4:                                           ; preds = %bb6.i.i.i, %bb9
  %_1.val4.i = load i64, ptr %branches, align 8, !range !2805, !alias.scope !249606, !noundef !2388
  %37 = icmp eq i64 %_1.val4.i, 0
  br i1 %37, label %_RINvNtCsaLbZTW5namK_4core3ptr9drop_glueINtNtCsj8lNtgb2Igp_5alloc3vec3VecNtNtCsagrXUsJVDRq_3lin3ast4PredEEB1c_.exit, label %bb2.i.i.i6.i

bb2.i.i.i6.i:                                     ; preds = %bb4.i4
  %alloc_size.i.i.i.i7.i = mul nuw i64 %_1.val4.i, 72
; call __rustc::__rust_dealloc
  tail call void @_RNvCsfmtydf4BlqR_7___rustc14___rust_dealloc(ptr noundef nonnull %_1.val.i, i64 noundef %alloc_size.i.i.i.i7.i, i64 noundef range(i64 1, -9223372036854775807) 8) #86, !noalias !249606
  br label %_RINvNtCsaLbZTW5namK_4core3ptr9drop_glueINtNtCsj8lNtgb2Igp_5alloc3vec3VecNtNtCsagrXUsJVDRq_3lin3ast4PredEEB1c_.exit

common.resume.sink.split:                         ; preds = %cleanup.body.i, %cleanup.body.i22
  %_1.val2.i23.sink = phi i64 [ %_1.val2.i23, %cleanup.body.i22 ], [ %_1.val2.i, %cleanup.body.i ]
  %_20.sink = phi ptr [ %_20, %cleanup.body.i22 ], [ %_1.val.i, %cleanup.body.i ]
  %common.resume.op.ph = phi { ptr, i32 } [ %100, %cleanup.body.i22 ], [ %34, %cleanup.body.i ]
  %alloc_size.i.i.i.i.i25 = mul nuw i64 %_1.val2.i23.sink, 72
; call __rustc::__rust_dealloc
  tail call void @_RNvCsfmtydf4BlqR_7___rustc14___rust_dealloc(ptr noundef nonnull %_20.sink, i64 noundef %alloc_size.i.i.i.i.i25, i64 noundef range(i64 1, -9223372036854775807) 8) #86, !noalias !2388
  br label %common.resume

common.resume:                                    ; preds = %common.resume.sink.split, %cleanup.body, %cleanup.body.i22, %cleanup.body.i
  %common.resume.op = phi { ptr, i32 } [ %100, %cleanup.body.i22 ], [ %34, %cleanup.body.i ], [ %eh.lpad-body, %cleanup.body ], [ %common.resume.op.ph, %common.resume.sink.split ]
  resume { ptr, i32 } %common.resume.op

_RINvNtCsaLbZTW5namK_4core3ptr9drop_glueINtNtCsj8lNtgb2Igp_5alloc3vec3VecNtNtCsagrXUsJVDRq_3lin3ast4PredEEB1c_.exit: ; preds = %bb4.i4, %bb2.i.i.i6.i
  call void @llvm.lifetime.end.p0(ptr nonnull %branches)
  br label %bb14

cleanup:                                          ; preds = %bb6.i.i.i.i.i
  %38 = landingpad { ptr, i32 }
          cleanup
  br label %cleanup.body

cleanup.body:                                     ; preds = %cleanup.i.i.i.i.i, %bb2.i.i.i4.i.i.i.i.i.i, %bb50.i.i.i.i, %bb2.i.i.i4.i.i.i.i.i, %cleanup
  %eh.lpad-body = phi { ptr, i32 } [ %38, %cleanup ], [ %54, %cleanup.i.i.i.i.i ], [ %54, %bb2.i.i.i4.i.i.i.i.i.i ], [ %.pn.i.i.i.i, %bb2.i.i.i4.i.i.i.i.i ], [ %.pn.i.i.i.i, %bb50.i.i.i.i ]
; invoke core::ptr::drop_glue::<alloc::vec::Vec<lin::ast::Pred>>
  invoke fastcc void @_RINvNtCsaLbZTW5namK_4core3ptr9drop_glueINtNtCsj8lNtgb2Igp_5alloc3vec3VecNtNtCsagrXUsJVDRq_3lin3ast4PredEEB1c_(ptr noalias nofree noundef align 8 dereferenceable(24) %branches) #87
          to label %common.resume unwind label %terminate

bb11:                                             ; preds = %bb7
  %39 = getelementptr inbounds nuw i8, ptr %branches, i64 8
  %_20 = load ptr, ptr %39, align 8, !nonnull !2388, !noundef !2388
  %_2.i.i60.i.not = icmp eq i64 %uses.1, 0
  br i1 %_2.i.i60.i.not, label %bb18, label %bb3.lr.ph.i

bb3.lr.ph.i:                                      ; preds = %bb11
  %40 = getelementptr inbounds nuw i8, ptr %v.i.i.i.i.i, i64 8
  %41 = getelementptr inbounds nuw i8, ptr %v.i.i.i.i.i, i64 16
  %42 = getelementptr inbounds nuw i8, ptr %v.i20.i.i.i.i, i64 8
  %43 = getelementptr inbounds nuw i8, ptr %v.i20.i.i.i.i, i64 16
  %44 = getelementptr inbounds nuw i8, ptr %_51.i.i.i.i, i64 16
  %45 = getelementptr inbounds nuw i8, ptr %_51.i.i.i.i, i64 8
  %46 = getelementptr inbounds nuw i8, ptr %_6.i.i.i.i.i.i, i64 16
  %47 = getelementptr inbounds nuw i8, ptr %_6.i.i.i.i.i.i, i64 8
  br label %bb3.i

bb3.i:                                            ; preds = %bb1.backedge.i, %bb3.lr.ph.i
  %48 = phi i64 [ 0, %bb3.lr.ph.i ], [ %49, %bb1.backedge.i ]
  %49 = add nuw nsw i64 %48, 1
  %_3.i.i.i.i = getelementptr inbounds nuw [72 x i8], ptr %_20, i64 %48
  %_3.i1.i.i.i = getelementptr inbounds nuw [232 x i8], ptr %uses.0, i64 %48
  tail call void @llvm.experimental.noalias.scope.decl(metadata !249609)
  tail call void @llvm.experimental.noalias.scope.decl(metadata !249612)
  tail call void @llvm.experimental.noalias.scope.decl(metadata !249615)
  %50 = getelementptr inbounds nuw i8, ptr %_3.i1.i.i.i, i64 72
  %51 = load i64, ptr %50, align 8, !range !41402, !alias.scope !249618, !noalias !249619, !noundef !2388
  %.not.i.i.i.i = icmp eq i64 %51, -1
  br i1 %.not.i.i.i.i, label %bb55.i.i.i.i, label %bb53.i.i.i.i

bb53.i.i.i.i:                                     ; preds = %bb3.i
  call void @llvm.lifetime.start.p0(ptr nonnull %v.i.i.i.i.i), !noalias !249625
; call __rustc::__rust_no_alloc_shim_is_unstable_v2
  tail call void @_RNvCsfmtydf4BlqR_7___rustc35___rust_no_alloc_shim_is_unstable_v2() #86, !noalias !249629
; call __rustc::__rust_alloc
  %52 = tail call noundef align 8 dereferenceable_or_null(32) ptr @_RNvCsfmtydf4BlqR_7___rustc12___rust_alloc(i64 noundef 32, i64 noundef range(i64 1, -9223372036854775807) 8) #86, !noalias !249629
  %53 = icmp eq ptr %52, null
  br i1 %53, label %bb6.i.i.i.i.i, label %bb7.i.i.i.i.i

bb6.i.i.i.i.i:                                    ; preds = %bb53.i.i.i.i
; invoke alloc::raw_vec::handle_error
  invoke void @_RNvNtCsj8lNtgb2Igp_5alloc7raw_vec12handle_error(i64 noundef 8, i64 32) #85
          to label %.noexc unwind label %cleanup

.noexc:                                           ; preds = %bb6.i.i.i.i.i
  unreachable

bb7.i.i.i.i.i:                                    ; preds = %bb53.i.i.i.i
  store i64 4, ptr %v.i.i.i.i.i, align 8, !noalias !249625
  store ptr %52, ptr %40, align 8, !noalias !249625
  store i64 0, ptr %41, align 8, !noalias !249625
; invoke lin::index::flatten_and_into
  invoke fastcc void @_RNvNtCsagrXUsJVDRq_3lin5index16flatten_and_into(ptr noalias nofree noundef nonnull readonly align 8 captures(address, read_provenance) dereferenceable(72) %50, ptr noalias nofree noundef align 8 dereferenceable(24) %v.i.i.i.i.i)
          to label %_RNvNtCsagrXUsJVDRq_3lin5index11flatten_and.exit.i.i.i.i unwind label %cleanup.i.i.i.i.i, !noalias !249632

cleanup.i.i.i.i.i:                                ; preds = %bb7.i.i.i.i.i
  %54 = landingpad { ptr, i32 }
          cleanup
  %v.val.i.i.i.i.i = load i64, ptr %v.i.i.i.i.i, align 8, !noalias !249625
  %55 = icmp eq i64 %v.val.i.i.i.i.i, 0
  br i1 %55, label %cleanup.body, label %bb2.i.i.i4.i.i.i.i.i.i

bb2.i.i.i4.i.i.i.i.i.i:                           ; preds = %cleanup.i.i.i.i.i
  %v.val1.i.i.i.i.i = load ptr, ptr %40, align 8, !noalias !249625, !nonnull !2388, !noundef !2388
  %alloc_size.i.i.i.i5.i.i.i.i.i.i = shl nuw i64 %v.val.i.i.i.i.i, 3
; call __rustc::__rust_dealloc
  tail call void @_RNvCsfmtydf4BlqR_7___rustc14___rust_dealloc(ptr noundef nonnull %v.val1.i.i.i.i.i, i64 noundef %alloc_size.i.i.i.i5.i.i.i.i.i.i, i64 noundef range(i64 1, -9223372036854775807) 8) #86, !noalias !249632
  br label %cleanup.body

_RNvNtCsagrXUsJVDRq_3lin5index11flatten_and.exit.i.i.i.i: ; preds = %bb7.i.i.i.i.i
  %_57.sroa.0.0.copyload.i.i.i.i = load i64, ptr %v.i.i.i.i.i, align 8, !noalias !249633
  %_57.sroa.4.0.copyload.i.i.i.i = load ptr, ptr %40, align 8, !noalias !249633
  %_57.sroa.5.0.copyload.i.i.i.i = load i64, ptr %41, align 8, !noalias !249633
  call void @llvm.lifetime.end.p0(ptr nonnull %v.i.i.i.i.i), !noalias !249625
  %56 = shl nuw nsw i64 %_57.sroa.5.0.copyload.i.i.i.i, 3
  br label %bb55.i.i.i.i

bb55.i.i.i.i:                                     ; preds = %_RNvNtCsagrXUsJVDRq_3lin5index11flatten_and.exit.i.i.i.i, %bb3.i
  %pred_atoms.sroa.13.0.i.i.i.i = phi i64 [ %56, %_RNvNtCsagrXUsJVDRq_3lin5index11flatten_and.exit.i.i.i.i ], [ 0, %bb3.i ]
  %pred_atoms.sroa.8.0.i.i.i.i = phi ptr [ %_57.sroa.4.0.copyload.i.i.i.i, %_RNvNtCsagrXUsJVDRq_3lin5index11flatten_and.exit.i.i.i.i ], [ inttoptr (i64 8 to ptr), %bb3.i ]
  %pred_atoms.sroa.0.0.i.i.i.i = phi i64 [ %_57.sroa.0.0.copyload.i.i.i.i, %_RNvNtCsagrXUsJVDRq_3lin5index11flatten_and.exit.i.i.i.i ], [ 0, %bb3.i ]
  call void @llvm.lifetime.start.p0(ptr nonnull %v.i20.i.i.i.i), !noalias !249634
; call __rustc::__rust_no_alloc_shim_is_unstable_v2
  tail call void @_RNvCsfmtydf4BlqR_7___rustc35___rust_no_alloc_shim_is_unstable_v2() #86, !noalias !249638
; call __rustc::__rust_alloc
  %57 = tail call noundef align 8 dereferenceable_or_null(32) ptr @_RNvCsfmtydf4BlqR_7___rustc12___rust_alloc(i64 noundef 32, i64 noundef range(i64 1, -9223372036854775807) 8) #86, !noalias !249638
  %58 = icmp eq ptr %57, null
  br i1 %58, label %bb6.i28.i.i.i.i, label %bb7.i21.i.i.i.i

bb6.i28.i.i.i.i:                                  ; preds = %bb55.i.i.i.i
; invoke alloc::raw_vec::handle_error
  invoke void @_RNvNtCsj8lNtgb2Igp_5alloc7raw_vec12handle_error(i64 noundef 8, i64 32) #85
          to label %.noexc.i.i.i.i unwind label %cleanup.i.i.i.i, !noalias !249641

.noexc.i.i.i.i:                                   ; preds = %bb6.i28.i.i.i.i
  unreachable

bb7.i21.i.i.i.i:                                  ; preds = %bb55.i.i.i.i
  store i64 4, ptr %v.i20.i.i.i.i, align 8, !noalias !249634
  store ptr %57, ptr %42, align 8, !noalias !249634
  store i64 0, ptr %43, align 8, !noalias !249634
; invoke lin::index::flatten_and_into
  invoke fastcc void @_RNvNtCsagrXUsJVDRq_3lin5index16flatten_and_into(ptr noalias nofree noundef nonnull readonly align 8 captures(address, read_provenance) dereferenceable(72) %_3.i.i.i.i, ptr noalias nofree noundef align 8 dereferenceable(24) %v.i20.i.i.i.i)
          to label %bb1.i.i.i.i unwind label %cleanup.i22.i.i.i.i, !noalias !249642

cleanup.i22.i.i.i.i:                              ; preds = %bb7.i21.i.i.i.i
  %59 = landingpad { ptr, i32 }
          cleanup
  %v.val.i23.i.i.i.i = load i64, ptr %v.i20.i.i.i.i, align 8, !noalias !249634
  %60 = icmp eq i64 %v.val.i23.i.i.i.i, 0
  br i1 %60, label %bb50.i.i.i.i, label %bb2.i.i.i4.i.i24.i.i.i.i

bb2.i.i.i4.i.i24.i.i.i.i:                         ; preds = %cleanup.i22.i.i.i.i
  %v.val1.i25.i.i.i.i = load ptr, ptr %42, align 8, !noalias !249634, !nonnull !2388, !noundef !2388
  %alloc_size.i.i.i.i5.i.i26.i.i.i.i = shl nuw i64 %v.val.i23.i.i.i.i, 3
; call __rustc::__rust_dealloc
  tail call void @_RNvCsfmtydf4BlqR_7___rustc14___rust_dealloc(ptr noundef nonnull %v.val1.i25.i.i.i.i, i64 noundef %alloc_size.i.i.i.i5.i.i26.i.i.i.i, i64 noundef range(i64 1, -9223372036854775807) 8) #86, !noalias !249642
  br label %bb50.i.i.i.i

bb50.i.i.i.i:                                     ; preds = %bb2.i.i.i.i.i4.i.i.i.i.i.i, %cleanup1.i.i.i.i, %cleanup.i.i.i.i, %bb2.i.i.i4.i.i24.i.i.i.i, %cleanup.i22.i.i.i.i
  %.pn.i.i.i.i = phi { ptr, i32 } [ %59, %cleanup.i22.i.i.i.i ], [ %62, %cleanup.i.i.i.i ], [ %59, %bb2.i.i.i4.i.i24.i.i.i.i ], [ %lpad.phi.i.i.i.i, %cleanup1.i.i.i.i ], [ %lpad.phi.i.i.i.i, %bb2.i.i.i.i.i4.i.i.i.i.i.i ]
  %61 = icmp eq i64 %pred_atoms.sroa.0.0.i.i.i.i, 0
  br i1 %61, label %cleanup.body, label %bb2.i.i.i4.i.i.i.i.i

bb2.i.i.i4.i.i.i.i.i:                             ; preds = %bb50.i.i.i.i
  %alloc_size.i.i.i.i5.i.i.i.i.i = shl nuw i64 %pred_atoms.sroa.0.0.i.i.i.i, 3
  call void @llvm.assume(i1 true) [ "nonnull"(ptr %pred_atoms.sroa.8.0.i.i.i.i) ]
; call __rustc::__rust_dealloc
  tail call void @_RNvCsfmtydf4BlqR_7___rustc14___rust_dealloc(ptr noundef nonnull %pred_atoms.sroa.8.0.i.i.i.i, i64 noundef %alloc_size.i.i.i.i5.i.i.i.i.i, i64 noundef range(i64 1, -9223372036854775807) 8) #86, !noalias !249641
  br label %cleanup.body

cleanup.i.i.i.i:                                  ; preds = %bb6.i28.i.i.i.i
  %62 = landingpad { ptr, i32 }
          cleanup
  br label %bb50.i.i.i.i

bb1.i.i.i.i:                                      ; preds = %bb7.i21.i.i.i.i
  %_7.sroa.0.0.copyload.i.i.i.i = load i64, ptr %v.i20.i.i.i.i, align 8, !noalias !249643
  %_7.sroa.4.0.copyload.i.i.i.i = load ptr, ptr %42, align 8, !noalias !249643, !nonnull !2388, !noundef !2388
  %_7.sroa.5.0.copyload.i.i.i.i = load i64, ptr %43, align 8, !noalias !249643
  call void @llvm.lifetime.end.p0(ptr nonnull %v.i20.i.i.i.i), !noalias !249634
  %_63.i.i.i.i = icmp ult i64 %_7.sroa.5.0.copyload.i.i.i.i, 1152921504606846976
  tail call void @llvm.assume(i1 %_63.i.i.i.i)
  %_61.idx.i.i.i.i = shl nuw nsw i64 %_7.sroa.5.0.copyload.i.i.i.i, 3
  %_61.i.i.i.i = getelementptr inbounds nuw i8, ptr %_7.sroa.4.0.copyload.i.i.i.i, i64 %_61.idx.i.i.i.i
  %_9.i64.i.i.i.i = icmp eq i64 %_7.sroa.5.0.copyload.i.i.i.i, 0
  br i1 %_9.i64.i.i.i.i, label %bb6.i.i.i.i, label %bb5.lr.ph.i.i.i.i

bb5.lr.ph.i.i.i.i:                                ; preds = %bb1.i.i.i.i
  %_10.i.i.i.i.i = getelementptr inbounds nuw i8, ptr %pred_atoms.sroa.8.0.i.i.i.i, i64 %pred_atoms.sroa.13.0.i.i.i.i
  %63 = getelementptr inbounds nuw i8, ptr %_3.i1.i.i.i, i64 176
  %64 = load i64, ptr %63, align 8, !range !3853, !alias.scope !249618, !noalias !249619
  %.not6.i.i.i.i = icmp eq i64 %64, -1
  %rv.i.i.i.i = getelementptr inbounds nuw i8, ptr %_3.i1.i.i.i, i64 208
  %65 = getelementptr inbounds nuw i8, ptr %_3.i1.i.i.i, i64 192
  %_85.i.i.i.i = load i64, ptr %65, align 8, !alias.scope !249618, !noalias !249619
  %66 = getelementptr inbounds nuw i8, ptr %_3.i1.i.i.i, i64 184
  %_86.i.i.i.i = load ptr, ptr %66, align 8, !alias.scope !249618, !noalias !249619, !nonnull !2388
  %67 = getelementptr inbounds nuw i8, ptr %_3.i1.i.i.i, i64 200
  %68 = load i8, ptr %67, align 8, !range !2751, !alias.scope !249618, !noalias !249619
  %69 = getelementptr inbounds nuw i8, ptr %_3.i1.i.i.i, i64 160
  %_74.i.i.i.i = load ptr, ptr %69, align 8, !alias.scope !249618, !noalias !249619, !nonnull !2388
  %70 = getelementptr inbounds nuw i8, ptr %_3.i1.i.i.i, i64 168
  %_73.i.i.i.i = load i64, ptr %70, align 8, !alias.scope !249618, !noalias !249619
  %_78.idx.i.i.i.i = mul nuw nsw i64 %_73.i.i.i.i, 48
  %_78.i.i.i.i = getelementptr inbounds nuw i8, ptr %_74.i.i.i.i, i64 %_78.idx.i.i.i.i
  %_123.not.i.i.i.i.i = icmp eq i64 %_73.i.i.i.i, 0
  %_12.not.not.not.i.not.not.not.i.not.i.i.i.i188 = icmp samesign eq i64 %pred_atoms.sroa.13.0.i.i.i.i, 0
  br label %bb5.i.i.i.i

cleanup1.loopexit.i.i.i.i:                        ; preds = %bb13.i.i.i.i.i
  %lpad.loopexit.i.i.i.i = landingpad { ptr, i32 }
          cleanup
  br label %cleanup1.i.i.i.i

cleanup1.loopexit.split-lp.i.i.i.i:               ; preds = %bb30.i.i.i.i
  %lpad.loopexit.split-lp.i.i.i.i = landingpad { ptr, i32 }
          cleanup
  br label %cleanup1.i.i.i.i

cleanup1.i.i.i.i:                                 ; preds = %cleanup1.loopexit.split-lp.i.i.i.i, %cleanup1.loopexit.i.i.i.i
  %lpad.phi.i.i.i.i = phi { ptr, i32 } [ %lpad.loopexit.i.i.i.i, %cleanup1.loopexit.i.i.i.i ], [ %lpad.loopexit.split-lp.i.i.i.i, %cleanup1.loopexit.split-lp.i.i.i.i ]
  %71 = icmp eq i64 %_7.sroa.0.0.copyload.i.i.i.i, 0
  br i1 %71, label %bb50.i.i.i.i, label %bb2.i.i.i.i.i4.i.i.i.i.i.i

bb2.i.i.i.i.i4.i.i.i.i.i.i:                       ; preds = %cleanup1.i.i.i.i
  %alloc_size.i.i.i.i.i.i6.i.i.i.i.i.i = shl nuw i64 %_7.sroa.0.0.copyload.i.i.i.i, 3
; call __rustc::__rust_dealloc
  tail call void @_RNvCsfmtydf4BlqR_7___rustc14___rust_dealloc(ptr noundef nonnull %_7.sroa.4.0.copyload.i.i.i.i, i64 noundef %alloc_size.i.i.i.i.i.i6.i.i.i.i.i.i, i64 noundef range(i64 1, -9223372036854775807) 8) #86, !noalias !249644
  br label %bb50.i.i.i.i

bb5.i.i.i.i:                                      ; preds = %bb2.backedge.i.i.i.i, %bb5.lr.ph.i.i.i.i
  %iter.sroa.7.065.i.i.i.i = phi ptr [ %_7.sroa.4.0.copyload.i.i.i.i, %bb5.lr.ph.i.i.i.i ], [ %_23.i.i.i.i.i, %bb2.backedge.i.i.i.i ]
  %_23.i.i.i.i.i = getelementptr inbounds nuw i8, ptr %iter.sroa.7.065.i.i.i.i, i64 8
  %_17.i.i.i.i.i = load ptr, ptr %iter.sroa.7.065.i.i.i.i, align 8, !noalias !249647, !nonnull !2388, !align !2828, !noundef !2388
  tail call void @llvm.experimental.noalias.scope.decl(metadata !249650)
  br i1 %_12.not.not.not.i.not.not.not.i.not.i.i.i.i188, label %bb8.i.i.i.i, label %bb13.i.i.i.i.i.i

bb1.i.i.i.i.i.i:                                  ; preds = %bb13.i.i.i.i.i.i
  %_22.i.i.i.i.i.i = getelementptr inbounds nuw i8, ptr %_221.i.i.i.i.i.i189, i64 8
  %_12.not.not.not.i.not.not.not.i.not.i.i.i.i = icmp eq ptr %_22.i.i.i.i.i.i, %_10.i.i.i.i.i
  br i1 %_12.not.not.not.i.not.not.not.i.not.i.i.i.i, label %bb8.i.i.i.i, label %bb13.i.i.i.i.i.i

bb13.i.i.i.i.i.i:                                 ; preds = %bb5.i.i.i.i, %bb1.i.i.i.i.i.i
  %_221.i.i.i.i.i.i189 = phi ptr [ %_22.i.i.i.i.i.i, %bb1.i.i.i.i.i.i ], [ %pred_atoms.sroa.8.0.i.i.i.i, %bb5.i.i.i.i ]
  %ptr.val.i.i.i.i.i.i = load ptr, ptr %_221.i.i.i.i.i.i189, align 8, !alias.scope !249650, !noalias !249653, !nonnull !2388, !align !2828, !noundef !2388
; call <lin::ast::Pred as core::cmp::PartialEq>::eq
  %_0.i.i.i.i.i.i.i.i = tail call fastcc noundef zeroext i1 @_RNvXs18_NtCsagrXUsJVDRq_3lin3astNtB6_4PredNtNtCsaLbZTW5namK_4core3cmp9PartialEq2eq(ptr noalias nofree noundef nonnull readonly align 8 captures(address, read_provenance) dereferenceable(72) %ptr.val.i.i.i.i.i.i, ptr noalias nofree noundef nonnull readonly align 8 captures(address, read_provenance) dereferenceable(72) %_17.i.i.i.i.i), !noalias !249657
  br i1 %_0.i.i.i.i.i.i.i.i, label %bb2.backedge.i.i.i.i, label %bb1.i.i.i.i.i.i

bb6.i.i.i.i:                                      ; preds = %bb2.backedge.i.i.i.i, %bb1.i.i.i.i
  %72 = icmp eq i64 %_7.sroa.0.0.copyload.i.i.i.i, 0
  br i1 %72, label %bb41.i.i.i.i, label %bb2.i.i.i.i.i4.i.i31.i.i.i.i

bb2.i.i.i.i.i4.i.i31.i.i.i.i:                     ; preds = %bb6.i.i.i.i
  %alloc_size.i.i.i.i.i.i6.i.i32.i.i.i.i = shl nuw i64 %_7.sroa.0.0.copyload.i.i.i.i, 3
; call __rustc::__rust_dealloc
  tail call void @_RNvCsfmtydf4BlqR_7___rustc14___rust_dealloc(ptr noundef nonnull %_7.sroa.4.0.copyload.i.i.i.i, i64 noundef %alloc_size.i.i.i.i.i.i6.i.i32.i.i.i.i, i64 noundef range(i64 1, -9223372036854775807) 8) #86, !noalias !249658
  br label %bb41.i.i.i.i

bb41.i.i.i.i:                                     ; preds = %bb2.i.i.i.i.i4.i.i31.i.i.i.i, %bb6.i.i.i.i
  %73 = icmp eq i64 %pred_atoms.sroa.0.0.i.i.i.i, 0
  br i1 %73, label %bb1.backedge.i, label %_RNCNvNtCsagrXUsJVDRq_3lin5index17index_covers_pred0B5_.exit.thread.sink.split.i.i

bb8.i.i.i.i:                                      ; preds = %bb1.i.i.i.i.i.i, %bb5.i.i.i.i
  %74 = load i64, ptr %_17.i.i.i.i.i, align 8, !range !41674, !noalias !249641, !noundef !2388
  %75 = icmp eq i64 %74, -9223372036854775805
  br i1 %75, label %bb10.i.i.i.i, label %bb45.i.i.i.i

bb10.i.i.i.i:                                     ; preds = %bb8.i.i.i.i
  %76 = getelementptr inbounds nuw i8, ptr %_17.i.i.i.i.i, i64 56
  %77 = load i8, ptr %76, align 8, !range !2751, !noalias !249641, !noundef !2388
  %78 = icmp eq i8 %77, 0
  %79 = getelementptr inbounds nuw i8, ptr %_17.i.i.i.i.i, i64 32
  br i1 %78, label %bb12.i.i.i.i, label %bb11.i.i.i.i

bb12.i.i.i.i:                                     ; preds = %bb10.i.i.i.i
  %80 = load i64, ptr %79, align 8, !range !41674, !noalias !249641, !noundef !2388
  switch i64 %80, label %bb17.i.i.i.i [
    i64 -9223372036854775807, label %bb14.i.i.i.i
    i64 -9223372036854775806, label %bb18.i.i.i.i
  ]

bb11.i.i.i.i:                                     ; preds = %bb10.i.i.i.i
  %switch.i.i.i.i = icmp eq i8 %77, 1
  %or.cond9.i.i.i.i = select i1 %switch.i.i.i.i, i1 true, i1 %.not6.i.i.i.i
  br i1 %or.cond9.i.i.i.i, label %bb45.i.i.i.i, label %bb30.i.i.i.i

bb14.i.i.i.i:                                     ; preds = %bb12.i.i.i.i
  %81 = getelementptr inbounds nuw i8, ptr %_17.i.i.i.i.i, i64 40
  %_25.i.i.i.i = load i64, ptr %81, align 8, !noalias !249641, !noundef !2388
  %82 = add i64 %_25.i.i.i.i, -9007199254740992
  %_23.i.i.i.i = icmp ult i64 %82, -18014398509481983
  br i1 %_23.i.i.i.i, label %bb45.i.i.i.i, label %bb17.i.i.i.i

bb18.i.i.i.i:                                     ; preds = %bb12.i.i.i.i
  %83 = getelementptr inbounds nuw i8, ptr %_17.i.i.i.i.i, i64 40
  %_30.i.i.i.i = load double, ptr %83, align 8, !noalias !249641, !noundef !2388
  %_70.i.i.i.i = tail call double @llvm.fabs.f64(double %_30.i.i.i.i)
  %_29.i.i.i.i = fcmp one double %_70.i.i.i.i, +inf
  %_31.i.i.i.i = fcmp ult double %_70.i.i.i.i, f0x4340000000000000
  %or.cond.i.i.i.i = and i1 %_29.i.i.i.i, %_31.i.i.i.i
  br i1 %or.cond.i.i.i.i, label %bb17.i.i.i.i, label %bb45.i.i.i.i

bb17.i.i.i.i:                                     ; preds = %bb18.i.i.i.i, %bb14.i.i.i.i, %bb12.i.i.i.i
  tail call void @llvm.experimental.noalias.scope.decl(metadata !249661)
  br i1 %_123.not.i.i.i.i.i, label %bb45.i.i.i.i, label %bb13.lr.ph.i.i.i.i.i

bb13.lr.ph.i.i.i.i.i:                             ; preds = %bb17.i.i.i.i
  %84 = getelementptr inbounds nuw i8, ptr %_17.i.i.i.i.i, i64 16
  %_6.i.i.i.i.i.i.i = load ptr, ptr %84, align 8, !alias.scope !249664, !noalias !249667, !nonnull !2388, !noundef !2388
  %85 = getelementptr inbounds nuw i8, ptr %_17.i.i.i.i.i, i64 24
  %_5.i.i.i.i.i.i.i = load i64, ptr %85, align 8, !alias.scope !249664, !noalias !249667, !noundef !2388
  br label %bb13.i.i.i.i.i

bb13.i.i.i.i.i:                                   ; preds = %bb1.backedge.i.i.i.i.i, %bb13.lr.ph.i.i.i.i.i
  %_2224.i.i.i.i.i = phi ptr [ %_74.i.i.i.i, %bb13.lr.ph.i.i.i.i.i ], [ %_22.i.i.i.i.i, %bb1.backedge.i.i.i.i.i ]
  %_22.i.i.i.i.i = getelementptr inbounds nuw i8, ptr %_2224.i.i.i.i.i, i64 48
  tail call void @llvm.experimental.noalias.scope.decl(metadata !249673)
  %v.i.i.i.i.i.i = getelementptr inbounds nuw i8, ptr %_2224.i.i.i.i.i, i64 24
  call void @llvm.lifetime.start.p0(ptr nonnull %_6.i.i.i.i.i.i), !noalias !249674
; invoke alloc::str::join_generic_copy::<str, u8, alloc::string::String>
  invoke fastcc void @_RINvNtCsj8lNtgb2Igp_5alloc3str17join_generic_copyehNtNtB4_6string6StringECsagrXUsJVDRq_3lin(ptr noalias nofree noundef align 8 captures(none) dereferenceable(24) %_6.i.i.i.i.i.i, ptr noalias nofree noundef nonnull readonly align 8 captures(address, read_provenance) %_6.i.i.i.i.i.i.i, i64 noundef %_5.i.i.i.i.i.i.i, ptr noalias nofree noundef nonnull readonly captures(address, read_provenance) @alloc_0f52f98e9fa44caf6668aeb0b1c94ca5, i64 noundef 1)
          to label %.noexc37.i.i.i.i unwind label %cleanup1.loopexit.i.i.i.i, !noalias !249641

.noexc37.i.i.i.i:                                 ; preds = %bb13.i.i.i.i.i
  %86 = getelementptr inbounds nuw i8, ptr %_2224.i.i.i.i.i, i64 16
  %_10.i.i.i.i.i.i = load i64, ptr %86, align 8, !alias.scope !249673, !noalias !249675, !noundef !2388
  %_13.i.i.i.i.i.i = load i64, ptr %46, align 8, !noalias !249674, !noundef !2388
  %_15.i.i.i.i.i.i = icmp eq i64 %_10.i.i.i.i.i.i, %_13.i.i.i.i.i.i
  br i1 %_15.i.i.i.i.i.i, label %bb7.i.i.i.i.i.i, label %bb4.i.i.i.i.i.i

bb7.i.i.i.i.i.i:                                  ; preds = %.noexc37.i.i.i.i
  %_14.i.i.i.i.i.i = load ptr, ptr %47, align 8, !noalias !249674, !nonnull !2388, !noundef !2388
  %87 = getelementptr inbounds nuw i8, ptr %_2224.i.i.i.i.i, i64 8
  %_11.i.i.i.i.i.i = load ptr, ptr %87, align 8, !alias.scope !249673, !noalias !249675, !nonnull !2388, !noundef !2388
  %_16.i.i.i.i.i.i = tail call i32 @memcmp(ptr nonnull %_11.i.i.i.i.i.i, ptr nonnull %_14.i.i.i.i.i.i, i64 %_10.i.i.i.i.i.i), !noalias !249676
  %_5.i.i.i.i.i.i = icmp eq i32 %_16.i.i.i.i.i.i, 0
  br i1 %_5.i.i.i.i.i.i, label %bb2.i.i.i.i.i.i, label %bb4.i.i.i.i.i.i

bb4.i.i.i.i.i.i:                                  ; preds = %bb7.i.i.i.i.i.i, %.noexc37.i.i.i.i
  tail call void @llvm.experimental.noalias.scope.decl(metadata !249677)
  tail call void @llvm.experimental.noalias.scope.decl(metadata !249680)
  %_1.val2.i.i.i.i.i.i.i.i = load i64, ptr %_6.i.i.i.i.i.i, align 8, !range !2805, !alias.scope !249683, !noalias !249674, !noundef !2388
  %88 = icmp eq i64 %_1.val2.i.i.i.i.i.i.i.i, 0
  br i1 %88, label %_RNCNvNtCsagrXUsJVDRq_3lin5index21index_covers_conjunct0B5_.exit.i.i.i.i.i, label %bb2.i.i.i4.i.i.i.i.i.i.i.i

bb2.i.i.i4.i.i.i.i.i.i.i.i:                       ; preds = %bb4.i.i.i.i.i.i
  %_1.val3.i.i.i.i.i.i.i.i = load ptr, ptr %47, align 8, !alias.scope !249683, !noalias !249674, !nonnull !2388, !noundef !2388
; call __rustc::__rust_dealloc
  tail call void @_RNvCsfmtydf4BlqR_7___rustc14___rust_dealloc(ptr noundef nonnull %_1.val3.i.i.i.i.i.i.i.i, i64 noundef %_1.val2.i.i.i.i.i.i.i.i, i64 noundef range(i64 1, -9223372036854775807) 1) #86, !noalias !249684
  br label %_RNCNvNtCsagrXUsJVDRq_3lin5index21index_covers_conjunct0B5_.exit.i.i.i.i.i

bb2.i.i.i.i.i.i:                                  ; preds = %bb7.i.i.i.i.i.i
  tail call void @llvm.experimental.noalias.scope.decl(metadata !249685)
  tail call void @llvm.experimental.noalias.scope.decl(metadata !249688)
  %_1.val2.i.i1.i.i.i.i.i.i = load i64, ptr %_6.i.i.i.i.i.i, align 8, !range !2805, !alias.scope !249691, !noalias !249674, !noundef !2388
  %89 = icmp eq i64 %_1.val2.i.i1.i.i.i.i.i.i, 0
  br i1 %89, label %_RINvNtCsaLbZTW5namK_4core3ptr9drop_glueNtNtCsj8lNtgb2Igp_5alloc6string6StringECsagrXUsJVDRq_3lin.exit4.i.i.i.i.i.i, label %bb2.i.i.i4.i.i2.i.i.i.i.i.i

bb2.i.i.i4.i.i2.i.i.i.i.i.i:                      ; preds = %bb2.i.i.i.i.i.i
; call __rustc::__rust_dealloc
  tail call void @_RNvCsfmtydf4BlqR_7___rustc14___rust_dealloc(ptr noundef nonnull %_14.i.i.i.i.i.i, i64 noundef %_1.val2.i.i1.i.i.i.i.i.i, i64 noundef range(i64 1, -9223372036854775807) 1) #86, !noalias !249692
  br label %_RINvNtCsaLbZTW5namK_4core3ptr9drop_glueNtNtCsj8lNtgb2Igp_5alloc6string6StringECsagrXUsJVDRq_3lin.exit4.i.i.i.i.i.i

_RINvNtCsaLbZTW5namK_4core3ptr9drop_glueNtNtCsj8lNtgb2Igp_5alloc6string6StringECsagrXUsJVDRq_3lin.exit4.i.i.i.i.i.i: ; preds = %bb2.i.i.i4.i.i2.i.i.i.i.i.i, %bb2.i.i.i.i.i.i
  call void @llvm.lifetime.end.p0(ptr nonnull %_6.i.i.i.i.i.i), !noalias !249674
; call <lin::ast::Value as core::cmp::PartialEq>::eq
  %90 = tail call fastcc noundef zeroext i1 @_RNvXs1s_NtCsagrXUsJVDRq_3lin3astNtB6_5ValueNtNtCsaLbZTW5namK_4core3cmp9PartialEq2eq(ptr noalias nofree noundef readonly align 8 captures(address, read_provenance) dereferenceable(24) %v.i.i.i.i.i.i, ptr noalias nofree noundef readonly align 8 captures(address, read_provenance) dereferenceable(24) %79), !noalias !249675
  br i1 %90, label %bb2.backedge.i.i.i.i, label %bb1.backedge.i.i.i.i.i

bb1.backedge.i.i.i.i.i:                           ; preds = %_RNCNvNtCsagrXUsJVDRq_3lin5index21index_covers_conjunct0B5_.exit.i.i.i.i.i, %_RINvNtCsaLbZTW5namK_4core3ptr9drop_glueNtNtCsj8lNtgb2Igp_5alloc6string6StringECsagrXUsJVDRq_3lin.exit4.i.i.i.i.i.i
  %_12.not.i.i.i.i.i = icmp eq ptr %_22.i.i.i.i.i, %_78.i.i.i.i
  br i1 %_12.not.i.i.i.i.i, label %bb45.i.i.i.i, label %bb13.i.i.i.i.i

_RNCNvNtCsagrXUsJVDRq_3lin5index21index_covers_conjunct0B5_.exit.i.i.i.i.i: ; preds = %bb2.i.i.i4.i.i.i.i.i.i.i.i, %bb4.i.i.i.i.i.i
  call void @llvm.lifetime.end.p0(ptr nonnull %_6.i.i.i.i.i.i), !noalias !249674
  br label %bb1.backedge.i.i.i.i.i

bb45.i.i.i.i:                                     ; preds = %bb37.i.i.i.i, %bb36.i.i.i.i, %bb17.i.i.i.i, %bb18.i.i.i.i, %bb14.i.i.i.i, %bb11.i.i.i.i, %bb8.i.i.i.i, %bb1.backedge.i.i.i.i.i, %bb34.i.i.i.i
  %91 = icmp eq i64 %_7.sroa.0.0.copyload.i.i.i.i, 0
  br i1 %91, label %bb46.i.i.i.i, label %bb2.i.i.i.i.i4.i.i38.i.i.i.i

bb2.i.i.i.i.i4.i.i38.i.i.i.i:                     ; preds = %bb45.i.i.i.i
  %alloc_size.i.i.i.i.i.i6.i.i39.i.i.i.i = shl nuw i64 %_7.sroa.0.0.copyload.i.i.i.i, 3
; call __rustc::__rust_dealloc
  tail call void @_RNvCsfmtydf4BlqR_7___rustc14___rust_dealloc(ptr noundef nonnull %_7.sroa.4.0.copyload.i.i.i.i, i64 noundef %alloc_size.i.i.i.i.i.i6.i.i39.i.i.i.i, i64 noundef range(i64 1, -9223372036854775807) 8) #86, !noalias !249693
  br label %bb46.i.i.i.i

bb2.backedge.i.i.i.i:                             ; preds = %bb13.i.i.i.i.i.i, %_RINvNtCsaLbZTW5namK_4core3ptr9drop_glueNtNtCsj8lNtgb2Igp_5alloc6string6StringECsagrXUsJVDRq_3lin.exit4.i.i.i.i.i.i, %bb37.i.i.i.i
  %_9.i.i.i.i.i = icmp eq ptr %_23.i.i.i.i.i, %_61.i.i.i.i
  br i1 %_9.i.i.i.i.i, label %bb6.i.i.i.i, label %bb5.i.i.i.i

bb30.i.i.i.i:                                     ; preds = %bb11.i.i.i.i
  call void @llvm.lifetime.start.p0(ptr nonnull %_51.i.i.i.i), !noalias !249696
  %92 = getelementptr inbounds nuw i8, ptr %_17.i.i.i.i.i, i64 16
  %_6.i.i.i.i.i = load ptr, ptr %92, align 8, !alias.scope !249697, !noalias !249700, !nonnull !2388, !noundef !2388
  %93 = getelementptr inbounds nuw i8, ptr %_17.i.i.i.i.i, i64 24
  %_5.i.i.i.i.i = load i64, ptr %93, align 8, !alias.scope !249697, !noalias !249700, !noundef !2388
; invoke alloc::str::join_generic_copy::<str, u8, alloc::string::String>
  invoke fastcc void @_RINvNtCsj8lNtgb2Igp_5alloc3str17join_generic_copyehNtNtB4_6string6StringECsagrXUsJVDRq_3lin(ptr noalias nofree noundef align 8 captures(none) dereferenceable(24) %_51.i.i.i.i, ptr noalias nofree noundef nonnull readonly align 8 captures(address, read_provenance) %_6.i.i.i.i.i, i64 noundef %_5.i.i.i.i.i, ptr noalias nofree noundef nonnull readonly captures(address, read_provenance) @alloc_0f52f98e9fa44caf6668aeb0b1c94ca5, i64 noundef 1)
          to label %bb32.i.i.i.i unwind label %cleanup1.loopexit.split-lp.i.i.i.i, !noalias !249641

bb32.i.i.i.i:                                     ; preds = %bb30.i.i.i.i
  %_88.i.i.i.i = load i64, ptr %44, align 8, !noalias !249696, !noundef !2388
  %_90.i.i.i.i = icmp eq i64 %_85.i.i.i.i, %_88.i.i.i.i
  br i1 %_90.i.i.i.i, label %bb62.i.i.i.i, label %bb33.i.i.i.i

bb62.i.i.i.i:                                     ; preds = %bb32.i.i.i.i
  %_89.i.i.i.i = load ptr, ptr %45, align 8, !noalias !249696, !nonnull !2388, !noundef !2388
  %_91.i.i.i.i = tail call i32 @memcmp(ptr nonnull %_86.i.i.i.i, ptr nonnull %_89.i.i.i.i, i64 %_85.i.i.i.i), !noalias !249641
  %_83.not.i.i.i.i = icmp eq i32 %_91.i.i.i.i, 0
  br i1 %_83.not.i.i.i.i, label %bb35.i.i.i.i, label %bb33.i.i.i.i

bb33.i.i.i.i:                                     ; preds = %bb62.i.i.i.i, %bb32.i.i.i.i
  tail call void @llvm.experimental.noalias.scope.decl(metadata !249702)
  tail call void @llvm.experimental.noalias.scope.decl(metadata !249705)
  %_1.val2.i.i.i.i.i.i = load i64, ptr %_51.i.i.i.i, align 8, !range !2805, !alias.scope !249708, !noalias !249696, !noundef !2388
  %94 = icmp eq i64 %_1.val2.i.i.i.i.i.i, 0
  br i1 %94, label %bb34.i.i.i.i, label %bb2.i.i.i4.i.i42.i.i.i.i

bb2.i.i.i4.i.i42.i.i.i.i:                         ; preds = %bb33.i.i.i.i
  %_1.val3.i.i.i.i.i.i = load ptr, ptr %45, align 8, !alias.scope !249708, !noalias !249696, !nonnull !2388, !noundef !2388
; call __rustc::__rust_dealloc
  tail call void @_RNvCsfmtydf4BlqR_7___rustc14___rust_dealloc(ptr noundef nonnull %_1.val3.i.i.i.i.i.i, i64 noundef %_1.val2.i.i.i.i.i.i, i64 noundef range(i64 1, -9223372036854775807) 1) #86, !noalias !249709
  br label %bb34.i.i.i.i

bb35.i.i.i.i:                                     ; preds = %bb62.i.i.i.i
  tail call void @llvm.experimental.noalias.scope.decl(metadata !249710)
  tail call void @llvm.experimental.noalias.scope.decl(metadata !249713)
  %_1.val2.i.i43.i.i.i.i = load i64, ptr %_51.i.i.i.i, align 8, !range !2805, !alias.scope !249716, !noalias !249696, !noundef !2388
  %95 = icmp eq i64 %_1.val2.i.i43.i.i.i.i, 0
  br i1 %95, label %bb36.i.i.i.i, label %bb2.i.i.i4.i.i44.i.i.i.i

bb2.i.i.i4.i.i44.i.i.i.i:                         ; preds = %bb35.i.i.i.i
; call __rustc::__rust_dealloc
  tail call void @_RNvCsfmtydf4BlqR_7___rustc14___rust_dealloc(ptr noundef nonnull %_89.i.i.i.i, i64 noundef %_1.val2.i.i43.i.i.i.i, i64 noundef range(i64 1, -9223372036854775807) 1) #86, !noalias !249717
  br label %bb36.i.i.i.i

bb36.i.i.i.i:                                     ; preds = %bb2.i.i.i4.i.i44.i.i.i.i, %bb35.i.i.i.i
  call void @llvm.lifetime.end.p0(ptr nonnull %_51.i.i.i.i), !noalias !249696
  %96 = load i8, ptr %76, align 8, !range !2751, !noalias !249641, !noundef !2388
  %_92.not.i.i.i.i = icmp eq i8 %68, %96
  br i1 %_92.not.i.i.i.i, label %bb37.i.i.i.i, label %bb45.i.i.i.i

bb37.i.i.i.i:                                     ; preds = %bb36.i.i.i.i
; call <lin::ast::Value as core::cmp::PartialEq>::eq
  %_95.i.i.i.i = tail call fastcc noundef zeroext i1 @_RNvXs1s_NtCsagrXUsJVDRq_3lin3astNtB6_5ValueNtNtCsaLbZTW5namK_4core3cmp9PartialEq2eq(ptr noalias nofree noundef readonly align 8 captures(address, read_provenance) dereferenceable(24) %rv.i.i.i.i, ptr noalias nofree noundef readonly align 8 captures(address, read_provenance) dereferenceable(24) %79), !noalias !249641
  br i1 %_95.i.i.i.i, label %bb2.backedge.i.i.i.i, label %bb45.i.i.i.i

bb34.i.i.i.i:                                     ; preds = %bb2.i.i.i4.i.i42.i.i.i.i, %bb33.i.i.i.i
  call void @llvm.lifetime.end.p0(ptr nonnull %_51.i.i.i.i), !noalias !249696
  br label %bb45.i.i.i.i

bb46.i.i.i.i:                                     ; preds = %bb2.i.i.i.i.i4.i.i38.i.i.i.i, %bb45.i.i.i.i
  %97 = icmp eq i64 %pred_atoms.sroa.0.0.i.i.i.i, 0
  br i1 %97, label %bb18, label %_RNCNvNtCsagrXUsJVDRq_3lin5index17index_covers_pred0B5_.exit.thread.sink.split.i.thread.i

_RNCNvNtCsagrXUsJVDRq_3lin5index17index_covers_pred0B5_.exit.thread.sink.split.i.thread.i: ; preds = %bb46.i.i.i.i
  %alloc_size.i.i.i.i5.i48.i.i6.i5.i = shl nuw i64 %pred_atoms.sroa.0.0.i.i.i.i, 3
  call void @llvm.assume(i1 true) [ "nonnull"(ptr %pred_atoms.sroa.8.0.i.i.i.i) ]
; call __rustc::__rust_dealloc
  tail call void @_RNvCsfmtydf4BlqR_7___rustc14___rust_dealloc(ptr noundef nonnull %pred_atoms.sroa.8.0.i.i.i.i, i64 noundef %alloc_size.i.i.i.i5.i48.i.i6.i5.i, i64 noundef range(i64 1, -9223372036854775807) 8) #86, !noalias !249641
  br label %bb18

_RNCNvNtCsagrXUsJVDRq_3lin5index17index_covers_pred0B5_.exit.thread.sink.split.i.i: ; preds = %bb41.i.i.i.i
  %alloc_size.i.i.i.i5.i48.i.i6.i.i = shl nuw i64 %pred_atoms.sroa.0.0.i.i.i.i, 3
  call void @llvm.assume(i1 true) [ "nonnull"(ptr %pred_atoms.sroa.8.0.i.i.i.i) ]
; call __rustc::__rust_dealloc
  tail call void @_RNvCsfmtydf4BlqR_7___rustc14___rust_dealloc(ptr noundef nonnull %pred_atoms.sroa.8.0.i.i.i.i, i64 noundef %alloc_size.i.i.i.i5.i48.i.i6.i.i, i64 noundef range(i64 1, -9223372036854775807) 8) #86, !noalias !249641
  br label %bb1.backedge.i

bb1.backedge.i:                                   ; preds = %_RNCNvNtCsagrXUsJVDRq_3lin5index17index_covers_pred0B5_.exit.thread.sink.split.i.i, %bb41.i.i.i.i
  %exitcond.not.i = icmp eq i64 %49, %uses.1
  br i1 %exitcond.not.i, label %bb18, label %bb3.i

bb18:                                             ; preds = %bb1.backedge.i, %bb11, %bb46.i.i.i.i, %_RNCNvNtCsagrXUsJVDRq_3lin5index17index_covers_pred0B5_.exit.thread.sink.split.i.thread.i
  %_3.i7 = phi i1 [ false, %bb46.i.i.i.i ], [ false, %_RNCNvNtCsagrXUsJVDRq_3lin5index17index_covers_pred0B5_.exit.thread.sink.split.i.thread.i ], [ true, %bb11 ], [ true, %bb1.backedge.i ]
  tail call void @llvm.experimental.noalias.scope.decl(metadata !249718)
  %_7.i.i.i12190 = icmp eq i64 %uses.1, 0
  br i1 %_7.i.i.i12190, label %bb4.i27, label %bb5.i.i.i13

bb6.i.i.i10:                                      ; preds = %bb5.i.i.i13
  %_7.i.i.i12 = icmp eq i64 %98, %uses.1
  br i1 %_7.i.i.i12, label %bb4.i27, label %bb5.i.i.i13

bb5.i.i.i13:                                      ; preds = %bb18, %bb6.i.i.i10
  %_3.sroa.0.0.i.i.i11191 = phi i64 [ %98, %bb6.i.i.i10 ], [ 0, %bb18 ]
  %_6.i.i.i14 = getelementptr inbounds nuw [72 x i8], ptr %_20, i64 %_3.sroa.0.0.i.i.i11191
  %98 = add nuw nsw i64 %_3.sroa.0.0.i.i.i11191, 1
; invoke core::ptr::drop_glue::<lin::ast::Pred>
  invoke fastcc void @_RINvNtCsaLbZTW5namK_4core3ptr9drop_glueNtNtCsagrXUsJVDRq_3lin3ast4PredEBF_(ptr noalias nofree noundef align 8 dereferenceable(72) %_6.i.i.i14)
          to label %bb6.i.i.i10 unwind label %cleanup.i.i.i15, !noalias !249718

bb4.i.i.i16:                                      ; preds = %bb3.i.i.i19
  %99 = add i64 %_3.sroa.0.1.i.i.i17193, 1
  %_5.i.i.i18 = icmp eq i64 %99, %uses.1
  br i1 %_5.i.i.i18, label %cleanup.body.i22, label %bb3.i.i.i19

cleanup.i.i.i15:                                  ; preds = %bb5.i.i.i13
  %100 = landingpad { ptr, i32 }
          cleanup
  %_5.i.i.i18192 = icmp eq i64 %98, %uses.1
  br i1 %_5.i.i.i18192, label %cleanup.body.i22, label %bb3.i.i.i19

bb3.i.i.i19:                                      ; preds = %cleanup.i.i.i15, %bb4.i.i.i16
  %_3.sroa.0.1.i.i.i17193 = phi i64 [ %99, %bb4.i.i.i16 ], [ %98, %cleanup.i.i.i15 ]
  %_4.i.i.i20 = getelementptr inbounds nuw [72 x i8], ptr %_20, i64 %_3.sroa.0.1.i.i.i17193
; invoke core::ptr::drop_glue::<lin::ast::Pred>
  invoke fastcc void @_RINvNtCsaLbZTW5namK_4core3ptr9drop_glueNtNtCsagrXUsJVDRq_3lin3ast4PredEBF_(ptr noalias nofree noundef align 8 dereferenceable(72) %_4.i.i.i20) #87
          to label %bb4.i.i.i16 unwind label %terminate.i.i.i21, !noalias !249718

terminate.i.i.i21:                                ; preds = %bb3.i.i.i19
  %101 = landingpad { ptr, i32 }
          filter [0 x ptr] zeroinitializer
; call core::panicking::panic_in_cleanup
  tail call void @_RNvNtCsaLbZTW5namK_4core9panicking16panic_in_cleanup() #88, !noalias !249718
  unreachable

cleanup.body.i22:                                 ; preds = %bb4.i.i.i16, %cleanup.i.i.i15
  %_1.val2.i23 = load i64, ptr %branches, align 8, !range !2805, !alias.scope !249718, !noundef !2388
  %102 = icmp eq i64 %_1.val2.i23, 0
  br i1 %102, label %common.resume, label %common.resume.sink.split

bb4.i27:                                          ; preds = %bb6.i.i.i10, %bb18
  %_1.val4.i28 = load i64, ptr %branches, align 8, !range !2805, !alias.scope !249718, !noundef !2388
  %103 = icmp eq i64 %_1.val4.i28, 0
  br i1 %103, label %_RINvNtCsaLbZTW5namK_4core3ptr9drop_glueINtNtCsj8lNtgb2Igp_5alloc3vec3VecNtNtCsagrXUsJVDRq_3lin3ast4PredEEB1c_.exit31, label %bb2.i.i.i6.i29

bb2.i.i.i6.i29:                                   ; preds = %bb4.i27
  %alloc_size.i.i.i.i7.i30 = mul nuw i64 %_1.val4.i28, 72
; call __rustc::__rust_dealloc
  tail call void @_RNvCsfmtydf4BlqR_7___rustc14___rust_dealloc(ptr noundef nonnull %_20, i64 noundef %alloc_size.i.i.i.i7.i30, i64 noundef range(i64 1, -9223372036854775807) 8) #86, !noalias !249718
  br label %_RINvNtCsaLbZTW5namK_4core3ptr9drop_glueINtNtCsj8lNtgb2Igp_5alloc3vec3VecNtNtCsagrXUsJVDRq_3lin3ast4PredEEB1c_.exit31

_RINvNtCsaLbZTW5namK_4core3ptr9drop_glueINtNtCsj8lNtgb2Igp_5alloc3vec3VecNtNtCsagrXUsJVDRq_3lin3ast4PredEEB1c_.exit31: ; preds = %bb4.i27, %bb2.i.i.i6.i29
  call void @llvm.lifetime.end.p0(ptr nonnull %branches)
  br label %bb14

terminate:                                        ; preds = %cleanup.body
  %104 = landingpad { ptr, i32 }
          filter [0 x ptr] zeroinitializer
; call core::panicking::panic_in_cleanup
  tail call void @_RNvNtCsaLbZTW5namK_4core9panicking16panic_in_cleanup() #88
  unreachable
}
