define internal fastcc noundef zeroext i1 @_RNvNtCsagrXUsJVDRq_3lin5index17index_covers_pred(ptr noalias nofree noundef nonnull readonly align 8 captures(address, read_provenance) dereferenceable(72) %pred, ptr noalias nofree noundef nonnull readonly align 8 captures(address, read_provenance) %uses.0, i64 noundef range(i64 0, 39755913951960241) %uses.1) unnamed_addr #2 personality ptr @_RNvCsfmtydf4BlqR_7___rustc19rust_eh_personality !guid !249574 {
start:
  %_6.i.i.i.i.i.i = alloca [24 x i8], align 8
  %v.i20.i.i.i.i = alloca [24 x i8], align 8
  %v.i.i.i.i.i = alloca [24 x i8], align 8
  %_51.i.i.i.i = alloca [24 x i8], align 8
  %_4 = alloca [24 x i8], align 8
  %branches = alloca [24 x i8], align 8
  call void @llvm.lifetime.start.p0(ptr nonnull %_4)
; call lin::index::dnf
  call fastcc void @_RNvNtCsagrXUsJVDRq_3lin5index3dnf(ptr noalias nofree noundef align 8 captures(address) dereferenceable(24) %_4, ptr noalias nofree noundef readonly align 8 captures(address, read_provenance) dereferenceable(72) %pred)
  %0 = load i64, ptr %_4, align 8, !range !3853, !noundef !2388
  %.not = icmp eq i64 %0, -1
  br i1 %.not, label %bb3, label %bb2

bb2:                                              ; preds = %start
  call void @llvm.lifetime.start.p0(ptr nonnull %branches)
  call void @llvm.memcpy.p0.p0.i64(ptr noundef nonnull align 8 dereferenceable(24) %branches, ptr noundef nonnull align 8 dereferenceable(24) %_4, i64 24, i1 false)
  call void @llvm.lifetime.end.p0(ptr nonnull %_4)
  %1 = getelementptr inbounds nuw i8, ptr %branches, i64 16
  %_7 = load i64, ptr %1, align 8, !noundef !2388
  %_13 = icmp ult i64 %_7, 128102389400760776
  tail call void @llvm.assume(i1 %_13)
  %_6.not = icmp eq i64 %_7, %uses.1
  br i1 %_6.not, label %bb6, label %bb4

bb3:                                              ; preds = %start
  call void @llvm.lifetime.end.p0(ptr nonnull %_4)
  br label %bb9

bb4:                                              ; preds = %bb2
  tail call void @llvm.experimental.noalias.scope.decl(metadata !249575)
  %2 = getelementptr inbounds nuw i8, ptr %branches, i64 8
  %_1.val.i = load ptr, ptr %2, align 8, !alias.scope !249575, !nonnull !2388, !noundef !2388
  %_7.i.i.i178 = icmp eq i64 %_7, 0
  br i1 %_7.i.i.i178, label %bb4.i, label %bb5.i.i.i

bb6.i.i.i:                                        ; preds = %bb5.i.i.i
  %_7.i.i.i = icmp eq i64 %3, %_7
  br i1 %_7.i.i.i, label %bb4.i, label %bb5.i.i.i

bb5.i.i.i:                                        ; preds = %bb4, %bb6.i.i.i
  %_3.sroa.0.0.i.i.i179 = phi i64 [ %3, %bb6.i.i.i ], [ 0, %bb4 ]
  %_6.i.i.i = getelementptr inbounds nuw [72 x i8], ptr %_1.val.i, i64 %_3.sroa.0.0.i.i.i179
  %3 = add nuw nsw i64 %_3.sroa.0.0.i.i.i179, 1
; invoke core::ptr::drop_glue::<lin::ast::Pred>
  invoke fastcc void @_RINvNtCsaLbZTW5namK_4core3ptr9drop_glueNtNtCsagrXUsJVDRq_3lin3ast4PredEBF_(ptr noalias nofree noundef align 8 dereferenceable(72) %_6.i.i.i)
          to label %bb6.i.i.i unwind label %cleanup.i.i.i, !noalias !249575

bb4.i.i.i:                                        ; preds = %bb3.i.i.i
  %4 = add i64 %_3.sroa.0.1.i.i.i181, 1
  %_5.i.i.i = icmp eq i64 %4, %_7
  br i1 %_5.i.i.i, label %cleanup.body.i, label %bb3.i.i.i

cleanup.i.i.i:                                    ; preds = %bb5.i.i.i
  %5 = landingpad { ptr, i32 }
          cleanup
  %_5.i.i.i180 = icmp eq i64 %3, %_7
  br i1 %_5.i.i.i180, label %cleanup.body.i, label %bb3.i.i.i

bb3.i.i.i:                                        ; preds = %cleanup.i.i.i, %bb4.i.i.i
  %_3.sroa.0.1.i.i.i181 = phi i64 [ %4, %bb4.i.i.i ], [ %3, %cleanup.i.i.i ]
  %_4.i.i.i = getelementptr inbounds nuw [72 x i8], ptr %_1.val.i, i64 %_3.sroa.0.1.i.i.i181
; invoke core::ptr::drop_glue::<lin::ast::Pred>
  invoke fastcc void @_RINvNtCsaLbZTW5namK_4core3ptr9drop_glueNtNtCsagrXUsJVDRq_3lin3ast4PredEBF_(ptr noalias nofree noundef align 8 dereferenceable(72) %_4.i.i.i) #87
          to label %bb4.i.i.i unwind label %terminate.i.i.i, !noalias !249575

terminate.i.i.i:                                  ; preds = %bb3.i.i.i
  %6 = landingpad { ptr, i32 }
          filter [0 x ptr] zeroinitializer
; call core::panicking::panic_in_cleanup
  tail call void @_RNvNtCsaLbZTW5namK_4core9panicking16panic_in_cleanup() #88, !noalias !249575
  unreachable

cleanup.body.i:                                   ; preds = %bb4.i.i.i, %cleanup.i.i.i
  %_1.val2.i = load i64, ptr %branches, align 8, !range !2805, !alias.scope !249575, !noundef !2388
  %7 = icmp eq i64 %_1.val2.i, 0
  br i1 %7, label %common.resume, label %common.resume.sink.split

bb4.i:                                            ; preds = %bb6.i.i.i, %bb4
  %_1.val4.i = load i64, ptr %branches, align 8, !range !2805, !alias.scope !249575, !noundef !2388
  %8 = icmp eq i64 %_1.val4.i, 0
  br i1 %8, label %_RINvNtCsaLbZTW5namK_4core3ptr9drop_glueINtNtCsj8lNtgb2Igp_5alloc3vec3VecNtNtCsagrXUsJVDRq_3lin3ast4PredEEB1c_.exit, label %bb2.i.i.i6.i

bb2.i.i.i6.i:                                     ; preds = %bb4.i
  %alloc_size.i.i.i.i7.i = mul nuw i64 %_1.val4.i, 72
; call __rustc::__rust_dealloc
  tail call void @_RNvCsfmtydf4BlqR_7___rustc14___rust_dealloc(ptr noundef nonnull %_1.val.i, i64 noundef %alloc_size.i.i.i.i7.i, i64 noundef range(i64 1, -9223372036854775807) 8) #86, !noalias !249575
  br label %_RINvNtCsaLbZTW5namK_4core3ptr9drop_glueINtNtCsj8lNtgb2Igp_5alloc3vec3VecNtNtCsagrXUsJVDRq_3lin3ast4PredEEB1c_.exit

common.resume.sink.split:                         ; preds = %cleanup.body.i, %cleanup.body.i16
  %_1.val2.i17.sink = phi i64 [ %_1.val2.i17, %cleanup.body.i16 ], [ %_1.val2.i, %cleanup.body.i ]
  %_17.sink = phi ptr [ %_17, %cleanup.body.i16 ], [ %_1.val.i, %cleanup.body.i ]
  %common.resume.op.ph = phi { ptr, i32 } [ %71, %cleanup.body.i16 ], [ %5, %cleanup.body.i ]
  %alloc_size.i.i.i.i.i19 = mul nuw i64 %_1.val2.i17.sink, 72
; call __rustc::__rust_dealloc
  tail call void @_RNvCsfmtydf4BlqR_7___rustc14___rust_dealloc(ptr noundef nonnull %_17.sink, i64 noundef %alloc_size.i.i.i.i.i19, i64 noundef range(i64 1, -9223372036854775807) 8) #86, !noalias !2388
  br label %common.resume

common.resume:                                    ; preds = %common.resume.sink.split, %cleanup.body, %cleanup.body.i16, %cleanup.body.i
  %common.resume.op = phi { ptr, i32 } [ %71, %cleanup.body.i16 ], [ %5, %cleanup.body.i ], [ %eh.lpad-body, %cleanup.body ], [ %common.resume.op.ph, %common.resume.sink.split ]
  resume { ptr, i32 } %common.resume.op

_RINvNtCsaLbZTW5namK_4core3ptr9drop_glueINtNtCsj8lNtgb2Igp_5alloc3vec3VecNtNtCsagrXUsJVDRq_3lin3ast4PredEEB1c_.exit: ; preds = %bb4.i, %bb2.i.i.i6.i
  call void @llvm.lifetime.end.p0(ptr nonnull %branches)
  br label %bb9

cleanup:                                          ; preds = %bb6.i.i.i.i.i
  %9 = landingpad { ptr, i32 }
          cleanup
  br label %cleanup.body

cleanup.body:                                     ; preds = %cleanup.i.i.i.i.i, %bb2.i.i.i4.i.i.i.i.i.i, %bb50.i.i.i.i, %bb2.i.i.i4.i.i.i.i.i, %cleanup
  %eh.lpad-body = phi { ptr, i32 } [ %9, %cleanup ], [ %25, %cleanup.i.i.i.i.i ], [ %25, %bb2.i.i.i4.i.i.i.i.i.i ], [ %.pn.i.i.i.i, %bb2.i.i.i4.i.i.i.i.i ], [ %.pn.i.i.i.i, %bb50.i.i.i.i ]
; invoke core::ptr::drop_glue::<alloc::vec::Vec<lin::ast::Pred>>
  invoke fastcc void @_RINvNtCsaLbZTW5namK_4core3ptr9drop_glueINtNtCsj8lNtgb2Igp_5alloc3vec3VecNtNtCsagrXUsJVDRq_3lin3ast4PredEEB1c_(ptr noalias nofree noundef align 8 dereferenceable(24) %branches) #87
          to label %common.resume unwind label %terminate

bb6:                                              ; preds = %bb2
  %10 = getelementptr inbounds nuw i8, ptr %branches, i64 8
  %_17 = load ptr, ptr %10, align 8, !nonnull !2388, !noundef !2388
  %_2.i.i60.i.not = icmp eq i64 %uses.1, 0
  br i1 %_2.i.i60.i.not, label %bb13, label %bb3.lr.ph.i

bb3.lr.ph.i:                                      ; preds = %bb6
  %11 = getelementptr inbounds nuw i8, ptr %v.i.i.i.i.i, i64 8
  %12 = getelementptr inbounds nuw i8, ptr %v.i.i.i.i.i, i64 16
  %13 = getelementptr inbounds nuw i8, ptr %v.i20.i.i.i.i, i64 8
  %14 = getelementptr inbounds nuw i8, ptr %v.i20.i.i.i.i, i64 16
  %15 = getelementptr inbounds nuw i8, ptr %_51.i.i.i.i, i64 16
  %16 = getelementptr inbounds nuw i8, ptr %_51.i.i.i.i, i64 8
  %17 = getelementptr inbounds nuw i8, ptr %_6.i.i.i.i.i.i, i64 16
  %18 = getelementptr inbounds nuw i8, ptr %_6.i.i.i.i.i.i, i64 8
  br label %bb3.i

bb3.i:                                            ; preds = %bb1.backedge.i, %bb3.lr.ph.i
  %19 = phi i64 [ 0, %bb3.lr.ph.i ], [ %20, %bb1.backedge.i ]
  %20 = add nuw nsw i64 %19, 1
  %_3.i.i.i.i = getelementptr inbounds nuw [72 x i8], ptr %_17, i64 %19
  %_3.i1.i.i.i = getelementptr inbounds nuw [232 x i8], ptr %uses.0, i64 %19
  tail call void @llvm.experimental.noalias.scope.decl(metadata !249578)
  tail call void @llvm.experimental.noalias.scope.decl(metadata !249581)
  tail call void @llvm.experimental.noalias.scope.decl(metadata !249584)
  %21 = getelementptr inbounds nuw i8, ptr %_3.i1.i.i.i, i64 72
  %22 = load i64, ptr %21, align 8, !range !41402, !alias.scope !249587, !noalias !249588, !noundef !2388
  %.not.i.i.i.i = icmp eq i64 %22, -1
  br i1 %.not.i.i.i.i, label %bb55.i.i.i.i, label %bb53.i.i.i.i

bb53.i.i.i.i:                                     ; preds = %bb3.i
  call void @llvm.lifetime.start.p0(ptr nonnull %v.i.i.i.i.i), !noalias !249594
; call __rustc::__rust_no_alloc_shim_is_unstable_v2
  tail call void @_RNvCsfmtydf4BlqR_7___rustc35___rust_no_alloc_shim_is_unstable_v2() #86, !noalias !249598
; call __rustc::__rust_alloc
  %23 = tail call noundef align 8 dereferenceable_or_null(32) ptr @_RNvCsfmtydf4BlqR_7___rustc12___rust_alloc(i64 noundef 32, i64 noundef range(i64 1, -9223372036854775807) 8) #86, !noalias !249598
  %24 = icmp eq ptr %23, null
  br i1 %24, label %bb6.i.i.i.i.i, label %bb7.i.i.i.i.i

bb6.i.i.i.i.i:                                    ; preds = %bb53.i.i.i.i
; invoke alloc::raw_vec::handle_error
  invoke void @_RNvNtCsj8lNtgb2Igp_5alloc7raw_vec12handle_error(i64 noundef 8, i64 32) #85
          to label %.noexc unwind label %cleanup

.noexc:                                           ; preds = %bb6.i.i.i.i.i
  unreachable

bb7.i.i.i.i.i:                                    ; preds = %bb53.i.i.i.i
  store i64 4, ptr %v.i.i.i.i.i, align 8, !noalias !249594
  store ptr %23, ptr %11, align 8, !noalias !249594
  store i64 0, ptr %12, align 8, !noalias !249594
; invoke lin::index::flatten_and_into
  invoke fastcc void @_RNvNtCsagrXUsJVDRq_3lin5index16flatten_and_into(ptr noalias nofree noundef nonnull readonly align 8 captures(address, read_provenance) dereferenceable(72) %21, ptr noalias nofree noundef align 8 dereferenceable(24) %v.i.i.i.i.i)
          to label %_RNvNtCsagrXUsJVDRq_3lin5index11flatten_and.exit.i.i.i.i unwind label %cleanup.i.i.i.i.i, !noalias !249601

cleanup.i.i.i.i.i:                                ; preds = %bb7.i.i.i.i.i
  %25 = landingpad { ptr, i32 }
          cleanup
  %v.val.i.i.i.i.i = load i64, ptr %v.i.i.i.i.i, align 8, !noalias !249594
  %26 = icmp eq i64 %v.val.i.i.i.i.i, 0
  br i1 %26, label %cleanup.body, label %bb2.i.i.i4.i.i.i.i.i.i

bb2.i.i.i4.i.i.i.i.i.i:                           ; preds = %cleanup.i.i.i.i.i
  %v.val1.i.i.i.i.i = load ptr, ptr %11, align 8, !noalias !249594, !nonnull !2388, !noundef !2388
  %alloc_size.i.i.i.i5.i.i.i.i.i.i = shl nuw i64 %v.val.i.i.i.i.i, 3
; call __rustc::__rust_dealloc
  tail call void @_RNvCsfmtydf4BlqR_7___rustc14___rust_dealloc(ptr noundef nonnull %v.val1.i.i.i.i.i, i64 noundef %alloc_size.i.i.i.i5.i.i.i.i.i.i, i64 noundef range(i64 1, -9223372036854775807) 8) #86, !noalias !249601
  br label %cleanup.body

_RNvNtCsagrXUsJVDRq_3lin5index11flatten_and.exit.i.i.i.i: ; preds = %bb7.i.i.i.i.i
  %_57.sroa.0.0.copyload.i.i.i.i = load i64, ptr %v.i.i.i.i.i, align 8, !noalias !249602
  %_57.sroa.4.0.copyload.i.i.i.i = load ptr, ptr %11, align 8, !noalias !249602
  %_57.sroa.5.0.copyload.i.i.i.i = load i64, ptr %12, align 8, !noalias !249602
  call void @llvm.lifetime.end.p0(ptr nonnull %v.i.i.i.i.i), !noalias !249594
  %27 = shl nuw nsw i64 %_57.sroa.5.0.copyload.i.i.i.i, 3
  br label %bb55.i.i.i.i

bb55.i.i.i.i:                                     ; preds = %_RNvNtCsagrXUsJVDRq_3lin5index11flatten_and.exit.i.i.i.i, %bb3.i
  %pred_atoms.sroa.13.0.i.i.i.i = phi i64 [ %27, %_RNvNtCsagrXUsJVDRq_3lin5index11flatten_and.exit.i.i.i.i ], [ 0, %bb3.i ]
  %pred_atoms.sroa.8.0.i.i.i.i = phi ptr [ %_57.sroa.4.0.copyload.i.i.i.i, %_RNvNtCsagrXUsJVDRq_3lin5index11flatten_and.exit.i.i.i.i ], [ inttoptr (i64 8 to ptr), %bb3.i ]
  %pred_atoms.sroa.0.0.i.i.i.i = phi i64 [ %_57.sroa.0.0.copyload.i.i.i.i, %_RNvNtCsagrXUsJVDRq_3lin5index11flatten_and.exit.i.i.i.i ], [ 0, %bb3.i ]
  call void @llvm.lifetime.start.p0(ptr nonnull %v.i20.i.i.i.i), !noalias !249603
; call __rustc::__rust_no_alloc_shim_is_unstable_v2
  tail call void @_RNvCsfmtydf4BlqR_7___rustc35___rust_no_alloc_shim_is_unstable_v2() #86, !noalias !249607
; call __rustc::__rust_alloc
  %28 = tail call noundef align 8 dereferenceable_or_null(32) ptr @_RNvCsfmtydf4BlqR_7___rustc12___rust_alloc(i64 noundef 32, i64 noundef range(i64 1, -9223372036854775807) 8) #86, !noalias !249607
  %29 = icmp eq ptr %28, null
  br i1 %29, label %bb6.i28.i.i.i.i, label %bb7.i21.i.i.i.i

bb6.i28.i.i.i.i:                                  ; preds = %bb55.i.i.i.i
; invoke alloc::raw_vec::handle_error
  invoke void @_RNvNtCsj8lNtgb2Igp_5alloc7raw_vec12handle_error(i64 noundef 8, i64 32) #85
          to label %.noexc.i.i.i.i unwind label %cleanup.i.i.i.i, !noalias !249610

.noexc.i.i.i.i:                                   ; preds = %bb6.i28.i.i.i.i
  unreachable

bb7.i21.i.i.i.i:                                  ; preds = %bb55.i.i.i.i
  store i64 4, ptr %v.i20.i.i.i.i, align 8, !noalias !249603
  store ptr %28, ptr %13, align 8, !noalias !249603
  store i64 0, ptr %14, align 8, !noalias !249603
; invoke lin::index::flatten_and_into
  invoke fastcc void @_RNvNtCsagrXUsJVDRq_3lin5index16flatten_and_into(ptr noalias nofree noundef nonnull readonly align 8 captures(address, read_provenance) dereferenceable(72) %_3.i.i.i.i, ptr noalias nofree noundef align 8 dereferenceable(24) %v.i20.i.i.i.i)
          to label %bb1.i.i.i.i unwind label %cleanup.i22.i.i.i.i, !noalias !249611

cleanup.i22.i.i.i.i:                              ; preds = %bb7.i21.i.i.i.i
  %30 = landingpad { ptr, i32 }
          cleanup
  %v.val.i23.i.i.i.i = load i64, ptr %v.i20.i.i.i.i, align 8, !noalias !249603
  %31 = icmp eq i64 %v.val.i23.i.i.i.i, 0
  br i1 %31, label %bb50.i.i.i.i, label %bb2.i.i.i4.i.i24.i.i.i.i

bb2.i.i.i4.i.i24.i.i.i.i:                         ; preds = %cleanup.i22.i.i.i.i
  %v.val1.i25.i.i.i.i = load ptr, ptr %13, align 8, !noalias !249603, !nonnull !2388, !noundef !2388
  %alloc_size.i.i.i.i5.i.i26.i.i.i.i = shl nuw i64 %v.val.i23.i.i.i.i, 3
; call __rustc::__rust_dealloc
  tail call void @_RNvCsfmtydf4BlqR_7___rustc14___rust_dealloc(ptr noundef nonnull %v.val1.i25.i.i.i.i, i64 noundef %alloc_size.i.i.i.i5.i.i26.i.i.i.i, i64 noundef range(i64 1, -9223372036854775807) 8) #86, !noalias !249611
  br label %bb50.i.i.i.i

bb50.i.i.i.i:                                     ; preds = %bb2.i.i.i.i.i4.i.i.i.i.i.i, %cleanup1.i.i.i.i, %cleanup.i.i.i.i, %bb2.i.i.i4.i.i24.i.i.i.i, %cleanup.i22.i.i.i.i
  %.pn.i.i.i.i = phi { ptr, i32 } [ %30, %cleanup.i22.i.i.i.i ], [ %33, %cleanup.i.i.i.i ], [ %30, %bb2.i.i.i4.i.i24.i.i.i.i ], [ %lpad.phi.i.i.i.i, %cleanup1.i.i.i.i ], [ %lpad.phi.i.i.i.i, %bb2.i.i.i.i.i4.i.i.i.i.i.i ]
  %32 = icmp eq i64 %pred_atoms.sroa.0.0.i.i.i.i, 0
  br i1 %32, label %cleanup.body, label %bb2.i.i.i4.i.i.i.i.i

bb2.i.i.i4.i.i.i.i.i:                             ; preds = %bb50.i.i.i.i
  %alloc_size.i.i.i.i5.i.i.i.i.i = shl nuw i64 %pred_atoms.sroa.0.0.i.i.i.i, 3
  call void @llvm.assume(i1 true) [ "nonnull"(ptr %pred_atoms.sroa.8.0.i.i.i.i) ]
; call __rustc::__rust_dealloc
  tail call void @_RNvCsfmtydf4BlqR_7___rustc14___rust_dealloc(ptr noundef nonnull %pred_atoms.sroa.8.0.i.i.i.i, i64 noundef %alloc_size.i.i.i.i5.i.i.i.i.i, i64 noundef range(i64 1, -9223372036854775807) 8) #86, !noalias !249610
  br label %cleanup.body

cleanup.i.i.i.i:                                  ; preds = %bb6.i28.i.i.i.i
  %33 = landingpad { ptr, i32 }
          cleanup
  br label %bb50.i.i.i.i

bb1.i.i.i.i:                                      ; preds = %bb7.i21.i.i.i.i
  %_7.sroa.0.0.copyload.i.i.i.i = load i64, ptr %v.i20.i.i.i.i, align 8, !noalias !249612
  %_7.sroa.4.0.copyload.i.i.i.i = load ptr, ptr %13, align 8, !noalias !249612, !nonnull !2388, !noundef !2388
  %_7.sroa.5.0.copyload.i.i.i.i = load i64, ptr %14, align 8, !noalias !249612
  call void @llvm.lifetime.end.p0(ptr nonnull %v.i20.i.i.i.i), !noalias !249603
  %_63.i.i.i.i = icmp ult i64 %_7.sroa.5.0.copyload.i.i.i.i, 1152921504606846976
  tail call void @llvm.assume(i1 %_63.i.i.i.i)
  %_61.idx.i.i.i.i = shl nuw nsw i64 %_7.sroa.5.0.copyload.i.i.i.i, 3
  %_61.i.i.i.i = getelementptr inbounds nuw i8, ptr %_7.sroa.4.0.copyload.i.i.i.i, i64 %_61.idx.i.i.i.i
  %_9.i64.i.i.i.i = icmp eq i64 %_7.sroa.5.0.copyload.i.i.i.i, 0
  br i1 %_9.i64.i.i.i.i, label %bb6.i.i.i.i, label %bb5.lr.ph.i.i.i.i

bb5.lr.ph.i.i.i.i:                                ; preds = %bb1.i.i.i.i
  %_10.i.i.i.i.i = getelementptr inbounds nuw i8, ptr %pred_atoms.sroa.8.0.i.i.i.i, i64 %pred_atoms.sroa.13.0.i.i.i.i
  %34 = getelementptr inbounds nuw i8, ptr %_3.i1.i.i.i, i64 176
  %35 = load i64, ptr %34, align 8, !range !3853, !alias.scope !249587, !noalias !249588
  %.not6.i.i.i.i = icmp eq i64 %35, -1
  %rv.i.i.i.i = getelementptr inbounds nuw i8, ptr %_3.i1.i.i.i, i64 208
  %36 = getelementptr inbounds nuw i8, ptr %_3.i1.i.i.i, i64 192
  %_85.i.i.i.i = load i64, ptr %36, align 8, !alias.scope !249587, !noalias !249588
  %37 = getelementptr inbounds nuw i8, ptr %_3.i1.i.i.i, i64 184
  %_86.i.i.i.i = load ptr, ptr %37, align 8, !alias.scope !249587, !noalias !249588, !nonnull !2388
  %38 = getelementptr inbounds nuw i8, ptr %_3.i1.i.i.i, i64 200
  %39 = load i8, ptr %38, align 8, !range !2751, !alias.scope !249587, !noalias !249588
  %40 = getelementptr inbounds nuw i8, ptr %_3.i1.i.i.i, i64 160
  %_74.i.i.i.i = load ptr, ptr %40, align 8, !alias.scope !249587, !noalias !249588, !nonnull !2388
  %41 = getelementptr inbounds nuw i8, ptr %_3.i1.i.i.i, i64 168
  %_73.i.i.i.i = load i64, ptr %41, align 8, !alias.scope !249587, !noalias !249588
  %_78.idx.i.i.i.i = mul nuw nsw i64 %_73.i.i.i.i, 48
  %_78.i.i.i.i = getelementptr inbounds nuw i8, ptr %_74.i.i.i.i, i64 %_78.idx.i.i.i.i
  %_123.not.i.i.i.i.i = icmp eq i64 %_73.i.i.i.i, 0
  %_12.not.not.not.i.not.not.not.i.not.i.i.i.i182 = icmp samesign eq i64 %pred_atoms.sroa.13.0.i.i.i.i, 0
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
  %42 = icmp eq i64 %_7.sroa.0.0.copyload.i.i.i.i, 0
  br i1 %42, label %bb50.i.i.i.i, label %bb2.i.i.i.i.i4.i.i.i.i.i.i

bb2.i.i.i.i.i4.i.i.i.i.i.i:                       ; preds = %cleanup1.i.i.i.i
  %alloc_size.i.i.i.i.i.i6.i.i.i.i.i.i = shl nuw i64 %_7.sroa.0.0.copyload.i.i.i.i, 3
; call __rustc::__rust_dealloc
  tail call void @_RNvCsfmtydf4BlqR_7___rustc14___rust_dealloc(ptr noundef nonnull %_7.sroa.4.0.copyload.i.i.i.i, i64 noundef %alloc_size.i.i.i.i.i.i6.i.i.i.i.i.i, i64 noundef range(i64 1, -9223372036854775807) 8) #86, !noalias !249613
  br label %bb50.i.i.i.i

bb5.i.i.i.i:                                      ; preds = %bb2.backedge.i.i.i.i, %bb5.lr.ph.i.i.i.i
  %iter.sroa.7.065.i.i.i.i = phi ptr [ %_7.sroa.4.0.copyload.i.i.i.i, %bb5.lr.ph.i.i.i.i ], [ %_23.i.i.i.i.i, %bb2.backedge.i.i.i.i ]
  %_23.i.i.i.i.i = getelementptr inbounds nuw i8, ptr %iter.sroa.7.065.i.i.i.i, i64 8
  %_17.i.i.i.i.i = load ptr, ptr %iter.sroa.7.065.i.i.i.i, align 8, !noalias !249616, !nonnull !2388, !align !2828, !noundef !2388
  tail call void @llvm.experimental.noalias.scope.decl(metadata !249619)
  br i1 %_12.not.not.not.i.not.not.not.i.not.i.i.i.i182, label %bb8.i.i.i.i, label %bb13.i.i.i.i.i.i

bb1.i.i.i.i.i.i:                                  ; preds = %bb13.i.i.i.i.i.i
  %_22.i.i.i.i.i.i = getelementptr inbounds nuw i8, ptr %_221.i.i.i.i.i.i183, i64 8
  %_12.not.not.not.i.not.not.not.i.not.i.i.i.i = icmp eq ptr %_22.i.i.i.i.i.i, %_10.i.i.i.i.i
  br i1 %_12.not.not.not.i.not.not.not.i.not.i.i.i.i, label %bb8.i.i.i.i, label %bb13.i.i.i.i.i.i

bb13.i.i.i.i.i.i:                                 ; preds = %bb5.i.i.i.i, %bb1.i.i.i.i.i.i
  %_221.i.i.i.i.i.i183 = phi ptr [ %_22.i.i.i.i.i.i, %bb1.i.i.i.i.i.i ], [ %pred_atoms.sroa.8.0.i.i.i.i, %bb5.i.i.i.i ]
  %ptr.val.i.i.i.i.i.i = load ptr, ptr %_221.i.i.i.i.i.i183, align 8, !alias.scope !249619, !noalias !249622, !nonnull !2388, !align !2828, !noundef !2388
; call <lin::ast::Pred as core::cmp::PartialEq>::eq
  %_0.i.i.i.i.i.i.i.i = tail call fastcc noundef zeroext i1 @_RNvXs18_NtCsagrXUsJVDRq_3lin3astNtB6_4PredNtNtCsaLbZTW5namK_4core3cmp9PartialEq2eq(ptr noalias nofree noundef nonnull readonly align 8 captures(address, read_provenance) dereferenceable(72) %ptr.val.i.i.i.i.i.i, ptr noalias nofree noundef nonnull readonly align 8 captures(address, read_provenance) dereferenceable(72) %_17.i.i.i.i.i), !noalias !249626
  br i1 %_0.i.i.i.i.i.i.i.i, label %bb2.backedge.i.i.i.i, label %bb1.i.i.i.i.i.i

bb6.i.i.i.i:                                      ; preds = %bb2.backedge.i.i.i.i, %bb1.i.i.i.i
  %43 = icmp eq i64 %_7.sroa.0.0.copyload.i.i.i.i, 0
  br i1 %43, label %bb41.i.i.i.i, label %bb2.i.i.i.i.i4.i.i31.i.i.i.i

bb2.i.i.i.i.i4.i.i31.i.i.i.i:                     ; preds = %bb6.i.i.i.i
  %alloc_size.i.i.i.i.i.i6.i.i32.i.i.i.i = shl nuw i64 %_7.sroa.0.0.copyload.i.i.i.i, 3
; call __rustc::__rust_dealloc
  tail call void @_RNvCsfmtydf4BlqR_7___rustc14___rust_dealloc(ptr noundef nonnull %_7.sroa.4.0.copyload.i.i.i.i, i64 noundef %alloc_size.i.i.i.i.i.i6.i.i32.i.i.i.i, i64 noundef range(i64 1, -9223372036854775807) 8) #86, !noalias !249627
  br label %bb41.i.i.i.i

bb41.i.i.i.i:                                     ; preds = %bb2.i.i.i.i.i4.i.i31.i.i.i.i, %bb6.i.i.i.i
  %44 = icmp eq i64 %pred_atoms.sroa.0.0.i.i.i.i, 0
  br i1 %44, label %bb1.backedge.i, label %_RNCNvNtCsagrXUsJVDRq_3lin5index17index_covers_pred0B5_.exit.thread.sink.split.i.i

bb8.i.i.i.i:                                      ; preds = %bb1.i.i.i.i.i.i, %bb5.i.i.i.i
  %45 = load i64, ptr %_17.i.i.i.i.i, align 8, !range !41674, !noalias !249610, !noundef !2388
  %46 = icmp eq i64 %45, -9223372036854775805
  br i1 %46, label %bb10.i.i.i.i, label %bb45.i.i.i.i

bb10.i.i.i.i:                                     ; preds = %bb8.i.i.i.i
  %47 = getelementptr inbounds nuw i8, ptr %_17.i.i.i.i.i, i64 56
  %48 = load i8, ptr %47, align 8, !range !2751, !noalias !249610, !noundef !2388
  %49 = icmp eq i8 %48, 0
  %50 = getelementptr inbounds nuw i8, ptr %_17.i.i.i.i.i, i64 32
  br i1 %49, label %bb12.i.i.i.i, label %bb11.i.i.i.i

bb12.i.i.i.i:                                     ; preds = %bb10.i.i.i.i
  %51 = load i64, ptr %50, align 8, !range !41674, !noalias !249610, !noundef !2388
  switch i64 %51, label %bb17.i.i.i.i [
    i64 -9223372036854775807, label %bb14.i.i.i.i
    i64 -9223372036854775806, label %bb18.i.i.i.i
  ]

bb11.i.i.i.i:                                     ; preds = %bb10.i.i.i.i
  %switch.i.i.i.i = icmp eq i8 %48, 1
  %or.cond9.i.i.i.i = select i1 %switch.i.i.i.i, i1 true, i1 %.not6.i.i.i.i
  br i1 %or.cond9.i.i.i.i, label %bb45.i.i.i.i, label %bb30.i.i.i.i

bb14.i.i.i.i:                                     ; preds = %bb12.i.i.i.i
  %52 = getelementptr inbounds nuw i8, ptr %_17.i.i.i.i.i, i64 40
  %_25.i.i.i.i = load i64, ptr %52, align 8, !noalias !249610, !noundef !2388
  %53 = add i64 %_25.i.i.i.i, -9007199254740992
  %_23.i.i.i.i = icmp ult i64 %53, -18014398509481983
  br i1 %_23.i.i.i.i, label %bb45.i.i.i.i, label %bb17.i.i.i.i

bb18.i.i.i.i:                                     ; preds = %bb12.i.i.i.i
  %54 = getelementptr inbounds nuw i8, ptr %_17.i.i.i.i.i, i64 40
  %_30.i.i.i.i = load double, ptr %54, align 8, !noalias !249610, !noundef !2388
  %_70.i.i.i.i = tail call double @llvm.fabs.f64(double %_30.i.i.i.i)
  %_29.i.i.i.i = fcmp one double %_70.i.i.i.i, +inf
  %_31.i.i.i.i = fcmp ult double %_70.i.i.i.i, f0x4340000000000000
  %or.cond.i.i.i.i = and i1 %_29.i.i.i.i, %_31.i.i.i.i
  br i1 %or.cond.i.i.i.i, label %bb17.i.i.i.i, label %bb45.i.i.i.i

bb17.i.i.i.i:                                     ; preds = %bb18.i.i.i.i, %bb14.i.i.i.i, %bb12.i.i.i.i
  tail call void @llvm.experimental.noalias.scope.decl(metadata !249630)
  br i1 %_123.not.i.i.i.i.i, label %bb45.i.i.i.i, label %bb13.lr.ph.i.i.i.i.i

bb13.lr.ph.i.i.i.i.i:                             ; preds = %bb17.i.i.i.i
  %55 = getelementptr inbounds nuw i8, ptr %_17.i.i.i.i.i, i64 16
  %_6.i.i.i.i.i.i.i = load ptr, ptr %55, align 8, !alias.scope !249633, !noalias !249636, !nonnull !2388, !noundef !2388
  %56 = getelementptr inbounds nuw i8, ptr %_17.i.i.i.i.i, i64 24
  %_5.i.i.i.i.i.i.i = load i64, ptr %56, align 8, !alias.scope !249633, !noalias !249636, !noundef !2388
  br label %bb13.i.i.i.i.i

bb13.i.i.i.i.i:                                   ; preds = %bb1.backedge.i.i.i.i.i, %bb13.lr.ph.i.i.i.i.i
  %_2224.i.i.i.i.i = phi ptr [ %_74.i.i.i.i, %bb13.lr.ph.i.i.i.i.i ], [ %_22.i.i.i.i.i, %bb1.backedge.i.i.i.i.i ]
  %_22.i.i.i.i.i = getelementptr inbounds nuw i8, ptr %_2224.i.i.i.i.i, i64 48
  tail call void @llvm.experimental.noalias.scope.decl(metadata !249642)
  %v.i.i.i.i.i.i = getelementptr inbounds nuw i8, ptr %_2224.i.i.i.i.i, i64 24
  call void @llvm.lifetime.start.p0(ptr nonnull %_6.i.i.i.i.i.i), !noalias !249643
; invoke alloc::str::join_generic_copy::<str, u8, alloc::string::String>
  invoke fastcc void @_RINvNtCsj8lNtgb2Igp_5alloc3str17join_generic_copyehNtNtB4_6string6StringECsagrXUsJVDRq_3lin(ptr noalias nofree noundef align 8 captures(none) dereferenceable(24) %_6.i.i.i.i.i.i, ptr noalias nofree noundef nonnull readonly align 8 captures(address, read_provenance) %_6.i.i.i.i.i.i.i, i64 noundef %_5.i.i.i.i.i.i.i, ptr noalias nofree noundef nonnull readonly captures(address, read_provenance) @alloc_0f52f98e9fa44caf6668aeb0b1c94ca5, i64 noundef 1)
          to label %.noexc37.i.i.i.i unwind label %cleanup1.loopexit.i.i.i.i, !noalias !249610

.noexc37.i.i.i.i:                                 ; preds = %bb13.i.i.i.i.i
  %57 = getelementptr inbounds nuw i8, ptr %_2224.i.i.i.i.i, i64 16
  %_10.i.i.i.i.i.i = load i64, ptr %57, align 8, !alias.scope !249642, !noalias !249644, !noundef !2388
  %_13.i.i.i.i.i.i = load i64, ptr %17, align 8, !noalias !249643, !noundef !2388
  %_15.i.i.i.i.i.i = icmp eq i64 %_10.i.i.i.i.i.i, %_13.i.i.i.i.i.i
  br i1 %_15.i.i.i.i.i.i, label %bb7.i.i.i.i.i.i, label %bb4.i.i.i.i.i.i

bb7.i.i.i.i.i.i:                                  ; preds = %.noexc37.i.i.i.i
  %_14.i.i.i.i.i.i = load ptr, ptr %18, align 8, !noalias !249643, !nonnull !2388, !noundef !2388
  %58 = getelementptr inbounds nuw i8, ptr %_2224.i.i.i.i.i, i64 8
  %_11.i.i.i.i.i.i = load ptr, ptr %58, align 8, !alias.scope !249642, !noalias !249644, !nonnull !2388, !noundef !2388
  %_16.i.i.i.i.i.i = tail call i32 @memcmp(ptr nonnull %_11.i.i.i.i.i.i, ptr nonnull %_14.i.i.i.i.i.i, i64 %_10.i.i.i.i.i.i), !noalias !249645
  %_5.i.i.i.i.i.i = icmp eq i32 %_16.i.i.i.i.i.i, 0
  br i1 %_5.i.i.i.i.i.i, label %bb2.i.i.i.i.i.i, label %bb4.i.i.i.i.i.i

bb4.i.i.i.i.i.i:                                  ; preds = %bb7.i.i.i.i.i.i, %.noexc37.i.i.i.i
  tail call void @llvm.experimental.noalias.scope.decl(metadata !249646)
  tail call void @llvm.experimental.noalias.scope.decl(metadata !249649)
  %_1.val2.i.i.i.i.i.i.i.i = load i64, ptr %_6.i.i.i.i.i.i, align 8, !range !2805, !alias.scope !249652, !noalias !249643, !noundef !2388
  %59 = icmp eq i64 %_1.val2.i.i.i.i.i.i.i.i, 0
  br i1 %59, label %_RNCNvNtCsagrXUsJVDRq_3lin5index21index_covers_conjunct0B5_.exit.i.i.i.i.i, label %bb2.i.i.i4.i.i.i.i.i.i.i.i

bb2.i.i.i4.i.i.i.i.i.i.i.i:                       ; preds = %bb4.i.i.i.i.i.i
  %_1.val3.i.i.i.i.i.i.i.i = load ptr, ptr %18, align 8, !alias.scope !249652, !noalias !249643, !nonnull !2388, !noundef !2388
; call __rustc::__rust_dealloc
  tail call void @_RNvCsfmtydf4BlqR_7___rustc14___rust_dealloc(ptr noundef nonnull %_1.val3.i.i.i.i.i.i.i.i, i64 noundef %_1.val2.i.i.i.i.i.i.i.i, i64 noundef range(i64 1, -9223372036854775807) 1) #86, !noalias !249653
  br label %_RNCNvNtCsagrXUsJVDRq_3lin5index21index_covers_conjunct0B5_.exit.i.i.i.i.i

bb2.i.i.i.i.i.i:                                  ; preds = %bb7.i.i.i.i.i.i
  tail call void @llvm.experimental.noalias.scope.decl(metadata !249654)
  tail call void @llvm.experimental.noalias.scope.decl(metadata !249657)
  %_1.val2.i.i1.i.i.i.i.i.i = load i64, ptr %_6.i.i.i.i.i.i, align 8, !range !2805, !alias.scope !249660, !noalias !249643, !noundef !2388
  %60 = icmp eq i64 %_1.val2.i.i1.i.i.i.i.i.i, 0
  br i1 %60, label %_RINvNtCsaLbZTW5namK_4core3ptr9drop_glueNtNtCsj8lNtgb2Igp_5alloc6string6StringECsagrXUsJVDRq_3lin.exit4.i.i.i.i.i.i, label %bb2.i.i.i4.i.i2.i.i.i.i.i.i

bb2.i.i.i4.i.i2.i.i.i.i.i.i:                      ; preds = %bb2.i.i.i.i.i.i
; call __rustc::__rust_dealloc
  tail call void @_RNvCsfmtydf4BlqR_7___rustc14___rust_dealloc(ptr noundef nonnull %_14.i.i.i.i.i.i, i64 noundef %_1.val2.i.i1.i.i.i.i.i.i, i64 noundef range(i64 1, -9223372036854775807) 1) #86, !noalias !249661
  br label %_RINvNtCsaLbZTW5namK_4core3ptr9drop_glueNtNtCsj8lNtgb2Igp_5alloc6string6StringECsagrXUsJVDRq_3lin.exit4.i.i.i.i.i.i

_RINvNtCsaLbZTW5namK_4core3ptr9drop_glueNtNtCsj8lNtgb2Igp_5alloc6string6StringECsagrXUsJVDRq_3lin.exit4.i.i.i.i.i.i: ; preds = %bb2.i.i.i4.i.i2.i.i.i.i.i.i, %bb2.i.i.i.i.i.i
  call void @llvm.lifetime.end.p0(ptr nonnull %_6.i.i.i.i.i.i), !noalias !249643
; call <lin::ast::Value as core::cmp::PartialEq>::eq
  %61 = tail call fastcc noundef zeroext i1 @_RNvXs1s_NtCsagrXUsJVDRq_3lin3astNtB6_5ValueNtNtCsaLbZTW5namK_4core3cmp9PartialEq2eq(ptr noalias nofree noundef readonly align 8 captures(address, read_provenance) dereferenceable(24) %v.i.i.i.i.i.i, ptr noalias nofree noundef readonly align 8 captures(address, read_provenance) dereferenceable(24) %50), !noalias !249644
  br i1 %61, label %bb2.backedge.i.i.i.i, label %bb1.backedge.i.i.i.i.i

bb1.backedge.i.i.i.i.i:                           ; preds = %_RNCNvNtCsagrXUsJVDRq_3lin5index21index_covers_conjunct0B5_.exit.i.i.i.i.i, %_RINvNtCsaLbZTW5namK_4core3ptr9drop_glueNtNtCsj8lNtgb2Igp_5alloc6string6StringECsagrXUsJVDRq_3lin.exit4.i.i.i.i.i.i
  %_12.not.i.i.i.i.i = icmp eq ptr %_22.i.i.i.i.i, %_78.i.i.i.i
  br i1 %_12.not.i.i.i.i.i, label %bb45.i.i.i.i, label %bb13.i.i.i.i.i

_RNCNvNtCsagrXUsJVDRq_3lin5index21index_covers_conjunct0B5_.exit.i.i.i.i.i: ; preds = %bb2.i.i.i4.i.i.i.i.i.i.i.i, %bb4.i.i.i.i.i.i
  call void @llvm.lifetime.end.p0(ptr nonnull %_6.i.i.i.i.i.i), !noalias !249643
  br label %bb1.backedge.i.i.i.i.i

bb45.i.i.i.i:                                     ; preds = %bb37.i.i.i.i, %bb36.i.i.i.i, %bb17.i.i.i.i, %bb18.i.i.i.i, %bb14.i.i.i.i, %bb11.i.i.i.i, %bb8.i.i.i.i, %bb1.backedge.i.i.i.i.i, %bb34.i.i.i.i
  %62 = icmp eq i64 %_7.sroa.0.0.copyload.i.i.i.i, 0
  br i1 %62, label %bb46.i.i.i.i, label %bb2.i.i.i.i.i4.i.i38.i.i.i.i

bb2.i.i.i.i.i4.i.i38.i.i.i.i:                     ; preds = %bb45.i.i.i.i
  %alloc_size.i.i.i.i.i.i6.i.i39.i.i.i.i = shl nuw i64 %_7.sroa.0.0.copyload.i.i.i.i, 3
; call __rustc::__rust_dealloc
  tail call void @_RNvCsfmtydf4BlqR_7___rustc14___rust_dealloc(ptr noundef nonnull %_7.sroa.4.0.copyload.i.i.i.i, i64 noundef %alloc_size.i.i.i.i.i.i6.i.i39.i.i.i.i, i64 noundef range(i64 1, -9223372036854775807) 8) #86, !noalias !249662
  br label %bb46.i.i.i.i

bb2.backedge.i.i.i.i:                             ; preds = %bb13.i.i.i.i.i.i, %_RINvNtCsaLbZTW5namK_4core3ptr9drop_glueNtNtCsj8lNtgb2Igp_5alloc6string6StringECsagrXUsJVDRq_3lin.exit4.i.i.i.i.i.i, %bb37.i.i.i.i
  %_9.i.i.i.i.i = icmp eq ptr %_23.i.i.i.i.i, %_61.i.i.i.i
  br i1 %_9.i.i.i.i.i, label %bb6.i.i.i.i, label %bb5.i.i.i.i

bb30.i.i.i.i:                                     ; preds = %bb11.i.i.i.i
  call void @llvm.lifetime.start.p0(ptr nonnull %_51.i.i.i.i), !noalias !249665
  %63 = getelementptr inbounds nuw i8, ptr %_17.i.i.i.i.i, i64 16
  %_6.i.i.i.i.i = load ptr, ptr %63, align 8, !alias.scope !249666, !noalias !249669, !nonnull !2388, !noundef !2388
  %64 = getelementptr inbounds nuw i8, ptr %_17.i.i.i.i.i, i64 24
  %_5.i.i.i.i.i = load i64, ptr %64, align 8, !alias.scope !249666, !noalias !249669, !noundef !2388
; invoke alloc::str::join_generic_copy::<str, u8, alloc::string::String>
  invoke fastcc void @_RINvNtCsj8lNtgb2Igp_5alloc3str17join_generic_copyehNtNtB4_6string6StringECsagrXUsJVDRq_3lin(ptr noalias nofree noundef align 8 captures(none) dereferenceable(24) %_51.i.i.i.i, ptr noalias nofree noundef nonnull readonly align 8 captures(address, read_provenance) %_6.i.i.i.i.i, i64 noundef %_5.i.i.i.i.i, ptr noalias nofree noundef nonnull readonly captures(address, read_provenance) @alloc_0f52f98e9fa44caf6668aeb0b1c94ca5, i64 noundef 1)
          to label %bb32.i.i.i.i unwind label %cleanup1.loopexit.split-lp.i.i.i.i, !noalias !249610

bb32.i.i.i.i:                                     ; preds = %bb30.i.i.i.i
  %_88.i.i.i.i = load i64, ptr %15, align 8, !noalias !249665, !noundef !2388
  %_90.i.i.i.i = icmp eq i64 %_85.i.i.i.i, %_88.i.i.i.i
  br i1 %_90.i.i.i.i, label %bb62.i.i.i.i, label %bb33.i.i.i.i

bb62.i.i.i.i:                                     ; preds = %bb32.i.i.i.i
  %_89.i.i.i.i = load ptr, ptr %16, align 8, !noalias !249665, !nonnull !2388, !noundef !2388
  %_91.i.i.i.i = tail call i32 @memcmp(ptr nonnull %_86.i.i.i.i, ptr nonnull %_89.i.i.i.i, i64 %_85.i.i.i.i), !noalias !249610
  %_83.not.i.i.i.i = icmp eq i32 %_91.i.i.i.i, 0
  br i1 %_83.not.i.i.i.i, label %bb35.i.i.i.i, label %bb33.i.i.i.i

bb33.i.i.i.i:                                     ; preds = %bb62.i.i.i.i, %bb32.i.i.i.i
  tail call void @llvm.experimental.noalias.scope.decl(metadata !249671)
  tail call void @llvm.experimental.noalias.scope.decl(metadata !249674)
  %_1.val2.i.i.i.i.i.i = load i64, ptr %_51.i.i.i.i, align 8, !range !2805, !alias.scope !249677, !noalias !249665, !noundef !2388
  %65 = icmp eq i64 %_1.val2.i.i.i.i.i.i, 0
  br i1 %65, label %bb34.i.i.i.i, label %bb2.i.i.i4.i.i42.i.i.i.i

bb2.i.i.i4.i.i42.i.i.i.i:                         ; preds = %bb33.i.i.i.i
  %_1.val3.i.i.i.i.i.i = load ptr, ptr %16, align 8, !alias.scope !249677, !noalias !249665, !nonnull !2388, !noundef !2388
; call __rustc::__rust_dealloc
  tail call void @_RNvCsfmtydf4BlqR_7___rustc14___rust_dealloc(ptr noundef nonnull %_1.val3.i.i.i.i.i.i, i64 noundef %_1.val2.i.i.i.i.i.i, i64 noundef range(i64 1, -9223372036854775807) 1) #86, !noalias !249678
  br label %bb34.i.i.i.i

bb35.i.i.i.i:                                     ; preds = %bb62.i.i.i.i
  tail call void @llvm.experimental.noalias.scope.decl(metadata !249679)
  tail call void @llvm.experimental.noalias.scope.decl(metadata !249682)
  %_1.val2.i.i43.i.i.i.i = load i64, ptr %_51.i.i.i.i, align 8, !range !2805, !alias.scope !249685, !noalias !249665, !noundef !2388
  %66 = icmp eq i64 %_1.val2.i.i43.i.i.i.i, 0
  br i1 %66, label %bb36.i.i.i.i, label %bb2.i.i.i4.i.i44.i.i.i.i

bb2.i.i.i4.i.i44.i.i.i.i:                         ; preds = %bb35.i.i.i.i
; call __rustc::__rust_dealloc
  tail call void @_RNvCsfmtydf4BlqR_7___rustc14___rust_dealloc(ptr noundef nonnull %_89.i.i.i.i, i64 noundef %_1.val2.i.i43.i.i.i.i, i64 noundef range(i64 1, -9223372036854775807) 1) #86, !noalias !249686
  br label %bb36.i.i.i.i

bb36.i.i.i.i:                                     ; preds = %bb2.i.i.i4.i.i44.i.i.i.i, %bb35.i.i.i.i
  call void @llvm.lifetime.end.p0(ptr nonnull %_51.i.i.i.i), !noalias !249665
  %67 = load i8, ptr %47, align 8, !range !2751, !noalias !249610, !noundef !2388
  %_92.not.i.i.i.i = icmp eq i8 %39, %67
  br i1 %_92.not.i.i.i.i, label %bb37.i.i.i.i, label %bb45.i.i.i.i

bb37.i.i.i.i:                                     ; preds = %bb36.i.i.i.i
; call <lin::ast::Value as core::cmp::PartialEq>::eq
  %_95.i.i.i.i = tail call fastcc noundef zeroext i1 @_RNvXs1s_NtCsagrXUsJVDRq_3lin3astNtB6_5ValueNtNtCsaLbZTW5namK_4core3cmp9PartialEq2eq(ptr noalias nofree noundef readonly align 8 captures(address, read_provenance) dereferenceable(24) %rv.i.i.i.i, ptr noalias nofree noundef readonly align 8 captures(address, read_provenance) dereferenceable(24) %50), !noalias !249610
  br i1 %_95.i.i.i.i, label %bb2.backedge.i.i.i.i, label %bb45.i.i.i.i

bb34.i.i.i.i:                                     ; preds = %bb2.i.i.i4.i.i42.i.i.i.i, %bb33.i.i.i.i
  call void @llvm.lifetime.end.p0(ptr nonnull %_51.i.i.i.i), !noalias !249665
  br label %bb45.i.i.i.i

bb46.i.i.i.i:                                     ; preds = %bb2.i.i.i.i.i4.i.i38.i.i.i.i, %bb45.i.i.i.i
  %68 = icmp eq i64 %pred_atoms.sroa.0.0.i.i.i.i, 0
  br i1 %68, label %bb13, label %_RNCNvNtCsagrXUsJVDRq_3lin5index17index_covers_pred0B5_.exit.thread.sink.split.i.thread.i

_RNCNvNtCsagrXUsJVDRq_3lin5index17index_covers_pred0B5_.exit.thread.sink.split.i.thread.i: ; preds = %bb46.i.i.i.i
  %alloc_size.i.i.i.i5.i48.i.i6.i5.i = shl nuw i64 %pred_atoms.sroa.0.0.i.i.i.i, 3
  call void @llvm.assume(i1 true) [ "nonnull"(ptr %pred_atoms.sroa.8.0.i.i.i.i) ]
; call __rustc::__rust_dealloc
  tail call void @_RNvCsfmtydf4BlqR_7___rustc14___rust_dealloc(ptr noundef nonnull %pred_atoms.sroa.8.0.i.i.i.i, i64 noundef %alloc_size.i.i.i.i5.i48.i.i6.i5.i, i64 noundef range(i64 1, -9223372036854775807) 8) #86, !noalias !249610
  br label %bb13

_RNCNvNtCsagrXUsJVDRq_3lin5index17index_covers_pred0B5_.exit.thread.sink.split.i.i: ; preds = %bb41.i.i.i.i
  %alloc_size.i.i.i.i5.i48.i.i6.i.i = shl nuw i64 %pred_atoms.sroa.0.0.i.i.i.i, 3
  call void @llvm.assume(i1 true) [ "nonnull"(ptr %pred_atoms.sroa.8.0.i.i.i.i) ]
; call __rustc::__rust_dealloc
  tail call void @_RNvCsfmtydf4BlqR_7___rustc14___rust_dealloc(ptr noundef nonnull %pred_atoms.sroa.8.0.i.i.i.i, i64 noundef %alloc_size.i.i.i.i5.i48.i.i6.i.i, i64 noundef range(i64 1, -9223372036854775807) 8) #86, !noalias !249610
  br label %bb1.backedge.i

bb1.backedge.i:                                   ; preds = %_RNCNvNtCsagrXUsJVDRq_3lin5index17index_covers_pred0B5_.exit.thread.sink.split.i.i, %bb41.i.i.i.i
  %exitcond.not.i = icmp eq i64 %20, %uses.1
  br i1 %exitcond.not.i, label %bb13, label %bb3.i

bb13:                                             ; preds = %bb1.backedge.i, %bb6, %bb46.i.i.i.i, %_RNCNvNtCsagrXUsJVDRq_3lin5index17index_covers_pred0B5_.exit.thread.sink.split.i.thread.i
  %_3.i = phi i1 [ false, %bb46.i.i.i.i ], [ false, %_RNCNvNtCsagrXUsJVDRq_3lin5index17index_covers_pred0B5_.exit.thread.sink.split.i.thread.i ], [ true, %bb6 ], [ true, %bb1.backedge.i ]
  tail call void @llvm.experimental.noalias.scope.decl(metadata !249687)
  %_7.i.i.i6184 = icmp eq i64 %uses.1, 0
  br i1 %_7.i.i.i6184, label %bb4.i21, label %bb5.i.i.i7

bb6.i.i.i4:                                       ; preds = %bb5.i.i.i7
  %_7.i.i.i6 = icmp eq i64 %69, %uses.1
  br i1 %_7.i.i.i6, label %bb4.i21, label %bb5.i.i.i7

bb5.i.i.i7:                                       ; preds = %bb13, %bb6.i.i.i4
  %_3.sroa.0.0.i.i.i5185 = phi i64 [ %69, %bb6.i.i.i4 ], [ 0, %bb13 ]
  %_6.i.i.i8 = getelementptr inbounds nuw [72 x i8], ptr %_17, i64 %_3.sroa.0.0.i.i.i5185
  %69 = add nuw nsw i64 %_3.sroa.0.0.i.i.i5185, 1
; invoke core::ptr::drop_glue::<lin::ast::Pred>
  invoke fastcc void @_RINvNtCsaLbZTW5namK_4core3ptr9drop_glueNtNtCsagrXUsJVDRq_3lin3ast4PredEBF_(ptr noalias nofree noundef align 8 dereferenceable(72) %_6.i.i.i8)
          to label %bb6.i.i.i4 unwind label %cleanup.i.i.i9, !noalias !249687

bb4.i.i.i10:                                      ; preds = %bb3.i.i.i13
  %70 = add i64 %_3.sroa.0.1.i.i.i11187, 1
  %_5.i.i.i12 = icmp eq i64 %70, %uses.1
  br i1 %_5.i.i.i12, label %cleanup.body.i16, label %bb3.i.i.i13

cleanup.i.i.i9:                                   ; preds = %bb5.i.i.i7
  %71 = landingpad { ptr, i32 }
          cleanup
  %_5.i.i.i12186 = icmp eq i64 %69, %uses.1
  br i1 %_5.i.i.i12186, label %cleanup.body.i16, label %bb3.i.i.i13

bb3.i.i.i13:                                      ; preds = %cleanup.i.i.i9, %bb4.i.i.i10
  %_3.sroa.0.1.i.i.i11187 = phi i64 [ %70, %bb4.i.i.i10 ], [ %69, %cleanup.i.i.i9 ]
  %_4.i.i.i14 = getelementptr inbounds nuw [72 x i8], ptr %_17, i64 %_3.sroa.0.1.i.i.i11187
; invoke core::ptr::drop_glue::<lin::ast::Pred>
  invoke fastcc void @_RINvNtCsaLbZTW5namK_4core3ptr9drop_glueNtNtCsagrXUsJVDRq_3lin3ast4PredEBF_(ptr noalias nofree noundef align 8 dereferenceable(72) %_4.i.i.i14) #87
          to label %bb4.i.i.i10 unwind label %terminate.i.i.i15, !noalias !249687

terminate.i.i.i15:                                ; preds = %bb3.i.i.i13
  %72 = landingpad { ptr, i32 }
          filter [0 x ptr] zeroinitializer
; call core::panicking::panic_in_cleanup
  tail call void @_RNvNtCsaLbZTW5namK_4core9panicking16panic_in_cleanup() #88, !noalias !249687
  unreachable

cleanup.body.i16:                                 ; preds = %bb4.i.i.i10, %cleanup.i.i.i9
  %_1.val2.i17 = load i64, ptr %branches, align 8, !range !2805, !alias.scope !249687, !noundef !2388
  %73 = icmp eq i64 %_1.val2.i17, 0
  br i1 %73, label %common.resume, label %common.resume.sink.split

bb4.i21:                                          ; preds = %bb6.i.i.i4, %bb13
  %_1.val4.i22 = load i64, ptr %branches, align 8, !range !2805, !alias.scope !249687, !noundef !2388
  %74 = icmp eq i64 %_1.val4.i22, 0
  br i1 %74, label %_RINvNtCsaLbZTW5namK_4core3ptr9drop_glueINtNtCsj8lNtgb2Igp_5alloc3vec3VecNtNtCsagrXUsJVDRq_3lin3ast4PredEEB1c_.exit25, label %bb2.i.i.i6.i23

bb2.i.i.i6.i23:                                   ; preds = %bb4.i21
  %alloc_size.i.i.i.i7.i24 = mul nuw i64 %_1.val4.i22, 72
; call __rustc::__rust_dealloc
  tail call void @_RNvCsfmtydf4BlqR_7___rustc14___rust_dealloc(ptr noundef nonnull %_17, i64 noundef %alloc_size.i.i.i.i7.i24, i64 noundef range(i64 1, -9223372036854775807) 8) #86, !noalias !249687
  br label %_RINvNtCsaLbZTW5namK_4core3ptr9drop_glueINtNtCsj8lNtgb2Igp_5alloc3vec3VecNtNtCsagrXUsJVDRq_3lin3ast4PredEEB1c_.exit25

_RINvNtCsaLbZTW5namK_4core3ptr9drop_glueINtNtCsj8lNtgb2Igp_5alloc3vec3VecNtNtCsagrXUsJVDRq_3lin3ast4PredEEB1c_.exit25: ; preds = %bb4.i21, %bb2.i.i.i6.i23
  call void @llvm.lifetime.end.p0(ptr nonnull %branches)
  br label %bb9

bb9:                                              ; preds = %bb3, %_RINvNtCsaLbZTW5namK_4core3ptr9drop_glueINtNtCsj8lNtgb2Igp_5alloc3vec3VecNtNtCsagrXUsJVDRq_3lin3ast4PredEEB1c_.exit, %_RINvNtCsaLbZTW5namK_4core3ptr9drop_glueINtNtCsj8lNtgb2Igp_5alloc3vec3VecNtNtCsagrXUsJVDRq_3lin3ast4PredEEB1c_.exit25
  %_0.sroa.0.0.off0 = phi i1 [ false, %_RINvNtCsaLbZTW5namK_4core3ptr9drop_glueINtNtCsj8lNtgb2Igp_5alloc3vec3VecNtNtCsagrXUsJVDRq_3lin3ast4PredEEB1c_.exit ], [ %_3.i, %_RINvNtCsaLbZTW5namK_4core3ptr9drop_glueINtNtCsj8lNtgb2Igp_5alloc3vec3VecNtNtCsagrXUsJVDRq_3lin3ast4PredEEB1c_.exit25 ], [ false, %bb3 ]
  ret i1 %_0.sroa.0.0.off0

terminate:                                        ; preds = %cleanup.body
  %75 = landingpad { ptr, i32 }
          filter [0 x ptr] zeroinitializer
; call core::panicking::panic_in_cleanup
  tail call void @_RNvNtCsaLbZTW5namK_4core9panicking16panic_in_cleanup() #88
  unreachable
}
