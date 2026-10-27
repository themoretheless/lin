use lin::{Db,Quotas};
fn main(){
 let path=std::env::temp_dir().join(format!("lin-schema-reject-{}",std::process::id()));std::fs::create_dir(&path).unwrap();
 let mut db=Db::open(&path).unwrap().with_quotas(Quotas{max_query_bytes:64*1024*1024,..Default::default()});
 db.run(r#"insert docs {id:"seed",body:"seed"}"#).unwrap();db.checkpoint().unwrap();let generation=db.store.r#gen;
 let name="x".repeat(17*1024*1024);let query=format!("col {name} {{id: text}}");
 let error=db.run(&query).unwrap_err();assert!(error.to_string().contains("16MiB"),"unexpected error: {error}");assert_eq!(db.store.r#gen,generation);assert!(db.run("docs | count").is_ok());
 let close=db.close();drop(db);let reopened=Db::open(&path);let message=reopened.as_ref().err().map(ToString::to_string);
 println!("{}",serde_json::json!({"wal_rejected":true,"generation_unchanged":true,"close_ok":close.is_ok(),"reopen_ok":reopened.is_ok(),"reopen_error":message}));drop(reopened);std::fs::remove_dir_all(&path).unwrap();assert!(message.is_none(),"reopen after rejected schema: {message:?}");
}
