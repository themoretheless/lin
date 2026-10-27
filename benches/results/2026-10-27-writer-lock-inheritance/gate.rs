#[cfg(unix)]
fn main(){
 let dir=std::env::temp_dir().join(format!("lin-lock-inheritance-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
 let mut db=lin::Db::open(&dir).unwrap();
 db.run(r#"insert docs {id:"lock-probe",uri:"fixture://lock",title:"locked",layer:"wiki"}"#).unwrap();
 let mut pipe=[0;2];assert_eq!(unsafe{libc::pipe(pipe.as_mut_ptr())},0);
 let pid=unsafe{libc::fork()};assert!(pid>=0);
 if pid==0{
  // Only async-signal-safe syscalls in forked child; no Rust allocation/drop.
  unsafe{libc::close(pipe[1]);let mut byte=0u8;libc::read(pipe[0],(&mut byte as *mut u8).cast(),1);libc::_exit(0);}
 }
 unsafe{libc::close(pipe[0]);}
 let closed=db.close();
 let reopened=if closed.is_ok(){lin::Db::open(&dir)}else{Err(lin::Error::new("close failed"))};
 let message=reopened.as_ref().err().map(ToString::to_string);
 let byte=1u8;unsafe{libc::write(pipe[1],(&byte as *const u8).cast(),1);libc::close(pipe[1]);let mut status=0;assert_eq!(libc::waitpid(pid,&mut status,0),pid);assert_eq!(status,0);}
 let ok=reopened.is_ok();if let Ok(mut reopened)=reopened{reopened.close().unwrap();}
 let mut after_child=lin::Db::open(&dir).unwrap();let row=after_child.store.get_by_id("docs","lock-probe").unwrap();assert_eq!(row["title"].text(),Some("locked"));after_child.close().unwrap();
 println!("{}",serde_json::json!({"close_ok":closed.is_ok(),"reopen_while_child_holds_fds":ok,"reopen_error":message,"reopen_after_child_exit":true,"exact_data_readback":true}));
 std::fs::remove_dir_all(&dir).unwrap();
 assert!(ok,"writer lock survives close while fork child retains descriptors");
}
#[cfg(not(unix))]fn main(){panic!("Unix flock diagnostic only");}
