use btae_eval::dsh::{read_dsh,DshLimits};
fn main() {
 let args:Vec<String>=std::env::args().collect();
 let code=args[2].parse::<i32>().unwrap();
 match read_dsh(std::fs::File::open(&args[1]).unwrap(),DshLimits::default(),Some(code)) {
  Ok(s)=>println!("{}",serde_json::to_string_pretty(&s).unwrap()),
  Err(e)=>{println!("{}",serde_json::json!({"accepted_transcript":false,"parse_error":e.to_string(),"exit_code":code,"billing_verified":false}));}
 }
}
