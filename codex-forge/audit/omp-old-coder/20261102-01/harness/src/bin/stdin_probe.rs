use btae_eval::dsh::{read_dsh, DshLimits};
fn main() {
    let mut b = Vec::new();
    std::io::Read::read_to_end(&mut std::io::stdin(), &mut b).unwrap();
    let lim = DshLimits::default();
    match read_dsh(std::io::Cursor::new(b), lim, Some(0)) {
        Ok(s) => println!("OK completed={} lines={} usage={:?} status={:?}", s.completed, s.lines_received, s.usage, s.usage_status),
        Err(e) => println!("ERR {e}"),
    }
}
