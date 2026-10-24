use lin::{Db,OpenOpts};
fn main() {
 let path=std::env::args().nth(1).expect("fixture path");
 match Db::open_with(&path,OpenOpts::default()) {
  Ok(db)=>{println!("opened gen={} docs={}",db.r#gen(),db.store.collection("docs").len());},
  Err(e)=>{eprintln!("rejected: {e}");std::process::exit(2);}
 }
}
